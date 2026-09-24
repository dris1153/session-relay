//! Plain-text search over the full content of a session (not only what the previews show).
//! Subagents and saved outputs are not searched: they are read only when opened.

use serde::Serialize;

use super::{Block, Item, Transcript};

const MAX_HITS: usize = 1000;

/// An item of the session, and the block inside it for a Claude turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Hit {
    pub item: usize,
    pub block: Option<usize>,
}

impl Transcript {
    /// Case-insensitive substring search, in order. System events (thousands of hook notices in
    /// a long session) count only when `system` is on, like the viewer shows them.
    pub fn search(&self, query: &str, system: bool) -> Vec<Hit> {
        let needle = Needle::new(query);
        if needle.text.is_empty() {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for (item, entry) in self.items.iter().enumerate() {
            match entry {
                Item::User { text, .. } if needle.in_text(text) => hits.push(Hit { item, block: None }),
                Item::Event { noisy, text, .. } if (system || !noisy) && needle.in_text(text) => hits.push(Hit { item, block: None }),
                Item::Assistant { blocks, .. } => hits.extend(blocks.iter().enumerate().filter(|(_, b)| needle.in_block(b)).map(|(block, _)| Hit { item, block: Some(block) })),
                _ => {}
            }
            if hits.len() >= MAX_HITS {
                hits.truncate(MAX_HITS);
                break;
            }
        }
        hits
    }
}

struct Needle {
    text: String,
    ascii: bool,
}

impl Needle {
    fn new(query: &str) -> Self {
        let text = query.trim().to_lowercase();
        Self { ascii: text.is_ascii(), text }
    }

    fn in_text(&self, hay: &str) -> bool {
        if !self.ascii {
            return hay.to_lowercase().contains(&self.text);
        }
        // Most searches are ASCII: compare bytes without lowercasing megabytes of output.
        let needle = self.text.as_bytes();
        hay.len() >= needle.len() && hay.as_bytes().windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))
    }

    fn in_block(&self, block: &Block) -> bool {
        match block {
            Block::Text { text } => self.in_text(text),
            Block::Thinking { text } => text.as_deref().is_some_and(|t| self.in_text(t)),
            // The input is JSON (a Windows path appears as `C:\\x`); the summary shows it as typed.
            Block::Tool { name, summary, input, output, diff, .. } => {
                self.in_text(name)
                    || self.in_text(summary)
                    || self.in_text(input)
                    || output.as_deref().is_some_and(|o| self.in_text(o))
                    || diff.iter().any(|f| self.in_text(&f.path) || f.hunks.iter().any(|h| h.lines.iter().any(|l| self.in_text(l))))
            }
        }
    }
}
