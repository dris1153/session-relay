use std::io::{BufRead, Write};

use crate::app_paths::app_dir;
use crate::engine::auth;

/// `git credential` helper for the store clone: git asks, we answer from Credential Manager.
/// Keeps the token out of argv, env and `.git/config`. Returns the process exit code.
pub fn run(action: Option<&str>) -> i32 {
    crate::engine::logging::init(app_dir().join("logs"), "git-credential");
    let request: Vec<String> = std::io::stdin().lock().lines().map_while(Result::ok).take_while(|l| !l.is_empty()).collect();
    // `store`/`erase` are ignored: we own the token lifecycle.
    if action != Some("get") || !is_github_https(&request) {
        return 0;
    }
    match auth::valid_token(&app_dir()) {
        Ok(token) => {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "username=x-access-token\npassword={token}\n");
            0
        }
        Err(e) => {
            log::warn!("credential helper: {}", e.code());
            1
        }
    }
}

/// The token only ever goes to github.com over https.
fn is_github_https(request: &[String]) -> bool {
    let field = |key: &str| request.iter().find_map(|l| l.strip_prefix(key));
    field("protocol=") == Some("https") && field("host=") == Some("github.com")
}

#[cfg(test)]
mod tests {
    fn request(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn answers_only_github_over_https() {
        assert!(super::is_github_https(&request(&["protocol=https", "host=github.com", "path=me/store.git"])));
        assert!(!super::is_github_https(&request(&["protocol=http", "host=github.com"])));
        assert!(!super::is_github_https(&request(&["protocol=https", "host=github.com.evil.test"])));
        assert!(!super::is_github_https(&request(&["host=github.com"])));
    }
}
