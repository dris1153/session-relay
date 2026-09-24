//! Claude Code session transcripts (`<sid>.jsonl`) turned into what the viewer shows. The format
//! is undocumented: anything unexpected becomes a hidden event, never an error.

mod branch;
mod compare;
mod content;
mod export;
mod items;
mod preview;
mod records;
mod resolve;
mod search;
pub mod source;
mod tool_result;
mod usage;

use std::collections::{HashMap, HashSet};

use serde::Serialize;

pub use compare::first_difference;
pub use content::Image;
pub use export::write_markdown;
pub use preview::cut;
pub use search::Hit;
pub use resolve::{resolve, Resolved};
pub use tool_result::{AgentRef, FileDiff, Hunk};
pub use usage::Usage;

/// Previews sent to the window; the full text stays here until the user expands it.
pub const INPUT_PREVIEW: usize = 2 * 1024;
pub const OUTPUT_PREVIEW: usize = 8 * 1024;
pub const DIFF_PREVIEW_LINES: usize = 400;
/// "Show all" of a saved output: a 10 MB log in one `<pre>` would freeze the window.
pub const DETAIL_MAX: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    /// `images`: detail refs (`img:<n>`).
    User { uuid: String, at: Option<String>, text: String, images: Vec<String> },
    Assistant { uuid: String, at: Option<String>, model: Option<String>, usage: Option<Usage>, blocks: Vec<Block> },
    /// `event` is the record kind (`compact`, `api_error`, `mention`, `queued`, or a raw type name).
    Event { uuid: Option<String>, at: Option<String>, event: String, noisy: bool, text: String },
}

// Most blocks of a session are tool calls: boxing the big variant would buy nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text { text: String },
    /// Claude Code usually stores only a signature, so `text` is mostly `None`.
    Thinking { text: Option<String> },
    Tool {
        id: String,
        name: String,
        summary: String,
        input: String,
        input_truncated: bool,
        output: Option<String>,
        output_truncated: bool,
        is_error: bool,
        diff: Vec<FileDiff>,
        diff_truncated: bool,
        agent: Option<AgentRef>,
        images: Vec<String>,
        #[serde(skip)]
        persisted: Option<String>,
    },
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

/// What a reference points at; files and agents are read by the caller (they need the store).
pub enum Detail {
    Text(String),
    /// Path under the session folder.
    File(String),
    Agent(String),
    Image(Image),
}

#[derive(Debug, Default)]
pub struct Transcript {
    pub meta: SessionMeta,
    pub items: Vec<Item>,
    /// tool_use id → (item, block), for pairing results and serving details.
    tools: HashMap<String, (usize, usize)>,
    images: Vec<Image>,
    agents: HashSet<String>,
}

pub fn parse(bytes: &[u8]) -> Transcript {
    items::build(bytes, false)
}

/// A subagent's own transcript: all its records are sidechain records.
pub fn parse_agent(bytes: &[u8]) -> Transcript {
    items::build(bytes, true)
}

impl Transcript {
    /// Items with tool input/output and diffs cut to their previews.
    pub fn view(&self) -> Vec<Item> {
        self.items.iter().map(preview::item).collect()
    }

    /// `in:<tool id>`, `out:<tool id>`, `img:<n>` or `agent:<id>`.
    pub fn detail(&self, reference: &str) -> Option<Detail> {
        let (part, id) = reference.split_once(':')?;
        match part {
            "img" => return self.images.get(id.parse::<usize>().ok()?).cloned().map(Detail::Image),
            "agent" => return self.agents.contains(id).then(|| Detail::Agent(id.to_owned())),
            _ => {}
        }
        let &(item, block) = self.tools.get(id)?;
        let Item::Assistant { blocks, .. } = &self.items[item] else { return None };
        let Block::Tool { input, output, persisted, .. } = &blocks[block] else { return None };
        match part {
            "in" => Some(Detail::Text(input.clone())),
            "out" => match persisted {
                Some(name) => Some(Detail::File(format!("tool-results/{name}"))),
                None => output.clone().map(Detail::Text),
            },
            _ => None,
        }
    }
}
