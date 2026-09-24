import type { FileRow, FileState, Side } from "./tauri-commands";

export type SessionRow = {
  /** Session id, or "memory" for the project's memory files. */
  id: string;
  title: string | null;
  size: number;
  savedAt: string | null;
  savedBy: string | null;
  state: FileState;
  files: string[];
  /** State of `<id>.jsonl` itself: the copy the viewer reads. */
  transcript: FileState | null;
};

// Most urgent first: a session shows the state of its most urgent file.
const URGENCY: FileState[] = ["diverged", "local_ahead", "local_only", "remote_ahead", "remote_only", "local_deleted", "remote_deleted", "in_sync"];

/** Same rule as the engine: `<sid>.jsonl` or `<sid>/...` with a 36-char id. */
function sessionOf(rel: string): string {
  const first = rel.split("/")[0];
  const id = first.endsWith(".jsonl") ? first.slice(0, -6) : first;
  return id.length === 36 ? id : "memory";
}

export function groupSessions(files: FileRow[]): SessionRow[] {
  const rows = new Map<string, SessionRow>();
  for (const f of files) {
    const id = sessionOf(f.rel);
    const row = rows.get(id) ?? { id, title: null, size: 0, savedAt: null, savedBy: null, state: "in_sync" as FileState, files: [], transcript: null };
    row.files.push(f.rel);
    row.size += f.size ?? 0;
    if (f.rel === `${id}.jsonl`) {
      row.title = f.title;
      row.transcript = f.state;
    }
    if (f.saved_at && (!row.savedAt || f.saved_at > row.savedAt)) {
      row.savedAt = f.saved_at;
      row.savedBy = f.saved_by;
    }
    if (URGENCY.indexOf(f.state) < URGENCY.indexOf(row.state)) row.state = f.state;
    rows.set(id, row);
  }
  return [...rows.values()].sort((a, b) => (a.id === "memory" ? 1 : b.id === "memory" ? -1 : (b.savedAt ?? "").localeCompare(a.savedAt ?? "")));
}

/** The copy to read: the cloud one when this machine has none or an older one. */
export function sessionSide(state: FileState): Side {
  return state === "remote_only" || state === "local_deleted" || state === "remote_ahead" ? "cloud" : "local";
}

/** For PowerShell, the VS Code default on Windows: single quotes keep `$` and backticks literal. */
export function resumeCommand(root: string, subpath: string): string {
  const folder = [root, ...subpath.split("/").filter(Boolean)].join("\\");
  return `cd '${folder.replace(/'/g, "''")}'; claude --resume`;
}
