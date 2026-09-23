import { Button } from "./components/button";
import { LanguageSwitch } from "./components/language-switch";
import { OnboardingCard } from "./components/onboarding-card";
import { OnboardingFlow } from "./features/onboarding/onboarding-flow";
import { t } from "./lib/i18n";

export default function App() {
  return (
    <>
      <LanguageSwitch />
      <OnboardingFlow
        ready={(check, signOut) => (
          // Placeholder until the Phase 4 dashboard.
          <OnboardingCard step={4} total={4} title={t("ready.title")} lead={t("ready.lead", { owner: check.repo?.owner ?? "", name: check.repo?.name ?? "" })}>
            <Button variant="secondary" className="self-start" onClick={signOut}>
              {t("ready.logout")}
            </Button>
          </OnboardingCard>
        )}
      />
    </>
  );
}
