import { useState } from "react";
import { Button } from "../../components/button";
import { FolderList, FolderRow, pickFolder } from "../../components/folder-fields";
import { ErrorNote, OnboardingCard } from "../../components/onboarding-card";
import { TextField } from "../../components/text-field";
import { errorText, t, useLanguage } from "../../lib/i18n";
import { api, errorCode, type AppState } from "../../lib/tauri-commands";

export function StepMachineAndRoots({ app, onDone }: { app: AppState; onDone: () => void }) {
  useLanguage();
  const [machineName, setMachineName] = useState(app.machine_name);
  const [claudeHome, setClaudeHome] = useState(app.claude_home);
  const [roots, setRoots] = useState<string[]>(app.workspace_roots);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

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
      <TextField label={t("settings.machine_name")} value={machineName} onChange={(e) => setMachineName(e.target.value)} />
      <FolderRow label={t("settings.claude_home")} path={claudeHome} onChange={async () => setClaudeHome((await pickFolder()) ?? claudeHome)} />
      <FolderList label={t("settings.workspace_roots")} folders={roots} onChange={setRoots} />
      <Button className="self-start" onClick={finish} disabled={busy || machineName.trim().length === 0}>
        {busy ? t("common.working") : t("onboarding.machine.finish")}
      </Button>
    </OnboardingCard>
  );
}
