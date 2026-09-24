import { t } from "../../lib/i18n";
import type { FileDiff } from "../../lib/tauri-commands";

/** Monochrome (DESIGN.md): additions on Soft Stone, removals in Ashen; the +/- sign carries the meaning. */
const LINE: Record<string, string> = { "+": "bg-soft-stone text-carbon-ink", "-": "text-ashen" };

export function DiffView({ files, truncated }: { files: FileDiff[]; truncated: boolean }) {
  return (
    <div className="flex flex-col gap-2">
      {files.map((file, i) => (
        <div key={i} className="overflow-hidden rounded-control border border-chalk">
          <p className="truncate border-b border-chalk bg-soft-stone px-3 py-1 font-mono text-caption text-graphite" title={file.path}>
            {file.path}
          </p>
          <div className="max-h-96 overflow-auto py-1 font-mono text-caption leading-relaxed">
            {file.hunks.map((hunk, j) => (
              <div key={j} className="min-w-max">
                <div className="px-3 text-pebble">
                  @@ −{hunk.old_start} +{hunk.new_start} @@
                </div>
                {hunk.lines.map((line, k) => (
                  <div key={k} className={`whitespace-pre px-3 ${LINE[line[0]] ?? "text-graphite"}`}>
                    {line || " "}
                  </div>
                ))}
              </div>
            ))}
          </div>
        </div>
      ))}
      {truncated && <p className="text-caption text-ashen">{t("viewer.diff_truncated")}</p>}
    </div>
  );
}
