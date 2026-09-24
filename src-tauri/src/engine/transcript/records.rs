//! One transcript line → the few fields the viewer needs. Lenient: a field with an unexpected
//! shape is dropped, never the whole line.

use serde_json::Value;

use super::{Block, Usage};

pub enum Payload {
    /// `meta`: text Claude Code injected (skill bodies, compact summaries), not typed by the user.
    /// `context`: what the editor attached (`<ide_opened_file>`, `<ide_selection>`).
    User { text: Option<String>, context: Vec<String>, images: usize, results: Vec<ToolResult>, meta: bool },
    Assistant { message_id: Option<String>, model: Option<String>, usage: Option<Usage>, blocks: Vec<Block> },
    System { subtype: String, text: String },
    Attachment { kind: String, text: String },
    Other(String),
}

pub struct ToolResult {
    pub id: String,
    pub text: String,
    pub is_error: bool,
}

pub struct Rec {
    pub uuid: Option<String>,
    pub parent: Option<String>,
    /// Set across a compaction, where `parentUuid` is null.
    pub logical_parent: Option<String>,
    pub sidechain: bool,
    pub at: Option<String>,
    pub version: Option<String>,
    pub git_branch: Option<String>,
    pub cwd: Option<String>,
    pub payload: Payload,
}

/// Keys whose value best names what a tool call does, in order of preference.
const SUMMARY_KEYS: [&str; 10] = ["command", "file_path", "path", "pattern", "url", "query", "description", "prompt", "skill", "subject"];
const SUMMARY_CHARS: usize = 120;
/// User "prompts" that Claude Code writes itself (background task results, command output).
const SYSTEM_TAGS: [&str; 7] = ["<task-notification>", "<local-command-stdout>", "<local-command-stderr>", "<local-command-caveat>", "<system-reminder>", "<bash-stdout>", "<bash-stderr>"];
pub const INTERRUPTED: &str = "[Request interrupted by user";
const EVENT_TEXT_CHARS: usize = 500;

pub fn parse_line(line: &[u8]) -> Option<Rec> {
    let v: Value = serde_json::from_slice(line).ok()?;
    let kind = text_at(&v, "type")?;
    let payload = match kind.as_str() {
        "user" => user(&v),
        "assistant" => assistant(&v),
        "system" => Payload::System { subtype: text_at(&v, "subtype").unwrap_or_default(), text: system_text(&v) },
        "attachment" => attachment(&v),
        _ => Payload::Other(kind),
    };
    Some(Rec {
        uuid: text_at(&v, "uuid"),
        parent: text_at(&v, "parentUuid"),
        logical_parent: text_at(&v, "logicalParentUuid"),
        sidechain: flag(&v, "isSidechain"),
        at: text_at(&v, "timestamp"),
        version: text_at(&v, "version"),
        git_branch: text_at(&v, "gitBranch"),
        cwd: text_at(&v, "cwd"),
        payload,
    })
}

/// `content`, or for an API error the error's message and the retry count (`… (2/10)`).
fn system_text(v: &Value) -> String {
    if let Some(text) = text_at(v, "content") {
        return text;
    }
    let error = v.get("error");
    let message = error.and_then(|e| text_at(e, "formatted").or_else(|| text_at(e, "message"))).or_else(|| error.and_then(Value::as_str).map(str::to_owned)).unwrap_or_default();
    match (v.get("retryAttempt").and_then(Value::as_u64), v.get("maxRetries").and_then(Value::as_u64)) {
        (Some(n), Some(max)) => format!("{message} ({n}/{max})"),
        _ => message,
    }
}

fn text_at(v: &Value, key: &str) -> Option<String> {
    v.get(key)?.as_str().map(str::to_owned)
}

fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn count(v: Option<&Value>, key: &str) -> u64 {
    v.and_then(|u| u.get(key)).and_then(Value::as_u64).unwrap_or(0)
}

fn user(v: &Value) -> Payload {
    let meta = flag(v, "isMeta") || flag(v, "isCompactSummary") || flag(v, "isVisibleInTranscriptOnly");
    let (mut texts, mut context, mut images, mut results) = (Vec::new(), Vec::new(), 0, Vec::new());
    match v.pointer("/message/content") {
        Some(Value::String(s)) => texts.push(s.clone()),
        Some(Value::Array(blocks)) => {
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => match text_at(b, "text") {
                        Some(t) if t.trim_start().starts_with("<ide_") => context.push(t),
                        t => texts.extend(t),
                    },
                    Some("image") => images += 1,
                    Some("tool_result") => results.push(ToolResult {
                        id: text_at(b, "tool_use_id").unwrap_or_default(),
                        text: content_text(b.get("content")),
                        is_error: flag(b, "is_error"),
                    }),
                    _ => {}
                }
            }
        }
        _ => {}
    }
    Payload::User { text: (!texts.is_empty()).then(|| texts.join("\n\n")), context, images, results, meta }
}

/// A tool result's `content` is a string or a list of text/image blocks.
fn content_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| match p.get("type").and_then(Value::as_str) {
                Some("text") => text_at(p, "text"),
                Some("image") => Some("[image]".to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn assistant(v: &Value) -> Payload {
    let message = v.get("message");
    let usage = message.and_then(|m| m.get("usage")).map(|u| Usage {
        input: count(Some(u), "input_tokens"),
        output: count(Some(u), "output_tokens"),
        cache_read: count(Some(u), "cache_read_input_tokens"),
        cache_creation: count(Some(u), "cache_creation_input_tokens"),
    });
    let blocks = message
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .map(|blocks| blocks.iter().filter_map(block).collect())
        .unwrap_or_default();
    Payload::Assistant {
        message_id: message.and_then(|m| text_at(m, "id")),
        model: message.and_then(|m| text_at(m, "model")).filter(|m| m != "<synthetic>"),
        usage,
        blocks,
    }
}

fn block(b: &Value) -> Option<Block> {
    match b.get("type")?.as_str()? {
        "text" => Some(Block::Text { text: text_at(b, "text")? }),
        "thinking" => Some(Block::Thinking { text: text_at(b, "thinking").filter(|t| !t.trim().is_empty()) }),
        "redacted_thinking" => Some(Block::Thinking { text: None }),
        "tool_use" => {
            let input = b.get("input").cloned().unwrap_or(Value::Null);
            Some(Block::Tool {
                id: text_at(b, "id").unwrap_or_default(),
                name: text_at(b, "name").unwrap_or_default(),
                summary: summary(&input),
                input: serde_json::to_string_pretty(&input).unwrap_or_default(),
                input_truncated: false,
                output: None,
                output_truncated: false,
                is_error: false,
            })
        }
        _ => None,
    }
}

fn summary(input: &Value) -> String {
    let text = SUMMARY_KEYS.iter().find_map(|k| input.get(*k)?.as_str().filter(|s| !s.trim().is_empty())).unwrap_or_default();
    shorten(text.lines().next().unwrap_or_default(), SUMMARY_CHARS)
}

fn attachment(v: &Value) -> Payload {
    let a = v.get("attachment");
    let kind = a.and_then(|a| text_at(a, "type")).unwrap_or_else(|| "attachment".into());
    let text = ["filename", "displayPath", "path", "prompt", "content"].iter().find_map(|k| a.and_then(|a| a.get(*k)).map(|v| content_text(Some(v))).filter(|t| !t.is_empty())).unwrap_or_default();
    Payload::Attachment { kind, text: shorten(&text, EVENT_TEXT_CHARS) }
}

/// Text in a user record that the user did not type.
pub fn is_system_text(text: &str) -> bool {
    let text = text.trim_start();
    SYSTEM_TAGS.iter().any(|tag| text.starts_with(tag)) || text.starts_with(INTERRUPTED)
}

pub fn shorten(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}
