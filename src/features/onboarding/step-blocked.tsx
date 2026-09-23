import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "../../components/button";
import { OnboardingCard } from "../../components/onboarding-card";
import { t, useLanguage } from "../../lib/i18n";

export function StepGitMissing({ version, onRetry }: { version: string | null; onRetry: () => void }) {
  useLanguage();
  const lead = version ? t("onboarding.git.lead_old", { version }) : t("onboarding.git.lead_missing");
  return (
    <OnboardingCard step={0} total={4} title={t("onboarding.git.title")} lead={lead}>
      <div className="flex flex-wrap gap-3">
        <Button onClick={() => openUrl("https://git-scm.com/download/win")}>{t("onboarding.git.download")}</Button>
        <Button variant="secondary" onClick={onRetry}>
          {t("common.retry")}
        </Button>
      </div>
    </OnboardingCard>
  );
}

export function StepNotConfigured() {
  useLanguage();
  return <OnboardingCard step={0} total={4} title={t("onboarding.config.title")} lead={t("onboarding.config.lead")} children={null} />;
}
