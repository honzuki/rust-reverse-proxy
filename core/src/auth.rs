use rand::RngCore;
use sha2::{Digest, Sha512};

const TOKEN_SIZE: usize = 512 / 8;

pub const METADATA_KEY: &str = "authorization";
pub type TokenHash = String;
pub type Token = String;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to parse the token")]
    FailedToParseToken,
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn hash_token(token: &str) -> Result<TokenHash> {
    let token = hex::decode(token).map_err(|_| Error::FailedToParseToken)?;

    let mut hasher = Sha512::new();
    hasher.update(token);

    Ok(hex::encode(hasher.finalize()))
}

/// Generates a cryptographically secure token
///
/// returns the token as hex-encoded string
pub fn generate_token() -> Token {
    let mut token = [0u8; TOKEN_SIZE];
    rand::thread_rng().fill_bytes(&mut token);

    hex::encode(token)
}
