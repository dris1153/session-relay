use super::{Item, Transcript};

/// Where two copies of one session part: the index, in each copy, of its first item the other
/// copy does not have at the same place. `None` for a copy that ends where they agree.
pub fn first_difference(a: &Transcript, b: &Transcript) -> (Option<usize>, Option<usize>) {
    let same = a.items.iter().zip(&b.items).take_while(|(x, y)| key(x) == key(y)).count();
    ((same < a.items.len()).then_some(same), (same < b.items.len()).then_some(same))
}

/// Records keep their uuid across machines; an event without one compares by its text.
fn key(item: &Item) -> (&str, &str) {
    match item {
        Item::User { uuid, .. } | Item::Assistant { uuid, .. } | Item::Event { uuid: Some(uuid), .. } => (uuid, ""),
        Item::Event { uuid: None, event, text, .. } => (event, text),
    }
}
