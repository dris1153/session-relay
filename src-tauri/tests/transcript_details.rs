mod support;

use std::sync::Arc;

use session_relay_lib::engine::error::{Error, Result};
use session_relay_lib::engine::overview;
use session_relay_lib::engine::sync::SyncMode;
use session_relay_lib::engine::transcript::source::{load_rel, Side};
use session_relay_lib::engine::transcript::{parse, parse_agent, resolve, Block, Detail, Item, Resolved, DIFF_PREVIEW_LINES};
use support::*;

const DETAILS: &[u8] = include_bytes!("fixtures/transcripts/details.jsonl");

fn tools(items: &[Item]) -> Vec<&Block> {
    items
        .iter()
        .flat_map(|i| match i {
            Item::Assistant { blocks, .. } => blocks.iter().collect(),
            _ => Vec::new(),
        })
        .filter(|b| matches!(b, Block::Tool { .. }))
        .collect()
}

#[test]
fn reads_diffs_saved_outputs_agents_and_images() {
    let t = parse(DETAILS);
    let Item::User { images, .. } = &t.items[0] else { panic!() };
    assert_eq!(images, &["img:0"], "only image types the viewer can show are kept");
    assert!(matches!(t.detail("img:0"), Some(Detail::Image(img)) if img.media_type == "image/png"));

    let calls = tools(&t.items);
    let diff_of = |i: usize| match calls[i] {
        Block::Tool { diff, .. } => diff.iter().map(|f| format!("{}:{}", f.path, f.hunks.iter().flat_map(|h| h.lines.clone()).collect::<Vec<_>>().join("|"))).collect::<Vec<_>>(),
        _ => unreachable!(),
    };
    assert_eq!(diff_of(0), ["src/app.ts:-const x = a;|+const x = b;"]);
    assert_eq!(diff_of(1), ["src/new.ts:+line one|+line two"], "a new file is all additions");
    assert_eq!(diff_of(2), [".gitignore:-x|+y"], "edits made through the shell");
    assert!(matches!(t.detail("out:t_bash"), Some(Detail::File(rel)) if rel == "tool-results/toolu_big.txt"));
    let view = t.view();
    assert!(tools(&view).iter().any(|b| matches!(b, Block::Tool { id, output_truncated: true, .. } if id == "t_bash")), "a saved output always offers the full text");

    let Block::Tool { agent: Some(agent), .. } = calls[3] else { panic!() };
    assert_eq!((agent.id.as_str(), agent.kind.as_deref(), agent.description.as_deref()), ("a0b1c2d3", Some("code-reviewer"), Some("Review the fix")));
    assert!(matches!(t.detail("agent:a0b1c2d3"), Some(Detail::Agent(id)) if id == "a0b1c2d3"));
    assert!(t.detail("agent:someone-else").is_none(), "only agents started in this session");
    let Block::Tool { images, .. } = calls[4] else { panic!() };
    assert_eq!(images, &["img:1"]);
}

#[test]
fn previews_long_diffs_and_parses_a_subagent_file() {
    let lines: Vec<String> = (0..DIFF_PREVIEW_LINES + 50).map(|i| format!("+line {i}")).collect();
    let records = [
        serde_json::json!({"type": "assistant", "uuid": "a", "message": {"id": "m", "content": [{"type": "tool_use", "id": "w", "name": "Write", "input": {}}]}}),
        serde_json::json!({"type": "user", "uuid": "r", "parentUuid": "a", "message": {"content": [{"type": "tool_result", "tool_use_id": "w", "content": "ok"}]}, "toolUseResult": {"filePath": "big.txt", "structuredPatch": [{"oldStart": 1, "newStart": 1, "lines": lines}]}}),
    ];
    let t = parse(records.map(|r| r.to_string()).join("\n").as_bytes());
    let Block::Tool { diff, diff_truncated, .. } = tools(&t.view())[0].clone() else { panic!() };
    assert!(diff_truncated && diff[0].hunks[0].lines.len() == DIFF_PREVIEW_LINES);

    let agent = [
        serde_json::json!({"type": "user", "uuid": "s1", "parentUuid": null, "isSidechain": true, "message": {"content": "Look for bugs"}}),
        serde_json::json!({"type": "assistant", "uuid": "s2", "parentUuid": "s1", "isSidechain": true, "message": {"id": "sm", "content": [{"type": "text", "text": "None found"}]}}),
    ];
    let bytes = agent.map(|r| r.to_string()).join("\n");
    assert!(parse(bytes.as_bytes()).items.is_empty(), "a session hides sidechain records");
    assert_eq!(parse_agent(bytes.as_bytes()).items.len(), 2, "a subagent's own file shows them");
}

#[test]
fn resolves_refs_inside_nested_agents() {
    let root = Arc::new(parse(DETAILS));
    let agent = |id: &str, child: Option<&str>| {
        let mut lines = vec![serde_json::json!({"type": "assistant", "uuid": "x", "isSidechain": true, "message": {"id": "m", "content": [{"type": "tool_use", "id": "t_in", "name": "Bash", "input": {"command": format!("echo {id}")}}]}}).to_string()];
        if let Some(child) = child {
            lines.push(serde_json::json!({"type": "assistant", "uuid": "y", "parentUuid": "x", "isSidechain": true, "message": {"id": "m2", "content": [{"type": "tool_use", "id": "t_agent", "name": "Agent", "input": {"description": "deeper"}}]}}).to_string());
            lines.push(serde_json::json!({"type": "user", "uuid": "z", "parentUuid": "y", "isSidechain": true, "message": {"content": [{"type": "tool_result", "tool_use_id": "t_agent", "content": "done"}]}, "toolUseResult": {"agentId": child}}).to_string());
        }
        Arc::new(parse_agent(lines.join("\n").as_bytes()))
    };
    let mut opened = Vec::new();
    let mut open = |scope: &str, id: &str| -> Result<_> {
        opened.push(scope.to_owned());
        Ok(agent(id, (id == "a0b1c2d3").then_some("c9")))
    };
    let Resolved::Detail(Detail::Text(input)) = resolve(Arc::clone(&root), "agent:a0b1c2d3/agent:c9/in:t_in", &mut open).unwrap() else { panic!() };
    assert!(input.contains("echo c9"), "the ref resolves inside the nested agent");
    assert!(matches!(resolve(Arc::clone(&root), "agent:a0b1c2d3", &mut open), Ok(Resolved::Agent(_))));
    assert!(resolve(Arc::clone(&root), "agent:c9", &mut open).is_err(), "an agent the session did not start");
    assert!(resolve(Arc::clone(&root), "in:t_edit/in:t_edit", &mut open).is_err());
    assert!(resolve(root, &["agent:a0b1c2d3"; 9].join("/"), &mut open).is_err(), "depth is capped");
    assert_eq!(opened[..2], ["agent:a0b1c2d3/", "agent:a0b1c2d3/agent:c9/"], "each agent is opened under its own scope");
}

#[test]
fn serves_session_files_from_this_machine_and_the_cloud_only() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let id = identity();
    let (mut a, mut b) = (Machine::new(tmp.path(), "A", &url, &id), Machine::new(tmp.path(), "B", &url, &id));
    a.link();
    b.link();
    a.write(&format!("{SID}.jsonl"), transcript_line("user", &a.checkout, "go").as_bytes());
    a.write(&format!("{SID}/tool-results/out.txt"), b"full output");
    a.write(&format!("{SID}/subagents/agent-abc123.jsonl"), b"{}\n");
    a.sync(SyncMode::Auto);
    overview::refresh(&b.engine, std::time::Duration::ZERO).unwrap();

    let read = |m: &Machine, side, rel| load_rel(&m.engine, &m.key(), SID, side, rel, None).map(|l| l.unwrap().bytes);
    assert_eq!(read(&a, Side::Local, "tool-results/out.txt").unwrap(), b"full output");
    assert_eq!(read(&b, Side::Cloud, "tool-results/out.txt").unwrap(), b"full output");
    assert_eq!(read(&b, Side::Cloud, "subagents/agent-abc123.jsonl").unwrap(), b"{}\n");
    assert!(matches!(read(&b, Side::Local, "tool-results/out.txt"), Err(Error::SessionNotFound)));
    assert!(matches!(read(&a, Side::Local, "tool-results/cleaned-up.txt"), Err(Error::SessionNotFound)), "a saved output Claude removed");
    for bad in ["../x.txt", "tool-results/../../x", "tool-results/CON", "tool-results/nul.txt", "subagents/agent-a.b.jsonl", "subagents/other.jsonl", "memory/MEMORY.md", "tool-results"] {
        assert!(matches!(read(&a, Side::Local, bad), Err(Error::Invalid(_))), "{bad}");
    }
}
