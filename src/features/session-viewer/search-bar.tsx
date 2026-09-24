import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { clearHighlight } from "../../lib/scroll-to-item";
import { errorText, t } from "../../lib/i18n";
import { errorCode, type Hit } from "../../lib/tauri-commands";

/** Search over the whole session, run in Rust (outputs past the preview included). Enter moves
 *  to the next match, Shift+Enter to the previous one. */
export function SearchBar({ search, onGo }: { search: (query: string) => Promise<Hit[]>; onGo: (hit: Hit, query: string) => void }) {
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<{ query: string; hits: Hit[]; at: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Only the newest search may set results: an older, slower one would point at stale matches.
  const latest = useRef(0);

  // Another scope (system events shown or hidden) means other matches.
  useEffect(() => setResult(null), [search]);

  const step = (delta: number) => {
    if (!query.trim()) {
      clearHighlight();
      return setResult(null);
    }
    if (result?.query === query) {
      if (result.hits.length === 0) return;
      const at = (result.at + delta + result.hits.length) % result.hits.length;
      setResult({ ...result, at });
      return onGo(result.hits[at], query);
    }
    setError(null);
    const request = ++latest.current;
    search(query).then(
      (hits) => {
        if (request !== latest.current) return;
        setResult({ query, hits, at: 0 });
        if (hits.length > 0) onGo(hits[0], query);
      },
      (e) => request === latest.current && setError(errorCode(e)),
    );
  };

  const onKey = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    if (e.key !== "Enter") return;
    e.preventDefault();
    step(e.shiftKey ? -1 : 1);
  };

  const fresh = result?.query === query;
  return (
    <div className="flex items-center gap-2">
      <input
        type="search"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={onKey}
        placeholder={t("viewer.search_placeholder")}
        aria-label={t("viewer.search_placeholder")}
        className="w-56 rounded-control border border-mist bg-paper-white px-3 py-1.5 text-body text-carbon-ink"
      />
      {fresh && <span className="text-caption text-ashen">{result.hits.length === 0 ? t("viewer.search_none") : `${result.at + 1} / ${result.hits.length}`}</span>}
      {fresh && result.hits.length > 1 && (
        <>
          <button type="button" onClick={() => step(-1)} aria-label={t("viewer.search_prev")} className="rounded-control px-1.5 text-body text-graphite hover:text-carbon-ink">
            ↑
          </button>
          <button type="button" onClick={() => step(1)} aria-label={t("viewer.search_next")} className="rounded-control px-1.5 text-body text-graphite hover:text-carbon-ink">
            ↓
          </button>
        </>
      )}
      {error && <span className="text-caption text-clay">{errorText(error)}</span>}
    </div>
  );
}
