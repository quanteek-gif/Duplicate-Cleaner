// Formatting helpers shared by every page. Everything here follows the active
// interface language, so "48.8 KB" becomes "48,8 КБ" and "2m ago" becomes
// "2 минуты назад" without each page having to think about it.

import { formatCount, getLang, locale, t, tc } from "../i18n";

const UNITS = [
  "unit.byte",
  "unit.kilobyte",
  "unit.megabyte",
  "unit.gigabyte",
  "unit.terabyte",
  "unit.petabyte",
] as const;

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return `0 ${t("unit.byte")}`;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = value >= 100 || unit === 0 ? 0 : value >= 10 ? 1 : 2;
  const shown = new Intl.NumberFormat(locale(), {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value);
  return `${shown} ${t(UNITS[unit] ?? "unit.byte")}`;
}

export function formatNumber(value: number): string {
  return formatCount(value);
}

export function formatDate(ms: number | null | undefined): string {
  if (!ms) return "—";
  try {
    return new Date(ms).toLocaleString(locale(), {
      year: "numeric",
      month: "short",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "—";
  }
}

export function formatDateShort(ms: number | null | undefined): string {
  if (!ms) return "—";
  try {
    return new Date(ms).toLocaleDateString(locale(), {
      month: "short",
      day: "2-digit",
      year: "numeric",
    });
  } catch {
    return "—";
  }
}

export function formatDuration(ms: number): string {
  // "1m 05s" reads as a compact badge in English; Russian units are words and
  // need the space: "1 мин 05 с".
  const gap = getLang() === "ru" ? " " : "";
  const s = `${gap}${t("unit.secondShort")}`;
  const m = `${gap}${t("unit.minuteShort")}`;
  const h = `${gap}${t("unit.hourShort")}`;
  if (!Number.isFinite(ms) || ms < 0) return `0${s}`;
  const seconds = Math.floor(ms / 1000);
  if (seconds < 60) return `${seconds}${s}`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  if (minutes < 60) return `${minutes}${m} ${rest.toString().padStart(2, "0")}${s}`;
  const hours = Math.floor(minutes / 60);
  return `${hours}${h} ${(minutes % 60).toString().padStart(2, "0")}${m}`;
}

export function formatRelative(ms: number | null | undefined): string {
  if (!ms) return "—";
  const diff = Date.now() - ms;
  if (diff < 45_000) return t("unit.justNow");
  const minutes = Math.round(diff / 60_000);
  if (minutes < 60) return tc("minuteAgo", minutes);
  const hours = Math.round(minutes / 60);
  if (hours < 24) return tc("hourAgo", hours);
  const days = Math.round(hours / 24);
  if (days < 30) return tc("dayAgo", days);
  return formatDateShort(ms);
}

export function shortPath(path: string, maxLength = 64): string {
  if (path.length <= maxLength) return path;
  const parts = path.split(/[\\/]/);
  let tail = parts.pop() ?? "";
  while (parts.length > 0 && tail.length < maxLength * 0.6) {
    tail = `${parts.pop()}/${tail}`;
  }
  return `…/${tail}`;
}

export function parentPath(path: string): string {
  const index = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return index > 0 ? path.slice(0, index) : path;
}

export function baseName(path: string): string {
  const index = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return index >= 0 ? path.slice(index + 1) : path;
}

export function fileKind(extension: string): string {
  return extension ? extension.toUpperCase() : t("unit.fileTypeFallback");
}

/** Mirrors `MAX_SIMILAR_THRESHOLD` in `models/settings.rs`: a 64-bit pHash
 * can differ by at most 64 bits, and anything past half of that matches
 * unrelated pictures. The backend clamps to this value, so the sliders must
 * not offer more. */
export const MAX_SIMILAR_THRESHOLD = 32;

const CATEGORY_KEYS: Record<string, Parameters<typeof t>[0]> = {
  video: "category.video",
  image: "category.image",
  audio: "category.audio",
  archive: "category.archive",
  document: "category.document",
  executable: "category.executable",
  code: "category.code",
  font: "category.font",
  other: "category.other",
};

/** Localised name of a file category; unknown categories pass through. */
export function categoryLabel(category: string): string {
  const key = CATEGORY_KEYS[category];
  return key ? t(key) : category;
}

export const CATEGORY_COLORS: Record<string, string> = {
  video: "#ff7a45",
  image: "#4f8cff",
  audio: "#c084fc",
  archive: "#f5b533",
  document: "#3ecf8e",
  executable: "#f56c6c",
  code: "#38c7f5",
  font: "#a3a3a3",
  other: "#6b7079",
};

export function percent(part: number, total: number): number {
  if (!total) return 0;
  return Math.min(100, Math.max(0, (part / total) * 100));
}

/** Format a Windows drive root like `C:\` for display. */
export function driveLabel(mountPoint: string, name: string): string {
  const letter = /^([A-Za-z]):/.exec(mountPoint);
  if (letter?.[1]) return t("unit.localDisk", { letter: letter[1].toUpperCase() });
  return name || mountPoint;
}

/** `true` when the interface is Russian — used by tests and diagnostics. */
export function isRussian(): boolean {
  return getLang() === "ru";
}
