import { useCallback, useEffect, useState } from "react";
import { api, type Checkout } from "./tauri-commands";

/** Checkouts under the workspace folders, scanned each time the link panel opens (roots may have changed in Settings). */
export function useWorkspaceScan(needed: boolean) {
  const [checkouts, setCheckouts] = useState<Checkout[] | null>(null);
  const [roots, setRoots] = useState<string[]>([]);

  const rescan = useCallback(() => {
    api.getAppState().then((app) => setRoots(app.workspace_roots), () => setRoots([]));
    api.scanWorkspaces().then(setCheckouts, () => setCheckouts([]));
  }, []);

  useEffect(() => {
    if (!needed) setCheckouts(null);
    else if (checkouts === null) rescan();
  }, [needed, checkouts, rescan]);

  return { checkouts, roots, rescan };
}
