use std::path::PathBuf;

/// Machine-local locations the engine works with.
#[derive(Debug, Clone)]
pub struct CoreConfig {
    /// `app_local_data_dir()`, never the install dir.
    pub app_dir: PathBuf,
    pub claude_home: PathBuf,
    pub machine_name: String,
}

impl CoreConfig {
    pub fn projects_dir(&self) -> PathBuf {
        self.claude_home.join("projects")
    }
    pub fn sessions_registry_dir(&self) -> PathBuf {
        self.claude_home.join("sessions")
    }
    pub fn store_dir(&self) -> PathBuf {
        self.app_dir.join("store")
    }
    pub fn base_dir(&self) -> PathBuf {
        self.app_dir.join("base")
    }
    pub fn backups_dir(&self) -> PathBuf {
        self.app_dir.join("backups")
    }
    pub fn links_file(&self) -> PathBuf {
        self.app_dir.join("links.json")
    }
    pub fn activity_file(&self) -> PathBuf {
        self.app_dir.join("activity.jsonl")
    }
    pub fn lock_file(&self) -> PathBuf {
        self.app_dir.join("sync.lock")
    }
    pub fn empty_hooks_dir(&self) -> PathBuf {
        self.app_dir.join("hooks-empty")
    }
}

const MAX_ENCODED_LEN: usize = 200;

/// Claude Code's project dir name for a cwd (verified in Spike F):
/// every UTF-16 unit outside `[A-Za-z0-9]` becomes `-`; names over 200 chars are cut
/// and suffixed with base36 of the absolute Java-style hash of the raw path.
pub fn encode_dir(cwd: &str) -> String {
    let encoded: String = cwd
        .encode_utf16()
        .map(|u| match char::from_u32(u32::from(u)) {
            Some(c) if c.is_ascii_alphanumeric() => c,
            _ => '-',
        })
        .collect();
    if encoded.len() <= MAX_ENCODED_LEN {
        return encoded;
    }
    format!("{}-{}", &encoded[..MAX_ENCODED_LEN], base36(i64::from(java_hash(cwd)).unsigned_abs()))
}

fn java_hash(s: &str) -> i32 {
    s.encode_utf16().fold(0i32, |h, u| h.wrapping_mul(31).wrapping_add(i32::from(u)))
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).expect("ascii digits")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRATCH: &str = r"C:\Users\Dris\AppData\Local\Temp\claude\d--Workspace-Personal-Desktops-claude-sync-session\c33f2610-fa73-4ff7-b415-fea9d20ebc8e\scratchpad";

    #[test]
    fn matches_claude_short_names() {
        assert_eq!(encode_dir(r"d:\Workspace\Personal\Mobiles\pigshare"), "d--Workspace-Personal-Mobiles-pigshare");
        let vi = format!(r"{SCRATCH}\spikeF-Dự án.v2");
        assert!(encode_dir(&vi).ends_with("-scratchpad-spikeF-D---n-v2"));
    }

    #[test]
    fn matches_claude_long_name_rule() {
        let long = format!(r"{SCRATCH}\spikeF-long-{}\{}", "a".repeat(50), "b".repeat(40));
        let got = encode_dir(&long);
        assert_eq!(got.len(), 207);
        assert!(got.ends_with("-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-9tdwhq"), "{got}");
    }
}
