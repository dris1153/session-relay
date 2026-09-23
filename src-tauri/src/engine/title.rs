use std::sync::LazyLock;

use regex::Regex;

const MAX_TITLE_CHARS: usize = 80;

static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").expect("static regex"));

/// Session title from transcript lines: `ai-title` → `last-prompt` → first user text.
#[derive(Default)]
pub struct TitleScan {
    ai_title: Option<String>,
    last_prompt: Option<String>,
    first_user: Option<String>,
    /// Only the first user line is parsed; later ones are mostly large tool results.
    first_user_seen: bool,
}

impl TitleScan {
    pub fn feed(&mut self, line: &[u8]) {
        let has = |needle: &[u8]| memchr::memmem::find(line, needle).is_some();
        if has(br#""type":"ai-title""#) {
            self.ai_title = field(line, &["aiTitle"]).or(self.ai_title.take());
        } else if has(br#""type":"last-prompt""#) {
            self.last_prompt = field(line, &["lastPrompt"]).or(self.last_prompt.take());
        } else if !self.first_user_seen && has(br#""type":"user""#) {
            self.first_user_seen = true;
            self.first_user = user_text(line);
        }
    }

    pub fn finish(self) -> Option<String> {
        [self.ai_title, self.last_prompt, self.first_user].into_iter().flatten().map(|t| clean(&t)).find(|t| !t.is_empty())
    }
}

fn field(line: &[u8], path: &[&str]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(line).ok()?;
    path.iter().try_fold(&value, |v, k| v.get(k))?.as_str().map(str::to_owned)
}

/// `message.content` is a string in the CLI and an array of blocks in the VS Code extension.
fn user_text(line: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(line).ok()?;
    let content = value.get("message")?.get("content")?;
    content.as_str().map(str::to_owned).or_else(|| {
        content.as_array()?.iter().find_map(|b| (b.get("type")?.as_str()? == "text").then(|| b.get("text")?.as_str().map(str::to_owned)).flatten())
    })
}

fn clean(text: &str) -> String {
    let plain = TAGS.replace_all(text, " ");
    let collapsed = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    match collapsed.char_indices().nth(MAX_TITLE_CHARS) {
        Some((cut, _)) => format!("{}…", &collapsed[..cut]),
        None => collapsed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_ai_title_then_last_prompt_then_first_user() {
        let mut t = TitleScan::default();
        t.feed(br#"{"type":"user","message":{"content":"<ide_opened_file>x</ide_opened_file>fix   the  bug"}}"#);
        assert_eq!(TitleScan { first_user: t.first_user.clone(), ..Default::default() }.finish().as_deref(), Some("x fix the bug"));
        let mut vscode = TitleScan::default();
        vscode.feed(br#"{"type":"user","message":{"content":[{"type":"text","text":"from vs code"}]}}"#);
        vscode.feed(br#"{"type":"user","message":{"content":"a later tool result"}}"#);
        assert_eq!(vscode.finish().as_deref(), Some("from vs code"));
        t.feed(br#"{"type":"last-prompt","lastPrompt":"later prompt"}"#);
        t.feed(br#"{"type":"ai-title","sessionId":"s","aiTitle":"Free public APIs project brainstorm"}"#);
        assert_eq!(t.finish().as_deref(), Some("Free public APIs project brainstorm"));
    }
}
