import { invoke } from "@tauri-apps/api/core";

export type RepoRef = { owner: string; name: string };

export type HookStatus = "installed" | "not_installed" | "stale_path" | "malformed";

export type AppState = {
  git_version: string | null;
  git_ok: boolean;
  has_client_id: boolean;
  signed_in: boolean;
  identity_unlocked: boolean;
  repo: RepoRef | null;
  machine_name: string;
  claude_home: string;
  workspace_roots: string[];
  language: "vi" | "en" | null;
  autostart: boolean;
  hooks: HookStatus;
  claude_untested: string | null;
};

export type LoginCode = { user_code: string; verification_uri: string; expires_in: number };

export type StorageState =
  | "no_installation"
  | "repo_missing"
  | "repo_public"
  | "repo_foreign"
  | "needs_new_key"
  | "needs_unlock"
  | "ready";

export type User = { login: string; avatar_url: string };

export type StorageCheck = {
  state: StorageState;
  user: User;
  repo: RepoRef | null;
  install_url: string;
  create_repo_url: string;
};

export type AuthChanged = { signed_in: boolean; reason: string | null };

export type ProjectStatus = "synced" | "local_ahead" | "remote_ahead" | "both" | "diverged" | "not_linked" | "no_remote";

export type FileState = "in_sync" | "local_only" | "remote_only" | "local_ahead" | "remote_ahead" | "diverged" | "local_deleted" | "remote_deleted";

export type FileRow = {
  rel: string;
  state: FileState;
  conflict: boolean;
  title: string | null;
  size: number | null;
  saved_by: string | null;
  saved_at: string | null;
  local_size: number | null;
  local_modified: string | null;
};

export type Checkout = { remote: string; path: string };

export type ProjectView = {
  key_hash: string;
  remote: string;
  owner: string;
  name: string;
  subpath: string;
  status: ProjectStatus;
  local_root: string | null;
  files: FileRow[];
  unreadable: [string, string][];
};

export type Dashboard = {
  projects: ProjectView[];
  unmanaged: number;
  secondary: number;
  errors: [string, string][];
  offline: boolean;
  fetched_at: string | null;
  auto_save_failures: [string, string][];
};

export type Activity = { ts: string; key_hash: string; source: "gui" | "hook"; action: string; result: string; pushed: number; pulled: number };

export type SyncReport = { pushed: string[]; pulled: string[]; skipped: [string, string][] };

export type SaveAllItem = { key_hash: string; report: SyncReport | null; error: string | null };

export type LinkResult = { origin_matches: boolean; found_remote: string | null };

/** Counted steps name the item being worked on: `current` of `total`. */
export type Progress =
  | { step: "waiting" | "checking" }
  | { step: "download" | "upload" | "clone"; percent: number }
  | { step: "restore" | "save" | "evaluate"; current: number; total: number };

export type SyncMode = "auto" | "force_local" | "force_remote";

export type SettingsPatch = Partial<Pick<AppState, "machine_name" | "claude_home" | "workspace_roots" | "language" | "autostart">>;

type CommandError = { code: string; detail: string };

/** Stable error code from a rejected command (see Rust `Error::code`). */
export function errorCode(error: unknown): string {
  return (error as CommandError | undefined)?.code ?? "internal";
}

export const api = {
  getAppState: () => invoke<AppState>("get_app_state"),
  startLogin: () => invoke<LoginCode>("start_login"),
  logout: () => invoke<void>("logout"),
  checkStorage: () => invoke<StorageCheck>("check_storage"),
  passphraseStrength: (passphrase: string) => invoke<number>("passphrase_strength", { passphrase }),
  createKey: (passphrase: string) => invoke<void>("create_key", { passphrase }),
  unlockKey: (passphrase: string) => invoke<void>("unlock_key", { passphrase }),
  saveSettings: (patch: SettingsPatch) => invoke<AppState>("save_settings", { patch }),
  setAutoSave: (enabled: boolean) => invoke<AppState>("set_auto_save", { enabled }),
  listProjects: (fetch: boolean) => invoke<Dashboard>("list_projects", { fetch }),
  localProjects: () => invoke<Dashboard | null>("local_projects"),
  projectActivity: (keyHash: string) => invoke<Activity[]>("project_activity", { keyHash }),
  syncProject: (keyHash: string, mode: SyncMode, files?: string[]) => invoke<SyncReport>("sync_project", { keyHash, mode, files: files ?? null }),
  saveAll: () => invoke<SaveAllItem[]>("save_all"),
  linkProject: (keyHash: string, localRoot: string, force: boolean) => invoke<LinkResult>("link_project", { keyHash, localRoot, force }),
  deleteRemoteSession: (keyHash: string, sessionId: string) => invoke<void>("delete_remote_session", { keyHash, sessionId }),
  openProjectFolder: (keyHash: string) => invoke<void>("open_project_folder", { keyHash }),
  clearLocalData: () => invoke<void>("clear_local_data"),
  scanWorkspaces: () => invoke<Checkout[]>("scan_workspaces"),
  cloneProject: (keyHash: string, root: string) => invoke<string>("clone_project", { keyHash, root }),
  cancelClone: () => invoke<void>("cancel_clone"),
};
