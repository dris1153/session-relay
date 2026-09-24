use super::error::Error;

/// Only `Error::Git` means "the clone may be broken" and triggers a rebuild. A refused credential
/// (signed out, grant revoked, repo no longer shared) or an unreachable network is not that:
/// rebuilding would throw away a clone that is fine.
pub fn failure(args: &str, stderr: &str) -> Error {
    const AUTH: [&str; 6] = ["could not read Username", "Authentication failed", "returned error: 401", "returned error: 403", "Permission to", "Repository not found"];
    const NETWORK: [&str; 7] = ["unable to access", "Could not resolve host", "Failed to connect", "timed out", "early EOF", "Connection reset", "remote end hung up"];
    let stderr = stderr.trim().to_string();
    if AUTH.iter().any(|s| stderr.contains(s)) {
        Error::Auth(stderr)
    } else if NETWORK.iter().any(|s| stderr.contains(s)) {
        Error::Network(stderr)
    } else {
        Error::Git { args: args.into(), stderr }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refused_credentials_are_auth_errors_not_broken_clones() {
        let refused = "fatal: could not read Username for 'https://github.com': terminal prompts disabled";
        assert!(matches!(failure("fetch", refused), Error::Auth(_)));
        assert!(matches!(failure("fetch", "fatal: Authentication failed for 'https://github.com/me/s.git/'"), Error::Auth(_)));
        assert!(matches!(failure("fetch", "fatal: unable to access 'https://github.com/me/s.git/': Could not resolve host: github.com"), Error::Network(_)));
        assert!(matches!(failure("fetch", "fatal: bad object HEAD"), Error::Git { .. }));
    }
}
