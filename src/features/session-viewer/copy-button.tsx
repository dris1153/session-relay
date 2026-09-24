import { useState } from "react";
import { t } from "../../lib/i18n";
import type { Item } from "../../lib/tauri-commands";

/** What a message says, as plain text: tool calls as one line each. */
function plainText(item: Item): string {
  if (item.kind !== "assistant") return item.text;
  return item.blocks
    .map((b) => (b.type === "text" ? b.text : b.type === "tool" ? `[${b.name}] ${b.summary}` : ""))
    .filter(Boolean)
    .join("\n\n");
}

export function CopyButton({ item }: { item: Item }) {
  const [done, setDone] = useState(false);
  const copy = () =>
    navigator.clipboard.writeText(plainText(item)).then(() => {
      setDone(true);
      setTimeout(() => setDone(false), 1500);
    }, () => {});
  return (
    <button type="button" onClick={copy} className="rounded-control px-1.5 text-caption text-pebble opacity-0 transition-opacity hover:text-carbon-ink focus-visible:opacity-100 group-hover:opacity-100">
      {done ? t("viewer.copied") : t("viewer.copy")}
    </button>
  );
}
