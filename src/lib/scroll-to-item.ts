/** Name of the CSS highlight painting search matches (`::highlight(viewer-search)`). */
const HIGHLIGHT = "viewer-search";

/** Brings an item (and one of its blocks) of the conversation into view; opens the block when
 *  it is a collapsed tool call and highlights `query` inside. */
export function scrollToItem(box: HTMLElement | null, item: number, block: number | null = null, query = "") {
  const li = box?.querySelector<HTMLElement>(`[data-index="${item}"]`);
  if (!li) return;
  // The item's own block, not one of a subagent conversation opened inside it.
  const target = (block === null ? null : li.querySelector<HTMLElement>(`:scope > div > [data-block="${block}"]`)) ?? li;
  // Tool calls and long events sit collapsed in a <details>.
  const details = target instanceof HTMLDetailsElement ? target : target.querySelector<HTMLDetailsElement>(":scope > details");
  if (details) details.open = true;
  target.scrollIntoView({ block: "center" });
  // Heights above settle as they render (`content-visibility`): aim again once they did.
  requestAnimationFrame(() => requestAnimationFrame(() => target.scrollIntoView({ block: "center" })));
  try {
    highlight(target, query);
  } catch {
    // A highlight is a nicety: never let it break the jump.
  }
}

export function clearHighlight() {
  CSS.highlights?.delete(HIGHLIGHT);
}

function highlight(root: HTMLElement, query: string) {
  clearHighlight();
  const needle = query.trim().toLowerCase();
  if (!needle || !CSS.highlights) return;
  const ranges: Range[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node && ranges.length < 200; node = walker.nextNode()) {
    const raw = node.textContent ?? "";
    const text = raw.toLowerCase();
    // Lowercasing can change the length ("İ"): offsets would no longer fit the node.
    if (text.length !== raw.length) continue;
    for (let at = text.indexOf(needle); at >= 0; at = text.indexOf(needle, at + needle.length)) {
      const range = new Range();
      range.setStart(node, at);
      range.setEnd(node, at + needle.length);
      ranges.push(range);
    }
  }
  CSS.highlights.set(HIGHLIGHT, new Highlight(...ranges));
}
