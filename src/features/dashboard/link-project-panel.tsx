import { useState } from "react";
import { Button } from "../../components/button";
import { t } from "../../lib/i18n";
import type { Checkout, ProjectView } from "../../lib/tauri-commands";

export type LinkActions = {
  linkFolder: (folder: string) => void;
  pickFolder: () => void;
  clone: (root: string) => void;
  cancelClone: () => void;
  rescan: () => void;
};

/** A cloud project with no checkout here: link a found checkout, clone one, or pick a folder. */
export function LinkProjectPanel({ project, checkouts, roots, busy, cloning, actions }: { project: ProjectView; checkouts: Checkout[] | null; roots: string[]; busy: boolean; cloning: boolean; actions: LinkActions }) {
  const [root, setRoot] = useState<string | null>(null);
  const matches = (checkouts ?? []).filter((c) => c.remote === project.remote);
  const cloneRoot = root && roots.includes(root) ? root : (roots[0] ?? null);

  return (
    <section className="flex flex-col gap-4 rounded-card bg-soft-stone p-6">
      <h2 className="text-caption font-medium uppercase tracking-wide text-pebble">{t("link.title")}</h2>
      {checkouts === null && <p className="text-body text-ashen">{t("common.working")}</p>}
      {matches.map((match) => (
        <div key={match.path} className="flex items-center justify-between gap-4">
          <span className="min-w-0 truncate font-mono text-body text-carbon-ink" title={match.path}>
            {t("link.found", { path: match.path })}
          </span>
          <Button className="shrink-0" onClick={() => actions.linkFolder(match.path)} disabled={busy}>
            {t("link.link_and_restore")}
          </Button>
        </div>
      ))}
      {checkouts !== null && matches.length === 0 && (
        <p className="text-body text-graphite">{t(roots.length ? "link.none" : "link.no_roots")}</p>
      )}
      {checkouts !== null && matches.length === 0 && cloneRoot && (
        <div className="flex flex-wrap items-center gap-3">
          {roots.length > 1 && (
            <select aria-label={t("link.clone_root")} value={cloneRoot} onChange={(e) => setRoot(e.target.value)} className="rounded-control border border-mist bg-paper-white px-3 py-2 text-body text-carbon-ink">
              {roots.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </select>
          )}
          {cloning ? (
            <Button variant="secondary" onClick={actions.cancelClone}>
              {t("link.cancel")}
            </Button>
          ) : (
            <Button onClick={() => actions.clone(cloneRoot)} disabled={busy}>
              {t("link.clone", { path: `${cloneRoot.replace(/[\\/]+$/, "")}\\${project.name}` })}
            </Button>
          )}
        </div>
      )}
      <div className="flex flex-wrap gap-3">
        <Button variant="secondary" onClick={actions.pickFolder} disabled={busy}>
          {t("link.other")}
        </Button>
        <Button variant="ghost" onClick={actions.rescan} disabled={busy}>
          {t("link.rescan")}
        </Button>
      </div>
    </section>
  );
}
