import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { Button } from "../../components/button";
import { ErrorNote, OnboardingCard } from "../../components/onboarding-card";
import { TextField } from "../../components/text-field";
import { errorText, t, useLanguage } from "../../lib/i18n";
import { api, errorCode, type AppState } from "../../lib/tauri-commands";

async function pickFolder(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false });
  return typeof picked === "string" ? picked : null;
}

export function StepMachineAndRoots({ app, onDone }: { app: AppState; onDone: () => void }) {
  useLanguage();
  const [machineName, setMachineName] = useState(app.machine_name);
  const [claudeHome, setClaudeHome] = useState(app.claude_home);
  const [roots, setRoots] = useState<string[]>(app.workspace_roots);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const addRoot = async () => {
    const folder = await pickFolder();
    if (folder && !roots.includes(folder)) setRoots([...roots, folder]);
  };

  const finish = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.saveSettings({ machine_name: machineName, claude_home: claudeHome, workspace_roots: roots });
      onDone(); // stays busy: this screen is replaced right away
    } catch (e) {
      setError(errorCode(e));
      setBusy(false);
    }
  };

  return (
    <OnboardingCard step={4} total={4} title={t("onboarding.machine.title")} lead={t("onboarding.machine.lead")}>
      <ErrorNote message={error && errorText(error)} />
      <TextField label={t("onboarding.machine.name")} value={machineName} onChange={(e) => setMachineName(e.target.value)} />
      <FolderRow label={t("onboarding.machine.claude_home")} path={claudeHome} onChange={async () => setClaudeHome((await pickFolder()) ?? claudeHome)} />
      <div className="flex flex-col gap-2">
        <span className="text-body font-medium text-graphite">{t("onboarding.machine.roots")}</span>
        {roots.length === 0 && <span className="text-body text-ashen">{t("onboarding.machine.roots_empty")}</span>}
        {roots.map((root) => (
          <div key={root} className="flex items-center justify-between gap-3 rounded-control bg-soft-stone px-3 py-2">
            <span className="truncate font-mono text-body text-carbon-ink" title={root}>
              {root}
            </span>
            <Button variant="ghost" className="px-2 py-0" onClick={() => setRoots(roots.filter((r) => r !== root))}>
              {t("onboarding.machine.remove")}
            </Button>
          </div>
        ))}
        <Button variant="secondary" className="self-start" onClick={addRoot}>
          {t("onboarding.machine.add_root")}
        </Button>
      </div>
      <Button className="self-start" onClick={finish} disabled={busy || machineName.trim().length === 0}>
        {busy ? t("common.working") : t("onboarding.machine.finish")}
      </Button>
    </OnboardingCard>
  );
}

function FolderRow({ label, path, onChange }: { label: string; path: string; onChange: () => void }) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-body font-medium text-graphite">{label}</span>
      <div className="flex items-center justify-between gap-3 rounded-control border border-mist px-3 py-2">
        <span className="truncate font-mono text-body text-carbon-ink" title={path}>
          {path}
        </span>
        <Button variant="ghost" className="px-2 py-0" onClick={onChange}>
          {t("onboarding.machine.change")}
        </Button>
      </div>
    </div>
  );
}
