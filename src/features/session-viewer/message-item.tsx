import type { Ref } from "react";
import { formatClock, formatCount, formatDateTime } from "../../lib/format";
import { t } from "../../lib/i18n";
import type { Block, Item } from "../../lib/tauri-commands";
import { ImageThumbs } from "./image-thumbs";
import { MarkdownText } from "./markdown-text";
import { ToolBlock, type LoadDetail } from "./tool-block";

/** Off-screen items skip layout and paint: a long session stays smooth without a virtual list. */
const LAZY = "[content-visibility:auto] [contain-intrinsic-size:auto_120px]";
/** Events whose text reads well inline; the rest keep their text behind a disclosure. */
const INLINE = new Set(["compact", "api_error", "mention", "queued", "interrupted"]);
const LABELED = new Set([...INLINE, "ide", "meta", "notification", "tool_result"]);

/** Hidden unless system events are shown: system noise, and turns with only signature-only thinking. */
export function isShown(item: Item): boolean {
  if (item.kind === "event") return !item.noisy;
  if (item.kind === "assistant") return item.blocks.some((b) => b.type !== "thinking" || b.text);
  return true;
}

/** `anchor`: the element to scroll back to (a turn's prompt). */
export function MessageItem({ item, detail, anchor }: { item: Item; detail: LoadDetail; anchor?: Ref<HTMLLIElement> }) {
  if (item.kind === "event") return <EventLine item={item} />;
  const user = item.kind === "user";
  const heading = [
    t(user ? "viewer.you" : "viewer.claude"),
    item.at && formatClock(item.at),
    !user && item.model,
    !user && item.usage && t("viewer.turn_tokens", { input: formatCount(item.usage.input + item.usage.cache_creation), output: formatCount(item.usage.output) }),
  ];
  return (
    <li ref={anchor} className={`flex flex-col gap-2 ${LAZY}`}>
      <p className="text-caption text-ashen" title={item.at ? formatDateTime(item.at) : undefined}>
        {heading.filter(Boolean).join(" · ")}
      </p>
      {user ? (
        <div className="rounded-card bg-soft-stone px-5 py-4">
          <p className="whitespace-pre-wrap break-words text-[15px] text-carbon-ink">{item.text}</p>
          {item.images.length > 0 && (
            <div className="mt-2">
              <ImageThumbs refs={item.images} detail={detail} />
            </div>
          )}
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          {item.blocks.map((block, i) => (
            <BlockView key={i} block={block} detail={detail} />
          ))}
        </div>
      )}
    </li>
  );
}

function BlockView({ block, detail }: { block: Block; detail: LoadDetail }) {
  if (block.type === "text") return <MarkdownText text={block.text} />;
  if (block.type === "tool") return <ToolBlock tool={block} detail={detail} />;
  if (!block.text) return null; // Claude Code kept only a signature
  return (
    <details className="text-caption text-ashen">
      <summary className="cursor-pointer">{t("viewer.thinking")}</summary>
      <p className="mt-2 whitespace-pre-wrap border-l-2 border-chalk pl-3">{block.text}</p>
    </details>
  );
}

function EventLine({ item }: { item: Extract<Item, { kind: "event" }> }) {
  const label = LABELED.has(item.event) ? t(`viewer.event.${item.event}`, { text: item.text }) : item.event;
  if (item.event === "compact") {
    return (
      <li className={`flex items-center gap-3 text-caption text-ashen ${LAZY}`}>
        <span className="h-px flex-1 bg-chalk" />
        {label}
        <span className="h-px flex-1 bg-chalk" />
      </li>
    );
  }
  if (INLINE.has(item.event) || !item.text) {
    return (
      <li className={`truncate text-caption text-ashen ${LAZY}`} title={item.text || undefined}>
        {label}
      </li>
    );
  }
  return (
    <li className={`text-caption text-ashen ${LAZY}`}>
      <details>
        <summary className="cursor-pointer">{label}</summary>
        <pre className="mt-2 max-h-80 overflow-auto whitespace-pre-wrap break-words rounded-control bg-soft-stone p-3 font-mono">{item.text}</pre>
      </details>
    </li>
  );
}
