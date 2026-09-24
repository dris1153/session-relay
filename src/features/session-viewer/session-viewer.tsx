import { useMemo, useState } from "react";
import { Button } from "../../components/button";
import { ErrorNote } from "../../components/onboarding-card";
import { formatCount, formatDateTime } from "../../lib/format";
import { errorText, t, useLanguage } from "../../lib/i18n";
import type { SessionMeta, Side } from "../../lib/tauri-commands";
import { useSessionView } from "../../lib/use-session-view";
import { Conversation } from "./conversation";
import { isShown } from "./message-item";
import { ShowSystem } from "./show-system";

export type ViewedSession = { keyHash: string; sessionId: string; side: Side };

/** The conversation of one session, shown in place of the project pane. */
export function SessionViewer({ session, onBack }: { session: ViewedSession; onBack: () => void }) {
  useLanguage();
  const { view, error, detail, retry } = useSessionView(session.keyHash, session.sessionId, session.side);
  const [showSystem, setShowSystem] = useState(false);
  // Keyed by position in the whole list, so toggling system events keeps opened blocks open.
  const entries = useMemo(() => view?.items.map((item, index) => ({ item, index })).filter(({ item }) => showSystem || isShown(item)) ?? [], [view, showSystem]);

  return (
    <article className="flex min-w-0 flex-1 flex-col">
      <header className="flex flex-col gap-2 border-b border-chalk px-8 py-5">
        <div className="flex items-center gap-3">
          <button type="button" onClick={onBack} autoFocus className="shrink-0 rounded-control px-2 py-1 text-body text-graphite hover:text-carbon-ink">
            <span aria-hidden>← </span>
            {t("viewer.back")}
          </button>
          <h1 className="min-w-0 truncate font-serif text-heading-sm text-carbon-ink">{view?.meta.title ?? session.sessionId}</h1>
        </div>
        {view && <MetaLines meta={view.meta} side={view.side} />}
        <label className="flex items-center gap-2 self-start text-caption text-graphite">
          <input type="checkbox" className="accent-carbon-ink" checked={showSystem} onChange={(e) => setShowSystem(e.target.checked)} />
          {t("viewer.show_system")}
        </label>
      </header>
      {error ? (
        <div className="flex flex-col items-start gap-3 px-8 py-6">
          <ErrorNote message={errorText(error)} />
          <Button variant="secondary" onClick={retry}>
            {t("common.retry")}
          </Button>
        </div>
      ) : !view ? (
        <p className="px-8 py-6 text-body text-ashen">{t("viewer.loading")}</p>
      ) : entries.length === 0 ? (
        <p className="px-8 py-6 text-body text-ashen">{t("viewer.empty")}</p>
      ) : (
        <ShowSystem.Provider value={showSystem}>
          <Conversation entries={entries} detail={detail} />
        </ShowSystem.Provider>
      )}
    </article>
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
