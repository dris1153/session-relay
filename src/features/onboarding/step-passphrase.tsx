import { useEffect, useState } from "react";
import { Button } from "../../components/button";
import { ErrorNote, OnboardingCard } from "../../components/onboarding-card";
import { TextField } from "../../components/text-field";
import { errorText, t, useLanguage } from "../../lib/i18n";
import { api, errorCode, type RepoRef } from "../../lib/tauri-commands";

type Mode = "create" | "unlock";
const MIN_STRENGTH = 3;

/** First machine creates the key; every other machine unlocks the published one. */
export function StepPassphrase({ initialMode, repo, onDone }: { initialMode: Mode; repo: RepoRef | null; onDone: () => Promise<void> }) {
  useLanguage();
  const [mode, setMode] = useState<Mode>(initialMode);
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  const [strength, setStrength] = useState(0);
  const [error, setError] = useState<string | null>(null); // error code, rendered in the current language
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (mode !== "create") return;
    const timer = setTimeout(() => api.passphraseStrength(passphrase).then(setStrength, () => setStrength(0)), 200);
    return () => clearTimeout(timer);
  }, [mode, passphrase]);

  const mismatch = mode === "create" && confirm.length > 0 && confirm !== passphrase;
  const ready = mode === "unlock" ? passphrase.length > 0 : strength >= MIN_STRENGTH && confirm === passphrase && acknowledged;

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await (mode === "create" ? api.createKey(passphrase) : api.unlockKey(passphrase));
      // Stay busy until the next screen replaces this one.
      await onDone();
      return;
    } catch (e) {
      const code = errorCode(e);
      if (code === "key_exists") {
        setMode("unlock");
        setPassphrase("");
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
        <TextField label={t("onboarding.passphrase.field")} type="password" autoFocus autoComplete={mode === "create" ? "new-password" : "current-password"} value={passphrase} onChange={(e) => setPassphrase(e.target.value)} hint={mode === "create" ? t("onboarding.passphrase.hint") : undefined} />
        {mode === "create" && (
          <>
            <StrengthMeter strength={strength} empty={passphrase.length === 0} />
            <TextField label={t("onboarding.passphrase.confirm")} type="password" autoComplete="new-password" value={confirm} onChange={(e) => setConfirm(e.target.value)} hint={mismatch ? t("onboarding.passphrase.mismatch") : undefined} />
            <label className="flex items-start gap-3 text-body text-graphite">
              <input type="checkbox" className="mt-1 accent-carbon-ink" checked={acknowledged} onChange={(e) => setAcknowledged(e.target.checked)} />
              {t("onboarding.passphrase.ack")}
            </label>
          </>
        )}
        <Button type="submit" className="self-start" disabled={!ready || busy}>
          {busy ? t("common.working") : t(`onboarding.passphrase.${mode}`)}
        </Button>
      </form>
    </OnboardingCard>
  );
}

function passphraseError(code: string): string {
  if (code === "key_exists") return t("onboarding.passphrase.key_exists");
  return code === "decrypt_failed" ? t("onboarding.passphrase.wrong") : errorText(code);
}

function StrengthMeter({ strength, empty }: { strength: number; empty: boolean }) {
  return (
    <div className="flex items-center gap-3" aria-live="polite">
      <div className="flex flex-1 gap-1.5">
        {[1, 2, 3, 4].map((level) => (
          <span key={level} className={`h-1.5 flex-1 rounded-full ${!empty && strength >= level ? "bg-graphite" : "bg-chalk"}`} />
        ))}
      </div>
      <span className="w-20 text-right text-caption text-ashen">{empty ? "" : t(`onboarding.passphrase.strength_${strength}`)}</span>
    </div>
  );
}
