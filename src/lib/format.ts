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

// A long session formats thousands of values per render: one formatter per language and style.
const formatters = new Map<string, Intl.DateTimeFormat | Intl.NumberFormat>();
function formatter<T extends Intl.DateTimeFormat | Intl.NumberFormat>(style: string, make: (language: string) => T): T {
  const key = `${currentLanguage()}:${style}`;
  if (!formatters.has(key)) formatters.set(key, make(currentLanguage()));
  return formatters.get(key) as T;
}

/** "24 Sep 2026, 10:02" in the active language. */
export function formatDateTime(iso: string): string {
  return formatter("datetime", (l) => new Intl.DateTimeFormat(l, { dateStyle: "medium", timeStyle: "short" })).format(new Date(iso));
}

export function formatClock(iso: string): string {
  return formatter("clock", (l) => new Intl.DateTimeFormat(l, { hour: "2-digit", minute: "2-digit" })).format(new Date(iso));
}

/** 1234567 → "1.2M" (compact, active language). */
export function formatCount(n: number): string {
  return formatter("count", (l) => new Intl.NumberFormat(l, { notation: "compact", maximumFractionDigits: 1 })).format(n);
}

export function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
