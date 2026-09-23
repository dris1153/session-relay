use age::secrecy::{ExposeSecret, SecretString};

use super::error::{Error, Result};
use super::project_identity::ProjectKey;

const NAMES_CONTEXT: &str = "session-relay v1 names";
const MAC_CONTEXT: &str = "session-relay v1 manifest mac";
const ZSTD_LEVEL: i32 = 3;
/// Pinned so a fast machine cannot pick a factor a slower one refuses to decrypt.
const SCRYPT_WORK_FACTOR: u8 = 18;
const MAX_SCRYPT_WORK_FACTOR: u8 = 22;

/// Everything derived from the age identity. The recipient is never persisted anywhere,
/// so ciphertext cannot be forged without the identity itself.
pub struct Keys {
    identity: age::x25519::Identity,
    recipient: age::x25519::Recipient,
    names: [u8; 32],
    mac: [u8; 32],
}

impl Keys {
    pub fn generate() -> Self {
        Self::from_identity(age::x25519::Identity::generate())
    }

    pub fn parse(identity: &str) -> Result<Self> {
        identity.trim().parse::<age::x25519::Identity>().map(Self::from_identity).map_err(|_| Error::Decrypt)
    }

    fn from_identity(identity: age::x25519::Identity) -> Self {
        let secret = identity.to_string();
        let names = blake3::derive_key(NAMES_CONTEXT, secret.expose_secret().as_bytes());
        let mac = blake3::derive_key(MAC_CONTEXT, secret.expose_secret().as_bytes());
        Self { recipient: identity.to_public(), identity, names, mac }
    }

    pub fn identity_secret(&self) -> SecretString {
        self.identity.to_string()
    }

    /// Keyed hash hiding content and project names from anyone without the identity.
    pub fn hasher(&self) -> blake3::Hasher {
        blake3::Hasher::new_keyed(&self.names)
    }

    pub fn chunk_name(&self, data: &[u8]) -> String {
        self.hasher().update(data).finalize().to_hex()[..32].to_string()
    }

    pub fn key_hash16(&self, key: &ProjectKey) -> String {
        let id = format!("{}\n{}", key.remote, key.subpath);
        self.hasher().update(id.as_bytes()).finalize().to_hex()[..16].to_string()
    }

    pub fn mac(&self, data: &[u8]) -> String {
        blake3::keyed_hash(&self.mac, data).to_hex().to_string()
    }

    /// zstd then age: encrypted data does not compress.
    pub fn seal(&self, plain: &[u8]) -> Result<Vec<u8>> {
        let compressed = zstd::encode_all(plain, ZSTD_LEVEL).map_err(|e| Error::Encrypt(e.to_string()))?;
        age::encrypt(&self.recipient, &compressed).map_err(|e| Error::Encrypt(e.to_string()))
    }

    pub fn open(&self, sealed: &[u8]) -> Result<Vec<u8>> {
        let compressed = age::decrypt(&self.identity, sealed).map_err(|_| Error::Decrypt)?;
        zstd::decode_all(compressed.as_slice()).map_err(|_| Error::Decrypt)
    }
}

/// Passphrase-wraps the identity for `keys/identity.age`.
pub fn wrap_identity(keys: &Keys, passphrase: &str) -> Result<Vec<u8>> {
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.to_owned()));
    recipient.set_work_factor(SCRYPT_WORK_FACTOR);
    age::encrypt(&recipient, keys.identity_secret().expose_secret().as_bytes()).map_err(|e| Error::Encrypt(e.to_string()))
}

pub fn unwrap_identity(wrapped: &[u8], passphrase: &str) -> Result<Keys> {
    let mut identity = age::scrypt::Identity::new(SecretString::from(passphrase.to_owned()));
    identity.set_max_work_factor(MAX_SCRYPT_WORK_FACTOR);
    let plain = age::decrypt(&identity, wrapped).map_err(|_| Error::Decrypt)?;
    Keys::parse(std::str::from_utf8(&plain).map_err(|_| Error::Decrypt)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_roundtrip_and_wrong_key_fails() {
        let (a, b) = (Keys::generate(), Keys::generate());
        let sealed = a.seal(b"hello transcript").unwrap();
        assert_eq!(a.open(&sealed).unwrap(), b"hello transcript");
        assert!(matches!(b.open(&sealed), Err(Error::Decrypt)));
        assert_eq!(a.chunk_name(b"x"), a.chunk_name(b"x"));
        assert_ne!(a.chunk_name(b"x"), b.chunk_name(b"x"));
    }

    #[test]
    fn passphrase_wrap_roundtrip() {
        let keys = Keys::generate();
        let wrapped = wrap_identity(&keys, "correct horse battery staple").unwrap();
        let back = unwrap_identity(&wrapped, "correct horse battery staple").unwrap();
        assert_eq!(back.chunk_name(b"x"), keys.chunk_name(b"x"));
        assert!(matches!(unwrap_identity(&wrapped, "wrong"), Err(Error::Decrypt)));
    }
}
