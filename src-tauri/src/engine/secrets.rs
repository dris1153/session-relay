use std::collections::HashMap;
use std::sync::Once;

use age::secrecy::{ExposeSecret, SecretString};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::error::{Error, Result};

const SERVICE: &str = "session-relay";
const TOKENS: &str = "github-tokens";
const IDENTITY: &str = "age-identity";

/// GitHub App user tokens: the access token lives 8 h, the refresh token rotates on every use.
#[derive(Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access: String,
    pub refresh: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

static STORE: Once = Once::new();

/// Windows Credential Manager with Local persistence (Spike C: the default, Enterprise, roams
/// with the profile). Any process of the same user can read these; see README threat model.
fn entry(user: &str) -> Result<keyring_core::Entry> {
    STORE.call_once(|| match windows_native_keyring_store::Store::new() {
        Ok(store) => keyring_core::set_default_store(store),
        Err(e) => log::error!("credential store unavailable: {e}"),
    });
    let modifiers = HashMap::from([("persistence", "Local")]);
    keyring_core::Entry::new_with_modifiers(SERVICE, user, &modifiers).map_err(|e| Error::Secrets(e.to_string()))
}

fn get(user: &str) -> Result<Option<String>> {
    match entry(user)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::Secrets(e.to_string())),
    }
}

fn set(user: &str, value: &str) -> Result<()> {
    entry(user)?.set_password(value).map_err(|e| Error::Secrets(e.to_string()))
}

fn delete(user: &str) -> Result<()> {
    match entry(user)?.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error::Secrets(e.to_string())),
    }
}

/// Unreadable tokens count as signed out, so the UI can offer a fresh sign-in.
pub fn load_tokens() -> Result<Option<Tokens>> {
    Ok(get(TOKENS)?.and_then(|json| serde_json::from_str(&json).inspect_err(|e| log::error!("stored tokens are unreadable: {e}")).ok()))
}

pub fn save_tokens(tokens: &Tokens) -> Result<()> {
    set(TOKENS, &serde_json::to_string(tokens).expect("serializable"))
}

pub fn load_identity() -> Result<Option<SecretString>> {
    Ok(get(IDENTITY)?.map(SecretString::from))
}

pub fn save_identity(identity: &SecretString) -> Result<()> {
    set(IDENTITY, identity.expose_secret())
}

/// Sign-out: drops the tokens and the cached key from this machine.
pub fn clear_all() -> Result<()> {
    delete(TOKENS)?;
    delete(IDENTITY)
}
