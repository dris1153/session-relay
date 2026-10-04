import { useState } from "react";
import { Button } from "../../components/button";
import { ConfirmDialog } from "../../components/confirm-dialog";
import { ErrorNote } from "../../components/onboarding-card";
import { RecoveryKeyDisplay } from "../../components/recovery-key-display";
import { errorText, t, useLanguage } from "../../lib/i18n";
import { api, errorCode } from "../../lib/tauri-commands";
import { NewPassphraseFields } from "../onboarding/new-passphrase-fields";

type Panel = "none" | "passphrase" | "recovery_key";

// Outlives the component: leaving Settings while the key is being created must not lose a key
// that is already the only valid one. Cleared once the user confirms they saved it.
let unseenRecoveryKey: string | null = null;

/** Change the passphrase or replace the recovery key of the unlocked storage key. */
export function KeySettings() {
  useLanguage();
  const [panel, setPanel] = useState<Panel>(unseenRecoveryKey ? "recovery_key" : "none");
  const [newPassphrase, setNewPassphrase] = useState<string | null>(null); // null until acceptable
  const [recoveryKey, setRecoveryKey] = useState(unseenRecoveryKey ?? "");
  const [saved, setSaved] = useState(false);
  const [confirmNew, setConfirmNew] = useState(false);
  const [status, setStatus] = useState<{ error: string } | "changed" | null>(null);
  const [busy, setBusy] = useState(false);

  const act = async (work: () => Promise<void>) => {
    setBusy(true);
    setStatus(null);
    try {
      await work();
    } catch (e) {
      setStatus({ error: errorCode(e) });
    } finally {
      setBusy(false);
    }
  };

  const savePassphrase = () =>
    act(async () => {
      await api.changePassphrase(newPassphrase!);
      setPanel("none");
      setStatus("changed");
    });

  const createRecoveryKey = () =>
    act(async () => {
      unseenRecoveryKey = await api.newRecoveryKey();
      setRecoveryKey(unseenRecoveryKey);
      setPanel("recovery_key");
    });

  return (
    <section className="flex max-w-[560px] flex-col gap-3">
      <h2 className="text-caption font-medium uppercase tracking-wide text-pebble">{t("settings.encryption")}</h2>
      <p className="text-body text-ashen">{t("settings.encryption_note")}</p>
      {status === "changed" && <p className="text-body text-ashen">{t("settings.passphrase_changed")}</p>}
      {status && typeof status === "object" && <ErrorNote message={errorText(status.error)} />}
      {panel === "none" && (
        <div className="flex flex-wrap gap-3">
          <Button variant="secondary" onClick={() => setPanel("passphrase")} disabled={busy}>
            {t("settings.change_passphrase")}
          </Button>
          <Button variant="secondary" onClick={() => setConfirmNew(true)} disabled={busy}>
            {busy ? t("common.working") : t("settings.new_recovery_key")}
          </Button>
        </div>
      )}
      {panel === "passphrase" && (
        <div className="flex flex-col gap-6">
          <NewPassphraseFields label={t("onboarding.passphrase.new_field")} ack={false} focus onChange={setNewPassphrase} />
          <div className="flex gap-3">
            <Button onClick={savePassphrase} disabled={newPassphrase === null || busy}>
              {busy ? t("common.working") : t("settings.change_passphrase_save")}
            </Button>
            <Button variant="ghost" onClick={() => setPanel("none")} disabled={busy}>
              {t("common.cancel")}
            </Button>
          </div>
        </div>
      )}
      {panel === "recovery_key" && (
        <div className="flex flex-col gap-4">
          <p className="text-body text-ashen">{t("settings.new_recovery_shown")}</p>
          <RecoveryKeyDisplay code={recoveryKey} />
          <label className="flex items-start gap-3 text-body text-graphite">
            <input type="checkbox" className="mt-1 accent-carbon-ink" checked={saved} onChange={(e) => setSaved(e.target.checked)} />
            {t("onboarding.recovery.ack")}
          </label>
          <Button
            className="self-start"
            disabled={!saved}
            onClick={() => {
              unseenRecoveryKey = null;
              setRecoveryKey("");
              setSaved(false);
              setPanel("none");
            }}
          >
            {t("common.close")}
          </Button>
        </div>
      )}
      <ConfirmDialog
        open={confirmNew}
        title={t("settings.new_recovery_title")}
        body={t("settings.new_recovery_body")}
        confirm={t("settings.new_recovery_key")}
        onConfirm={() => {
          setConfirmNew(false);
          createRecoveryKey();
        }}
        onCancel={() => setConfirmNew(false)}
      />
    </section>
  );
}
