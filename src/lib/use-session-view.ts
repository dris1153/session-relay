import { useCallback, useEffect, useState } from "react";
import { api, errorCode, type SessionView, type Side } from "./tauri-commands";

/** One session open in the viewer; full tool input/output is fetched when the user asks. */
export function useSessionView(keyHash: string, sessionId: string, side: Side) {
  const [view, setView] = useState<SessionView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    setView(null);
    setError(null);
    api.openSession(keyHash, sessionId, side).then(
      (v) => live && setView(v),
      (e) => live && setError(errorCode(e)),
    );
    return () => {
      live = false;
    };
  }, [keyHash, sessionId, side, attempt]);

  const detail = useCallback((reference: string) => api.sessionDetail(keyHash, sessionId, side, reference), [keyHash, sessionId, side]);
  return { view, error, detail, retry: () => setAttempt((n) => n + 1) };
}
