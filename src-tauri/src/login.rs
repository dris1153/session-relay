use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::app_state::AppState;
use crate::engine::auth;
use crate::engine::error::Error;
use crate::engine::github_api::{self, DeviceCode, Poll};

#[derive(Clone, Serialize)]
pub struct AuthChanged {
    pub signed_in: bool,
    /// `expired`, `denied` or an error code when sign-in did not complete.
    pub reason: Option<String>,
}

/// Polls GitHub until the user approves the device code, then stores the tokens.
/// A newer login attempt makes this loop exit quietly.
pub fn spawn_poll(app: AppHandle, state: Arc<AppState>, code: DeviceCode, attempt: u64) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(code.expires_in);
        let mut interval = Duration::from_secs(code.interval.max(5));
        let outcome = loop {
            std::thread::sleep(interval);
            if state.login_attempt.load(Ordering::SeqCst) != attempt {
                return;
            }
            if Instant::now() > deadline {
                break AuthChanged { signed_in: false, reason: Some("expired".into()) };
            }
            match github_api::poll_device(&code.device_code) {
                Ok(Poll::Pending) => {}
                Ok(Poll::SlowDown) => interval += Duration::from_secs(5),
                Ok(Poll::Done(tokens)) => match auth::save_login(&state.app_dir, &tokens) {
                    Ok(()) => break AuthChanged { signed_in: true, reason: None },
                    Err(e) => break AuthChanged { signed_in: false, reason: Some(e.code().into()) },
                },
                Ok(Poll::Expired) => break AuthChanged { signed_in: false, reason: Some("expired".into()) },
                Ok(Poll::Denied) => break AuthChanged { signed_in: false, reason: Some("denied".into()) },
                // Already retried inside the client; the code may still be valid, so keep polling.
                Err(Error::Network(e)) => log::warn!("device flow poll: {e}"),
                Err(e) => break AuthChanged { signed_in: false, reason: Some(e.code().into()) },
            }
        };
        let _ = app.emit("auth-changed", outcome);
    });
}
