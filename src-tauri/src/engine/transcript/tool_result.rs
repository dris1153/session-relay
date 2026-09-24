//! What Claude Code records about a tool call beyond its text result: file diffs, the subagent
//! it started, and where an output too large for the transcript was saved.

use serde::Serialize;
use serde_json::Value;

use super::records::text_at;

#[derive(Debug, Clone, Serialize)]
pub struct Hunk {
    pub old_start: u64,
    pub new_start: u64,
    /// Unified diff lines: `+added`, `-removed`, ` context`.
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentRef {
    pub id: String,
    pub kind: Option<String>,
    pub description: Option<String>,
}

#[derive(Default)]
pub struct Extras {
    pub diff: Vec<FileDiff>,
    pub agent: Option<AgentRef>,
    /// File name under `<sid>/tool-results/`.
    pub persisted: Option<String>,
    /// Claude Code recorded only some of the files a shell command changed.
    pub more_files: bool,
}

pub fn extras(name: &str, input: &str, text: &str, extra: Option<&Value>) -> Extras {
    let recorded = extra.and_then(|e| text_at(e, "persistedOutputPath")).and_then(|p| file_name_of(&p));
    let mut out = Extras { persisted: recorded.or_else(|| persisted_name(text)), ..Default::default() };
    let Some(extra) = extra else { return out };
    if let Some(path) = text_at(extra, "filePath") {
        // Write of a new file has no patch: the whole content is the addition.
        let hunks = match (patch(extra.get("structuredPatch")), text_at(extra, "content")) {
            (h, Some(content)) if h.is_empty() && text_at(extra, "type").as_deref() == Some("create") => {
                vec![Hunk { old_start: 0, new_start: 1, lines: content.lines().map(|l| format!("+{l}")).collect() }]
            }
            (h, _) => h,
        };
        if !hunks.is_empty() {
            out.diff.push(FileDiff { path, hunks });
        }
    }
    // Edits made through the shell (`sed`, redirects) are recorded per file.
    if let Some(files) = extra.pointer("/bashEditDiff/files").and_then(Value::as_array) {
        out.diff.extend(files.iter().filter_map(|f| Some(FileDiff { path: text_at(f, "filePath")?, hunks: patch(f.get("hunks")) })));
        out.more_files = extra.pointer("/bashEditDiff/moreFiles").and_then(Value::as_u64).is_some_and(|n| n > 0);
    }
    if matches!(name, "Agent" | "Task") {
        if let Some(id) = text_at(extra, "agentId").filter(|id| valid_agent_id(id)) {
            let input: Value = serde_json::from_str(input).unwrap_or(Value::Null);
            out.agent = Some(AgentRef { id, kind: text_at(&input, "subagent_type").or_else(|| text_at(extra, "agentType")), description: text_at(&input, "description") });
        }
    }
    out
}

fn patch(hunks: Option<&Value>) -> Vec<Hunk> {
    let Some(hunks) = hunks.and_then(Value::as_array) else { return Vec::new() };
    hunks
        .iter()
        .map(|h| Hunk {
            old_start: h.get("oldStart").and_then(Value::as_u64).unwrap_or(0),
            new_start: h.get("newStart").and_then(Value::as_u64).unwrap_or(0),
            lines: h.get("lines").and_then(Value::as_array).map(|l| l.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default(),
        })
        .collect()
}

/// `<persisted-output>… Full output saved to: C:\…\<sid>\tool-results\<name>.txt …`
fn persisted_name(text: &str) -> Option<String> {
    let rest = text.trim_start().strip_prefix("<persisted-output>")?;
    file_name_of(rest.split_once("saved to:")?.1.lines().next()?.trim())
}

fn file_name_of(path: &str) -> Option<String> {
    let name = path.rsplit(['\\', '/']).next()?;
    valid_file_name(name).then(|| name.to_owned())
}

fn valid_file_name(name: &str) -> bool {
    (1..=128).contains(&name.len()) && !name.starts_with('.') && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}

fn valid_agent_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric())
}
