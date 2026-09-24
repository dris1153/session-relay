use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use crate::engine::transcript::source::Side;
use crate::engine::transcript::Transcript;

/// Two sessions (both copies of a diverged one); images keep a big session's parse heavy.
const SESSIONS: usize = 2;
/// Subagents opened inside those sessions; a session can start well over a hundred.
const AGENTS: usize = 10;

struct Entry {
    key_hash: String,
    session_id: String,
    side: Side,
    /// "" for the session, `agent:<id>/…` for a subagent opened inside it.
    scope: String,
    signature: String,
    transcript: Arc<Transcript>,
}

impl Entry {
    fn session(&self) -> (&str, &str, Side) {
        (&self.key_hash, &self.session_id, self.side)
    }
}

/// Sessions and subagents open in the viewer, parsed once, most recently used first.
/// Holds decrypted cloud content, so it is emptied whenever the engine goes away.
#[derive(Default)]
pub struct TranscriptCache(Mutex<Vec<Entry>>);

impl TranscriptCache {
    /// The cached parse and the content signature it was made from.
    pub fn find(&self, key_hash: &str, session_id: &str, side: Side, scope: &str) -> Option<(String, Arc<Transcript>)> {
        let mut entries = self.entries();
        let at = entries.iter().position(|e| e.session() == (key_hash, session_id, side) && e.scope == scope)?;
        let entry = entries.remove(at);
        let found = (entry.signature.clone(), Arc::clone(&entry.transcript));
        entries.insert(0, entry);
        Some(found)
    }

    /// A new parse of the session itself drops its subagents too: they may have changed with it.
    pub fn insert(&self, key_hash: &str, session_id: &str, side: Side, scope: &str, signature: String, transcript: Transcript) -> Arc<Transcript> {
        let transcript = Arc::new(transcript);
        let mut entries = self.entries();
        entries.retain(|e| !(e.session() == (key_hash, session_id, side) && (scope.is_empty() || e.scope == scope)));
        entries.insert(0, Entry { key_hash: key_hash.into(), session_id: session_id.into(), side, scope: scope.into(), signature, transcript: Arc::clone(&transcript) });
        let (mut sessions, mut agents) = (0, 0);
        entries.retain(|e| {
            let count = if e.scope.is_empty() { &mut sessions } else { &mut agents };
            *count += 1;
            *count <= if e.scope.is_empty() { SESSIONS } else { AGENTS }
        });
        let open: HashSet<(String, String, Side)> = entries.iter().filter(|e| e.scope.is_empty()).map(|e| (e.key_hash.clone(), e.session_id.clone(), e.side)).collect();
        entries.retain(|e| e.scope.is_empty() || open.contains(&(e.key_hash.clone(), e.session_id.clone(), e.side)));
        transcript
    }

    pub fn clear(&self) {
        self.entries().clear();
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, Vec<Entry>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(cache: &TranscriptCache, session: &str, scope: &str) {
        cache.insert("k", session, Side::Local, scope, String::new(), Transcript::default());
    }

    #[test]
    fn many_subagents_never_push_out_their_session() {
        let cache = TranscriptCache::default();
        put(&cache, "s1", "");
        for i in 0..30 {
            put(&cache, "s1", &format!("agent:a{i}/"));
        }
        assert!(cache.find("k", "s1", Side::Local, "").is_some());
        assert!(cache.find("k", "s1", Side::Local, "agent:a29/").is_some());
        assert!(cache.find("k", "s1", Side::Local, "agent:a0/").is_none(), "agents are capped");
    }

    #[test]
    fn keeps_the_two_most_recently_used_sessions_and_only_their_agents() {
        let cache = TranscriptCache::default();
        put(&cache, "s1", "");
        put(&cache, "s1", "agent:x/");
        put(&cache, "s2", "");
        assert!(cache.find("k", "s1", Side::Local, "").is_some(), "using s1 makes s2 the oldest");
        put(&cache, "s3", "");
        assert!(cache.find("k", "s2", Side::Local, "").is_none());
        assert!(cache.find("k", "s1", Side::Local, "agent:x/").is_some());
        put(&cache, "s1", "");
        assert!(cache.find("k", "s1", Side::Local, "agent:x/").is_none(), "a new parse of the session drops its agents");
    }
}
