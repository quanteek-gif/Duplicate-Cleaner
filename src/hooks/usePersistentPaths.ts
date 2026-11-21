// Folder selections that survive navigation and app restarts.
//
// Both the Scan page and the Similar Photos page let the user build a list of
// target folders. The pages unmount when the user navigates away, so the list
// lives in localStorage under a page-specific key instead of component state.

import { useCallback, useEffect, useState } from "react";

/** Read a persisted string list, ignoring anything malformed. */
export function readStoredPaths(key: string): string[] {
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((item): item is string => typeof item === "string");
  } catch {
    // Private mode, quota exceeded or corrupted JSON: start from an empty list.
    return [];
  }
}

function writeStoredPaths(key: string, paths: string[]): void {
  try {
    window.localStorage.setItem(key, JSON.stringify(paths));
  } catch {
    // Storage unavailable: the selection simply won't persist.
  }
}

/** Case-insensitive de-duplication that keeps the original order. */
export function addUniquePath(current: string[], path: string): string[] {
  const normalised = path.trim();
  if (normalised === "") return current;
  if (current.some((existing) => existing.toLowerCase() === normalised.toLowerCase())) {
    return current;
  }
  return [...current, normalised];
}

/**
 * `useState` for a list of folder paths that is mirrored to localStorage.
 *
 * ```ts
 * const [paths, setPaths] = usePersistentPaths("duplicate-cleaner.similar-folders");
 * ```
 */
export function usePersistentPaths(
  key: string,
): [string[], (update: (current: string[]) => string[]) => void] {
  const [paths, setPaths] = useState<string[]>(() => readStoredPaths(key));

  useEffect(() => {
    writeStoredPaths(key, paths);
  }, [key, paths]);

  const update = useCallback((change: (current: string[]) => string[]) => {
    setPaths(change);
  }, []);

  return [paths, update];
}
