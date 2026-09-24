//! What the window gets: tool input/output, events and diffs cut to their previews.

use super::{Block, FileDiff, Hunk, Item, DIFF_PREVIEW_LINES, INPUT_PREVIEW, OUTPUT_PREVIEW};

pub fn item(item: &Item) -> Item {
    match item {
        // Skill bodies and command output can be long; nothing reads past the preview.
        Item::Event { uuid, at, event, noisy, text } => Item::Event { uuid: uuid.clone(), at: at.clone(), event: event.clone(), noisy: *noisy, text: cut(text, OUTPUT_PREVIEW).0 },
        Item::Assistant { uuid, at, model, usage, blocks } => Item::Assistant { uuid: uuid.clone(), at: at.clone(), model: model.clone(), usage: *usage, blocks: blocks.iter().map(block).collect() },
        user => user.clone(),
    }
}

fn block(block: &Block) -> Block {
    let Block::Tool { id, name, summary, input, output, is_error, diff, diff_truncated: more_files, agent, images, persisted, .. } = block else { return block.clone() };
    let (input, input_truncated) = cut(input, INPUT_PREVIEW);
    let (output, cut_off) = output.as_deref().map_or((None, false), |o| {
        let (o, cut_off) = cut(o, OUTPUT_PREVIEW);
        (Some(o), cut_off)
    });
    let (diff, diff_truncated) = cut_diff(diff);
    Block::Tool {
        id: id.clone(),
        name: name.clone(),
        summary: summary.clone(),
        input,
        input_truncated,
        output,
        output_truncated: cut_off || persisted.is_some(),
        is_error: *is_error,
        diff,
        diff_truncated: diff_truncated || *more_files,
        agent: agent.clone(),
        images: images.clone(),
        persisted: None,
    }
}

/// The first `max` bytes (on a char boundary), and whether anything was cut.
pub fn cut(text: &str, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text.to_owned(), false);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), true)
}

/// At most `DIFF_PREVIEW_LINES` lines across all files of one call.
fn cut_diff(diff: &[FileDiff]) -> (Vec<FileDiff>, bool) {
    let mut left = DIFF_PREVIEW_LINES;
    let mut cut_off = false;
    let files = diff
        .iter()
        .map(|f| FileDiff {
            path: f.path.clone(),
            hunks: f
                .hunks
                .iter()
                .filter_map(|h| {
                    cut_off |= h.lines.len() > left;
                    let lines: Vec<String> = h.lines.iter().take(left).cloned().collect();
                    left -= lines.len();
                    (!lines.is_empty()).then_some(Hunk { old_start: h.old_start, new_start: h.new_start, lines })
                })
                .collect(),
        })
        .collect();
    (files, cut_off)
}
