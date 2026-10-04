import { useState } from "react";
import { Button } from "../../components/button";
import { ErrorNote, OnboardingCard } from "../../components/onboarding-card";
import { TextField } from "../../components/text-field";
import { errorText, t, useLanguage } from "../../lib/i18n";
import { api, errorCode, type RepoRef } from "../../lib/tauri-commands";
import { NewPassphraseFields } from "./new-passphrase-fields";
import { StepRecoveryUnlock } from "./step-recovery-unlock";
import { StepStartOver } from "./step-start-over";

type Mode = "create" | "unlock" | "recover" | "start_over";

/** First machine creates the key; every other machine unlocks the published one. */
export function StepPassphrase({ initialMode, repo, onDone }: { initialMode: "create" | "unlock"; repo: RepoRef | null; onDone: (recoveryKey?: string) => Promise<void> }) {
  useLanguage();
  const [mode, setMode] = useState<Mode>(initialMode);
  const [passphrase, setPassphrase] = useState(""); // unlock mode
  const [newPassphrase, setNewPassphrase] = useState<string | null>(null); // create mode, null until acceptable
  const [error, setError] = useState<string | null>(null); // error code, rendered in the current language
  const [busy, setBusy] = useState(false);

  if (mode === "recover") return <StepRecoveryUnlock repo={repo} onBack={() => setMode("unlock")} onDone={onDone} />;
  if (mode === "start_over") return <StepStartOver repo={repo} onBack={() => setMode("unlock")} onDone={onDone} />;

  const ready = mode === "unlock" ? passphrase.length > 0 : newPassphrase !== null;

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      let recoveryKey: string | undefined;
      if (mode === "create") recoveryKey = await api.createKey(newPassphrase!);
      else await api.unlockKey(passphrase);
      // Stay busy until the next screen replaces this one.
      await onDone(recoveryKey);
      return;
    } catch (e) {
      const code = errorCode(e);
      if (code === "key_exists") {
        setMode("unlock");
        setPassphrase("");
        setNewPassphrase(null);
      }
      setError(code);
    }
    setBusy(false);
  };

  const params = { owner: repo?.owner ?? "", name: repo?.name ?? "" };
  return (
    <OnboardingCard step={3} total={4} title={t(`onboarding.passphrase.${mode}_title`)} lead={t(`onboarding.passphrase.${mode}_lead`, params)}>
      <ErrorNote message={error && passphraseError(error)} />
      <form
        className="flex flex-col gap-6"
        onSubmit={(e) => {
          e.preventDefault();
          if (ready && !busy) submit();
        }}
      >
        {mode === "create" ? (
          <NewPassphraseFields label={t("onboarding.passphrase.field")} ack focus onChange={setNewPassphrase} />
        ) : (
          <TextField label={t("onboarding.passphrase.field")} type="password" autoFocus autoComplete="current-password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} />
        )}
        <Button type="submit" className="self-start" disabled={!ready || busy}>
          {busy ? t("common.working") : t(`onboarding.passphrase.${mode}`)}
        </Button>
      </form>
      {mode === "unlock" && (
        <div className="flex flex-col items-start gap-1">
          <Button variant="ghost" className="px-0" onClick={() => setMode("recover")} disabled={busy}>
            {t("onboarding.passphrase.use_recovery")}
          </Button>
          <Button variant="ghost" className="px-0" onClick={() => setMode("start_over")} disabled={busy}>
            {t("onboarding.passphrase.start_over")}
          </Button>
        </div>
      )}
    </OnboardingCard>
  );
}

function passphraseError(code: string): string {
  if (code === "key_exists") return t("onboarding.passphrase.key_exists");
  return code === "decrypt_failed" ? t("onboarding.passphrase.wrong") : errorText(code);
}
