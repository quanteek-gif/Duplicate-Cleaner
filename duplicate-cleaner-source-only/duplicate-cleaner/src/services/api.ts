// Typed wrappers around every Tauri command. The UI never calls `invoke`
// directly, which keeps the Rust/TS contract in one place.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ActiveScanInfo,
  AppSettings,
  DashboardStats,
  DeletionLogEntry,
  DeletionPlan,
  DeletionReport,
  DeletionRequest,
  DiskMapResult,
  DriveInfo,
  FolderEstimate,
  LargeFilesResult,
  QuickFolder,
  ScanOptions,
  ScanProgress,
  ScanRecord,
  ScanResult,
  SimilarPhotosResult,
  SimilarProgress,
} from "../types";

/** `true` when running inside the Tauri webview (false in a plain browser). */
export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const api = {
  // ---------------------------------------------------------------- scans
  startScan: (options: ScanOptions) =>
    invoke<ScanResult>("start_scan", { options }),
  cancelScan: (scanId: string) => invoke<boolean>("cancel_scan", { scanId }),
  pauseScan: (scanId: string) => invoke<boolean>("pause_scan", { scanId }),
  resumeScan: (scanId: string) => invoke<boolean>("resume_scan", { scanId }),
  getActiveScan: () => invoke<ActiveScanInfo | null>("get_active_scan"),
  scanLargeFiles: (options: ScanOptions, limit = 100) =>
    invoke<LargeFilesResult>("scan_large_files_cmd", { options, limit }),
  estimateFolder: (paths: string[]) =>
    invoke<FolderEstimate>("estimate_folder", { paths }),

  // ----------------------------------------------------------- duplicates
  getScanResult: (scanId: string) =>
    invoke<ScanResult>("get_scan_result", { scanId }),
  listRecentScans: (limit = 20) =>
    invoke<ScanRecord[]>("list_recent_scans", { limit }),
  deleteScan: (scanId: string) => invoke<void>("delete_scan", { scanId }),
  findSimilarPhotos: (
    paths: string[],
    threshold?: number,
    excludedPaths?: string[],
  ) =>
    invoke<SimilarPhotosResult>("find_similar_photos", {
      paths,
      threshold: threshold ?? null,
      excludedPaths: excludedPaths ?? null,
    }),
  analyzeDiskUsage: (root: string) =>
    invoke<DiskMapResult>("analyze_disk_usage", { root }),
  getImagePreview: (path: string, maxDim = 512) =>
    invoke<string>("get_image_preview", { path, maxDim }),

  // ------------------------------------------------------------- deletion
  previewDeletion: (request: DeletionRequest) =>
    invoke<DeletionPlan>("preview_deletion", { request }),
  deleteFiles: (request: DeletionRequest) =>
    invoke<DeletionReport>("delete_files", { request }),
  openInExplorer: (path: string) => invoke<void>("open_in_explorer", { path }),
  revealInFolder: (path: string) => invoke<void>("reveal_in_folder", { path }),
  listDeletions: (limit = 200) =>
    invoke<DeletionLogEntry[]>("list_deletions", { limit }),
  clearDeletionHistory: () => invoke<void>("clear_deletion_history"),

  // ------------------------------------------------------------- settings
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) =>
    invoke<AppSettings>("save_settings", { settings }),
  clearHashCache: () => invoke<number>("clear_hash_cache"),

  // ----------------------------------------------------------- statistics
  getDashboardStats: () => invoke<DashboardStats>("get_dashboard_stats"),
  getDrives: () => invoke<DriveInfo[]>("get_drives"),
  getQuickFolders: () => invoke<QuickFolder[]>("get_quick_folders"),
};

/** Subscribe to duplicate-scan progress events. */
export const onScanProgress = (
  handler: (progress: ScanProgress) => void,
): Promise<UnlistenFn> => listen<ScanProgress>("scan://progress", (e) => handler(e.payload));

/** Subscribe to similar-photos progress events. */
export const onSimilarProgress = (
  handler: (progress: SimilarProgress) => void,
): Promise<UnlistenFn> =>
  listen<SimilarProgress>("similar://progress", (e) => handler(e.payload));

/** Normalise an unknown error to a readable string. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return "Unexpected error";
}
