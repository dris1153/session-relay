use std::borrow::Cow;
use std::path::Path;

use regex::bytes::Regex;

/// Stands for this machine's `<claude_home>\projects\<enc>\` (JSON-escaped, with the trailing
/// separator) inside stored transcripts. Raw 0x01 bytes cannot occur in valid JSONL, so the
/// placeholder never collides with real content.
pub const PLACEHOLDER: &[u8] = b"\x01SR_PROJECT_DIR\x01";

/// Transcripts embed absolute paths to `tool-results` files (Spike A: reads fail on another
/// profile). Stored bytes use a placeholder so chunks stay identical across machines.
pub struct PathRewrite {
    /// JSON-escaped project dir followed by an escaped `\`, so `d--app` never matches `d--app-v2`.
    local_prefix: Vec<u8>,
    finder: Regex,
}

impl PathRewrite {
    pub fn new(project_dir: &Path) -> Self {
        let json = serde_json::to_string(&project_dir.to_string_lossy()).expect("string serializes");
        let mut local_prefix = json.as_bytes()[1..json.len() - 1].to_vec();
        local_prefix.extend_from_slice(br"\\");
        // Claude may spell the dir with a different drive/encoded-name case than the folder on disk.
        let pattern = format!("(?i-u){}", regex::escape(std::str::from_utf8(&local_prefix).expect("utf8")));
        Self { finder: Regex::new(&pattern).expect("escaped literal"), local_prefix }
    }

    pub fn normalize<'a>(&self, line: &'a [u8]) -> Cow<'a, [u8]> {
        self.finder.replace_all(line, PLACEHOLDER)
    }

    pub fn expand<'a>(&self, data: &'a [u8]) -> Cow<'a, [u8]> {
        let mut out = Vec::new();
        let mut last = 0;
        for at in memchr::memmem::find_iter(data, PLACEHOLDER) {
            out.extend_from_slice(&data[last..at]);
            out.extend_from_slice(&self.local_prefix);
            last = at + PLACEHOLDER.len();
        }
        if last == 0 {
            return Cow::Borrowed(data);
        }
        out.extend_from_slice(&data[last..]);
        Cow::Owned(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_any_case_and_expands_to_other_machine() {
        let a = PathRewrite::new(Path::new(r"C:\Users\PC\.claude\projects\d--Work-app"));
        let line = br#"{"note":"saved to C:\\Users\\PC\\.claude\\projects\\D--Work-app\\s\\tool-results\\x.txt","other":"C:\\Users\\PC\\x"}"#;
        let stored = a.normalize(line);
        assert_eq!(stored.as_ref(), b"{\"note\":\"saved to \x01SR_PROJECT_DIR\x01s\\\\tool-results\\\\x.txt\",\"other\":\"C:\\\\Users\\\\PC\\\\x\"}");

        let b = PathRewrite::new(Path::new(r"C:\Users\Dris\.claude\projects\D--Code-app"));
        let restored = b.expand(&stored);
        assert!(std::str::from_utf8(&restored).unwrap().contains(r"C:\\Users\\Dris\\.claude\\projects\\D--Code-app\\s\\tool-results"));
        assert_eq!(b.normalize(&restored).as_ref(), stored.as_ref());
        assert!(matches!(b.expand(b"plain"), Cow::Borrowed(_)));
    }

    #[test]
    fn leaves_sibling_dirs_and_literal_placeholder_text_alone() {
        let r = PathRewrite::new(Path::new(r"C:\c\.claude\projects\d--Work-app"));
        let sibling = br#"{"p":"C:\\c\\.claude\\projects\\d--Work-app-v2\\x"}"#;
        assert_eq!(r.normalize(sibling).as_ref(), sibling.as_slice());
        let literal = br#"{"text":"the {{SR_PROJECT_DIR}} token"}"#;
        assert_eq!(r.expand(literal).as_ref(), literal.as_slice());
    }

    #[test]
    fn handles_non_ascii_project_dir() {
        let r = PathRewrite::new(Path::new(r"C:\Users\Dũng\.claude\projects\x"));
        let line = r#"{"p":"C:\\Users\\Dũng\\.claude\\projects\\x\\a.txt"}"#.as_bytes();
        assert_eq!(r.normalize(line).as_ref(), b"{\"p\":\"\x01SR_PROJECT_DIR\x01a.txt\"}");
    }
}
