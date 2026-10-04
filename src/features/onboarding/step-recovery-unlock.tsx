import { useState } from "react";
import { Button } from "../../components/button";
import { ErrorNote, OnboardingCard } from "../../components/onboarding-card";
import { TextField } from "../../components/text-field";
import { errorText, t, useLanguage } from "../../lib/i18n";
import { api, errorCode, type RepoRef } from "../../lib/tauri-commands";
import { NewPassphraseFields } from "./new-passphrase-fields";

/** Unlocks with the recovery key and sets a new passphrase; the data and the key itself stay as they are. */
export function StepRecoveryUnlock({ repo, onBack, onDone }: { repo: RepoRef | null; onBack: () => void; onDone: () => Promise<void> }) {
  useLanguage();
  const [recoveryKey, setRecoveryKey] = useState("");
  const [newPassphrase, setNewPassphrase] = useState<string | null>(null); // null until acceptable
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const ready = recoveryKey.trim().length > 0 && newPassphrase !== null;

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.recoverKey(recoveryKey, newPassphrase!);
      // Stay busy until the next screen replaces this one.
      await onDone();
      return;
    } catch (e) {
      setError(errorCode(e));
    }
    setBusy(false);
  };

  const params = { owner: repo?.owner ?? "", name: repo?.name ?? "" };
  return (
    <OnboardingCard step={3} total={4} title={t("onboarding.recovery_unlock.title")} lead={t("onboarding.recovery_unlock.lead", params)}>
      <ErrorNote message={error && (error === "decrypt_failed" ? t("onboarding.recovery.wrong") : errorText(error))} />
      <form
        className="flex flex-col gap-6"
        onSubmit={(e) => {
          e.preventDefault();
          if (ready && !busy) submit();
        }}
      >
        <TextField label={t("onboarding.recovery_unlock.field")} autoFocus autoComplete="off" spellCheck={false} className="font-mono" value={recoveryKey} onChange={(e) => setRecoveryKey(e.target.value)} />
        <NewPassphraseFields label={t("onboarding.passphrase.new_field")} ack={false} focus={false} onChange={setNewPassphrase} />
        <Button type="submit" className="self-start" disabled={!ready || busy}>
          {busy ? t("common.working") : t("onboarding.recovery_unlock.submit")}
        </Button>
      </form>
      <Button variant="ghost" className="self-start px-0" onClick={onBack} disabled={busy}>
        {t("common.back")}
      </Button>
    </OnboardingCard>
  );
}
