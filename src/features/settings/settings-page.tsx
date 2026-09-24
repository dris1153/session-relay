import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { Button } from "../../components/button";
import { ConfirmDialog } from "../../components/confirm-dialog";
import { FolderList, FolderRow, pickFolder } from "../../components/folder-fields";
import { ErrorNote } from "../../components/onboarding-card";
import { TextField } from "../../components/text-field";
import { errorText, setLanguage, t, useLanguage, type Language } from "../../lib/i18n";
import { api, errorCode, type AppState } from "../../lib/tauri-commands";

const REVOKE_URL = "https://github.com/settings/apps/authorizations";

export function SettingsPage({ onSignOut }: { onSignOut: () => void }) {
  const language = useLanguage();
  const [app, setApp] = useState<AppState | null>(null);
  const [status, setStatus] = useState<{ error: string } | "saved" | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);

  useEffect(() => {
    api.getAppState().then(setApp, (e) => setStatus({ error: errorCode(e) }));
    // The tray can switch auto-save too: pick that up without touching unsaved edits.
    const onFocus = () => api.getAppState().then((fresh) => setApp((a) => a && { ...a, hooks: fresh.hooks }), () => {});
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  const act = async (work: () => Promise<unknown>, done: "saved" | null = "saved") => {
    setBusy(true);
    setStatus(null);
    try {
      await work();
      setStatus(done);
    } catch (e) {
      setStatus({ error: errorCode(e) });
    } finally {
      setBusy(false);
    }
  };

  if (!app) return <div className="flex-1 p-8">{status && typeof status === "object" && <ErrorNote message={errorText(status.error)} />}</div>;
  const edit = (patch: Partial<AppState>) => setApp({ ...app, ...patch });
  const save = () => act(async () => setApp(await api.saveSettings({ machine_name: app.machine_name, claude_home: app.claude_home, workspace_roots: app.workspace_roots, autostart: app.autostart })));
  const chooseLanguage = (next: Language) => {
    setLanguage(next);
    api.saveSettings({ language: next }).catch(() => {});
  };

  return (
    <article className="flex min-w-0 flex-1 flex-col gap-8 overflow-y-auto p-8">
      <h1 className="font-serif text-heading font-normal text-carbon-ink">{t("settings.title")}</h1>
      {status === "saved" && <p className="text-body text-ashen">{t("settings.saved")}</p>}
      {status && typeof status === "object" && <ErrorNote message={errorText(status.error)} />}
      <section className="flex max-w-[560px] flex-col gap-6">
        <TextField label={t("settings.machine_name")} value={app.machine_name} onChange={(e) => edit({ machine_name: e.target.value })} />
        <FolderRow label={t("settings.claude_home")} path={app.claude_home} onChange={async () => edit({ claude_home: (await pickFolder()) ?? app.claude_home })} />
        <FolderList label={t("settings.workspace_roots")} folders={app.workspace_roots} onChange={(workspace_roots) => edit({ workspace_roots })} />
        <label className="flex items-center gap-3 text-body text-graphite">
          <input type="checkbox" className="accent-carbon-ink" checked={app.autostart} onChange={(e) => edit({ autostart: e.target.checked })} />
          {t("settings.autostart")}
        </label>
        <div className="flex flex-col gap-1">
          <label className="flex items-center gap-3 text-body text-graphite">
            <input
              type="checkbox"
              className="accent-carbon-ink"
              checked={app.hooks === "installed" || app.hooks === "stale_path"}
              disabled={busy || app.hooks === "malformed"}
              onChange={(e) => act(async () => {
                const fresh = await api.setAutoSave(e.target.checked);
                setApp((a) => a && { ...a, hooks: fresh.hooks });
              })}
            />
            {t("settings.auto_save")}
          </label>
          <span className="pl-7 text-caption text-ashen">{t(app.hooks === "malformed" ? "settings.auto_save_malformed" : "settings.auto_save_hint")}</span>
        </div>
        <Button className="self-start" onClick={save} disabled={busy || app.machine_name.trim().length === 0}>
          {busy ? t("common.working") : t("settings.save")}
        </Button>
      </section>
      <section className="flex max-w-[560px] flex-col gap-3">
        <h2 className="text-caption font-medium uppercase tracking-wide text-pebble">{t("settings.language")}</h2>
        <select value={language} onChange={(e) => chooseLanguage(e.target.value as Language)} className="self-start rounded-control border border-mist bg-paper-white px-3 py-2 text-[15px] text-carbon-ink">
          <option value="vi">{t("language.vi")}</option>
          <option value="en">{t("language.en")}</option>
        </select>
      </section>
      <section className="flex max-w-[560px] flex-col gap-3">
        <h2 className="text-caption font-medium uppercase tracking-wide text-pebble">{t("settings.storage")}</h2>
        {app.repo && (
          <button type="button" onClick={() => openUrl(`https://github.com/${app.repo!.owner}/${app.repo!.name}`)} className="self-start text-body text-graphite underline decoration-chalk underline-offset-4 hover:text-carbon-ink">
            github.com/{app.repo.owner}/{app.repo.name}
          </button>
        )}
        <p className="text-body text-ashen">{t("settings.cleanup_note")}</p>
        <div className="flex flex-wrap gap-3">
          <Button variant="secondary" onClick={() => setConfirmClear(true)} disabled={busy}>
            {t("settings.clear_local")}
          </Button>
          <Button variant="secondary" onClick={onSignOut} disabled={busy}>
            {t("settings.sign_out")}
          </Button>
          <Button variant="ghost" onClick={() => openUrl(REVOKE_URL)}>
            {t("settings.revoke")}
          </Button>
        </div>
      </section>
      <ConfirmDialog
        open={confirmClear}
        title={t("settings.clear_title")}
        body={t("settings.clear_body")}
        confirm={t("settings.clear_local")}
        onConfirm={() => {
          setConfirmClear(false);
          act(api.clearLocalData, null);
        }}
        onCancel={() => setConfirmClear(false)}
      />
    </article>
  );
}
