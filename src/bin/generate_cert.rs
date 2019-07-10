use std::fs;
use failure::ResultExt;

fn main() -> Result<(), failure::Error> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]);
    let key = cert.serialize_private_key_der();
    let cert = cert.serialize_der();
    fs::create_dir_all("certs").context("failed to create certificate directory")?;
    fs::write("certs/cert.der", &cert).context("failed to write certificate")?;
    fs::write("certs/key.der", &key).context("failed to write private key")?;
    Ok(())
}