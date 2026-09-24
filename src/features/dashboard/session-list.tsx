import { formatSize, relativeTime } from "../../lib/format";
import type { SessionRow } from "../../lib/sessions";
import { t } from "../../lib/i18n";

/** One row per Claude session (transcript + subagents + tool results) and one for memory. */
export function SessionList({ sessions, disabled, onRestore, onDelete }: { sessions: SessionRow[]; disabled: boolean; onRestore: (row: SessionRow) => void; onDelete: (row: SessionRow) => void }) {
  if (sessions.length === 0) return <p className="text-body text-ashen">{t("sessions.empty")}</p>;
  return (
    <ul className="flex flex-col divide-y divide-chalk">
      {sessions.map((row) => {
        const inCloud = row.id !== "memory" && row.state !== "local_only" && row.state !== "remote_deleted";
        return (
          <li key={row.id} className="flex items-center gap-4 py-3">
            <div className="min-w-0 flex-1">
              <p className="truncate text-[15px] text-carbon-ink" title={row.title ?? row.id}>
                {row.id === "memory" ? t("sessions.memory") : (row.title ?? t("sessions.untitled", { id: row.id.slice(0, 8) }))}
              </p>
              <p className="text-caption text-ashen">
                {[t(`file_state.${row.state}`), row.savedAt && relativeTime(row.savedAt), row.savedBy, row.size > 0 && formatSize(row.size)].filter(Boolean).join(" · ")}
              </p>
            </div>
            {row.state === "local_deleted" && (
              <button type="button" disabled={disabled} onClick={() => onRestore(row)} className="shrink-0 rounded-control px-2 py-1 text-body font-medium text-graphite hover:text-carbon-ink disabled:opacity-50">
                {t("sessions.restore")}
              </button>
            )}
            {inCloud && (
              <button type="button" disabled={disabled} onClick={() => onDelete(row)} className="shrink-0 rounded-control px-2 py-1 text-body text-ashen hover:text-carbon-ink disabled:opacity-50">
                {t("sessions.delete_cloud")}
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );
}
