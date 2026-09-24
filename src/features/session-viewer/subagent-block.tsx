import { useCallback, useContext, useState } from "react";
import { errorText, t } from "../../lib/i18n";
import { errorCode, type AgentRef, type Item } from "../../lib/tauri-commands";
import { isShown, MessageItem } from "./message-item";
import { ShowSystem } from "./show-system";
import type { LoadDetail } from "./tool-block";

/** The conversation a subagent had on its own, loaded when opened; nested agents open the same way. */
export function SubagentBlock({ agent, detail }: { agent: AgentRef; detail: LoadDetail }) {
  const [items, setItems] = useState<Item[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const showSystem = useContext(ShowSystem);
  // Refs inside the agent resolve in the agent's own transcript.
  const nested = useCallback<LoadDetail>((ref) => detail(`agent:${agent.id}/${ref}`), [detail, agent.id]);

  const open = () => {
    setLoading(true);
    setError(null);
    detail(`agent:${agent.id}`)
      .then((d) => setItems(d.kind === "session" ? d.items : []), (e) => setError(errorCode(e)))
      .finally(() => setLoading(false));
  };

  if (!items) {
    return (
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-caption text-ashen">{[agent.kind, agent.description].filter(Boolean).join(" · ")}</span>
        <button type="button" onClick={open} disabled={loading} className="rounded-control px-1 text-caption text-graphite underline decoration-mist underline-offset-4 hover:text-carbon-ink disabled:opacity-50">
          {loading ? t("common.working") : t("viewer.open_agent")}
        </button>
        {error && <span className="text-caption text-clay">{errorText(error)}</span>}
      </div>
    );
  }
  return (
    <ol className="flex flex-col gap-5 border-l-2 border-chalk pl-4">
      {items.map((item, index) => (showSystem || isShown(item) ? <MessageItem key={index} item={item} detail={nested} /> : null))}
    </ol>
  );
}
