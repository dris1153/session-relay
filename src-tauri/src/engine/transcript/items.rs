use std::collections::{HashMap, HashSet};

use super::branch::{self, is_conversation};
use super::records::{self, Payload, Rec};
use super::{Block, Item, Transcript};
use crate::engine::title::TitleScan;

pub fn build(bytes: &[u8]) -> Transcript {
    let mut title = TitleScan::default();
    let mut recs = Vec::new();
    for line in bytes.split(|&b| b == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l)).filter(|l| !l.is_empty()) {
        title.feed(line);
        recs.extend(records::parse_line(line));
    }
    let rewound = branch::rewound(&recs);
    let mut t = Transcript::default();
    let mut messages: HashMap<String, usize> = HashMap::new();
    let mut seen = HashSet::new();
    for (i, rec) in recs.into_iter().enumerate() {
        if rec.sidechain {
            continue;
        }
        note_meta(&mut t, &rec);
        // Records without a uuid (title, mode, file history, queue) only feed the metadata.
        if rec.uuid.is_none() && !is_conversation(&rec.payload) {
            continue;
        }
        if rewound.contains(&i) {
            t.meta.off_branch += usize::from(is_conversation(&rec.payload));
            continue;
        }
        // Resuming can append earlier records again under the same uuid.
        if rec.uuid.as_ref().is_some_and(|u| !seen.insert(u.clone())) {
            continue;
        }
        add(&mut t, &mut messages, rec);
    }
    t.meta.title = title.finish();
    t
}

fn note_meta(t: &mut Transcript, rec: &Rec) {
    let m = &mut t.meta;
    if m.started_at.is_none() {
        m.started_at.clone_from(&rec.at);
    }
    for (slot, value) in [(&mut m.ended_at, &rec.at), (&mut m.version, &rec.version), (&mut m.git_branch, &rec.git_branch), (&mut m.cwd, &rec.cwd)] {
        if value.is_some() {
            slot.clone_from(value);
        }
    }
}

fn add(t: &mut Transcript, messages: &mut HashMap<String, usize>, rec: Rec) {
    let Rec { uuid, at, payload, .. } = rec;
    match payload {
        Payload::Assistant { message_id, model, usage, blocks } => {
            // One API message is written as several records (one per block) sharing its id.
            let index = match message_id.as_ref().and_then(|id| messages.get(id)) {
                Some(&i) => {
                    // Records of one message repeat its usage; keep the most complete one.
                    if let (Some(new), Item::Assistant { usage: old, .. }) = (usage, &mut t.items[i]) {
                        if old.is_none_or(|o| new.output > o.output) {
                            if let Some(o) = old.replace(new) {
                                t.meta.usage -= o;
                            }
                            t.meta.usage += new;
                        }
                    }
                    i
                }
                None => {
                    if let Some(u) = usage {
                        t.meta.usage += u;
                    }
                    if let Some(m) = &model {
                        if !t.meta.models.contains(m) {
                            t.meta.models.push(m.clone());
                        }
                    }
                    t.items.push(Item::Assistant { uuid: uuid.clone().unwrap_or_default(), at: at.clone(), model, usage, blocks: Vec::new() });
                    let i = t.items.len() - 1;
                    if let Some(id) = message_id {
                        messages.insert(id, i);
                    }
                    i
                }
            };
            let Item::Assistant { blocks: target, .. } = &mut t.items[index] else { return };
            for block in blocks {
                if let Block::Tool { id, .. } = &block {
                    t.tools.insert(id.clone(), (index, target.len()));
                    t.meta.tool_calls += 1;
                }
                target.push(block);
            }
        }
        Payload::User { text, context, images, results, meta } => {
            for c in context {
                t.items.push(event(&uuid, &at, "ide", true, c));
            }
            for r in results {
                if !attach_result(t, &r.id, r.text.clone(), r.is_error) {
                    t.items.push(event(&uuid, &at, "tool_result", true, r.text));
                }
            }
            let Some(text) = text.filter(|s| !s.trim().is_empty()).or_else(|| (images > 0).then(String::new)) else { return };
            if meta {
                t.items.push(event(&uuid, &at, "meta", true, text));
            } else if text.starts_with(records::INTERRUPTED) {
                t.items.push(event(&uuid, &at, "interrupted", false, text));
            } else if records::is_system_text(&text) {
                t.items.push(event(&uuid, &at, "notification", true, text));
            } else {
                t.meta.prompts += 1;
                t.items.push(Item::User { uuid: uuid.unwrap_or_default(), at, text: command_prompt(&text).unwrap_or(text), images });
            }
        }
        Payload::System { subtype, text } => {
            let (kind, noisy) = match subtype.as_str() {
                "compact_boundary" => ("compact", false),
                "api_error" => ("api_error", false),
                "" => ("system", true),
                other => (other, true),
            };
            t.items.push(event(&uuid, &at, kind, noisy, text));
        }
        Payload::Attachment { kind, text } => {
            let (kind, noisy) = match kind.as_str() {
                "file" => ("mention", false),
                "queued_command" => ("queued", false),
                other => (other, true),
            };
            t.items.push(event(&uuid, &at, kind, noisy, text));
        }
        Payload::Other(kind) => t.items.push(event(&uuid, &at, &kind, true, String::new())),
    }
}

fn attach_result(t: &mut Transcript, id: &str, text: String, error: bool) -> bool {
    let Some(&(item, block)) = t.tools.get(id) else { return false };
    let Item::Assistant { blocks, .. } = &mut t.items[item] else { return false };
    let Block::Tool { output, is_error, .. } = &mut blocks[block] else { return false };
    *output = Some(text);
    *is_error = error;
    true
}

fn event(uuid: &Option<String>, at: &Option<String>, kind: &str, noisy: bool, text: String) -> Item {
    Item::Event { uuid: uuid.clone(), at: at.clone(), event: kind.to_owned(), noisy, text }
}

/// A slash command is stored as `<command-message>…<command-name>/x</command-name><command-args>a</command-args>`;
/// a prompt that merely quotes those tags stays as typed.
fn command_prompt(text: &str) -> Option<String> {
    if !["<command-message>", "<command-name>"].iter().any(|tag| text.trim_start().starts_with(tag)) {
        return None;
    }
    let between = |open: &str, close: &str| text.split_once(open).and_then(|(_, rest)| rest.split_once(close)).map(|(inner, _)| inner.trim());
    let name = between("<command-name>", "</command-name>")?;
    let args = between("<command-args>", "</command-args>").unwrap_or_default();
    Some(records::shorten(format!("{name} {args}").trim(), 4000))
}
