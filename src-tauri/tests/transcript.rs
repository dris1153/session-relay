mod support;

use session_relay_lib::engine::error::Error;
use session_relay_lib::engine::overview;
use session_relay_lib::engine::sync::SyncMode;
use session_relay_lib::engine::transcript::source::{load, Side};
use session_relay_lib::engine::transcript::{parse, Block, Item, Usage, OUTPUT_PREVIEW};
use support::*;

const BASIC: &[u8] = include_bytes!("fixtures/transcripts/basic.jsonl");
const BRANCHES: &[u8] = include_bytes!("fixtures/transcripts/branches.jsonl");
const PARALLEL: &[u8] = include_bytes!("fixtures/transcripts/parallel.jsonl");

fn kinds(items: &[Item]) -> Vec<String> {
    items
        .iter()
        .map(|i| match i {
            Item::User { text, .. } => format!("user:{text}"),
            Item::Assistant { blocks, .. } => format!("assistant:{}", blocks.len()),
            Item::Event { event, noisy, .. } => format!("event:{event}{}", if *noisy { "~" } else { "" }),
        })
        .collect()
}

#[test]
fn groups_one_message_pairs_tool_results_and_skips_broken_lines() {
    let t = parse(BASIC);
    assert_eq!(kinds(&t.items), ["event:ide~", "user:Fix the login bug", "event:hook_non_blocking_error~", "event:mention", "assistant:3", "assistant:1", "event:stop_hook_summary~"]);
    let Item::Assistant { blocks, model, .. } = &t.items[4] else { panic!() };
    assert_eq!(model.as_deref(), Some("claude-opus-5-5"));
    assert!(matches!(&blocks[0], Block::Thinking { text: None }), "a signature-only thinking block has no text");
    let Block::Tool { name, summary, output, is_error, .. } = &blocks[2] else { panic!() };
    assert_eq!((name.as_str(), summary.as_str(), output.as_deref(), *is_error), ("Bash", "npm test", Some("1 failing"), true));

    let m = &t.meta;
    assert_eq!(m.usage, Usage { input: 13, output: 12, cache_read: 100, cache_creation: 20 }, "usage counted once per API message");
    assert_eq!((m.title.as_deref(), m.version.as_deref(), m.git_branch.as_deref()), (Some("Fix login bug"), Some("2.1.280"), Some("main")));
    assert_eq!((m.started_at.as_deref(), m.ended_at.as_deref()), (Some("2026-09-24T10:00:00.000Z"), Some("2026-09-24T10:00:13.000Z")));
    assert_eq!((m.prompts, m.tool_calls, m.off_branch), (1, 1, 0));
    assert!(t.detail("in:toolu_1").unwrap().contains("npm run lint"));
}

#[test]
fn hides_a_rewound_prompt_and_follows_a_compaction() {
    let t = parse(BRANCHES);
    assert_eq!(
        kinds(&t.items),
        [
            "user:Second try",
            "assistant:1",
            "event:compact",
            "event:meta~",
            "user:/cook plan.md",
            "event:queued",
            "event:meta~",
            "event:future-record~",
            "event:api_error",
            "event:notification~",
            "assistant:2",
            "event:tool_result~",
        ]
    );
    assert!(matches!(&t.items[0], Item::User { images: 1, .. }));
    let Item::Assistant { blocks, .. } = &t.items[1] else { panic!() };
    assert!(matches!(&blocks[0], Block::Tool { summary, output: Some(o), .. } if summary == "Review the code" && o == "No bugs found"));
    assert_eq!(t.meta.models, ["claude-sonnet-5", "claude-opus-5-5"], "the rewound answer's model is not counted");
    assert_eq!((t.meta.prompts, t.meta.tool_calls, t.meta.off_branch), (2, 2, 2));
}

#[test]
fn keeps_parallel_calls_and_survives_a_torn_line() {
    let t = parse(PARALLEL);
    assert_eq!(
        kinds(&t.items),
        ["user:Read both files", "assistant:2", "assistant:1", "user:Why does the viewer show <command-name>/cook</command-name> here?", "assistant:1", "event:api_error", "event:interrupted", "event:stop_hook_summary~"],
        "a torn line hides nothing, a re-appended record shows once, a quoted command tag stays as typed"
    );
    let Item::Assistant { blocks, .. } = &t.items[1] else { panic!() };
    let outputs: Vec<_> = blocks.iter().map(|b| match b {
        Block::Tool { output, .. } => output.as_deref(),
        _ => None,
    }).collect();
    assert_eq!(outputs, [Some("content of a"), Some("content of b")], "results parented to their own call pair up");
    assert!(matches!(&t.items[5], Item::Event { text, .. } if text == "Connection dropped (ECONNRESET) (2/10)"));
    assert_eq!((t.meta.usage.output, t.meta.prompts, t.meta.tool_calls, t.meta.off_branch), (9, 2, 2, 0), "the most complete usage record of a message counts");
}

#[test]
fn previews_long_outputs_and_serves_the_rest() {
    let long = "é".repeat(OUTPUT_PREVIEW);
    let lines = [
        serde_json::json!({"type": "user", "uuid": "u", "message": {"content": "go"}}),
        serde_json::json!({"type": "assistant", "uuid": "a", "parentUuid": "u", "message": {"id": "m", "content": [{"type": "tool_use", "id": "t", "name": "Read", "input": {"file_path": "x"}}]}}),
        serde_json::json!({"type": "user", "uuid": "r", "parentUuid": "a", "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "content": long}]}}),
    ];
    let t = parse(lines.map(|l| l.to_string()).join("\n").as_bytes());
    let Item::Assistant { blocks, .. } = &t.view()[1] else { panic!() };
    let Block::Tool { output: Some(o), output_truncated, .. } = &blocks[0] else { panic!() };
    assert!(*output_truncated && o.len() <= OUTPUT_PREVIEW && o.chars().all(|c| c == 'é'));
    assert_eq!(t.detail("out:t").unwrap(), long);
    assert!(parse(b"\xff\xfe garbage\n{}\n[1,2]\n").items.is_empty(), "garbage never fails");
}

/// Manual timing on a real file (read only): `SR_TRANSCRIPT=<path> cargo test --release --test transcript parse_real -- --ignored --nocapture`.
#[test]
#[ignore]
fn parse_real() {
    let bytes = std::fs::read(std::env::var("SR_TRANSCRIPT").expect("SR_TRANSCRIPT")).unwrap();
    let started = std::time::Instant::now();
    let t = parse(&bytes);
    let parsed = started.elapsed();
    let json = serde_json::to_vec(&t.view()).unwrap();
    let unanswered = t.items.iter().map(|i| match i {
        Item::Assistant { blocks, .. } => blocks.iter().filter(|b| matches!(b, Block::Tool { output: None, .. })).count(),
        _ => 0,
    }).sum::<usize>();
    println!("{} MB → {} items, parse {parsed:?}, view {:?}, view json {} KB, tools without result {unanswered}, meta {:?}", bytes.len() >> 20, t.items.len(), started.elapsed() - parsed, json.len() >> 10, t.meta);
}

#[test]
fn opens_the_local_file_and_the_cloud_copy_with_this_machines_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let id = identity();
    let (mut a, mut b) = (Machine::new(tmp.path(), "A", &url, &id), Machine::new(tmp.path(), "B", &url, &id));
    a.link();
    b.link();
    let reply = serde_json::json!({"type": "assistant", "uuid": "x", "parentUuid": null, "message": {"id": "m", "content": [{"type": "text", "text": "PLACEHOLDER"}]}});
    let line = reply.to_string().replace("PLACEHOLDER", &format!("saved to {}\\\\{SID}\\\\tool-results\\\\out.txt", a.escaped_dir()));
    a.write(&format!("{SID}.jsonl"), (transcript_line("user", &a.checkout, "fix the bug") + &line + "\n").as_bytes());
    a.sync(SyncMode::Auto);

    let local = parse(&load(&a.engine, &a.key(), SID, Side::Local, None).unwrap().unwrap().bytes);
    assert!(matches!(&local.items[0], Item::User { text, .. } if text == "fix the bug"));

    overview::refresh(&b.engine, std::time::Duration::ZERO).unwrap();
    let loaded = load(&b.engine, &b.key(), SID, Side::Cloud, None).unwrap().unwrap();
    assert!(load(&b.engine, &b.key(), SID, Side::Cloud, Some(&loaded.signature)).unwrap().is_none(), "an unchanged copy is not decrypted again");
    let cloud = parse(&loaded.bytes);
    let Item::Assistant { blocks, .. } = &cloud.items[1] else { panic!() };
    let Block::Text { text } = &blocks[0] else { panic!() };
    assert!(text.contains(&*b.project_dir().to_string_lossy()), "cloud paths point at this machine: {text}");
    assert!(matches!(load(&b.engine, &b.key(), SID, Side::Local, None), Err(Error::SessionNotFound)), "B has no local copy");
    assert!(matches!(load(&b.engine, &b.key(), "../../../../etc/passwd-0000000000000", Side::Local, None), Err(Error::Invalid(_))));
}
