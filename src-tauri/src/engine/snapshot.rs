use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use super::crypto::Keys;
use super::error::{Error, IoContext, Result};
use super::file_set::FileClass;
use super::fs_util::mtime_ns;
use super::manifest::ChunkRef;
use super::normalize::PathRewrite;
use super::chunker::Accumulator;
use super::title::TitleScan;

/// Append-class chunks are sealed at the first line end at/after this size, so appending
/// never changes already sealed chunks.
pub const TARGET_CHUNK: usize = 4 << 20;
/// Hard cut (also mid-line) keeps every stored file far below GitHub's 100 MB limit.
pub const MAX_CHUNK: usize = 32 << 20;

#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Full file length at open (incl. a partial last line); base quick-check compares this.
    pub file_len: u64,
    pub mtime_ns: i64,
    /// Size / keyed hash of the normalized stream (what gets stored).
    pub size: u64,
    pub hash: String,
    pub chunks: Vec<ChunkRef>,
    pub title: Option<String>,
    /// Chunk-style name of the requested normalized byte range, if the stream covered it.
    pub probe_name: Option<String>,
}

pub type ChunkSink<'a> = &'a mut dyn FnMut(&ChunkRef, &[u8]) -> Result<()>;

/// Reads the file once: fixes the length at open (append: up to the last `\n`, since Claude may
/// be mid-write), normalizes, hashes and chunks in one pass. Memory is bounded by `MAX_CHUNK`.
pub fn take(path: &Path, class: FileClass, keys: &Keys, rewrite: &PathRewrite, probe: Option<(u64, u64)>, sink: ChunkSink) -> Result<Snapshot> {
    let mut file = std::fs::File::open(path).at(path)?;
    let meta = file.metadata().at(path)?;
    let limit = match class {
        FileClass::Append => last_newline_end(&mut file, meta.len()).at(path)?,
        FileClass::Whole => meta.len(),
    };
    file.seek(SeekFrom::Start(0)).at(path)?;
    let mut acc = Accumulator::new(keys, class, probe, sink);
    let mut reader = BufReader::new(file.take(limit));
    let mut titles = TitleScan::default();
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = match class {
            FileClass::Append => reader.read_until(b'\n', &mut line),
            FileClass::Whole => reader.by_ref().take(MAX_CHUNK as u64).read_to_end(&mut line),
        }
        .at(path)?;
        if n == 0 {
            break;
        }
        match class {
            FileClass::Append => {
                titles.feed(&line);
                acc.push(&rewrite.normalize(&line))?;
            }
            FileClass::Whole => acc.push(&line)?,
        }
    }
    let (size, hash, chunks, probe_name) = acc.finish()?;
    if class == FileClass::Whole {
        let after = std::fs::metadata(path).at(path)?;
        if after.len() != meta.len() || mtime_ns(&after) != mtime_ns(&meta) {
            return Err(Error::Invalid(format!("{} changed while reading", path.display())));
        }
    }
    Ok(Snapshot { file_len: meta.len(), mtime_ns: mtime_ns(&meta), size, hash, chunks, title: titles.finish(), probe_name })
}

fn last_newline_end(file: &mut std::fs::File, len: u64) -> std::io::Result<u64> {
    let mut end = len;
    let mut buf = vec![0u8; 64 * 1024];
    while end > 0 {
        let start = end.saturating_sub(buf.len() as u64);
        let slice = &mut buf[..(end - start) as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(slice)?;
        if let Some(i) = slice.iter().rposition(|&b| b == b'\n') {
            return Ok(start + i as u64 + 1);
        }
        end = start;
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn snap(path: &Path, keys: &Keys, probe: Option<(u64, u64)>) -> Snapshot {
        let rewrite = PathRewrite::new(Path::new(r"C:\none"));
        take(path, FileClass::Append, keys, &rewrite, probe, &mut |_, _| Ok(())).unwrap()
    }

    #[test]
    fn appending_keeps_sealed_chunks_and_ignores_partial_line() {
        let keys = Keys::generate();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let line = format!("{{\"type\":\"assistant\",\"text\":\"{}\"}}\n", "x".repeat(1000));
        let mut f = std::fs::File::create(&path).unwrap();
        for _ in 0..9000 {
            f.write_all(line.as_bytes()).unwrap();
        }
        f.write_all(b"{\"partial\":").unwrap();
        f.flush().unwrap();
        let first = snap(&path, &keys, None);
        assert_eq!(first.size, 9000 * line.len() as u64, "partial last line excluded");
        assert!(first.chunks.len() >= 3);

        for _ in 0..500 {
            f.write_all(line.as_bytes()).unwrap();
        }
        f.flush().unwrap();
        let tail = first.chunks.last().unwrap().clone();
        let tail_start = first.size - tail.len;
        let second = snap(&path, &keys, Some((tail_start, tail.len)));
        let sealed = first.chunks.len() - 1;
        assert_eq!(first.chunks[..sealed], second.chunks[..sealed]);
        assert_eq!(second.probe_name.as_deref(), Some(tail.name.as_str()), "old tail is a prefix of the grown file");
        assert_eq!(second.chunks.iter().map(|c| c.len).sum::<u64>(), second.size);
    }

    #[test]
    fn whole_files_keep_bytes_without_trailing_newline() {
        let keys = Keys::generate();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.meta.json");
        std::fs::write(&path, br#"{"agentType":"general"}"#).unwrap();
        let rewrite = PathRewrite::new(Path::new(r"C:\none"));
        let mut stored = Vec::new();
        let s = take(&path, FileClass::Whole, &keys, &rewrite, None, &mut |_, d| {
            stored.extend_from_slice(d);
            Ok(())
        })
        .unwrap();
        assert_eq!(stored, br#"{"agentType":"general"}"#);
        assert_eq!(s.size, stored.len() as u64);
    }
}
