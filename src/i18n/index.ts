// Tiny, dependency-free i18n layer.
//
// * Messages live in `en.ts` / `ru.ts`; `en` is the source of truth for keys.
// * The active language is module state, so non-React helpers (formatting,
//   error mapping) can read it too. Components subscribe through `useT()`,
//   which re-renders them when the language changes.
// * Plural forms are real plural rules, not string concatenation: Russian
//   needs one/few/many (1 файл, 2 файла, 5 файлов).

import { useSyncExternalStore } from "react";
import { en, enPlurals, type MessageKey, type PluralKey } from "./en";
import { ru, ruPlurals } from "./ru";

export type Lang = "en" | "ru";

const STORAGE_KEY = "duplicate-cleaner:lang";

const DICTS: Record<Lang, Record<MessageKey, string>> = { en, ru };
const PLURALS: Record<Lang, Record<PluralKey, Record<string, string>>> = {
  en: enPlurals,
  ru: ruPlurals,
};

function detect(): Lang {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "en" || stored === "ru") return stored;
    const nav = typeof navigator === "undefined" ? "en" : navigator.language;
    return nav.toLowerCase().startsWith("ru") ? "ru" : "en";
  } catch {
    return "en";
  }
}

let current: Lang = typeof window === "undefined" ? "en" : detect();
if (typeof document !== "undefined") document.documentElement.lang = current;

const listeners = new Set<() => void>();

export function getLang(): Lang {
  return current;
}

export function setLang(next: Lang): void {
  if (next === current) return;
  current = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    /* private mode — the choice just won't survive a restart */
  }
  if (typeof document !== "undefined") document.documentElement.lang = next;
  for (const listener of listeners) listener();
}

export function subscribeLang(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** BCP-47 tag for `Intl` and `toLocaleString`. */
export function locale(): string {
  return current === "ru" ? "ru-RU" : "en-US";
}

export function formatCount(value: number): string {
  return new Intl.NumberFormat(locale()).format(Math.round(value));
}

type Vars = Record<string, string | number>;

function interpolate(template: string, vars?: Vars): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in vars ? String(vars[name]) : match,
  );
}

export function t(key: MessageKey, vars?: Vars): string {
  const template = DICTS[current][key] ?? en[key] ?? key;
  return interpolate(template, vars);
}

/** `one` / `few` / `many` for the active language. Exported for tests. */
export function pluralForm(lang: Lang, count: number): "one" | "few" | "many" | "other" {
  const abs = Math.abs(count);
  if (lang === "ru") {
    const mod100 = abs % 100;
    if (mod100 >= 11 && mod100 <= 14) return "many";
    const mod10 = abs % 10;
    if (mod10 === 1) return "one";
    if (mod10 >= 2 && mod10 <= 4) return "few";
    return "many";
  }
  return abs === 1 ? "one" : "other";
}

/**
 * Countable phrase with the number included: `tc("file", 3)` →
 * "3 files" / "3 файла".
 */
export function tc(key: PluralKey, count: number): string {
  const forms = PLURALS[current][key] ?? enPlurals[key];
  const form = pluralForm(current, count);
  const template = forms[form] ?? forms.other ?? forms.many ?? forms.one ?? "{n}";
  return interpolate(template, { n: formatCount(count) });
}

/** Reactive accessor: components re-render when the language changes. */
export function useT(): { t: typeof t; tc: typeof tc; lang: Lang } {
  const lang = useSyncExternalStore(subscribeLang, getLang, () => "en" as Lang);
  return { t, tc, lang };
}

/** Language of the running UI, for the settings switch. */
export function useLang(): Lang {
  return useSyncExternalStore(subscribeLang, getLang, () => "en" as Lang);
}

// ---------------------------------------------------------------------------
// Backend messages
// ---------------------------------------------------------------------------

/**
 * The Rust side sends a handful of human-readable sentences to the UI:
 * progress notes, keep recommendations and deletion refusals. They are
 * translated by exact match here; a message this table does not know (an
 * `io::Error` from the filesystem, for example) is shown verbatim rather than
 * swallowed, so nothing is ever hidden behind a generic placeholder.
 */
const BACKEND_MESSAGES: Record<string, MessageKey> = {
  "No size collisions found": "backend.noSizeCollisions",
  "No duplicates were found": "backend.noDuplicates",
  "scan finished": "backend.scanFinished",
  "discovery finished": "backend.discoveryFinished",
  "the scan has been cancelled": "backend.cancelled",
  "a scan is already running": "backend.alreadyRunning",
  "deletion was not confirmed by the user": "backend.notConfirmed",
  "select at least one folder or drive to scan": "backend.selectFolder",
  "select at least one folder to scan": "backend.selectFolderShort",
  "select at least one folder with photos": "backend.selectPhotoFolder",
  "minimum size is greater than maximum size": "backend.minGreaterThanMax",
  "file changed after the scan (hash mismatch) — not deleted": "backend.hashMismatch",
  "file disappeared or is not readable — not deleted": "backend.disappeared",
  "refusing to delete the last copy of a file": "backend.lastCopy",
  "path is in a protected system location": "backend.protectedPath",
  "symbolic links are never deleted": "backend.symlink",
  "not a regular file": "backend.notRegularFile",
  "empty path": "backend.emptyPath",
  "file no longer exists (it may have been removed or moved)": "backend.missingFile",
  "inside your documents/pictures folder": "backend.keepUserFolder",
  "the most recently modified copy": "backend.keepNewest",
  "the least deeply nested copy": "backend.keepTopLevel",
  "the shortest path": "backend.keepShortestPath",
};

/** Error prefixes produced by `AppError`'s `Display` impl. */
const BACKEND_PREFIXES: [string, MessageKey][] = [
  ["invalid argument: ", "backend.prefixInvalid"],
  ["access denied: ", "backend.prefixAccessDenied"],
  ["I/O error: ", "backend.prefixIo"],
  ["database error: ", "backend.prefixDatabase"],
  ["image error: ", "backend.prefixImage"],
  ["recycle bin error: ", "backend.prefixTrash"],
  ["serialization error: ", "backend.prefixSerialization"],
];

const SYSTEM_DIR = /^scanning system directory '(.*)' requires explicit permission/;
const SCAN_NOT_FOUND = /^scan '(.*)' not found$/;

export function localizeBackend(message: string | null | undefined): string {
  if (!message) return "";
  if (current === "en") return message;

  const systemDir = SYSTEM_DIR.exec(message);
  if (systemDir) return t("backend.systemDirPermission", { root: systemDir[1] ?? "" });

  const notFound = SCAN_NOT_FOUND.exec(message);
  if (notFound) return t("backend.scanNotFound", { id: notFound[1] ?? "" });

  const exact = BACKEND_MESSAGES[message];
  if (exact) return t(exact);

  for (const [prefix, key] of BACKEND_PREFIXES) {
    if (message.startsWith(prefix)) {
      // The remainder can be a mapped sentence of its own:
      // "invalid argument: select at least one folder to scan".
      return `${t(key)}${localizeBackend(message.slice(prefix.length))}`;
    }
  }
  return message;
}

/** Message key for a backend string, or `null` when it is not in the table. */
export function backendMessageKey(message: string): MessageKey | null {
  return BACKEND_MESSAGES[message] ?? null;
}
