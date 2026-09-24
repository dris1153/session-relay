//! A session as Markdown: what was asked, what Claude said, what ran and what it returned.

use std::io::{self, Write};
use std::sync::Arc;

use super::{cut, Block, FileDiff, Item, SessionMeta, Transcript};

/// Per tool output in the file: a 10 MB log would drown the conversation.
const OUTPUT_MAX: usize = 50 * 1024;
/// Subagents inside subagents: three levels below the session.
const AGENT_DEPTH: usize = 3;
/// Each subagent of the cloud copy is decrypted on its own: a session with 170 of them would
/// keep the export (and the sync lock) busy for minutes.
const AGENT_MAX: usize = 50;

/// `open_agent(scope, id)` gives a subagent's transcript (scope as in `resolve`), or `None`.
pub type OpenAgent<'a> = dyn FnMut(&str, &str) -> Option<Arc<Transcript>> + 'a;

pub fn write_markdown(t: &Transcript, out: &mut dyn Write, open_agent: &mut OpenAgent) -> io::Result<()> {
    header(&t.meta, out)?;
    let mut budget = AGENT_MAX;
    items(t, out, open_agent, "", 0, &mut budget)
}

fn header(m: &SessionMeta, out: &mut dyn Write) -> io::Result<()> {
    writeln!(out, "# {}\n", m.title.as_deref().unwrap_or("Claude Code session"))?;
    let facts = [
        ("Models", (!m.models.is_empty()).then(|| m.models.join(", "))),
        ("Claude Code", m.version.clone()),
        ("Branch", m.git_branch.clone()),
        ("Folder", m.cwd.clone()),
        ("Started", m.started_at.as_deref().map(time)),
        ("Last activity", m.ended_at.as_deref().map(time)),
    ];
    for (name, value) in facts {
        if let Some(value) = value {
            writeln!(out, "- {name}: {value}")?;
        }
    }
    let u = m.usage;
    writeln!(out, "- Prompts: {} · tool calls: {} · tokens in {} / out {} / from cache {}", m.prompts, m.tool_calls, u.input + u.cache_creation, u.output, u.cache_read)?;
    writeln!(out, "\n---\n")
}

/// `depth`: 0 for the session, 1 for its subagents…; `budget`: subagents still allowed.
fn items(t: &Transcript, out: &mut dyn Write, open_agent: &mut OpenAgent, scope: &str, depth: usize, budget: &mut usize) -> io::Result<()> {
    let h = "#".repeat((2 + 2 * depth).min(6));
    for item in &t.items {
        match item {
            Item::User { at, text, images, .. } => {
                writeln!(out, "{h} You{}\n\n{text}\n", stamp(at))?;
                if !images.is_empty() {
                    writeln!(out, "_[{} image(s)]_\n", images.len())?;
                }
            }
            Item::Assistant { at, model, blocks, .. } => {
                writeln!(out, "{h} Claude{}{}\n", stamp(at), model.as_deref().map(|m| format!(" · {m}")).unwrap_or_default())?;
                for b in blocks {
                    block(b, out, open_agent, scope, depth, budget)?;
                }
            }
            Item::Event { noisy: false, event, text, .. } => writeln!(out, "_{}_\n", event_line(event, text))?,
            Item::Event { .. } => {}
        }
    }
    Ok(())
}

fn block(b: &Block, out: &mut dyn Write, open_agent: &mut OpenAgent, scope: &str, depth: usize, budget: &mut usize) -> io::Result<()> {
    let Block::Tool { name, summary, input, output, is_error, diff, agent, images, .. } = b else {
        return match b {
            Block::Text { text } => writeln!(out, "{text}\n"),
            Block::Thinking { text: Some(t) } => writeln!(out, "> _Thinking:_ {}\n", t.replace('\n', "\n> ")),
            _ => Ok(()),
        };
    };
    writeln!(out, "**{name}**{}{summary}\n", if summary.is_empty() { "" } else { " — " })?;
    fenced(out, "json", input)?;
    for file in diff {
        fenced(out, "diff", &diff_text(file))?;
    }
    if let Some(o) = output {
        let (o, cut_off) = cut(o, OUTPUT_MAX);
        writeln!(out, "{}:\n", if *is_error { "Error" } else { "Output" })?;
        fenced(out, "", &o)?;
        if cut_off {
            writeln!(out, "_(output cut at 50 KB)_\n")?;
        }
    }
    if !images.is_empty() {
        writeln!(out, "_[{} image(s)]_\n", images.len())?;
    }
    let Some(agent) = agent.as_ref().filter(|_| depth < AGENT_DEPTH) else { return Ok(()) };
    writeln!(out, "{} Subagent{}\n", "#".repeat((3 + 2 * depth).min(6)), agent.description.as_deref().map(|d| format!(": {d}")).unwrap_or_default())?;
    if *budget == 0 {
        return writeln!(out, "_(not included: an export holds at most {AGENT_MAX} subagents)_\n");
    }
    *budget -= 1;
    let scope = format!("{scope}agent:{}/", agent.id);
    match open_agent(&scope, &agent.id) {
        Some(sub) => items(&sub, out, open_agent, &scope, depth + 1, budget),
        None => writeln!(out, "_(transcript not available)_\n"),
    }
}

/// A fence longer than any backtick run inside, so tool output never closes it early.
fn fenced(out: &mut dyn Write, lang: &str, text: &str) -> io::Result<()> {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat((longest + 1).max(3));
    writeln!(out, "{fence}{lang}\n{}\n{fence}\n", text.trim_end_matches('\n'))
}

fn diff_text(file: &FileDiff) -> String {
    let mut text = format!("--- {}", file.path);
    for hunk in &file.hunks {
        text.push_str(&format!("\n@@ -{} +{} @@", hunk.old_start, hunk.new_start));
        for line in &hunk.lines {
            text.push('\n');
            text.push_str(line);
        }
    }
    text
}

fn event_line(event: &str, text: &str) -> String {
    match event {
        "compact" => "Conversation compacted".into(),
        "api_error" => format!("API error: {text}"),
        "mention" => format!("Attached {text}"),
        "queued" => format!("Queued: {text}"),
        "interrupted" => "Interrupted by the user".into(),
        other => format!("{other}: {text}"),
    }
}

fn stamp(at: &Option<String>) -> String {
    at.as_deref().map(|a| format!(" · {}", time(a))).unwrap_or_default()
}

/// `2026-09-24T10:02:03.123Z` → `2026-09-24 10:02 UTC`.
fn time(iso: &str) -> String {
    iso.get(..16).map_or_else(|| iso.to_owned(), |t| format!("{} UTC", t.replace('T', " ")))
}
