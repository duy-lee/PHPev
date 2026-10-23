//! Local certificate authority for HTTPS sites (DESIGN_SPEC §4, M7).
//!
//! * The CA key lives in `certs\ca.key.pem`; site certificates are issued in
//!   memory by the daemon and never written to disk.
//! * The CA carries **name constraints**: it can only sign names under the
//!   development TLDs, so a leaked CA key cannot impersonate real websites.
//! * It is trusted through the *CurrentUser* Root store (no admin; Windows
//!   asks the user to confirm), and appended to PHP's `cacert.pem` so apps
//!   can call their own https:// URL.

use std::io;
use std::path::PathBuf;
use std::ptr;

use rcgen::{
    date_time_ymd, BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, GeneralSubtree, IsCa, KeyPair, KeyUsagePurpose, NameConstraints,
};
use windows_sys::Win32::Security::Cryptography::{
    CertAddEncodedCertificateToStore, CertCloseStore, CertCreateCertificateContext, CertDeleteCertificateFromStore,
    CertFindCertificateInStore, CertFreeCertificateContext, CertOpenStore, CERT_FIND_EXISTING,
    CERT_STORE_ADD_REPLACE_EXISTING, CERT_STORE_PROV_SYSTEM_W, CERT_SYSTEM_STORE_CURRENT_USER, HCERTSTORE,
    PKCS_7_ASN_ENCODING, X509_ASN_ENCODING,
};

use crate::layout::Layout;
use crate::wide::to_wide;
use crate::{clock, config};

const CA_NAME: &str = "PHPev Local Development CA";
/// TLDs the CA may sign for, besides the configured one.
const PERMITTED: &[&str] = &["localhost", "test", "local"];
const BUNDLE_BEGIN: &str = "# BEGIN PHPev local CA";
const BUNDLE_END: &str = "# END PHPev local CA";
/// Site certificates are short-lived; the daemon re-issues them on start.
const SITE_CERT_DAYS: i64 = 397;

pub struct Ca {
    key: KeyPair,
    /// Rebuilt from the key on every load; used only to sign site certificates
    /// (subject, key and key id match the stored certificate).
    signer: Certificate,
    /// The certificate as first written to disk: what the user trusted. ECDSA
    /// signatures are randomized, so a rebuilt certificate differs byte-wise.
    stored_der: Vec<u8>,
    stored_pem: String,
}

fn key_path(layout: &Layout) -> PathBuf {
    layout.certs_dir().join("ca.key.pem")
}

fn cert_path(layout: &Layout) -> PathBuf {
    layout.certs_dir().join("ca.crt.pem")
}

/// Records which TLDs the stored certificate permits; a change means a new
/// certificate (which the user then has to trust again).
fn constraints_path(layout: &Layout) -> PathBuf {
    layout.certs_dir().join("ca.constraints")
}

fn err(e: rcgen::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

/// True once `phpev secure` has created the CA key.
pub fn exists(layout: &Layout) -> bool {
    key_path(layout).is_file()
}

/// The stored CA certificate (DER), if the CA exists.
pub fn stored_der(layout: &Layout) -> Option<Vec<u8>> {
    pem_to_der(&std::fs::read_to_string(cert_path(layout)).ok()?)
}

impl Ca {
    /// Loads the CA, creating its key and certificate on first use. `extra_tld`
    /// adds the configured TLD to the name constraints if it is not built in;
    /// changing it issues a new CA certificate (same key and name).
    pub fn load_or_create(layout: &Layout, extra_tld: &str) -> io::Result<Self> {
        std::fs::create_dir_all(layout.certs_dir())?;
        let key = match std::fs::read_to_string(key_path(layout)) {
            Ok(pem) => KeyPair::from_pem(&pem).map_err(err)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let key = KeyPair::generate().map_err(err)?;
                config::write_atomic(&key_path(layout), key.serialize_pem().as_bytes())?;
                key
            }
            Err(e) => return Err(e),
        };

        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, CA_NAME);
        dn.push(DnType::OrganizationName, "PHPev");
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let mut permitted: Vec<String> = PERMITTED.iter().map(|s| (*s).to_owned()).collect();
        if !permitted.iter().any(|p| p == extra_tld) && !extra_tld.is_empty() {
            permitted.push(extra_tld.to_owned());
        }
        params.name_constraints = Some(NameConstraints {
            permitted_subtrees: permitted.into_iter().map(GeneralSubtree::DnsName).collect(),
            excluded_subtrees: Vec::new(),
        });
        // Fixed validity: rebuilding from the key yields the same CA every time.
        params.not_before = date_time_ymd(2024, 1, 1);
        params.not_after = date_time_ymd(2049, 12, 31);
        params.serial_number = Some(rcgen::SerialNumber::from(vec![0x50, 0x48, 0x50, 0x65, 0x76]));
        let constraints = permitted_list(&params);
        let signer = params.self_signed(&key).map_err(err)?;

        let stored = std::fs::read_to_string(cert_path(layout))
            .ok()
            .filter(|_| std::fs::read_to_string(constraints_path(layout)).ok().as_deref() == Some(constraints.as_str()))
            .and_then(|pem| pem_to_der(&pem).map(|der| (pem, der)));
        let (stored_pem, stored_der) = match stored {
            Some(found) => found,
            None => {
                config::write_atomic(&cert_path(layout), signer.pem().as_bytes())?;
                config::write_atomic(&constraints_path(layout), constraints.as_bytes())?;
                (signer.pem(), signer.der().to_vec())
            }
        };
        Ok(Self {
            key,
            signer,
            stored_der,
            stored_pem,
        })
    }

    pub fn der(&self) -> &[u8] {
        &self.stored_der
    }

    pub fn pem(&self) -> &str {
        &self.stored_pem
    }

    /// Issues a certificate for `host` (DER cert, DER PKCS#8 key).
    pub fn issue(&self, host: &str) -> io::Result<(Vec<u8>, Vec<u8>)> {
        let mut params = CertificateParams::new(vec![host.to_owned()]).map_err(err)?;
        params.distinguished_name.push(DnType::CommonName, host);
        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let ymd = |(y, m, d): (i64, u32, u32)| date_time_ymd(y as i32, m as u8, d as u8);
        // Start yesterday to tolerate small clock differences.
        params.not_before = ymd(clock::ymd_in_days(-1));
        params.not_after = ymd(clock::ymd_in_days(SITE_CERT_DAYS));
        let key = KeyPair::generate().map_err(err)?;
        let cert = params.signed_by(&key, &self.signer, &self.key).map_err(err)?;
        Ok((cert.der().to_vec(), key.serialize_der()))
    }
}

fn permitted_list(params: &CertificateParams) -> String {
    params
        .name_constraints
        .iter()
        .flat_map(|nc| nc.permitted_subtrees.iter())
        .map(|t| match t {
            GeneralSubtree::DnsName(n) => n.as_str(),
            _ => "",
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// First PEM block → DER (standard base64, line breaks ignored).
fn pem_to_der(pem: &str) -> Option<Vec<u8>> {
    let body: String = pem
        .lines()
        .skip_while(|l| !l.starts_with("-----BEGIN"))
        .skip(1)
        .take_while(|l| !l.starts_with("-----END"))
        .collect();
    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in body.bytes().filter(|&c| c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    (!out.is_empty()).then_some(out)
}

// ---------------------------------------------------------------- trust store

struct Store(HCERTSTORE);

impl Store {
    fn current_user_root() -> io::Result<Self> {
        let name = to_wide("Root");
        let h = unsafe {
            CertOpenStore(
                CERT_STORE_PROV_SYSTEM_W,
                0,
                0,
                CERT_SYSTEM_STORE_CURRENT_USER,
                name.as_ptr().cast(),
            )
        };
        if h.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(h))
    }

    /// Runs `f` with the store's copy of `der`, if present.
    fn with_existing<T>(
        &self,
        der: &[u8],
        f: impl FnOnce(*const windows_sys::Win32::Security::Cryptography::CERT_CONTEXT) -> T,
    ) -> Option<T> {
        unsafe {
            let probe =
                CertCreateCertificateContext(X509_ASN_ENCODING | PKCS_7_ASN_ENCODING, der.as_ptr(), der.len() as u32);
            if probe.is_null() {
                return None;
            }
            let found = CertFindCertificateInStore(
                self.0,
                X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
                0,
                CERT_FIND_EXISTING,
                probe.cast(),
                ptr::null(),
            );
            CertFreeCertificateContext(probe);
            (!found.is_null()).then(|| f(found))
        }
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        unsafe { CertCloseStore(self.0, 0) };
    }
}

pub fn is_trusted(der: &[u8]) -> io::Result<bool> {
    let store = Store::current_user_root()?;
    Ok(store
        .with_existing(der, |ctx| unsafe {
            CertFreeCertificateContext(ctx);
        })
        .is_some())
}

/// Adds the CA to CurrentUser\Root. Windows shows a confirmation dialog; a
/// refusal is `ErrorKind::PermissionDenied`.
pub fn trust(der: &[u8]) -> io::Result<()> {
    let store = Store::current_user_root()?;
    let ok = unsafe {
        CertAddEncodedCertificateToStore(
            store.0,
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            der.as_ptr(),
            der.len() as u32,
            CERT_STORE_ADD_REPLACE_EXISTING,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        let e = io::Error::last_os_error();
        // ERROR_CANCELLED (1223) when the user clicks "No".
        if e.raw_os_error() == Some(1223) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the certificate was not trusted (dialog declined)",
            ));
        }
        return Err(e);
    }
    Ok(())
}

/// Removes the CA from CurrentUser\Root (Windows asks to confirm). Returns
/// false if it was not there.
pub fn untrust(der: &[u8]) -> io::Result<bool> {
    let store = Store::current_user_root()?;
    match store.with_existing(der, |ctx| unsafe { CertDeleteCertificateFromStore(ctx) }) {
        None => Ok(false),
        Some(0) => Err(io::Error::last_os_error()),
        Some(_) => Ok(true),
    }
}

// ---------------------------------------------------------------- PHP bundle

/// Returns `bundle` with the PHPev CA block replaced by `ca_pem` (or removed
/// when `None`), leaving the curl CA list untouched.
pub fn render_bundle(bundle: &str, ca_pem: Option<&str>) -> String {
    let mut out = String::with_capacity(bundle.len() + 1024);
    let mut inside = false;
    for line in bundle.lines() {
        if line.trim() == BUNDLE_BEGIN {
            inside = true;
        } else if inside && line.trim() == BUNDLE_END {
            inside = false;
        } else if !inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    if let Some(pem) = ca_pem {
        out.push('\n');
        out.push_str(BUNDLE_BEGIN);
        out.push('\n');
        out.push_str(pem.trim_end());
        out.push('\n');
        out.push_str(BUNDLE_END);
        out.push('\n');
    }
    out
}

/// Adds (or removes) the CA in PHP's `cacert.pem`, if that file exists.
pub fn update_php_bundle(layout: &Layout, ca_pem: Option<&str>) -> io::Result<()> {
    let path = layout.cacert();
    let Ok(bundle) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let updated = render_bundle(&bundle, ca_pem);
    if updated != bundle {
        config::write_atomic(&path, updated.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::HomeSource;

    fn layout(name: &str) -> Layout {
        let root = std::env::temp_dir().join(format!("phpev-ca-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Layout::at(root, HomeSource::ProcessEnv)
    }

    #[test]
    fn ca_is_stable_across_reloads_and_issues_certs() {
        let l = layout("stable");
        let first = Ca::load_or_create(&l, "localhost").unwrap();
        let again = Ca::load_or_create(&l, "localhost").unwrap();
        // Same key, name and parameters: the CA the user trusted is reproduced.
        assert_eq!(first.der(), again.der());
        assert_eq!(pem_to_der(first.pem()).as_deref(), Some(first.der()));
        let (cert, key) = again.issue("blog.localhost").unwrap();
        assert!(!cert.is_empty() && !key.is_empty());
        // A different TLD changes the constraints, hence the certificate.
        let other = Ca::load_or_create(&l, "dev").unwrap();
        assert_ne!(other.der(), first.der());
        std::fs::remove_dir_all(l.root()).unwrap();
    }

    #[test]
    fn bundle_block_is_replaced_and_removed() {
        let curl = "## curl bundle\n-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\n";
        let with = render_bundle(
            curl,
            Some("-----BEGIN CERTIFICATE-----\nCA\n-----END CERTIFICATE-----\n"),
        );
        assert!(with.starts_with(curl) && with.contains(BUNDLE_BEGIN) && with.contains("\nCA\n"));
        let twice = render_bundle(&with, Some("NEW"));
        assert!(twice.contains("NEW") && !twice.contains("\nCA\n"));
        assert_eq!(render_bundle(&twice, None), curl);
    }
}
