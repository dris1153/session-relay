import { useEffect, useId, useRef } from "react";
import { Button } from "../../components/button";
import { formatSize, relativeTime } from "../../lib/format";
import { t } from "../../lib/i18n";
import { skipReason } from "../../lib/status-copy";
import type { FileRow } from "../../lib/tauri-commands";

/** Files both machines changed: the user keeps one side per file; the other is backed up first.
 * `skipped`: why the last attempt left a file as it was (e.g. its Claude session is open). */
export function ConflictDialog({ open, files, skipped, busy, onResolve, onClose, onView }: { open: boolean; files: FileRow[]; skipped: [string, string][]; busy: boolean; onResolve: (rel: string, mode: "force_local" | "force_remote") => void; onClose: () => void; onView: (rel: string) => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();

  useEffect(() => {
    const dialog = ref.current;
    if (open && !dialog?.open) dialog?.showModal();
    if (!open && dialog?.open) dialog.close();
  }, [open]);

  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
      className="m-auto w-full max-w-[640px] rounded-card-elevated bg-paper-white p-8 text-carbon-ink backdrop:bg-carbon-ink/20"
    >
      <h2 id={titleId} className="font-serif text-heading-sm">
        {t("conflict.title")}
      </h2>
      <p className="mt-3 text-body leading-relaxed text-ashen">{t("conflict.lead")}</p>
      <ul className="mt-6 flex max-h-[50vh] flex-col divide-y divide-chalk overflow-y-auto">
        {files.length === 0 && <li className="py-3 text-body text-ashen">{t("conflict.done")}</li>}
        {files.map((file) => {
          const reason = skipped.find(([rel]) => rel === file.rel)?.[1];
          return (
          <li key={file.rel} className="flex flex-col gap-3 py-4">
            <div className="flex items-center gap-3">
              <span className="min-w-0 flex-1 truncate text-[15px] text-carbon-ink" title={file.rel}>
                {file.title ?? file.rel}
              </span>
              {/* A transcript can be read on both sides first; the viewer marks where they part. */}
              {!file.rel.includes("/") && (
                <button type="button" onClick={() => onView(file.rel)} className="shrink-0 rounded-control px-2 py-1 text-body text-graphite underline decoration-mist underline-offset-4 hover:text-carbon-ink">
                  {t("conflict.view")}
                </button>
              )}
            </div>
            {reason && <span className="rounded-control bg-soft-stone px-3 py-2 text-caption text-carbon-ink">{skipReason(reason)}</span>}
            <div className="grid grid-cols-2 gap-3">
              <Side text={t("conflict.local", { size: file.local_size == null ? "–" : formatSize(file.local_size), time: file.local_modified ? relativeTime(file.local_modified) : "–" })} action={t("conflict.keep_local")} disabled={busy} onClick={() => onResolve(file.rel, "force_local")} />
              <Side text={t("conflict.cloud", { machine: file.saved_by ?? "?", size: file.size == null ? "–" : formatSize(file.size), time: file.saved_at ? relativeTime(file.saved_at) : "–" })} action={t("conflict.keep_cloud")} disabled={busy} onClick={() => onResolve(file.rel, "force_remote")} />
            </div>
          </li>
          );
        })}
      </ul>
      <div className="mt-8 flex justify-end">
        {/* Focus here, not on the first "keep" button: Enter must not overwrite anything. */}
        <Button variant="secondary" onClick={onClose} autoFocus>
          {t("common.close")}
        </Button>
      </div>
    </dialog>
  );
}

function Side({ text, action, disabled, onClick }: { text: string; action: string; disabled: boolean; onClick: () => void }) {
  return (
    <div className="flex flex-col items-start gap-2 rounded-control border border-chalk p-3">
      <span className="text-caption text-ashen">{text}</span>
      <Button variant="secondary" className="px-3 py-1 text-body" onClick={onClick} disabled={disabled}>
        {action}
      </Button>
    </div>
  );
}
