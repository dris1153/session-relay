use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("git {args} failed: {stderr}")]
    Git { args: String, stderr: String },
    #[error("git {args} timed out")]
    GitTimeout { args: String },
    #[error("push rejected because the remote changed")]
    LeaseRejected,
    #[error("another sync is running")]
    Busy,
    #[error("encryption failed: {0}")]
    Encrypt(String),
    #[error("decryption failed")]
    Decrypt,
    #[error("manifest integrity check failed")]
    ManifestTampered,
    #[error("manifest belongs to another project")]
    ManifestKeyMismatch,
    #[error("remote snapshot is older than one already seen")]
    RollbackDetected,
    #[error("project is not linked on this machine")]
    NotLinked,
    #[error("the store was created with a different key")]
    WrongIdentity,
    #[error("invalid data: {0}")]
    Invalid(String),
}

impl Error {
    /// Stable identifier the UI maps to localized text.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Io { .. } => "io",
            Error::Git { .. } => "git_failed",
            Error::GitTimeout { .. } => "git_timeout",
            Error::LeaseRejected => "lease_rejected",
            Error::Busy => "busy",
            Error::Encrypt(_) => "encrypt_failed",
            Error::Decrypt => "decrypt_failed",
            Error::ManifestTampered => "manifest_tampered",
            Error::ManifestKeyMismatch => "manifest_key_mismatch",
            Error::RollbackDetected => "rollback_detected",
            Error::NotLinked => "not_linked",
            Error::WrongIdentity => "wrong_identity",
            Error::Invalid(_) => "invalid_data",
        }
    }
}

pub trait IoContext<T> {
    fn at(self, path: &Path) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn at(self, path: &Path) -> Result<T> {
        self.map_err(|source| Error::Io { path: path.to_path_buf(), source })
    }
}
