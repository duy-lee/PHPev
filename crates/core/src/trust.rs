//! Authenticode verification for downloads that come without a published hash
//! (the VC++ redistributable from aka.ms).

use std::io;
use std::mem;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
use windows_sys::Win32::Security::WinTrust::{
    WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2,
    WINTRUST_DATA, WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOKE_WHOLECHAIN, WTD_STATEACTION_CLOSE,
    WTD_STATEACTION_VERIFY, WTD_UI_NONE,
};

use crate::wide::{from_wide, to_wide};

/// Checks that `file` carries a valid, trusted Authenticode signature (including
/// revocation) and returns the signer's display name.
pub fn signer(file: &Path) -> io::Result<String> {
    let path = to_wide(file);
    let mut file_info: WINTRUST_FILE_INFO = unsafe { mem::zeroed() };
    file_info.cbStruct = mem::size_of::<WINTRUST_FILE_INFO>() as u32;
    file_info.pcwszFilePath = path.as_ptr();

    let mut data: WINTRUST_DATA = unsafe { mem::zeroed() };
    data.cbStruct = mem::size_of::<WINTRUST_DATA>() as u32;
    data.dwUIChoice = WTD_UI_NONE;
    data.fdwRevocationChecks = WTD_REVOKE_WHOLECHAIN;
    data.dwUnionChoice = WTD_CHOICE_FILE;
    data.Anonymous.pFile = &mut file_info;
    data.dwStateAction = WTD_STATEACTION_VERIFY;

    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(
            INVALID_HANDLE_VALUE as _,
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };
    let name = if status == 0 {
        signer_name(data.hWVTStateData)
    } else {
        None
    };

    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        WinVerifyTrust(
            INVALID_HANDLE_VALUE as _,
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };

    if status != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{}: no valid Authenticode signature (WinVerifyTrust 0x{:08X})",
                file.display(),
                status as u32
            ),
        ));
    }
    name.ok_or_else(|| io::Error::other(format!("{}: could not read the signer", file.display())))
}

/// Requires a valid signature by "Microsoft Corporation".
pub fn require_microsoft(file: &Path) -> io::Result<()> {
    let who = signer(file)?;
    if who == "Microsoft Corporation" {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} is signed by \"{who}\", not Microsoft Corporation", file.display()),
        ))
    }
}

fn signer_name(state: HANDLE) -> Option<String> {
    unsafe {
        let prov = WTHelperProvDataFromStateData(state);
        if prov.is_null() {
            return None;
        }
        let sgnr = WTHelperGetProvSignerFromChain(prov, 0, 0, 0);
        if sgnr.is_null() || (*sgnr).csCertChain == 0 || (*sgnr).pasCertChain.is_null() {
            return None;
        }
        // Chain element 0 is the signing (leaf) certificate.
        let cert = (*(*sgnr).pasCertChain).pCert;
        let mut buf = [0u16; 256];
        let n = CertGetNameStringW(
            cert,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            ptr::null(),
            buf.as_mut_ptr(),
            buf.len() as u32,
        );
        (n > 1).then(|| from_wide(&buf).to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_file_is_rejected() {
        let f = std::env::temp_dir().join(format!("phpev-unsigned-{}.exe", std::process::id()));
        std::fs::write(&f, b"MZ not really a program").unwrap();
        assert!(signer(&f).is_err());
        assert!(require_microsoft(&f).is_err());
        std::fs::remove_file(&f).unwrap();
    }

    /// `PHPEV_TRUST_FILES="a.exe;b.exe" cargo test -- --ignored --nocapture signer_of`
    #[test]
    #[ignore = "needs real signed files"]
    fn signer_of_files_from_env() {
        for f in std::env::var("PHPEV_TRUST_FILES")
            .unwrap_or_default()
            .split(';')
            .filter(|s| !s.is_empty())
        {
            let p = Path::new(f);
            println!(
                "{f}: signer={:?} microsoft={:?}",
                signer(p),
                require_microsoft(p).is_ok()
            );
        }
    }
}
