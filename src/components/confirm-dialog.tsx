import { useEffect, useId, useRef } from "react";
import { t } from "../lib/i18n";
import { Button } from "./button";

/** Native modal dialog: focus trap and Escape come from the browser. */
export function ConfirmDialog({ open, title, body, confirm, onConfirm, onCancel }: { open: boolean; title: string; body: string; confirm: string; onConfirm: () => void; onCancel: () => void }) {
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
        onCancel();
      }}
      className="m-auto w-full max-w-[440px] rounded-card-elevated bg-paper-white p-8 text-carbon-ink backdrop:bg-carbon-ink/20"
    >
      <h2 id={titleId} className="font-serif text-heading-sm">
        {title}
      </h2>
      <p className="mt-3 text-body leading-relaxed text-ashen">{body}</p>
      <div className="mt-8 flex justify-end gap-3">
        <Button variant="secondary" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
        <Button onClick={onConfirm}>{confirm}</Button>
      </div>
    </dialog>
  );
}
