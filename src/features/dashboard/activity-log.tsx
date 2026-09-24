import { useEffect, useState } from "react";
import { relativeTime } from "../../lib/format";
import { errorText, t } from "../../lib/i18n";
import { api, type Activity } from "../../lib/tauri-commands";

/** Last saves and restores of one project, from this machine's activity log. `version` re-reads it. */
export function ActivityLog({ keyHash, version }: { keyHash: string; version: unknown }) {
  const [entries, setEntries] = useState<Activity[]>([]);

  useEffect(() => {
    let current = true;
    api.projectActivity(keyHash).then(
      (list) => current && setEntries(list),
      () => current && setEntries([]),
    );
    return () => {
      current = false;
    };
  }, [keyHash, version]);

  if (entries.length === 0) return <p className="text-body text-ashen">{t("activity.empty")}</p>;
  return (
    <ul className="flex flex-col gap-2">
      {entries.map((a) => (
        <li key={`${a.ts}-${a.action}`} className="flex justify-between gap-4 text-body">
          <span className="text-graphite">
            {t(`activity.${a.source}`)} · {a.action === "DeleteRemote" ? t("activity.delete_remote") : t("activity.counts", { pushed: a.pushed, pulled: a.pulled })}
            {a.result !== "ok" && <span className="text-carbon-ink"> · {errorText(a.result)}</span>}
          </span>
          <span className="shrink-0 text-ashen">{relativeTime(a.ts)}</span>
        </li>
      ))}
    </ul>
  );
}
