use std::collections::{HashMap, HashSet};

use super::records::{is_system_text, Payload, Rec};

/// Records of rewound branches, to leave out. Claude Code parents records loosely (API retries,
/// parallel tool calls parented to their own call, history re-appended on resume), so the parent
/// chain alone would hide real work. Only a typed prompt that is off the current branch marks a
/// rewind: that prompt and everything under it is left out.
pub fn rewound(recs: &[Rec]) -> HashSet<usize> {
    let path = current_path(recs);
    let mut children: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, r) in recs.iter().enumerate() {
        if let Some(p) = r.parent.as_deref().or(r.logical_parent.as_deref()) {
            children.entry(p).or_default().push(i);
        }
    }
    let mut hidden = HashSet::new();
    let mut stack: Vec<usize> = (0..recs.len()).filter(|&i| is_typed_prompt(&recs[i]) && recs[i].uuid.as_deref().is_some_and(|u| !path.contains(u))).collect();
    while let Some(i) = stack.pop() {
        let Some(uuid) = recs[i].uuid.as_deref() else { continue };
        if path.contains(uuid) || !hidden.insert(i) {
            continue;
        }
        stack.extend(children.get(uuid).into_iter().flatten().copied());
    }
    hidden
}

/// User and Claude turns: under a rewound prompt they count as hidden messages.
pub fn is_conversation(p: &Payload) -> bool {
    matches!(p, Payload::User { .. } | Payload::Assistant { .. })
}

/// Uuids from the newest turn back to the root through `parentUuid` (`logicalParentUuid` across a
/// compaction). A parent that failed to parse (a line torn by a crash) falls back to the previous
/// turn in the file.
fn current_path(recs: &[Rec]) -> HashSet<&str> {
    let index: HashMap<&str, usize> = recs.iter().enumerate().filter(|(_, r)| !r.sidechain).filter_map(|(i, r)| Some((r.uuid.as_deref()?, i))).collect();
    let mut path = HashSet::new();
    // The newest turn by time, not by position: resuming can re-append old records at the end.
    let turns = recs.iter().enumerate().filter(|(_, r)| !r.sidechain && r.uuid.is_some() && is_conversation(&r.payload));
    let mut cur = turns.max_by(|(i, a), (j, b)| a.at.cmp(&b.at).then(i.cmp(j))).map(|(i, _)| i);
    while let Some(i) = cur {
        let r = &recs[i];
        if !r.uuid.as_deref().is_some_and(|u| path.insert(u)) {
            break; // a cycle in a damaged file
        }
        cur = match r.parent.as_deref().or(r.logical_parent.as_deref()) {
            None => None,
            Some(p) => index.get(p).copied().or_else(|| r.logical_parent.as_deref().and_then(|l| index.get(l).copied())).or_else(|| previous_turn(recs, i)),
        };
    }
    path
}

fn previous_turn(recs: &[Rec], before: usize) -> Option<usize> {
    recs[..before].iter().rposition(|r| !r.sidechain && r.uuid.is_some() && is_conversation(&r.payload))
}

fn is_typed_prompt(r: &Rec) -> bool {
    match &r.payload {
        Payload::User { text: Some(text), meta: false, .. } => !r.sidechain && !is_system_text(text),
        _ => false,
    }
}
