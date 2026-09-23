use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::crypto::Keys;
use super::error::{Error, Result};
use super::file_set::{self, FileClass};
use super::project_identity::ProjectKey;

const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkRef {
    pub name: String,
    pub len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub class: FileClass,
    /// Size and keyed hash of the stored (normalized) content.
    pub size: u64,
    pub hash: String,
    pub chunks: Vec<ChunkRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub saved_by: String,
    /// When it was pushed.
    pub saved_at: DateTime<Utc>,
    /// When the file was last edited on the machine that pushed it (last-writer-wins compares this).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub v: u32,
    pub remote: String,
    pub subpath: String,
    /// Increases on every push; lets machines detect a replayed older snapshot.
    pub generation: u64,
    pub files: BTreeMap<String, FileEntry>,
}

/// On-disk form: the MAC covers the exact body bytes, so fields added by newer versions
/// do not break verification on older ones.
#[derive(Serialize, Deserialize)]
struct Sealed {
    body: String,
    mac: String,
}

impl Manifest {
    pub fn new(key: &ProjectKey) -> Self {
        Self { v: VERSION, remote: key.remote.clone(), subpath: key.subpath.clone(), generation: 0, files: BTreeMap::new() }
    }

    pub fn key(&self) -> ProjectKey {
        ProjectKey { remote: self.remote.clone(), subpath: self.subpath.clone() }
    }

    pub fn seal(&self, keys: &Keys) -> Result<Vec<u8>> {
        let body = serde_json::to_string(self).expect("serializable");
        let sealed = Sealed { mac: keys.mac(body.as_bytes()), body };
        keys.seal(&serde_json::to_vec(&sealed).expect("serializable"))
    }

    /// Decrypts and verifies integrity, project binding and every path (restore writes by these names).
    pub fn open(bytes: &[u8], keys: &Keys, expected: Option<&ProjectKey>) -> Result<Self> {
        let sealed: Sealed = serde_json::from_slice(&keys.open(bytes)?).map_err(|e| Error::Invalid(e.to_string()))?;
        if keys.mac(sealed.body.as_bytes()) != sealed.mac {
            return Err(Error::ManifestTampered);
        }
        let manifest: Manifest = serde_json::from_str(&sealed.body).map_err(|e| Error::Invalid(e.to_string()))?;
        if manifest.v > VERSION {
            return Err(Error::Invalid(format!("manifest v{} needs a newer app", manifest.v)));
        }
        if expected.is_some_and(|k| *k != manifest.key()) {
            return Err(Error::ManifestKeyMismatch);
        }
        for (rel, entry) in &manifest.files {
            let sizes_match = entry.chunks.iter().map(|c| c.len).sum::<u64>() == entry.size;
            let names_ok = entry.chunks.iter().all(|c| is_hex(&c.name, 32)) && is_hex(&entry.hash, 64);
            if file_set::classify(rel) != Some(entry.class) || !sizes_match || !names_ok {
                return Err(Error::Invalid(format!("manifest entry {rel}")));
            }
        }
        Ok(manifest)
    }
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> ProjectKey {
        ProjectKey { remote: "github.com/acme/app".into(), subpath: String::new() }
    }

    fn entry(class: FileClass) -> FileEntry {
        let chunks = vec![ChunkRef { name: "a".repeat(32), len: 3 }];
        FileEntry { class, size: 3, hash: "b".repeat(64), chunks, title: None, saved_by: "PC".into(), saved_at: Utc::now(), modified_at: None }
    }

    #[test]
    fn seal_open_verifies_mac_key_and_paths() {
        let keys = Keys::generate();
        let mut m = Manifest::new(&key());
        m.files.insert("memory/MEMORY.md".into(), entry(FileClass::Whole));
        let sealed = m.seal(&keys).unwrap();
        assert_eq!(Manifest::open(&sealed, &keys, Some(&key())).unwrap(), m);

        let other = ProjectKey { remote: "github.com/acme/other".into(), subpath: String::new() };
        assert!(matches!(Manifest::open(&sealed, &keys, Some(&other)), Err(Error::ManifestKeyMismatch)));

        let forged = Sealed { body: serde_json::to_string(&Manifest { generation: 99, ..m.clone() }).unwrap(), mac: "0".repeat(64) };
        let forged_bytes = keys.seal(&serde_json::to_vec(&forged).unwrap()).unwrap();
        assert!(matches!(Manifest::open(&forged_bytes, &keys, None), Err(Error::ManifestTampered)));

        let mut evil = Manifest::new(&key());
        evil.files.insert("../../settings.json".into(), entry(FileClass::Whole));
        assert!(matches!(Manifest::open(&evil.seal(&keys).unwrap(), &keys, None), Err(Error::Invalid(_))));
    }

    #[test]
    fn newer_fields_still_verify() {
        let keys = Keys::generate();
        let body = r#"{"v":1,"remote":"github.com/acme/app","subpath":"","generation":3,"files":{},"future_field":true}"#;
        let sealed = Sealed { body: body.into(), mac: keys.mac(body.as_bytes()) };
        let bytes = keys.seal(&serde_json::to_vec(&sealed).unwrap()).unwrap();
        assert_eq!(Manifest::open(&bytes, &keys, Some(&key())).unwrap().generation, 3);
    }
}
