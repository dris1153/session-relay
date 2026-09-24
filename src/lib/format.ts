import { currentLanguage } from "./i18n";

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 3600],
  ["month", 30 * 24 * 3600],
  ["day", 24 * 3600],
  ["hour", 3600],
  ["minute", 60],
];

/** "3 hours ago" / "3 giờ trước" in the active language. */
export function relativeTime(iso: string): string {
  const seconds = (new Date(iso).getTime() - Date.now()) / 1000;
  const format = new Intl.RelativeTimeFormat(currentLanguage(), { numeric: "auto" });
  const [unit, size] = UNITS.find(([, size]) => Math.abs(seconds) >= size) ?? ["second", 1];
  return format.format(Math.round(seconds / size), unit);
}

export function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
