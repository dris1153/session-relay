use super::*;
use crate::engine::manifest::ChunkRef;
use chrono::{TimeZone, Utc};

fn base(len: u64, hash: &str) -> BaseEntry {
    BaseEntry { raw_len: len, mtime_ns: 7, size: len, hash: hash.into() }
}

fn remote(class: FileClass, hash: &str, chunks: &[(&str, u64)]) -> FileEntry {
    let chunks: Vec<ChunkRef> = chunks.iter().map(|(n, l)| ChunkRef { name: (*n).into(), len: *l }).collect();
    let size = chunks.iter().map(|c| c.len).sum();
    let at = Utc.timestamp_opt(100, 0).unwrap();
    FileEntry { class, size, hash: hash.into(), chunks, title: None, saved_by: "A".into(), saved_at: at + chrono::Duration::hours(1), modified_at: Some(at) }
}

fn snap(hash: &str, chunks: &[(&str, u64)], probe: Option<&str>, mtime_ns: i64) -> Snapshot {
    let chunks: Vec<ChunkRef> = chunks.iter().map(|(n, l)| ChunkRef { name: (*n).into(), len: *l }).collect();
    let size = chunks.iter().map(|c| c.len).sum();
    Snapshot { file_len: size, mtime_ns, size, hash: hash.into(), chunks, title: None, probe_name: probe.map(Into::into) }
}

#[test]
fn quick_rules() {
    let (b, r_same, r_new) = (base(10, "h1"), remote(FileClass::Append, "h1", &[("a", 10)]), remote(FileClass::Append, "h2", &[("a", 12)]));
    let f = |class, meta, base, remote| quick(&Facts { class, local_meta: meta, base, remote });
    let append = FileClass::Append;
    assert_eq!(f(append, None, Some(&b), Some(&r_same)), Some(FileState::LocalDeleted));
    assert_eq!(f(append, None, Some(&b), Some(&r_new)), Some(FileState::RemoteAhead));
    assert_eq!(f(append, None, None, Some(&r_new)), Some(FileState::RemoteOnly));
    assert_eq!(f(append, Some((10, 7)), None, None), Some(FileState::LocalOnly));
    assert_eq!(f(append, Some((10, 7)), Some(&b), None), Some(FileState::RemoteDeleted));
    assert_eq!(f(append, Some((11, 8)), Some(&b), None), Some(FileState::LocalAhead));
    assert_eq!(f(append, Some((10, 7)), Some(&b), Some(&r_same)), Some(FileState::InSync));
    assert_eq!(f(append, Some((10, 7)), Some(&b), Some(&r_new)), None, "transcripts need a prefix proof");
    assert_eq!(f(FileClass::Whole, Some((10, 7)), Some(&b), Some(&r_new)), Some(FileState::RemoteAhead));
    assert_eq!(f(append, Some((11, 8)), Some(&b), Some(&r_same)), Some(FileState::LocalAhead));
    assert_eq!(f(append, Some((11, 8)), Some(&b), Some(&r_new)), None);
    assert_eq!(f(append, Some((11, 8)), None, Some(&r_new)), None);
}

#[test]
fn content_rules_for_append_files() {
    let facts = |r| Facts { class: FileClass::Append, local_meta: Some((0, 0)), base: None, remote: r };
    let r = remote(FileClass::Append, "hr", &[("s1", 4), ("t1", 2)]);
    let decide = |s: Snapshot, extends: bool| with_content(&facts(Some(&r)), &s, || Ok(extends)).unwrap().state;
    assert_eq!(decide(snap("hl", &[("s1", 4), ("t2", 5)], Some("t1"), 0), true), FileState::LocalAhead);
    assert_eq!(decide(snap("hl", &[("s1", 4), ("t0", 1)], None, 0), true), FileState::RemoteAhead);
    assert_eq!(decide(snap("hl", &[("s1", 4), ("t0", 1)], None, 0), false), FileState::Diverged);
    assert_eq!(decide(snap("hl", &[("s1", 4), ("x", 3)], Some("y"), 0), true), FileState::Diverged);
    assert_eq!(decide(snap("hr", &[("s1", 4), ("t1", 2)], None, 0), true), FileState::InSync);
}

#[test]
fn whole_files_use_base_then_last_writer_wins() {
    let r = remote(FileClass::Whole, "hr", &[("a", 3)]);
    let edited_ns = 100 * 1_000_000_000;
    let decide = |base: Option<&BaseEntry>, local: Snapshot| {
        let facts = Facts { class: FileClass::Whole, local_meta: Some((3, 0)), base, remote: Some(&r) };
        with_content(&facts, &local, || Ok(true)).unwrap()
    };
    // Only the mtime moved here: the cloud edit wins without a conflict.
    let unchanged_here = base(3, "hl");
    assert_eq!(decide(Some(&unchanged_here), snap("hl", &[("b", 3)], None, edited_ns + 99)), FileState::RemoteAhead.into());
    let cloud_is_base = base(3, "hr");
    assert_eq!(decide(Some(&cloud_is_base), snap("hl", &[("b", 3)], None, 0)), FileState::LocalAhead.into());
    // Both edited: compare edit times (not push time), flag the conflict.
    let old = base(3, "h0");
    assert_eq!(decide(Some(&old), snap("hl", &[("b", 3)], None, edited_ns + 1)), Decision { state: FileState::LocalAhead, conflict: true });
    assert_eq!(decide(Some(&old), snap("hl", &[("b", 3)], None, edited_ns - 1)), Decision { state: FileState::RemoteAhead, conflict: true });
}
