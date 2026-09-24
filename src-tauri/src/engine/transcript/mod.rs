//! Claude Code session transcripts (`<sid>.jsonl`) turned into what the viewer shows. The format
//! is undocumented: anything unexpected becomes a hidden event, never an error.

mod branch;
mod items;
mod records;
pub mod source;

use std::collections::HashMap;
use std::ops::{AddAssign, SubAssign};

use serde::Serialize;

/// Previews sent to the window; the full text stays here until the user expands it.
pub const INPUT_PREVIEW: usize = 2 * 1024;
pub const OUTPUT_PREVIEW: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
}

impl AddAssign for Usage {
    fn add_assign(&mut self, o: Self) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_creation += o.cache_creation;
    }
}

impl SubAssign for Usage {
    fn sub_assign(&mut self, o: Self) {
        self.input -= o.input;
        self.output -= o.output;
        self.cache_read -= o.cache_read;
        self.cache_creation -= o.cache_creation;
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    User { uuid: String, at: Option<String>, text: String, images: usize },
    Assistant { uuid: String, at: Option<String>, model: Option<String>, usage: Option<Usage>, blocks: Vec<Block> },
    /// `event` is the record kind (`compact`, `api_error`, `mention`, `queued`, or a raw type name).
    Event { uuid: Option<String>, at: Option<String>, event: String, noisy: bool, text: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text { text: String },
    /// Claude Code usually stores only a signature, so `text` is mostly `None`.
    Thinking { text: Option<String> },
    Tool { id: String, name: String, summary: String, input: String, input_truncated: bool, output: Option<String>, output_truncated: bool, is_error: bool },
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SessionMeta {
    pub title: Option<String>,
    pub models: Vec<String>,
    pub version: Option<String>,
    pub git_branch: Option<String>,
    pub cwd: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub usage: Usage,
    pub prompts: usize,
    pub tool_calls: usize,
    /// Messages left on rewound branches (not shown).
    pub off_branch: usize,
}

#[derive(Debug, Default)]
pub struct Transcript {
    pub meta: SessionMeta,
    pub items: Vec<Item>,
    /// tool_use id → (item, block), for pairing results and serving details.
    tools: HashMap<String, (usize, usize)>,
}

pub fn parse(bytes: &[u8]) -> Transcript {
    items::build(bytes)
}

impl Transcript {
    /// Items with tool input/output cut to their previews.
    pub fn view(&self) -> Vec<Item> {
        self.items.iter().map(preview).collect()
    }

    /// Full text behind a preview: `in:<tool id>` or `out:<tool id>`.
    pub fn detail(&self, reference: &str) -> Option<String> {
        let (part, id) = reference.split_once(':')?;
        let &(item, block) = self.tools.get(id)?;
        let Item::Assistant { blocks, .. } = &self.items[item] else { return None };
        let Block::Tool { input, output, .. } = &blocks[block] else { return None };
        match part {
            "in" => Some(input.clone()),
            "out" => output.clone(),
            _ => None,
        }
    }
}

fn preview(item: &Item) -> Item {
    if let Item::Event { uuid, at, event, noisy, text } = item {
        // Skill bodies and command output can be long; nothing reads past the preview.
        return Item::Event { uuid: uuid.clone(), at: at.clone(), event: event.clone(), noisy: *noisy, text: cut(text, OUTPUT_PREVIEW).0 };
    }
    let Item::Assistant { uuid, at, model, usage, blocks } = item else { return item.clone() };
    let blocks = blocks
        .iter()
        .map(|b| match b {
            Block::Tool { id, name, summary, input, output, is_error, .. } => {
                let (input, input_truncated) = cut(input, INPUT_PREVIEW);
                let (output, output_truncated) = output.as_deref().map_or((None, false), |o| {
                    let (o, cut_off) = cut(o, OUTPUT_PREVIEW);
                    (Some(o), cut_off)
                });
                Block::Tool { id: id.clone(), name: name.clone(), summary: summary.clone(), input, input_truncated, output, output_truncated, is_error: *is_error }
            }
            other => other.clone(),
        })
        .collect();
    Item::Assistant { uuid: uuid.clone(), at: at.clone(), model: model.clone(), usage: *usage, blocks }
}

fn cut(text: &str, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text.to_owned(), false);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), true)
}
