//! HTTPS for sites marked `secure` (M7): certificates are issued by the local
//! CA on first use per host (SNI), kept in memory only, and never for hosts
//! that are not secured sites.

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};

use phpev_core::ca::Ca;
use phpev_core::sites;
use rustls::crypto::ring::{default_provider, sign::any_supported_type};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use rustls::ServerConfig;

use crate::log::log_warn;
use crate::router::Router;

pub struct SniResolver {
    router: Arc<Router>,
    ca: Ca,
    issued: Mutex<HashMap<String, Arc<CertifiedKey>>>,
}

impl std::fmt::Debug for SniResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SniResolver")
    }
}

impl ResolvesServerCert for SniResolver {
    fn resolve(&self, hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        let host = hello.server_name()?.to_ascii_lowercase();
        if let Some(key) = self.issued.lock().unwrap_or_else(|e| e.into_inner()).get(&host) {
            return Some(key.clone());
        }
        let (config, all) = self.router.snapshot();
        let site = sites::find(&all, &host, &config.tld)?;
        if !site.secure {
            return None; // only secured sites get a certificate
        }
        let key = match self.issue(&host) {
            Ok(k) => Arc::new(k),
            Err(e) => {
                log_warn!("tls: cannot issue a certificate for {host}: {e}");
                return None;
            }
        };
        self.issued
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(host, key.clone());
        Some(key)
    }
}

impl SniResolver {
    fn issue(&self, host: &str) -> io::Result<CertifiedKey> {
        let (cert, key) = self.ca.issue(host)?;
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key));
        let signing = any_supported_type(&key).map_err(io::Error::other)?;
        Ok(CertifiedKey::new(vec![CertificateDer::from(cert)], signing))
    }
}

/// TLS settings for the router: TLS 1.2/1.3, HTTP/1.1 only.
pub fn server_config(router: Arc<Router>, ca: Ca) -> io::Result<Arc<ServerConfig>> {
    let resolver = Arc::new(SniResolver {
        router,
        ca,
        issued: Mutex::new(HashMap::new()),
    });
    let mut config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(io::Error::other)?
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use phpev_core::layout::{HomeSource, Layout};
    use rustls::client::danger::ServerCertVerifier;
    use rustls::client::WebPkiServerVerifier;
    use rustls::pki_types::{ServerName, UnixTime};

    /// The CA's name constraints must make standard verifiers reject any name
    /// outside the development TLDs, even when the CA itself signed it.
    #[test]
    fn name_constraints_reject_real_domains() {
        let root = std::env::temp_dir().join(format!("phpev-tls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let ca = Ca::load_or_create(&Layout::at(root.clone(), HomeSource::ProcessEnv), "localhost").unwrap();

        let mut roots = rustls::RootCertStore::empty();
        roots.add(CertificateDer::from(ca.der().to_vec())).unwrap();
        let verifier = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), Arc::new(default_provider()))
            .build()
            .unwrap();
        for (host, allowed) in [
            ("blog.localhost", true),
            ("shop.test", true),
            ("google.com", false),
            ("bank.vn", false),
        ] {
            let (cert, _) = ca.issue(host).unwrap();
            let name = ServerName::try_from(host.to_owned()).unwrap();
            let result = verifier.verify_server_cert(&CertificateDer::from(cert), &[], &name, &[], UnixTime::now());
            assert_eq!(result.is_ok(), allowed, "{host}: {result:?}");
        }
        std::fs::remove_dir_all(&root).unwrap();
    }
}
