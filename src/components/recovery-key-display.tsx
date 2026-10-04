import { useState } from "react";
import { t } from "../lib/i18n";
import { Button } from "./button";

/** The recovery key as selectable monospace text, with a copy button. */
export function RecoveryKeyDisplay({ code }: { code: string }) {
  const [copied, setCopied] = useState(false);
  const copy = () =>
    navigator.clipboard.writeText(code).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => {},
    );
  return (
    <div className="flex flex-col gap-3">
      <p className="select-all break-all rounded-control bg-soft-stone px-4 py-3 font-mono text-[15px] tracking-wide text-carbon-ink">{code}</p>
      <Button variant="secondary" className="self-start" onClick={copy}>
        {copied ? t("onboarding.recovery.copied") : t("onboarding.recovery.copy")}
      </Button>
    </div>
  );
}
