pub mod dashboard;
pub mod session_tools;
pub mod session_view;
pub mod settings;
pub mod setup;
pub mod workspace;

use serde::Serialize;

use crate::engine::error::Error;

/// What the UI receives on failure: a stable code it maps to localized text.
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: String,
    /// English detail for logs and bug reports; never shown as UI copy.
    pub detail: String,
}

impl From<Error> for CommandError {
    fn from(e: Error) -> Self {
        Self { code: e.code().to_string(), detail: e.to_string() }
    }
}

pub type CmdResult<T> = Result<T, CommandError>;

/// Engine work is blocking (git, crypto, file IO): keep it off the async runtime threads.
pub async fn blocking<T: Send + 'static>(work: impl FnOnce() -> crate::engine::error::Result<T> + Send + 'static) -> CmdResult<T> {
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(result) => result.map_err(Into::into),
        Err(e) => Err(CommandError { code: "internal".into(), detail: e.to_string() }),
    }
}
