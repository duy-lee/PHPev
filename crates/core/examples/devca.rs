//! Dev/test helper (not shipped). `devca` creates the PHPev CA in the real home
//! and adds it to PHP's bundle WITHOUT touching the Windows trust store, so
//! HTTPS can be tested non-interactively (`curl --cacert`); `phpev secure` is
//! the real flow. `devca issue <host> <out.der>` writes a leaf certificate.

fn main() -> std::io::Result<()> {
    let layout = phpev_core::layout::Layout::resolve()?;
    let config = phpev_core::config::Config::load(&layout.config_file())?;
    let ca = phpev_core::ca::Ca::load_or_create(&layout, &config.tld)?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [cmd, host, out] = args.as_slice() {
        if cmd == "issue" {
            std::fs::write(out, ca.issue(host)?.0)?;
            return Ok(());
        }
    }
    phpev_core::ca::update_php_bundle(&layout, Some(ca.pem()))?;
    println!("{}", layout.certs_dir().join("ca.crt.pem").display());
    Ok(())
}
