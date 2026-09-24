import { useCallback, useState } from "react";
import { formatCount, formatDateTime } from "../../lib/format";
import { errorText, t } from "../../lib/i18n";
import { api, errorCode, type Hit, type SessionMeta, type Side } from "../../lib/tauri-commands";
import { OutlineMenu } from "./outline-menu";
import { SearchBar } from "./search-bar";
import type { Entry } from "./turn-section";

export type HeaderProps = {
  session: { keyHash: string; sessionId: string; side: Side };
  meta: SessionMeta | null;
  entries: Entry[];
  showSystem: boolean;
  onShowSystem: (on: boolean) => void;
  /** Present when both copies exist and differ: switches the copy shown. */
  onSide: ((side: Side) => void) | null;
  divergeAt: number | null;
  onBack: () => void;
  onGo: (item: number, block?: number | null, query?: string) => void;
};

export function ViewerHeader({ session, meta, entries, showSystem, onShowSystem, onSide, divergeAt, onBack, onGo }: HeaderProps) {
  const search = useCallback((query: string) => api.searchSession(session.keyHash, session.sessionId, session.side, query, showSystem), [session.keyHash, session.sessionId, session.side, showSystem]);
  return (
    <header className="flex flex-col gap-2 border-b border-chalk px-8 py-5">
      <div className="flex items-center gap-3">
        <button type="button" onClick={onBack} autoFocus className="shrink-0 rounded-control px-2 py-1 text-body text-graphite hover:text-carbon-ink">
          <span aria-hidden>← </span>
          {t("viewer.back")}
        </button>
        <h1 className="min-w-0 truncate font-serif text-heading-sm text-carbon-ink">{meta?.title ?? session.sessionId}</h1>
      </div>
      {meta && <MetaLines meta={meta} side={session.side} />}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <SearchBar search={search} onGo={(hit: Hit, query) => onGo(hit.item, hit.block, query)} />
        <OutlineMenu entries={entries} onGo={(index) => onGo(index)} />
        {onSide && (
          <div role="group" className="flex rounded-control border border-mist bg-paper-white p-0.5">
            {(["local", "cloud"] as const).map((side) => (
              <button key={side} type="button" aria-pressed={session.side === side} onClick={() => session.side !== side && onSide(side)} className={`rounded-[6px] px-3 py-1 text-body ${session.side === side ? "bg-soft-stone text-carbon-ink" : "text-graphite hover:text-carbon-ink"}`}>
                {t(`viewer.side.${side}`)}
              </button>
            ))}
          </div>
        )}
        {divergeAt !== null && (
          <button type="button" onClick={() => onGo(divergeAt)} className="text-body text-graphite underline decoration-mist underline-offset-4 hover:text-carbon-ink">
            {t("viewer.go_diverge")}
          </button>
        )}
        <ExportButton session={session} title={meta?.title ?? session.sessionId} />
        <label className="flex items-center gap-2 text-caption text-graphite">
          <input type="checkbox" className="accent-carbon-ink" checked={showSystem} onChange={(e) => onShowSystem(e.target.checked)} />
          {t("viewer.show_system")}
        </label>
      </div>
    </header>
  );
}

function ExportButton({ session, title }: { session: HeaderProps["session"]; title: string }) {
  const [note, setNote] = useState<string | null>(null);
  // The save dialog is opened by the app itself: the window never hands it a path.
  const run = () => {
    setNote(t("common.working"));
    api.exportSession(session.keyHash, session.sessionId, session.side, title).then(
      (path) => setNote(path ? t("viewer.exported", { path }) : null),
      (e) => setNote(errorText(errorCode(e))),
    );
  };
  return (
    <span className="flex items-center gap-2">
      <button type="button" onClick={run} className="rounded-control border border-mist bg-paper-white px-3 py-1.5 text-body text-graphite hover:text-carbon-ink">
        {t("viewer.export")}
      </button>
      {note && <span className="max-w-[18rem] truncate text-caption text-ashen" title={note}>{note}</span>}
    </span>
  );
}

function MetaLines({ meta, side }: { meta: SessionMeta; side: Side }) {
  const span = meta.started_at && meta.ended_at ? `${formatDateTime(meta.started_at)} – ${formatDateTime(meta.ended_at)}` : null;
  const u = meta.usage;
  const first = [t(`viewer.side.${side}`), meta.models.join(", "), meta.version && t("viewer.version", { version: meta.version }), meta.git_branch, span];
  const second = [
    t("viewer.stats", { prompts: meta.prompts, tools: meta.tool_calls }),
    t("viewer.tokens", { input: formatCount(u.input + u.cache_creation), output: formatCount(u.output), cache: formatCount(u.cache_read) }),
  ];
  return (
    <div className="flex flex-col gap-0.5 text-caption text-ashen">
      <p>{first.filter(Boolean).join(" · ")}</p>
      <p>{second.join(" · ")}</p>
      {meta.off_branch > 0 && <p>{t("viewer.off_branch", { count: meta.off_branch })}</p>}
    </div>
  );
}
