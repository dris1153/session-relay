import { useState } from "react";
import { errorText, t } from "../../lib/i18n";
import { errorCode } from "../../lib/tauri-commands";
import type { LoadDetail } from "./tool-block";

/** Images are fetched only when asked for: a session full of screenshots stays light. */
export function ImageThumbs({ refs, detail }: { refs: string[]; detail: LoadDetail }) {
  const [uris, setUris] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [enlarged, setEnlarged] = useState<number | null>(null);
  if (refs.length === 0) return null;

  const load = () =>
    Promise.all(refs.map((ref) => detail(ref))).then(
      (details) => setUris(details.flatMap((d) => (d.kind === "image" ? [d.data_uri] : []))),
      (e) => setError(errorCode(e)),
    );

  if (!uris) {
    return (
      <div className="flex items-center gap-3">
        <button type="button" onClick={load} className="self-start rounded-control px-1 text-caption text-graphite underline decoration-mist underline-offset-4 hover:text-carbon-ink">
          {t("viewer.images", { count: refs.length })}
        </button>
        {error && <span className="text-caption text-clay">{errorText(error)}</span>}
      </div>
    );
  }
  return (
    <div className="flex flex-wrap gap-2">
      {uris.map((uri, i) => (
        <button key={i} type="button" onClick={() => setEnlarged(enlarged === i ? null : i)} className="rounded-control">
          <img src={uri} alt={t("viewer.image", { n: i + 1 })} className={enlarged === i ? "max-w-full rounded-control" : "max-h-32 rounded-control border border-chalk"} />
        </button>
      ))}
    </div>
  );
}
