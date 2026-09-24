import { useEffect, useRef, useState } from "react";
import { formatClock } from "../../lib/format";
import { t } from "../../lib/i18n";
import type { Entry } from "./turn-section";

/** Every prompt of the session in a drop-down; picking one scrolls to it. */
export function OutlineMenu({ entries, onGo }: { entries: Entry[]; onGo: (index: number) => void }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const prompts = entries.filter((e) => e.item.kind === "user");

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => !root.current?.contains(e.target as Node) && setOpen(false);
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);

  return (
    <div ref={root} className="relative">
      <button type="button" onClick={() => setOpen(!open)} aria-expanded={open} className="rounded-control border border-mist bg-paper-white px-3 py-1.5 text-body text-graphite hover:text-carbon-ink">
        {t("viewer.outline", { count: prompts.length })}
      </button>
      {open && (
        <ol className="absolute left-0 top-full z-20 mt-1 max-h-[60vh] w-[min(28rem,70vw)] overflow-y-auto rounded-card border border-chalk bg-paper-white p-2 shadow-[0_8px_24px_-12px_rgba(18,18,18,0.35)]">
          {prompts.map(({ item, index }) => (
            <li key={index}>
              <button
                type="button"
                onClick={() => {
                  setOpen(false);
                  onGo(index);
                }}
                className="flex w-full gap-3 rounded-control px-3 py-2 text-left hover:bg-soft-stone"
              >
                <span className="shrink-0 text-caption text-ashen">{item.at ? formatClock(item.at) : ""}</span>
                <span className="line-clamp-2 min-w-0 break-words text-body text-carbon-ink">{item.kind === "user" ? item.text : ""}</span>
              </button>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
