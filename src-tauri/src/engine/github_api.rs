use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::error::{Error, Result};
use super::secrets::Tokens;
use super::settings::RepoRef;

/// Public identifiers of the GitHub App, injected at build time (see `.env.example`).
pub const CLIENT_ID: Option<&str> = option_env!("SR_GITHUB_CLIENT_ID");
pub const APP_SLUG: Option<&str> = option_env!("SR_GITHUB_APP_SLUG");
const DEVICE_PAGE: &str = "https://github.com/login/device";
const ATTEMPTS: u32 = 3;

pub fn client_id() -> Result<&'static str> {
    CLIENT_ID.filter(|s| !s.is_empty()).ok_or(Error::NoClientId)
}

pub fn app_slug() -> Result<&'static str> {
    APP_SLUG.filter(|s| !s.is_empty()).ok_or(Error::NoClientId)
}

fn http() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder().user_agent("session-relay").timeout(Duration::from_secs(30)).build().map_err(|e| Error::Network(e.to_string()))
}

/// Sends with retries: network errors, 5xx and HTML pages are transient (Spike C).
/// No file we read starts with `<`, so the HTML test is safe for raw contents too.
fn send(mut request: impl FnMut() -> reqwest::blocking::RequestBuilder) -> Result<(u16, Vec<u8>)> {
    let mut last = String::new();
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(Duration::from_secs(u64::from(attempt) * 2));
        }
        match request().send() {
            Err(e) => last = e.to_string(),
            Ok(response) => {
                let status = response.status().as_u16();
                let body = response.bytes().map(|b| b.to_vec()).unwrap_or_default();
                if status < 500 && !body.trim_ascii_start().starts_with(b"<") {
                    return Ok((status, body));
                }
                last = format!("HTTP {status}");
            }
        }
    }
    Err(Error::Network(last))
}

fn send_json<T: DeserializeOwned>(mut request: impl FnMut() -> reqwest::blocking::RequestBuilder) -> Result<(u16, Option<T>)> {
    let (status, body) = send(|| request().header("Accept", "application/json"))?;
    Ok((status, serde_json::from_slice(&body).ok()))
}

#[derive(Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

pub fn request_device_code() -> Result<DeviceCode> {
    let client = http()?;
    let form = [("client_id", client_id()?)];
    let (_, code) = send_json::<DeviceCode>(|| client.post("https://github.com/login/device/code").form(&form))?;
    let code = code.ok_or_else(|| Error::Auth("device code refused".into()))?;
    // Never send the user anywhere else, whatever the response says.
    if code.verification_uri != DEVICE_PAGE {
        return Err(Error::Invalid(format!("unexpected verification uri {}", code.verification_uri)));
    }
    Ok(code)
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    error: Option<String>,
}

pub enum Poll {
    Pending,
    SlowDown,
    Done(Tokens),
    Expired,
    Denied,
}

pub fn poll_device(device_code: &str) -> Result<Poll> {
    let client = http()?;
    let form = [("client_id", client_id()?), ("device_code", device_code), ("grant_type", "urn:ietf:params:oauth:grant-type:device_code")];
    let (_, body) = send_json::<TokenResponse>(|| client.post("https://github.com/login/oauth/access_token").form(&form))?;
    let body = body.ok_or_else(|| Error::Network("empty token response".into()))?;
    Ok(match (body.error.as_deref(), tokens_from(&body)) {
        (_, Some(tokens)) => Poll::Done(tokens),
        (Some("authorization_pending"), _) => Poll::Pending,
        (Some("slow_down"), _) => Poll::SlowDown,
        (Some("expired_token"), _) => Poll::Expired,
        (Some("access_denied"), _) => Poll::Denied,
        (other, _) => return Err(Error::Auth(other.unwrap_or("no token").into())),
    })
}

/// Device-flow tokens refresh without a client secret (verified in Spike C); the refresh token rotates.
pub fn refresh(refresh_token: &str) -> Result<Tokens> {
    let client = http()?;
    let form = [("client_id", client_id()?), ("grant_type", "refresh_token"), ("refresh_token", refresh_token)];
    let (_, body) = send_json::<TokenResponse>(|| client.post("https://github.com/login/oauth/access_token").form(&form))?;
    let body = body.ok_or_else(|| Error::Network("empty token response".into()))?;
    tokens_from(&body).ok_or_else(|| Error::Auth(body.error.unwrap_or_else(|| "refresh failed".into())))
}

fn tokens_from(body: &TokenResponse) -> Option<Tokens> {
    let access = body.access_token.clone()?;
    let expires_at = body.expires_in.map(|s| chrono::Utc::now() + chrono::Duration::seconds(s));
    Some(Tokens { access, refresh: body.refresh_token.clone(), expires_at })
}

fn get(token: &str, path: &str, accept: &'static str) -> Result<Option<Vec<u8>>> {
    let client = http()?;
    let url = format!("https://api.github.com{path}");
    let (status, body) = send(|| client.get(&url).bearer_auth(token).header("Accept", accept).header("X-GitHub-Api-Version", "2022-11-28"))?;
    match status {
        401 => Err(Error::Auth("token rejected".into())),
        404 => Ok(None),
        200..=299 => Ok(Some(body)),
        status => Err(Error::Network(format!("GET {path}: HTTP {status}"))),
    }
}

/// None on 404, which for a user token also means "not shared with the app".
fn api<T: DeserializeOwned>(token: &str, path: &str) -> Result<Option<T>> {
    let Some(body) = get(token, path, "application/vnd.github+json")? else { return Ok(None) };
    serde_json::from_slice(&body).map(Some).map_err(|e| Error::Network(format!("GET {path}: {e}")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub login: String,
    pub avatar_url: String,
}

pub fn user(token: &str) -> Result<User> {
    api(token, "/user")?.ok_or_else(|| Error::Auth("no user for token".into()))
}

#[derive(Debug, Deserialize)]
pub struct Installation {
    pub app_slug: String,
}

pub fn installations(token: &str) -> Result<Vec<Installation>> {
    #[derive(Deserialize)]
    struct Page {
        installations: Vec<Installation>,
    }
    Ok(api::<Page>(token, "/user/installations?per_page=100")?.map_or_else(Vec::new, |p| p.installations))
}

#[derive(Debug, Deserialize)]
pub struct Owner {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub struct Repo {
    pub name: String,
    pub private: bool,
    pub owner: Owner,
}

pub fn repo(token: &str, owner: &str, name: &str) -> Result<Option<Repo>> {
    api(token, &format!("/repos/{owner}/{name}"))
}

/// Entry names at the root of `main`; empty for an empty repository or one without `main`.
pub fn root_names(token: &str, repo: &RepoRef) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Entry {
        name: String,
    }
    let entries: Option<Vec<Entry>> = api(token, &format!("/repos/{}/{}/contents/?ref=main", repo.owner, repo.name))?;
    Ok(entries.unwrap_or_default().into_iter().map(|e| e.name).collect())
}

/// Raw bytes of one file on `main`, without a clone.
pub fn file(token: &str, repo: &RepoRef, path: &str) -> Result<Option<Vec<u8>>> {
    get(token, &format!("/repos/{}/{}/contents/{path}?ref=main", repo.owner, repo.name), "application/vnd.github.raw+json")
}
