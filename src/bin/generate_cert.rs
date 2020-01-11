use std::fs;
use failure::ResultExt;

fn main() -> Result<(), failure::Error> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
    let key = cert.serialize_private_key_pem();
    let cert = cert.serialize_pem()?;
    fs::create_dir_all("certs").context("failed to create certificate directory")?;
    fs::write("certs/cert.pem", &cert).context("failed to write certificate")?;
    fs::write("certs/key.pem", &key).context("failed to write private key")?;
    Ok(())
}