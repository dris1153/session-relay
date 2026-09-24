import { useEffect, useState } from "react";
import { ConfirmDialog } from "../../components/confirm-dialog";
import { pickFolder } from "../../components/folder-fields";
import { ErrorNote } from "../../components/onboarding-card";
import { errorText, t, useLanguage } from "../../lib/i18n";
import { needsAttention, STATUS } from "../../lib/status-copy";
import { api, type ProjectView, type SyncMode, type SyncReport, type User } from "../../lib/tauri-commands";
import { useDashboard } from "../../lib/use-dashboard";
import { SettingsPage } from "../settings/settings-page";
import { DashboardHeader } from "./dashboard-header";
import { ProjectDetailPane, type DetailActions } from "./project-detail-pane";
import { ProjectSidebar } from "./project-sidebar";

type Confirm = { title: string; body: string; confirm: string; action: () => void };

export function DashboardPage({ user, onStorageProblem, onSignOut }: { user: User | null; onStorageProblem: () => void; onSignOut: () => void }) {
  useLanguage();
  const { data, error, progress, busy, run, dismissError } = useDashboard(onStorageProblem);
  const [selected, setSelected] = useState<string | null>(null);
  const [page, setPage] = useState<"projects" | "settings">("projects");
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [reports, setReports] = useState<Record<string, SyncReport>>({});
  const [activityVersion, setActivityVersion] = useState(0);
  const projects = data?.projects ?? [];
  const project = projects.find((p) => p.key_hash === selected) ?? projects.find((p) => needsAttention(p.status)) ?? projects[0] ?? null;
  // Pin the default pick, so the pane does not jump once this project stops needing attention.
  useEffect(() => {
    if (project && project.key_hash !== selected) setSelected(project.key_hash);
  }, [project, selected]);

  const sync = (p: ProjectView, mode: SyncMode, files?: string[]) =>
    run(p.key_hash, async () => {
      const report = await api.syncProject(p.key_hash, mode, files);
      setReports((all) => ({ ...all, [p.key_hash]: report }));
      setActivityVersion((v) => v + 1);
    });

  const link = async (p: ProjectView) => {
    const folder = await pickFolder();
    if (!folder) return;
    const result = await run(p.key_hash, () => api.linkProject(p.key_hash, folder, false));
    if (!result) return;
    if (result.origin_matches) return sync(p, "auto");
    setConfirm({
      title: t("link.mismatch_title"),
      body: result.found_remote ? t("link.mismatch_body", { found: result.found_remote, remote: p.remote }) : t("link.no_repo_body", { remote: p.remote }),
      confirm: t("link.anyway"),
      action: () => run(p.key_hash, () => api.linkProject(p.key_hash, folder, true)).then((r) => r && sync(p, "auto")),
    });
  };

  const actions = (p: ProjectView): DetailActions => ({
    primary: () => (STATUS[p.status].action === "link" ? link(p) : sync(p, "auto")),
    force: (mode) => setConfirm({ title: t(`confirm.${mode}_title`), body: t(`confirm.${mode}_body`), confirm: t(`project.${mode}`), action: () => sync(p, mode) }),
    restoreSession: (row) => sync(p, "auto", row.files),
    deleteSession: (row) =>
      setConfirm({
        title: t("confirm.delete_title"),
        body: t("confirm.delete_body", { title: row.title ?? row.id }),
        confirm: t("sessions.delete_cloud"),
        action: () => run(p.key_hash, () => api.deleteRemoteSession(p.key_hash, row.id)).then(() => setActivityVersion((v) => v + 1)),
      }),
    openFolder: () => run(p.key_hash, () => api.openProjectFolder(p.key_hash)),
    copyResume: () => p.local_root && navigator.clipboard.writeText(resumeCommand(p.local_root, p.subpath)).catch(() => {}),
  });

  const saveAll = () =>
    run("all", async () => {
      const items = await api.saveAll();
      items.forEach((item) => item.report && setReports((all) => ({ ...all, [item.key_hash]: item.report! })));
      const failed = items.find((item) => item.error);
      if (failed) throw { code: failed.error };
    });

  return (
    <div className="flex h-screen flex-col">
      <DashboardHeader data={data} progress={progress} working={busy !== null} canSaveAll={projects.some((p) => p.status === "local_ahead" || p.status === "both")} onSaveAll={saveAll} />
      {error && (
        <div className="flex items-start gap-3 px-8 pt-4">
          <div className="flex-1">
            <ErrorNote message={errorText(error)} />
          </div>
          <button type="button" onClick={dismissError} className="rounded-control px-2 py-3 text-body text-ashen hover:text-carbon-ink">
            {t("common.dismiss")}
          </button>
        </div>
      )}
      <div className="flex min-h-0 flex-1">
        <ProjectSidebar
          projects={projects}
          selected={project?.key_hash ?? null}
          onSelect={(keyHash) => {
            setSelected(keyHash);
            setPage("projects");
          }}
          user={user}
          onSettings={() => setPage("settings")}
        />
        {page === "settings" ? (
          <SettingsPage onSignOut={onSignOut} />
        ) : project ? (
          <ProjectDetailPane project={project} busy={busy !== null} running={busy === project.key_hash} report={reports[project.key_hash] ?? null} actions={actions(project)} activityVersion={activityVersion} />
        ) : (
          <p className="flex-1 p-8 text-body text-ashen">{data ? t("dashboard.empty", { unmanaged: data.unmanaged }) : t("dashboard.loading")}</p>
        )}
      </div>
      <ConfirmDialog
        open={confirm !== null}
        title={confirm?.title ?? ""}
        body={confirm?.body ?? ""}
        confirm={confirm?.confirm ?? ""}
        onConfirm={() => {
          const action = confirm?.action;
          setConfirm(null);
          action?.();
        }}
        onCancel={() => setConfirm(null)}
      />
    </div>
  );
}

/** For PowerShell, the VS Code default on Windows: single quotes keep `$` and backticks literal. */
function resumeCommand(root: string, subpath: string): string {
  const folder = [root, ...subpath.split("/").filter(Boolean)].join("\\");
  return `cd '${folder.replace(/'/g, "''")}'; claude --resume`;
}
