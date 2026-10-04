import { useState } from "react";
import { Button } from "../../components/button";
import { ErrorNote, OnboardingCard } from "../../components/onboarding-card";
import { RecoveryKeyDisplay } from "../../components/recovery-key-display";
import { errorText, t, useLanguage } from "../../lib/i18n";
import { errorCode } from "../../lib/tauri-commands";

/** Shows the new recovery key once; the user cannot continue before confirming they saved it. */
export function StepRecoveryKey({ code, onDone }: { code: string; onDone: () => Promise<void> }) {
  useLanguage();
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const next = async () => {
    setBusy(true);
    setError(null);
    try {
      await onDone();
    } catch (e) {
      setError(errorCode(e));
      setBusy(false);
    }
  };

  return (
    <OnboardingCard step={3} total={4} title={t("onboarding.recovery.title")} lead={t("onboarding.recovery.lead")}>
      <ErrorNote message={error && errorText(error)} />
      <RecoveryKeyDisplay code={code} />
      <label className="flex items-start gap-3 text-body text-graphite">
        <input type="checkbox" className="mt-1 accent-carbon-ink" checked={acknowledged} onChange={(e) => setAcknowledged(e.target.checked)} />
        {t("onboarding.recovery.ack")}
      </label>
      <Button className="self-start" disabled={!acknowledged || busy} onClick={next}>
        {busy ? t("common.working") : t("onboarding.recovery.continue")}
      </Button>
    </OnboardingCard>
  );
}
