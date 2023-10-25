use std::{fs::File, io::Write, path::Path};

use anyhow::Context;
use rcgen::generate_simple_self_signed;
use tonic::transport::Identity;

const SERVER_TLS_KEY_FILE_NAME: &str = "server.key";
const SERVER_TLS_CERT_FILE_NAME: &str = "server.pem";

// Generates an identity from the server's tls key.
// if it can't find the tls key, it will generate a self-signed key and use it instead.
pub fn load_server_identity(base: &Path) -> anyhow::Result<Identity> {
    let key_path = base.join(SERVER_TLS_KEY_FILE_NAME);
    let cert_path = base.join(SERVER_TLS_CERT_FILE_NAME);

    let (key, cert) = match read_files(&key_path, &cert_path) {
        Ok(data) => data,
        Err(_) => {
            // try to generate new ones
            generate_key(&key_path, &cert_path)
                .context("failed to generate self-signed tls key!")?
        }
    };

    println!(
        "The used tls certificate can be found at: {}",
        cert_path.display()
    );

    Ok(Identity::from_pem(cert, key))
}

// try to read the tls file if they exist
fn read_files(key_path: &Path, cert_path: &Path) -> anyhow::Result<(String, String)> {
    Ok((
        std::fs::read_to_string(key_path)?,
        std::fs::read_to_string(cert_path)?,
    ))
}

// create a self-signed key
fn generate_key(key_path: &Path, cert_path: &Path) -> anyhow::Result<(String, String)> {
    // generate new key
    let alt_names = rrp::tls::DEFAULT_ALT_NAMES
        .iter()
        .map(|name| name.to_string())
        .collect::<Vec<_>>();
    let generated = generate_simple_self_signed(alt_names)?;
    let key = generated.serialize_private_key_pem();
    let cert = generated.serialize_pem()?;

    // make sure to save the generated key
    let mut key_file = File::create(key_path)?;
    key_file.write_all(key.as_bytes())?;
    let mut cert_file = File::create(cert_path)?;
    cert_file.write_all(cert.as_bytes())?;

    Ok((key, cert))
}
