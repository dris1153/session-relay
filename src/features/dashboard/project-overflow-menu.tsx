import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { t } from "../../lib/i18n";

export type MenuItem = { label: string; onSelect: () => void; disabled?: boolean };

/** "⋯" menu for the rarer and forceful actions; ↑/↓ move between items, Escape or a click outside closes it. */
export function ProjectOverflowMenu({ items }: { items: MenuItem[] }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    root.current?.querySelector<HTMLButtonElement>("[role=menuitem]:not(:disabled)")?.focus();
    const close = (e: MouseEvent | KeyboardEvent) => {
      if (e instanceof KeyboardEvent ? e.key === "Escape" : !root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", close);
    };
  }, [open]);

  return (
    <div ref={root} className="relative">
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={t("project.more")}
        onClick={() => setOpen(!open)}
        className="rounded-control border border-chalk bg-paper-white px-3 py-2 text-[15px] leading-none text-graphite hover:border-mist hover:text-carbon-ink"
      >
        ⋯
      </button>
      {open && (
        <ul role="menu" onKeyDown={moveFocus} className="absolute right-0 z-10 mt-2 flex w-64 flex-col rounded-card border border-chalk bg-paper-white p-1 shadow-soft">
          {items.map((item) => (
            <li key={item.label} role="none">
              <button
                type="button"
                role="menuitem"
                disabled={item.disabled}
                onClick={() => {
                  setOpen(false);
                  item.onSelect();
                }}
                className="w-full rounded-control px-3 py-2 text-left text-body text-graphite hover:bg-soft-stone hover:text-carbon-ink disabled:opacity-50"
              >
                {item.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function moveFocus(e: ReactKeyboardEvent<HTMLUListElement>) {
  if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
  e.preventDefault();
  const items = [...e.currentTarget.querySelectorAll<HTMLButtonElement>("[role=menuitem]:not(:disabled)")];
  const at = items.indexOf(document.activeElement as HTMLButtonElement);
  items[(at + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length]?.focus();
}
