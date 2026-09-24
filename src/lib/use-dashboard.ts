import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorCode, type Dashboard, type Progress } from "./tauri-commands";

/** Errors that mean the storage itself changed: onboarding has to look again. */
const STORAGE_ERRORS = new Set(["not_logged_in", "auth_rejected", "wrong_identity", "store_reset"]);
const FOCUS_RELOAD_GAP_MS = 5000;
const BUSY_RETRY_MS = 3000;

const pause = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** Project list kept current from `projects-changed`, plus one-action-at-a-time bookkeeping. */
export function useDashboard(onStorageProblem: () => void) {
  const [data, setData] = useState<Dashboard | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  // Read synchronously by event handlers and to refuse a second action while one runs.
  const running = useRef<string | null>(null);
  const lastLoad = useRef(0);
  const storageProblem = useRef(onStorageProblem);
  useEffect(() => {
    storageProblem.current = onStorageProblem;
  });

  const fail = useCallback((e: unknown) => {
    const code = errorCode(e);
    setError(code);
    if (STORAGE_ERRORS.has(code)) storageProblem.current();
  }, []);

  const show = useCallback((next: Dashboard) => {
    lastLoad.current = Date.now();
    setData(next);
  }, []);

  /** `label` names what is running (a key hash, "all", "list") so the matching button shows it. */
  const run = useCallback(
    async <T>(label: string, action: () => Promise<T>): Promise<T | undefined> => {
      if (running.current) return undefined;
      running.current = label;
      setBusy(label);
      setProgress(null);
      setError(null);
      try {
        return await action();
      } catch (e) {
        fail(e);
        return undefined;
      } finally {
        running.current = null;
        setBusy(null);
        setProgress(null);
      }
    },
    [fail],
  );

  useEffect(() => {
    let alive = true;
    // Startup maintenance or a save may hold the store; its end publishes, and we keep asking meanwhile.
    run("list", async () => {
      while (alive && lastLoad.current === 0) {
        try {
          return show(await api.listProjects(true));
        } catch (e) {
          if (errorCode(e) !== "busy") throw e;
          await pause(BUSY_RETRY_MS);
        }
      }
    });
    const subscriptions = [
      listen<Dashboard>("projects-changed", (e) => show(e.payload)),
      // Background refreshes (watcher, tray) also report progress; only an action of ours shows it.
      listen<Progress>("sync-progress", (e) => running.current && setProgress(e.payload)),
    ];
    // Claude writes sessions all the time: re-read local state when the window comes back.
    const onFocus = () => {
      if (running.current || Date.now() - lastLoad.current < FOCUS_RELOAD_GAP_MS) return;
      lastLoad.current = Date.now();
      api.listProjects(false).then(show, (e) => errorCode(e) !== "busy" && fail(e));
    };
    window.addEventListener("focus", onFocus);
    return () => {
      alive = false;
      subscriptions.forEach((s) => s.then((stop) => stop()));
      window.removeEventListener("focus", onFocus);
    };
  }, [run, fail, show]);

  return { data, error, progress, busy, run, dismissError: () => setError(null) };
}
