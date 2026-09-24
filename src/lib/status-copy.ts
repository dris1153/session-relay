import type { FileRow, ProjectStatus, ProjectView } from "./tauri-commands";
import { errorText, t } from "./i18n";
import { relativeTime } from "./format";

/** Why a sync left a file out: `skip.<code>` copy, else the error text for that code. */
export function skipReason(code: string): string {
  const key = `skip.${code}`;
  return t(key) === key ? errorText(code) : t(key);
}

export type PrimaryAction = "save" | "restore" | "sync" | "link" | "resolve";

/** Monochrome glyph + the one primary action per status (plan §Requirements). */
export const STATUS: Record<ProjectStatus, { glyph: string; action: PrimaryAction | null }> = {
  local_ahead: { glyph: "●", action: "save" },
  remote_ahead: { glyph: "○", action: "restore" },
  both: { glyph: "◑", action: "sync" },
  diverged: { glyph: "◐", action: "resolve" },
  not_linked: { glyph: "◌", action: "link" },
  synced: { glyph: "✓", action: null },
  no_remote: { glyph: "–", action: null },
};

export function needsAttention(status: ProjectStatus): boolean {
  return status !== "synced" && status !== "no_remote";
}

/** Newest cloud save among files the cloud has ahead of this machine. */
function newestRemote(files: FileRow[]): FileRow | undefined {
  return files
    .filter((f) => (f.state === "remote_ahead" || f.state === "remote_only") && f.saved_at)
    .sort((a, b) => (b.saved_at ?? "").localeCompare(a.saved_at ?? ""))[0];
}

export function statusSentence(project: ProjectView): string {
  const newest = project.status === "remote_ahead" ? newestRemote(project.files) : undefined;
  if (newest?.saved_by && newest.saved_at) {
    return t("status.remote_ahead_by", { machine: newest.saved_by, time: relativeTime(newest.saved_at) });
  }
  return t(`status.${project.status}`);
}
