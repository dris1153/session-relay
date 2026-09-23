import { invoke } from "@tauri-apps/api/core";

export type RepoRef = { owner: string; name: string };

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

export type StorageCheck = {
  state: StorageState;
  user: { login: string; avatar_url: string };
  repo: RepoRef | null;
  install_url: string;
  create_repo_url: string;
};

export type AuthChanged = { signed_in: boolean; reason: string | null };

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
};
