import { useState } from "react";
import { Button } from "../../components/button";
import { ErrorNote, OnboardingCard } from "../../components/onboarding-card";
import { TextField } from "../../components/text-field";
import { errorText, t, useLanguage } from "../../lib/i18n";
import { api, errorCode, type RepoRef } from "../../lib/tauri-commands";
import { NewPassphraseFields } from "./new-passphrase-fields";

/** Replaces the cloud copy with a new store under a new key. The user must type the repository name first. */
export function StepStartOver({ repo, onBack, onDone }: { repo: RepoRef | null; onBack: () => void; onDone: (recoveryKey: string) => Promise<void> }) {
  useLanguage();
  const [typed, setTyped] = useState("");
  const [newPassphrase, setNewPassphrase] = useState<string | null>(null); // null until acceptable
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const repoName = repo ? `${repo.owner}/${repo.name}` : "";
  const ready = repoName !== "" && typed.trim() === repoName && newPassphrase !== null;

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      const recoveryKey = await api.resetStore(newPassphrase!);
      // Stay busy until the next screen replaces this one.
      await onDone(recoveryKey);
      return;
    } catch (e) {
      setError(errorCode(e));
    }
    setBusy(false);
  };

  return (
    <OnboardingCard step={3} total={4} title={t("onboarding.start_over.title")} lead={t("onboarding.start_over.warning", { repo: repoName })}>
      <ErrorNote message={error && errorText(error)} />
      <form
        className="flex flex-col gap-6"
        onSubmit={(e) => {
          e.preventDefault();
          if (ready && !busy) submit();
        }}
      >
        <TextField label={t("onboarding.start_over.confirm_field", { repo: repoName })} autoFocus autoComplete="off" spellCheck={false} value={typed} onChange={(e) => setTyped(e.target.value)} />
        <NewPassphraseFields label={t("onboarding.passphrase.new_field")} ack focus={false} onChange={setNewPassphrase} />
        <Button type="submit" className="self-start" disabled={!ready || busy}>
          {busy ? t("common.working") : t("onboarding.start_over.submit")}
        </Button>
      </form>
      <Button variant="ghost" className="self-start px-0" onClick={onBack} disabled={busy}>
        {t("common.back")}
      </Button>
    </OnboardingCard>
  );
}
