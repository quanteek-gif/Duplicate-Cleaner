// Global state for the active/last scan. Keeps the UI in sync with Rust
// progress events, even if the user navigates between pages mid-scan.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { api, errorMessage, isTauri, onScanProgress } from "../services/api";
import type { ScanOptions, ScanProgress, ScanRecord, ScanResult } from "../types";
import { useToast } from "../components/ui";
import { localizeBackend, useT } from "../i18n";

export interface ScanSettings {
  options: ScanOptions;
}

export const DEFAULT_SCAN_OPTIONS: ScanOptions = {
  paths: [],
  excludedPaths: [],
  excludedDirNames: [],
  extensions: [],
  minSize: null,
  maxSize: null,
  followSymlinks: false,
  includeSystemDirs: false,
  useHashCache: true,
};

interface ScanContextValue {
  progress: ScanProgress | null;
  running: boolean;
  paused: boolean;
  result: ScanResult | null;
  lastRecords: ScanRecord[];
  startScan: (options: ScanOptions) => Promise<ScanResult | null>;
  cancelScan: () => Promise<void>;
  pauseScan: () => Promise<void>;
  resumeScan: () => Promise<void>;
  loadScan: (scanId: string) => Promise<void>;
  clearResult: () => void;
  refreshRecent: () => Promise<void>;
  /** Drop successfully deleted paths from the in-memory result so rows disappear instantly. */
  applyDeletion: (paths: string[]) => void;
}

/** Phases after which a scan is over, whether it finished or not. */
const TERMINAL_PHASES = new Set(["completed", "cancelled", "failed"]);

const ScanContext = createContext<ScanContextValue | null>(null);

export function ScanProvider({ children }: { children: ReactNode }) {
  const toast = useToast();
  const { t } = useT();
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [result, setResult] = useState<ScanResult | null>(null);
  const [lastRecords, setLastRecords] = useState<ScanRecord[]>([]);
  const [running, setRunning] = useState(false);
  const [paused, setPaused] = useState(false);
  const activeScanId = useRef<string | null>(null);

  // Subscribe to progress events for the whole app lifetime.
  useEffect(() => {
    if (!isTauri()) return;
    let dispose: (() => void) | undefined;
    void onScanProgress((incoming) => {
      if (activeScanId.current && incoming.scanId !== activeScanId.current) return;
      setProgress(incoming);
      // Cancelled and failed runs end here too. Only clearing the flag on
      // "completed" left the page showing a live-looking progress bar for a
      // scan that had already stopped.
      if (TERMINAL_PHASES.has(incoming.phase)) {
        setRunning(false);
        setPaused(false);
      }
    }).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose?.();
  }, []);

  // Re-attach to a scan that is still running after a window reload.
  useEffect(() => {
    if (!isTauri()) return;
    void api
      .getActiveScan()
      .then((info) => {
        if (info) {
          activeScanId.current = info.scanId;
          setRunning(true);
          setPaused(info.paused);
        }
      })
      .catch(() => undefined);
  }, []);

  const refreshRecent = useCallback(async () => {
    if (!isTauri()) return;
    try {
      setLastRecords(await api.listRecentScans(12));
    } catch {
      /* not critical */
    }
  }, []);

  useEffect(() => {
    void refreshRecent();
  }, [refreshRecent]);

  const startScan = useCallback(
    async (options: ScanOptions): Promise<ScanResult | null> => {
      if (!isTauri()) {
        toast.push("error", t("scan.backendUnavailable"), t("scan.backendUnavailableBody"));
        return null;
      }
      if (options.paths.length === 0) {
        toast.push("error", t("scan.noFolders"), t("scan.noFoldersBody"));
        return null;
      }
      setRunning(true);
      setPaused(false);
      setProgress(null);
      try {
        const started = await api.startScan(options);
        activeScanId.current = started.scanId;
        setResult(started);
        setRunning(false);
        await refreshRecent();
        return started;
      } catch (error) {
        setRunning(false);
        setPaused(false);
        const message = errorMessage(error);
        if (!message.toLowerCase().includes("cancel")) {
          toast.push("error", t("scan.failed"), localizeBackend(message));
        }
        return null;
      }
    },
    [refreshRecent, toast, t],
  );

  const cancelScan = useCallback(async () => {
    const scanId = activeScanId.current ?? progress?.scanId;
    if (!scanId) return;
    try {
      await api.cancelScan(scanId);
      toast.push("info", t("scan.cancelled"));
    } catch (error) {
      toast.push("error", t("scan.cancelFailed"), localizeBackend(errorMessage(error)));
    }
  }, [progress?.scanId, toast, t]);

  const pauseScan = useCallback(async () => {
    const scanId = activeScanId.current ?? progress?.scanId;
    if (!scanId) return;
    try {
      await api.pauseScan(scanId);
      setPaused(true);
    } catch (error) {
      toast.push("error", t("scan.pauseFailed"), localizeBackend(errorMessage(error)));
    }
  }, [progress?.scanId, toast, t]);

  const resumeScan = useCallback(async () => {
    const scanId = activeScanId.current ?? progress?.scanId;
    if (!scanId) return;
    try {
      await api.resumeScan(scanId);
      setPaused(false);
    } catch (error) {
      toast.push("error", t("scan.resumeFailed"), localizeBackend(errorMessage(error)));
    }
  }, [progress?.scanId, toast, t]);

  const loadScan = useCallback(
    async (scanId: string) => {
      if (!isTauri()) return;
      try {
        const loaded = await api.getScanResult(scanId);
        activeScanId.current = loaded.scanId;
        setResult(loaded);
      } catch (error) {
        toast.push("error", t("scan.loadFailed"), localizeBackend(errorMessage(error)));
      }
    },
    [toast, t],
  );

  const clearResult = useCallback(() => {
    setResult(null);
    setProgress(null);
  }, []);

  const applyDeletion = useCallback((paths: string[]) => {
    if (paths.length === 0) return;
    const removed = new Set(paths);
    setResult((current) => {
      if (!current) return current;
      const groups = current.groups
        .map((group) => {
          const files = group.files.filter((file) => !removed.has(file.path));
          if (files.length === group.files.length) return group;
          const fileSize = files[0]?.size ?? group.size;
          return {
            ...group,
            files,
            size: fileSize,
            wastedBytes: fileSize * Math.max(0, files.length - 1),
          };
        })
        .filter((group) => group.files.length > 1);
      const duplicateFiles = groups.reduce((total, group) => total + group.files.length, 0);
      const wastedBytes = groups.reduce((total, group) => total + group.wastedBytes, 0);
      return {
        ...current,
        groups,
        stats: {
          ...current.stats,
          duplicateGroups: groups.length,
          duplicateFiles,
          wastedBytes,
        },
      };
    });
  }, []);

  const value = useMemo<ScanContextValue>(
    () => ({
      progress,
      running,
      paused,
      result,
      lastRecords,
      startScan,
      cancelScan,
      pauseScan,
      resumeScan,
      loadScan,
      clearResult,
      refreshRecent,
      applyDeletion,
    }),
    [
      progress,
      running,
      paused,
      result,
      lastRecords,
      startScan,
      cancelScan,
      pauseScan,
      resumeScan,
      loadScan,
      clearResult,
      refreshRecent,
      applyDeletion,
    ],
  );

  return <ScanContext.Provider value={value}>{children}</ScanContext.Provider>;
}

/** Default value used outside the provider (also keeps pages testable). */
const FALLBACK: ScanContextValue = {
  progress: null,
  running: false,
  paused: false,
  result: null,
  lastRecords: [],
  startScan: async () => null,
  cancelScan: async () => undefined,
  pauseScan: async () => undefined,
  resumeScan: async () => undefined,
  loadScan: async () => undefined,
  clearResult: () => undefined,
  refreshRecent: async () => undefined,
  applyDeletion: () => undefined,
};

export function useScan(): ScanContextValue {
  const context = useContext(ScanContext);
  return context ?? FALLBACK;
}
