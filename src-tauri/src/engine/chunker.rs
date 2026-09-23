use super::crypto::Keys;
use super::error::Result;
use super::file_set::FileClass;
use super::manifest::ChunkRef;
use super::snapshot::{ChunkSink, MAX_CHUNK, TARGET_CHUNK};

/// Hashes and cuts the normalized stream into content-addressed chunks.
pub(super) struct Accumulator<'a> {
    keys: &'a Keys,
    class: FileClass,
    full: blake3::Hasher,
    buf: Vec<u8>,
    offset: u64,
    chunks: Vec<ChunkRef>,
    probe: Option<(u64, u64, blake3::Hasher)>,
    sink: ChunkSink<'a>,
}

impl<'a> Accumulator<'a> {
    pub(super) fn new(keys: &'a Keys, class: FileClass, probe: Option<(u64, u64)>, sink: ChunkSink<'a>) -> Self {
        let probe = probe.map(|(start, len)| (start, len, keys.hasher()));
        Self { keys, class, full: keys.hasher(), buf: Vec::new(), offset: 0, chunks: Vec::new(), probe, sink }
    }

    pub(super) fn push(&mut self, data: &[u8]) -> Result<()> {
        self.full.update(data);
        if let Some((start, len, hasher)) = &mut self.probe {
            let (lo, hi) = (*start, *start + *len);
            let (from, to) = (self.offset.max(lo), (self.offset + data.len() as u64).min(hi));
            if from < to {
                hasher.update(&data[(from - self.offset) as usize..(to - self.offset) as usize]);
            }
        }
        self.offset += data.len() as u64;
        self.buf.extend_from_slice(data);
        while self.buf.len() >= MAX_CHUNK {
            let rest = self.buf.split_off(MAX_CHUNK);
            self.seal()?;
            self.buf = rest;
        }
        if self.class == FileClass::Append && self.buf.len() >= TARGET_CHUNK {
            self.seal()?;
        }
        Ok(())
    }

    fn seal(&mut self) -> Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        let chunk = ChunkRef { name: self.keys.chunk_name(&self.buf), len: self.buf.len() as u64 };
        (self.sink)(&chunk, &self.buf)?;
        self.chunks.push(chunk);
        self.buf.clear();
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<(u64, String, Vec<ChunkRef>, Option<String>)> {
        self.seal()?;
        let covered = |start: u64, len: u64| start + len <= self.offset;
        let probe_name = self.probe.take().filter(|(s, l, _)| covered(*s, *l)).map(|(_, _, h)| h.finalize().to_hex()[..32].to_string());
        Ok((self.offset, self.full.finalize().to_hex().to_string(), self.chunks, probe_name))
    }
}
