import { Button } from "../../components/button";
import { errorText, t, useLanguage } from "../../lib/i18n";
import { groupSessions, type SessionRow } from "../../lib/sessions";
import { STATUS, statusSentence } from "../../lib/status-copy";
import type { ProjectView, SyncReport } from "../../lib/tauri-commands";
import { ActivityLog } from "./activity-log";
import { ProjectOverflowMenu } from "./project-overflow-menu";
import { SessionList } from "./session-list";

export type DetailActions = {
  primary: () => void;
  force: (mode: "force_local" | "force_remote") => void;
  restoreSession: (row: SessionRow) => void;
  deleteSession: (row: SessionRow) => void;
  openFolder: () => void;
  copyResume: () => void;
};

/** Why files were left out, grouped by reason ("session_open" → close the Claude session first). */
function skipNotes(pairs: [string, string][]): string[] {
  const counts = new Map<string, number>();
  pairs.forEach(([, code]) => counts.set(code, (counts.get(code) ?? 0) + 1));
  return [...counts].map(([code, count]) => {
    const key = `skip.${code}`;
    return t("skip.line", { count, reason: t(key) === key ? errorText(code) : t(key) });
  });
}

export function ProjectDetailPane({ project, autoSaveError, busy, running, report, actions, activityVersion }: { project: ProjectView; autoSaveError: string | null; busy: boolean; running: boolean; report: SyncReport | null; actions: DetailActions; activityVersion: unknown }) {
  useLanguage();
  const { glyph, action } = STATUS[project.status];
  const linked = project.local_root !== null;
  const notes = [...(autoSaveError ? [t("project.auto_save_failed", { error: errorText(autoSaveError) })] : []), ...skipNotes(report?.skipped ?? []), ...skipNotes(project.unreadable)];

  return (
    <article className="flex min-w-0 flex-1 flex-col gap-8 overflow-y-auto p-8">
      <header className="flex items-start justify-between gap-6">
        <div className="min-w-0">
          <h1 className="truncate font-serif text-heading font-normal text-carbon-ink">
            {project.name}
            {project.subpath && <span className="text-ashen"> / {project.subpath}</span>}
          </h1>
          <p className="mt-1 text-body text-ashen">{project.remote}</p>
          <p className="mt-4 flex items-center gap-2 text-[16px] text-graphite">
            <span aria-hidden="true">{glyph}</span>
            {statusSentence(project)}
          </p>
          {project.status === "diverged" && <p className="mt-1 text-body text-ashen">{t("status.diverged_hint")}</p>}
          {project.local_root && (
            <p className="mt-1 truncate font-mono text-caption text-ashen" title={project.local_root}>
              {project.local_root}
            </p>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {action && (
            <Button onClick={actions.primary} disabled={busy}>
              {running ? t("common.working") : t(`action.${action}`)}
            </Button>
          )}
          <ProjectOverflowMenu
            items={[
              { label: t("project.force_local"), onSelect: () => actions.force("force_local"), disabled: busy || !linked },
              { label: t("project.force_remote"), onSelect: () => actions.force("force_remote"), disabled: busy || !linked },
              { label: t("project.open_folder"), onSelect: actions.openFolder, disabled: busy || !linked },
              { label: t("project.copy_resume"), onSelect: actions.copyResume, disabled: !linked },
            ]}
          />
        </div>
      </header>
      {notes.length > 0 && (
        <ul className="flex flex-col gap-1 rounded-control bg-soft-stone px-4 py-3 text-body text-carbon-ink">
          {notes.map((note) => (
            <li key={note}>{note}</li>
          ))}
        </ul>
      )}
      <section className="flex flex-col gap-3">
        <h2 className="text-caption font-medium uppercase tracking-wide text-pebble">{t("sessions.title")}</h2>
        <SessionList sessions={groupSessions(project.files)} disabled={busy} onRestore={actions.restoreSession} onDelete={actions.deleteSession} />
      </section>
      <section className="flex flex-col gap-3">
        <h2 className="text-caption font-medium uppercase tracking-wide text-pebble">{t("activity.title")}</h2>
        <ActivityLog keyHash={project.key_hash} version={activityVersion} />
      </section>
    </article>
  );
}
