use std::sync::Arc;

use session_relay_lib::engine::transcript::{first_difference, parse, parse_agent, write_markdown, Hit, OUTPUT_PREVIEW};

const BASIC: &[u8] = include_bytes!("fixtures/transcripts/basic.jsonl");
const DETAILS: &[u8] = include_bytes!("fixtures/transcripts/details.jsonl");

fn session(lines: &[serde_json::Value]) -> Vec<u8> {
    lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n").into_bytes()
}

fn turn(uuid: &str, parent: Option<&str>, text: &str) -> serde_json::Value {
    serde_json::json!({"type": "user", "uuid": uuid, "parentUuid": parent, "message": {"content": text}})
}

#[test]
fn searches_full_content_case_insensitively() {
    let t = parse(BASIC);
    assert_eq!(t.search("LOGIN BUG", false), [Hit { item: 1, block: None }], "the prompt, whatever the case");
    assert_eq!(t.search("npm run lint", false), [Hit { item: 4, block: Some(2) }], "inside a tool call's input");
    assert!(t.search("hook failed", false).is_empty(), "system events only while they are shown");
    assert_eq!(t.search("hook failed", true), [Hit { item: 2, block: None }]);
    assert!(t.search("non_blocking", true).is_empty(), "an event's type name is not its text");
    assert!(t.search("   ", true).is_empty());

    let tail = format!("{}needle at the end", "x".repeat(OUTPUT_PREVIEW * 2));
    let long = session(&[
        turn("u", None, "Sửa lỗi đăng nhập"),
        serde_json::json!({"type": "assistant", "uuid": "a", "parentUuid": "u", "message": {"id": "m", "content": [{"type": "tool_use", "id": "t", "name": "Read", "input": {}}]}}),
        serde_json::json!({"type": "user", "uuid": "r", "parentUuid": "a", "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "content": tail}]}}),
    ]);
    let t = parse(&long);
    assert_eq!(t.search("NEEDLE", false), [Hit { item: 1, block: Some(0) }], "past the preview the window shows");
    assert_eq!(t.search("ĐĂNG NHẬP", false), [Hit { item: 0, block: None }], "non-ASCII queries");
    let windows = parse(&session(&[serde_json::json!({"type": "assistant", "uuid": "a", "message": {"id": "m", "content": [{"type": "tool_use", "id": "t", "name": "Read", "input": {"file_path": r"D:\app\src\main.rs"}}]}})]));
    assert_eq!(windows.search(r"app\src\main", false).len(), 1, "a Windows path as typed, not as JSON escapes it");
}

#[test]
fn finds_where_two_copies_part() {
    let common = [turn("u1", None, "start"), turn("u2", Some("u1"), "second")];
    let here = session(&[common[0].clone(), common[1].clone(), turn("h", Some("u2"), "only here")]);
    let there = session(&[common[0].clone(), common[1].clone(), turn("c", Some("u2"), "only in the cloud"), turn("c2", Some("c"), "more")]);
    assert_eq!(first_difference(&parse(&here), &parse(&there)), (Some(2), Some(2)));
    let prefix = session(&common);
    assert_eq!(first_difference(&parse(&prefix), &parse(&there)), (None, Some(2)), "one copy only extends the other");
    assert_eq!(first_difference(&parse(&prefix), &parse(&prefix)), (None, None));
}

#[test]
fn exports_markdown_with_tools_diffs_and_subagents() {
    let t = parse(DETAILS);
    let agent = Arc::new(parse_agent(&session(&[serde_json::json!({"type": "assistant", "uuid": "s", "isSidechain": true, "message": {"id": "sm", "content": [{"type": "text", "text": "Looks fine to me"}]}})])));
    let mut asked = Vec::new();
    let mut out = Vec::new();
    write_markdown(&t, &mut out, &mut |scope, id| {
        asked.push(format!("{scope}|{id}"));
        Some(Arc::clone(&agent))
    })
    .unwrap();
    let md = String::from_utf8(out).unwrap();
    assert!(md.starts_with("# Fix this screen\n"), "{md}");
    assert!(md.contains("## You · 2026-09-24 12:00 UTC\n\nFix this screen"));
    assert!(md.contains("**Edit** — src/app.ts"));
    assert!(md.contains("```diff\n--- src/app.ts\n@@ -3 +3 @@\n-const x = a;\n+const x = b;\n```"));
    assert!(md.contains("### Subagent: Review the fix") && md.contains("Looks fine to me"));
    assert_eq!(asked, ["agent:a0b1c2d3/|a0b1c2d3"]);

    let tricky = session(&[
        turn("u", None, "show me"),
        serde_json::json!({"type": "assistant", "uuid": "a", "parentUuid": "u", "message": {"id": "m", "content": [{"type": "tool_use", "id": "t", "name": "Bash", "input": {"command": "cat README.md"}}]}}),
        serde_json::json!({"type": "user", "uuid": "r", "parentUuid": "a", "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "content": format!("```rust\nfn main() {{}}\n```\n{}", "y".repeat(60 * 1024))}]}}),
    ]);
    let mut out = Vec::new();
    write_markdown(&parse(&tricky), &mut out, &mut |_, _| None).unwrap();
    let md = String::from_utf8(out).unwrap();
    assert!(md.contains("````\n```rust"), "a longer fence keeps quoted fences inside");
    assert!(md.contains("_(output cut at 50 KB)_"));
    let mut out = Vec::new();
    write_markdown(&parse(BASIC), &mut out, &mut |_, _| None).unwrap();
    let md = String::from_utf8(out).unwrap();
    assert!(md.contains("_Attached src/login.ts_") && !md.contains("hook failed"), "system noise stays out: {md}");

    // An agent that keeps starting itself: the export stops three levels down.
    let mut out = Vec::new();
    let mut opened = 0;
    write_markdown(&t, &mut out, &mut |_, _| {
        opened += 1;
        Some(Arc::new(parse(DETAILS)))
    })
    .unwrap();
    assert_eq!(opened, 3);
}
