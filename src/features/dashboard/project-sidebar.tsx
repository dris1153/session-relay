import { useRef, useState, type KeyboardEvent } from "react";
import { t, useLanguage } from "../../lib/i18n";
import { needsAttention, STATUS } from "../../lib/status-copy";
import type { ProjectView, User } from "../../lib/tauri-commands";

type Filter = "attention" | "all";

/** Projects grouped by owner; ↑/↓ move the selection, like a list box. */
export function ProjectSidebar({ projects, selected, onSelect, user, onSettings }: { projects: ProjectView[]; selected: string | null; onSelect: (keyHash: string) => void; user: User | null; onSettings: () => void }) {
  useLanguage();
  const [filter, setFilter] = useState<Filter>("all");
  const list = useRef<HTMLDivElement>(null);
  const visible = projects.filter((p) => filter === "all" || needsAttention(p.status));
  const owners = [...new Set(visible.map((p) => p.owner))].sort();
  const ordered = owners.flatMap((owner) => visible.filter((p) => p.owner === owner));

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const at = ordered.findIndex((p) => p.key_hash === selected);
    const next = ordered[Math.min(ordered.length - 1, Math.max(0, at + (e.key === "ArrowDown" ? 1 : -1)))];
    if (!next) return;
    onSelect(next.key_hash);
    list.current?.querySelector<HTMLButtonElement>(`[data-key="${next.key_hash}"]`)?.focus();
  };

  return (
    <aside className="flex w-72 shrink-0 flex-col border-r border-chalk">
      <div className="flex gap-1 px-4 pt-4" role="group" aria-label={t("sidebar.filter")}>
        {(["all", "attention"] as Filter[]).map((f) => (
          <button key={f} type="button" aria-pressed={filter === f} onClick={() => setFilter(f)} className={`rounded-control px-3 py-1 text-body font-medium transition-colors ${filter === f ? "bg-soft-stone text-carbon-ink" : "text-ashen hover:text-graphite"}`}>
            {t(`sidebar.${f}`)}
          </button>
        ))}
      </div>
      <div ref={list} className="flex-1 overflow-y-auto px-2 pb-4" onKeyDown={onKeyDown}>
        {ordered.length === 0 && <p className="px-3 pt-6 text-body text-ashen">{t(filter === "all" ? "sidebar.empty" : "sidebar.all_synced")}</p>}
        {owners.map((owner) => (
          <section key={owner} className="pt-4">
            <h2 className="px-3 pb-1 text-caption font-medium uppercase tracking-wide text-pebble">{owner}</h2>
            <ul className="flex flex-col">
              {visible
                .filter((p) => p.owner === owner)
                .map((p) => (
                  <li key={p.key_hash}>
                    <button
                      type="button"
                      data-key={p.key_hash}
                      aria-current={p.key_hash === selected}
                      onClick={() => onSelect(p.key_hash)}
                      // `relative`: keeps the sr-only status inside the scrolling list; positioned
                      // against the page it would stretch the page to the length of the list.
                      className={`relative flex w-full items-center gap-3 rounded-control px-3 py-2 text-left transition-colors ${p.key_hash === selected ? "bg-soft-stone text-carbon-ink" : "text-graphite hover:bg-soft-stone/60"} ${p.status === "no_remote" ? "opacity-50" : ""}`}
                    >
                      <span aria-hidden="true" className="w-4 text-center text-ashen">
                        {STATUS[p.status].glyph}
                      </span>
                      <span className="min-w-0 flex-1 truncate text-[15px]">
                        {p.name}
                        {p.subpath && <span className="text-ashen"> / {p.subpath}</span>}
                      </span>
                      <span className="sr-only">{t(`status.${p.status}`)}</span>
                    </button>
                  </li>
                ))}
            </ul>
          </section>
        ))}
      </div>
      <footer className="flex items-center justify-between gap-3 border-t border-chalk px-4 py-3">
        <span className="flex min-w-0 items-center gap-2 text-body text-graphite">
          {user && <img src={user.avatar_url} alt="" className="h-6 w-6 rounded-full" />}
          <span className="truncate">{user?.login ?? ""}</span>
        </span>
        <button type="button" onClick={onSettings} className="rounded-control px-2 py-1 text-body font-medium text-graphite hover:text-carbon-ink">
          {t("settings.title")}
        </button>
      </footer>
    </aside>
  );
}
