import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { Button } from "../../components/button";
import { ErrorNote, OnboardingCard } from "../../components/onboarding-card";
import { errorText, t, useLanguage } from "../../lib/i18n";
import { api, errorCode, type LoginCode } from "../../lib/tauri-commands";

/** GitHub device flow: show the code, open github.com/login/device, wait for `auth-changed`.
 * Remounted (via `key`) for every failed attempt, so the same failure twice still resets it. */
export function StepGithubLogin({ failure }: { failure: string | null }) {
  useLanguage();
  const [code, setCode] = useState<LoginCode | null>(null);
  const [error, setError] = useState<string | null>(failure); // error code, rendered in the current language
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);

  const requestCode = async () => {
    setBusy(true);
    setError(null);
    setCopied(false);
    try {
      setCode(await api.startLogin());
    } catch (e) {
      setError(errorCode(e));
    } finally {
      setBusy(false);
    }
  };

  const copy = () =>
    code &&
    navigator.clipboard.writeText(code.user_code).then(
      () => setCopied(true),
      () => setCopied(false),
    );

  return (
    <OnboardingCard step={1} total={4} title={t("onboarding.login.title")} lead={t("onboarding.login.lead")}>
      <ErrorNote message={error && failureText(error)} />
      {code ? (
        <>
          <div className="flex flex-col gap-3 rounded-card bg-soft-stone p-6">
            <span className="text-body text-graphite">{t("onboarding.login.enter_code")}</span>
            <span className="select-all font-mono text-[32px] font-medium tracking-[0.2em] text-carbon-ink">{code.user_code}</span>
          </div>
          <div className="flex flex-wrap gap-3">
            <Button onClick={() => openUrl(code.verification_uri)}>{t("onboarding.login.open")}</Button>
            <Button variant="secondary" onClick={copy} aria-live="polite">
              {copied ? t("onboarding.login.copied") : t("onboarding.login.copy")}
            </Button>
          </div>
          <p className="flex items-center gap-2 text-body text-ashen">
            <span className="h-2 w-2 animate-pulse rounded-full bg-clay" aria-hidden="true" />
            {t("onboarding.login.waiting")}
          </p>
          <Button variant="ghost" className="self-start px-0" onClick={requestCode} disabled={busy}>
            {t("onboarding.login.new_code")}
          </Button>
        </>
      ) : (
        <Button className="self-start" onClick={requestCode} disabled={busy}>
          {busy ? t("common.working") : failure ? t("onboarding.login.new_code") : t("onboarding.login.get_code")}
        </Button>
      )}
    </OnboardingCard>
  );
}

/** Login-specific wording where there is one, else the generic error text. */
function failureText(code: string): string {
  const key = `onboarding.login.${code}`;
  return t(key) === key ? errorText(code) : t(key);
}
