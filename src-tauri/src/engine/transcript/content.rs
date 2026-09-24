//! The content of user records: typed text, editor context, pasted images and tool results.

use serde_json::Value;

use super::records::{flag, text_at, Payload};

/// Images the viewer can show; anything else is dropped.
const IMAGE_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];
/// About 10 MB of image as base64.
const IMAGE_MAX_BASE64: usize = 14 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Image {
    pub media_type: String,
    /// Base64, as Claude Code stores it.
    pub data: String,
}

pub struct ToolResult {
    pub id: String,
    pub text: String,
    pub is_error: bool,
    pub images: Vec<Image>,
    /// The record's `toolUseResult` (diffs, agent id), for its first result.
    pub extra: Option<Value>,
}

pub fn user(v: &Value) -> Payload {
    let meta = flag(v, "isMeta") || flag(v, "isCompactSummary") || flag(v, "isVisibleInTranscriptOnly");
    let (mut texts, mut context, mut images, mut results) = (Vec::new(), Vec::new(), Vec::new(), Vec::<ToolResult>::new());
    match v.pointer("/message/content") {
        Some(Value::String(s)) => texts.push(s.clone()),
        Some(Value::Array(blocks)) => {
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => match text_at(b, "text") {
                        Some(t) if t.trim_start().starts_with("<ide_") => context.push(t),
                        t => texts.extend(t),
                    },
                    Some("image") => images.extend(image(b)),
                    Some("tool_result") => {
                        let mut pictures = Vec::new();
                        let text = content_text(b.get("content"), Some(&mut pictures));
                        let extra = results.is_empty().then(|| v.get("toolUseResult").cloned()).flatten();
                        results.push(ToolResult { id: text_at(b, "tool_use_id").unwrap_or_default(), text, is_error: flag(b, "is_error"), images: pictures, extra });
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    Payload::User { text: (!texts.is_empty()).then(|| texts.join("\n\n")), context, images, results, meta }
}

/// A string, or a list of text/image blocks (images collected when asked for).
pub fn content_text(content: Option<&Value>, mut images: Option<&mut Vec<Image>>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| match p.get("type").and_then(Value::as_str) {
                Some("text") => text_at(p, "text"),
                Some("image") => {
                    if let (Some(list), Some(img)) = (images.as_deref_mut(), image(p)) {
                        list.push(img);
                    }
                    Some("[image]".to_owned())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn image(b: &Value) -> Option<Image> {
    let source = b.get("source")?;
    let media_type = text_at(source, "media_type").filter(|m| IMAGE_TYPES.contains(&m.as_str()))?;
    let data = text_at(source, "data").filter(|d| d.len() <= IMAGE_MAX_BASE64)?;
    (text_at(source, "type")? == "base64").then_some(Image { media_type, data })
}
