import { useEffect, useState } from "react";
import { TextField } from "../../components/text-field";
import { t } from "../../lib/i18n";
import { api } from "../../lib/tauri-commands";

const MIN_STRENGTH = 3;

/** A new passphrase with strength meter and confirmation. Reports it once acceptable, `null` otherwise. */
export function NewPassphraseFields({ label, ack, focus, onChange }: { label: string; ack: boolean; focus: boolean; onChange: (passphrase: string | null) => void }) {
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  // Scored together with the text it was computed for, so an older score never vouches for newer input.
  const [scored, setScored] = useState({ value: "", score: 0 });
  const strength = scored.value === passphrase ? scored.score : 0;

  useEffect(() => {
    let current = true;
    const timer = setTimeout(
      () =>
        api.passphraseStrength(passphrase).then(
          (score) => current && setScored({ value: passphrase, score }),
          () => current && setScored({ value: passphrase, score: 0 }),
        ),
      200,
    );
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [passphrase]);

  const valid = strength >= MIN_STRENGTH && confirm === passphrase && (acknowledged || !ack);
  useEffect(() => onChange(valid ? passphrase : null), [valid, passphrase]);

  const mismatch = confirm.length > 0 && confirm !== passphrase;
  return (
    <>
      <TextField label={label} type="password" autoFocus={focus} autoComplete="new-password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} hint={t("onboarding.passphrase.hint")} />
      <StrengthMeter strength={strength} empty={passphrase.length === 0} />
      <TextField label={t("onboarding.passphrase.confirm")} type="password" autoComplete="new-password" value={confirm} onChange={(e) => setConfirm(e.target.value)} hint={mismatch ? t("onboarding.passphrase.mismatch") : undefined} />
      {ack && (
        <label className="flex items-start gap-3 text-body text-graphite">
          <input type="checkbox" className="mt-1 accent-carbon-ink" checked={acknowledged} onChange={(e) => setAcknowledged(e.target.checked)} />
          {t("onboarding.passphrase.ack")}
        </label>
      )}
    </>
  );
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
