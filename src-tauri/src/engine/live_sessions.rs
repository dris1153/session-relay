use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryEntry {
    pid: u32,
    session_id: String,
    /// Process creation time as a FILETIME number; guards against pid reuse.
    proc_start: Option<String>,
}

/// Session ids whose Claude process is still running, from Claude's undocumented
/// `<claude_home>/sessions/<pid>.json` registry. Restoring over such a session would make
/// Claude append to a stale branch of the conversation.
pub fn live_session_ids(registry_dir: &Path) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(registry_dir) else { return HashSet::new() };
    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let parsed = serde_json::from_slice::<RegistryEntry>(&std::fs::read(e.path()).ok()?);
            // A format change would silently disable the guard; leave a trace.
            parsed.map_err(|err| log::warn!("unreadable Claude session registry entry: {err}")).ok()
        })
        .filter(|r| process_alive(r.pid, r.proc_start.as_deref().and_then(|s| s.parse().ok())))
        .map(|r| r.session_id)
        .collect()
}

fn process_alive(pid: u32, expected_start: Option<u64>) -> bool {
    match process_start_time(pid) {
        Some(start) => expected_start.is_none_or(|want| want == start),
        None => false,
    }
}

pub fn process_start_time(pid: u32) -> Option<u64> {
    // SAFETY: plain Win32 calls on a handle we own and close.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        let ok = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0;
        CloseHandle(handle);
        ok.then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_live_process_and_rejects_reused_pid() {
        let dir = tempfile::tempdir().unwrap();
        let pid = std::process::id();
        let start = process_start_time(pid).unwrap();
        let write = |name: &str, body: serde_json::Value| std::fs::write(dir.path().join(name), body.to_string()).unwrap();
        write("1.json", serde_json::json!({ "pid": pid, "sessionId": "live", "procStart": start.to_string() }));
        write("2.json", serde_json::json!({ "pid": pid, "sessionId": "reused", "procStart": (start + 1).to_string() }));
        write("3.json", serde_json::json!({ "pid": 4_000_000_000u32, "sessionId": "dead" }));
        write("4.json", serde_json::json!({ "garbage": true }));
        let live = live_session_ids(dir.path());
        assert_eq!(live, HashSet::from(["live".to_string()]));
    }
}
