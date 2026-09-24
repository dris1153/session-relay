use std::sync::{Arc, Mutex, PoisonError};

use crate::engine::transcript::source::Side;
use crate::engine::transcript::Transcript;

/// Both copies of a diverged session can be open at once (compare view).
const ENTRIES: usize = 2;

struct Entry {
    key_hash: String,
    session_id: String,
    side: Side,
    signature: String,
    transcript: Arc<Transcript>,
}

/// Sessions open in the viewer, parsed once: details and reopening an unchanged file reuse them.
/// Holds decrypted cloud content, so it is emptied whenever the engine goes away.
#[derive(Default)]
pub struct TranscriptCache(Mutex<Vec<Entry>>);

impl TranscriptCache {
    /// The cached parse and the content signature it was made from.
    pub fn find(&self, key_hash: &str, session_id: &str, side: Side) -> Option<(String, Arc<Transcript>)> {
        self.entries().iter().find(|e| e.key_hash == key_hash && e.session_id == session_id && e.side == side).map(|e| (e.signature.clone(), Arc::clone(&e.transcript)))
    }

    pub fn insert(&self, key_hash: &str, session_id: &str, side: Side, signature: String, transcript: Transcript) -> Arc<Transcript> {
        let transcript = Arc::new(transcript);
        let mut entries = self.entries();
        entries.retain(|e| !(e.key_hash == key_hash && e.session_id == session_id && e.side == side));
        entries.insert(0, Entry { key_hash: key_hash.into(), session_id: session_id.into(), side, signature, transcript: Arc::clone(&transcript) });
        entries.truncate(ENTRIES);
        transcript
    }

    pub fn clear(&self) {
        self.entries().clear();
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, Vec<Entry>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
