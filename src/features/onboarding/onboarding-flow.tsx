import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "../../components/button";
import { ErrorNote, OnboardingCard } from "../../components/onboarding-card";
import { currentLanguage, errorText, setLanguage, t, useLanguage } from "../../lib/i18n";
import { api, errorCode, type AppState, type AuthChanged, type StorageCheck, type User } from "../../lib/tauri-commands";
import { StepGitMissing, StepNotConfigured } from "./step-blocked";
import { StepGithubLogin } from "./step-github-login";
import { StepMachineAndRoots } from "./step-machine-and-roots";
import { StepPassphrase } from "./step-passphrase";
import { StepStorageRepo } from "./step-storage-repo";

type View =
  | { kind: "loading" }
  | { kind: "failed"; code: string }
  | { kind: "git"; version: string | null }
  | { kind: "not_configured" }
  | { kind: "login"; failure: string | null; attempt: number }
  | { kind: "storage"; check: StorageCheck }
  | { kind: "passphrase"; check: StorageCheck }
  | { kind: "machine"; app: AppState; check: StorageCheck }
  | { kind: "ready"; user: User | null };

const SIGN_IN_AGAIN = new Set(["not_logged_in", "auth_rejected"]);

/** Routes to the first unmet precondition, re-evaluated after every step, on `auth-changed`
 * and on `storage-changed`. `recheck` lets the app ask again (e.g. a sync hit a changed store). */
export function OnboardingFlow({ ready }: { ready: (user: User | null, signOut: () => void, recheck: () => void) => React.ReactNode }) {
  useLanguage();
  const [view, setView] = useState<View>({ kind: "loading" });
  const [busy, setBusy] = useState(false);
  // Only the latest advance() may set the view; it also keys the login step per attempt.
  const generation = useRef(0);

  const advance = useCallback(async (failure: string | null = null) => {
    const mine = ++generation.current;
    const show = (next: View) => mine === generation.current && setView(next);
    const login = (reason: string | null) => show({ kind: "login", failure: reason, attempt: mine });
    const route = (check: StorageCheck) => {
      if (check.state === "needs_new_key" || check.state === "needs_unlock") return show({ kind: "passphrase", check });
      if (check.state !== "ready") return show({ kind: "storage", check });
      show({ kind: "ready", user: check.user });
    };
    setBusy(true);
    try {
      const app = await api.getAppState();
      // First run: remember the language picked from the system locale, so the tray menu matches.
      if (app.language) setLanguage(app.language);
      else api.saveSettings({ language: currentLanguage() }).catch(() => {});
      if (!app.git_ok) return show({ kind: "git", version: app.git_version });
      if (!app.has_client_id) return show({ kind: "not_configured" });
      if (!app.signed_in) return login(failure);
      if (app.identity_unlocked) {
        // The cached key is enough for the dashboard (also offline); storage is checked behind it.
        if (mine === generation.current) setView((v) => (v.kind === "ready" ? v : { kind: "ready", user: null }));
        try {
          route(await api.checkStorage());
        } catch (e) {
          if (SIGN_IN_AGAIN.has(errorCode(e))) login(errorCode(e));
        }
        return;
      }
      route(await api.checkStorage());
    } catch (e) {
      const code = errorCode(e);
      if (SIGN_IN_AGAIN.has(code)) return login(code);
      show({ kind: "failed", code });
    } finally {
      if (mine === generation.current) setBusy(false);
    }
  }, []);

  useEffect(() => {
    advance();
    const subscriptions = [
      listen<AuthChanged>("auth-changed", (event) => {
        // Approved: leave the "waiting for approval" card while storage is checked.
        if (event.payload.signed_in) setView({ kind: "loading" });
        advance(event.payload.reason);
      }),
      listen("storage-changed", () => advance()),
    ];
    return () => {
      subscriptions.forEach((s) => s.then((stop) => stop()));
    };
  }, [advance]);

  const signOut = () => api.logout().then(() => advance());

  switch (view.kind) {
    case "loading":
      return <main className="flex min-h-screen items-center justify-center text-ashen">{t("common.working")}</main>;
    case "failed":
      return (
        <OnboardingCard step={0} total={4} title={t("common.failed")}>
          <ErrorNote message={errorText(view.code)} />
          <Button className="self-start" onClick={() => advance()} disabled={busy}>
            {t("common.retry")}
          </Button>
        </OnboardingCard>
      );
    case "git":
      return <StepGitMissing version={view.version} onRetry={() => advance()} />;
    case "not_configured":
      return <StepNotConfigured />;
    case "login":
      return <StepGithubLogin key={view.attempt} failure={view.failure} />;
    case "storage":
      return <StepStorageRepo check={view.check} busy={busy} onRetry={() => advance()} />;
    case "passphrase":
      // The key was just created or unlocked, so the store is ready: no second round trip to GitHub.
      return (
        <StepPassphrase
          initialMode={view.check.state === "needs_new_key" ? "create" : "unlock"}
          repo={view.check.repo}
          onDone={async () => setView({ kind: "machine", app: await api.getAppState(), check: { ...view.check, state: "ready" } })}
        />
      );
    case "machine":
      return <StepMachineAndRoots app={view.app} onDone={() => setView({ kind: "ready", user: view.check.user })} />;
    case "ready":
      return <>{ready(view.user, signOut, () => advance())}</>;
  }
}
