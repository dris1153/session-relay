import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "../../components/button";
import { OnboardingCard } from "../../components/onboarding-card";
import { t, useLanguage } from "../../lib/i18n";
import type { StorageCheck } from "../../lib/tauri-commands";

/** Guides the one-time repo creation + app installation, or explains why the repo is unusable. */
export function StepStorageRepo({ check, busy, onRetry }: { check: StorageCheck; busy: boolean; onRetry: () => void }) {
  useLanguage();
  const name = check.repo?.name ?? "claude-sessions";
  const params = { name, owner: check.repo?.owner ?? check.user.login };
  const needsSetup = check.state === "no_installation" || check.state === "repo_missing";

  return (
    <OnboardingCard step={2} total={4} title={t("onboarding.storage.title")} lead={t(`onboarding.storage.${check.state}`, params)}>
      {needsSetup && (
        <ol className="flex flex-col gap-4">
          <SetupItem index={1} text={t("onboarding.storage.step_create", params)} action={t("onboarding.storage.create")} onAction={() => openUrl(check.create_repo_url)} />
          <SetupItem index={2} text={t("onboarding.storage.step_install", params)} action={t("onboarding.storage.install")} onAction={() => openUrl(check.install_url)} />
        </ol>
      )}
      {check.state === "repo_public" && check.repo && (
        <Button variant="secondary" className="self-start" onClick={() => openUrl(`https://github.com/${check.repo!.owner}/${check.repo!.name}/settings`)}>
          {t("onboarding.storage.open_settings")}
        </Button>
      )}
      <Button className="self-start" onClick={onRetry} disabled={busy}>
        {busy ? t("common.working") : t("common.retry")}
      </Button>
    </OnboardingCard>
  );
}

function SetupItem({ index, text, action, onAction }: { index: number; text: string; action: string; onAction: () => void }) {
  return (
    <li className="flex items-start justify-between gap-4 rounded-card bg-soft-stone p-4">
      <span className="flex gap-3 text-[15px] text-graphite">
        <span className="font-medium text-clay">{index}</span>
        {text}
      </span>
      <Button variant="secondary" className="shrink-0" onClick={onAction}>
        {action}
      </Button>
    </li>
  );
}
