import { open } from "@tauri-apps/plugin-dialog";
import { t } from "../lib/i18n";
import { Button } from "./button";

export async function pickFolder(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false });
  return typeof picked === "string" ? picked : null;
}

export function FolderRow({ label, path, onChange }: { label: string; path: string; onChange: () => void }) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-body font-medium text-graphite">{label}</span>
      <div className="flex items-center justify-between gap-3 rounded-control border border-mist px-3 py-2">
        <span className="truncate font-mono text-body text-carbon-ink" title={path}>
          {path}
        </span>
        <Button variant="ghost" className="px-2 py-0" onClick={onChange}>
          {t("folders.change")}
        </Button>
      </div>
    </div>
  );
}

/** Editable list of folders (workspace roots). */
export function FolderList({ label, folders, onChange }: { label: string; folders: string[]; onChange: (folders: string[]) => void }) {
  const add = async () => {
    const folder = await pickFolder();
    if (folder && !folders.includes(folder)) onChange([...folders, folder]);
  };
  return (
    <div className="flex flex-col gap-2">
      <span className="text-body font-medium text-graphite">{label}</span>
      {folders.length === 0 && <span className="text-body text-ashen">{t("folders.empty")}</span>}
      {folders.map((folder) => (
        <div key={folder} className="flex items-center justify-between gap-3 rounded-control bg-soft-stone px-3 py-2">
          <span className="truncate font-mono text-body text-carbon-ink" title={folder}>
            {folder}
          </span>
          <Button variant="ghost" className="px-2 py-0" onClick={() => onChange(folders.filter((f) => f !== folder))}>
            {t("folders.remove")}
          </Button>
        </div>
      ))}
      <Button variant="secondary" className="self-start" onClick={add}>
        {t("folders.add")}
      </Button>
    </div>
  );
}
