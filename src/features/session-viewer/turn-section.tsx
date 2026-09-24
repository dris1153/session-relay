import { useEffect, useRef, useState, type RefObject } from "react";
import { formatClock } from "../../lib/format";
import { t } from "../../lib/i18n";
import type { Item } from "../../lib/tauri-commands";
import { MessageItem } from "./message-item";
import type { LoadDetail } from "./tool-block";

export type Entry = { item: Item; index: number };

/** One turn: a prompt and everything until the next one. While the prompt is scrolled out above,
 *  a compact copy sticks to the top of the box; the next turn's copy pushes it away. */
export function TurnSection({ prompt, rest, detail, box }: { prompt: Entry | null; rest: Entry[]; detail: LoadDetail; box: RefObject<HTMLDivElement | null> }) {
  const full = useRef<HTMLLIElement>(null);
  const [above, setAbove] = useState(false);

  useEffect(() => {
    const el = full.current;
    if (!el || !box.current) return;
    const watch = new IntersectionObserver(([e]) => setAbove(!e.isIntersecting && e.boundingClientRect.top < (e.rootBounds?.top ?? 0)), { root: box.current });
    watch.observe(el);
    return () => watch.disconnect();
  }, [box]);

  const user = prompt?.item.kind === "user" ? prompt.item : null;
  return (
    <li>
      {user && (
        // Zero height: the bar takes no room in the turn and only shows while the prompt is above.
        <div className="sticky top-0 z-10 h-0">
          {above && (
            <button
              type="button"
              onClick={() => full.current?.scrollIntoView({ block: "start", behavior: "smooth" })}
              title={t("viewer.back_to_prompt")}
              className="flex w-full flex-col items-start gap-0.5 border-b border-chalk bg-bone-parchment px-5 py-2 text-left shadow-[0_4px_12px_-8px_rgba(18,18,18,0.25)]"
            >
              <span className="text-caption text-ashen">{[t("viewer.you"), user.at && formatClock(user.at)].filter(Boolean).join(" · ")}</span>
              <span className="line-clamp-2 break-words text-body text-carbon-ink">{user.text}</span>
            </button>
          )}
        </div>
      )}
      <ol className="flex flex-col gap-6">
        {prompt && <MessageItem item={prompt.item} detail={detail} anchor={full} />}
        {rest.map(({ item, index }) => (
          <MessageItem key={index} item={item} detail={detail} />
        ))}
      </ol>
    </li>
  );
}
