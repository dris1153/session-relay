import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorCode, type Dashboard, type Progress } from "./tauri-commands";

/** Errors that mean the storage itself changed: onboarding has to look again. */
const STORAGE_ERRORS = new Set(["not_logged_in", "auth_rejected", "wrong_identity", "store_reset"]);
const FOCUS_RELOAD_GAP_MS = 5000;
const BUSY_RETRY_MS = 3000;
/** Steps a user action can cause; the rest (waiting, checking, evaluating) belong to loading the list. */
const ACTION_STEPS = new Set(["download", "upload", "clone", "restore", "save"]);

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
  const mounted = useRef(true);
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

  const load = useCallback(
    () =>
      run("list", async () => {
        // Phase 1: the snapshot already on disk, no network (none before the first download).
        const local = await api.localProjects().catch(() => null);
        if (local) show(local);
        // Phase 2: ask GitHub. While a save holds the store, retry until it ends or a publish arrives.
        const seen = lastLoad.current;
        for (;;) {
          try {
            return show(await api.listProjects(true));
          } catch (e) {
            if (errorCode(e) !== "busy") throw e;
          }
          if (!mounted.current || lastLoad.current !== seen) return;
          await pause(BUSY_RETRY_MS);
        }
      }),
    [run, show],
  );

  useEffect(() => {
    mounted.current = true;
    load();
    const subscriptions = [
      listen<Dashboard>("projects-changed", (e) => show(e.payload)),
      // Background refreshes (watcher, tray) report progress too: show only what our action causes.
      listen<Progress>("sync-progress", (e) => {
        if (running.current === "list" || (running.current && ACTION_STEPS.has(e.payload.step))) setProgress(e.payload);
      }),
    ];
    // Claude writes sessions all the time: re-read local state when the window comes back.
    const onFocus = () => {
      if (running.current || Date.now() - lastLoad.current < FOCUS_RELOAD_GAP_MS) return;
      lastLoad.current = Date.now();
      api.listProjects(false).then(show, (e) => errorCode(e) !== "busy" && fail(e));
    };
    window.addEventListener("focus", onFocus);
    return () => {
      mounted.current = false;
      subscriptions.forEach((s) => s.then((stop) => stop()));
      window.removeEventListener("focus", onFocus);
    };
  }, [load, fail, show]);

  return { data, error, progress, busy, run, reload: load, dismissError: () => setError(null) };
}
