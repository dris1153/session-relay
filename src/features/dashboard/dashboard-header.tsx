import { Button } from "../../components/button";
import { relativeTime } from "../../lib/format";
import { t, useLanguage } from "../../lib/i18n";
import type { Dashboard, Progress } from "../../lib/tauri-commands";

function progressText(p: Progress): { text: string; ratio: number } {
  if ("percent" in p) return { text: t(`progress.${p.step}`, { percent: p.percent }), ratio: p.percent / 100 };
  return { text: t(`progress.${p.step}`, { current: p.current, total: p.total }), ratio: p.total ? p.current / p.total : 0 };
}

export function DashboardHeader({ data, progress, working, canSaveAll, onSaveAll }: { data: Dashboard | null; progress: Progress | null; working: boolean; canSaveAll: boolean; onSaveAll: () => void }) {
  useLanguage();
  const shown = progress && progressText(progress);
  return (
    <header className="relative flex items-center justify-between gap-6 border-b border-chalk px-8 py-4">
      <span className="font-serif text-heading-sm text-carbon-ink">Session Relay</span>
      <div className="flex items-center gap-4">
        {(shown || working) && (
          <span className="text-body text-ashen" aria-live="polite">
            {shown?.text ?? t("common.working")}
          </span>
        )}
        {data?.offline && (
          <span className="rounded-control bg-soft-stone px-2 py-1 text-caption font-medium text-graphite" title={data.fetched_at ? t("header.fetched", { time: relativeTime(data.fetched_at) }) : undefined}>
            {t("header.offline")}
          </span>
        )}
        <Button variant="secondary" onClick={onSaveAll} disabled={working || !canSaveAll}>
          {t("header.save_all")}
        </Button>
      </div>
      {shown && (
        <div className="absolute inset-x-0 bottom-0 h-0.5 bg-chalk" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(shown.ratio * 100)}>
          <div className="h-full bg-graphite transition-[width] duration-300" style={{ width: `${Math.round(shown.ratio * 100)}%` }} />
        </div>
      )}
    </header>
  );
}
