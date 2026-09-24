import { useState } from "react";
import { errorText, t } from "../../lib/i18n";
import { errorCode, type Block } from "../../lib/tauri-commands";

type Tool = Extract<Block, { type: "tool" }>;
export type LoadDetail = (reference: string) => Promise<string>;

/** One tool call with its result, collapsed to `Name · what it did` until opened. */
export function ToolBlock({ tool, detail }: { tool: Tool; detail: LoadDetail }) {
  return (
    <details className="group rounded-control border border-chalk bg-paper-white">
      <summary className="flex cursor-pointer items-center gap-2 px-3 py-2 text-body">
        {/* A flex summary loses the native disclosure triangle. */}
        <span aria-hidden className="shrink-0 text-pebble transition-transform group-open:rotate-90">›</span>
        <span className="shrink-0 font-mono text-caption text-graphite">{tool.name}</span>
        <span className="min-w-0 truncate text-ashen">{tool.summary}</span>
        {tool.is_error && <span className="shrink-0 rounded-control bg-soft-stone px-1.5 text-caption text-clay">{t("viewer.tool_error")}</span>}
      </summary>
      <div className="flex flex-col gap-3 border-t border-chalk px-3 py-3">
        <Body label={t("viewer.input")} text={tool.input} truncated={tool.input_truncated} load={() => detail(`in:${tool.id}`)} />
        {tool.output === null ? (
          <p className="text-caption text-ashen">{t("viewer.no_result")}</p>
        ) : (
          <Body label={t("viewer.output")} text={tool.output} truncated={tool.output_truncated} load={() => detail(`out:${tool.id}`)} />
        )}
      </div>
    </details>
  );
}

function Body({ label, text, truncated, load }: { label: string; text: string; truncated: boolean; load: () => Promise<string> }) {
  const [full, setFull] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const showAll = () => {
    setLoading(true);
    setError(null);
    load()
      .then(setFull, (e) => setError(errorCode(e)))
      .finally(() => setLoading(false));
  };

  return (
    <div className="flex flex-col gap-1">
      <span className="text-caption font-medium uppercase tracking-wide text-pebble">{label}</span>
      <pre className="max-h-96 overflow-auto whitespace-pre-wrap break-words rounded-control bg-soft-stone p-3 font-mono text-caption text-carbon-ink">{full ?? text}</pre>
      {truncated && full === null && (
        <button type="button" onClick={showAll} disabled={loading} className="self-start rounded-control px-1 text-caption text-graphite underline decoration-mist underline-offset-4 hover:text-carbon-ink disabled:opacity-50">
          {loading ? t("common.working") : t("viewer.show_all")}
        </button>
      )}
      {error && <span className="text-caption text-clay">{errorText(error)}</span>}
    </div>
  );
}
