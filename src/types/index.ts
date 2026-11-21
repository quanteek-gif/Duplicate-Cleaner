// Shared TypeScript types — a 1:1 mirror of the Rust models (camelCase via serde).

export interface FileEntry {
  id: number;
  path: string;
  name: string;
  size: number;
  createdMs: number | null;
  modifiedMs: number | null;
  extension: string;
  sha256: string | null;
  phash: string | null;
}

export type KeepReason = "userFolder" | "newest" | "topLevel" | "shortestPath";

/** Which copy the Rust analyzer suggests keeping, and why. */
export interface RecommendedCopy {
  path: string;
  score: number;
  reason: KeepReason;
  explanation: string;
}

export interface DuplicateGroup {
  id: number;
  hash: string;
  size: number;
  wastedBytes: number;
  recommendedKeep: RecommendedCopy | null;
  files: FileEntry[];
}

export type ScanPhase =
  | "discovering"
  | "sizing"
  | "hashing"
  | "grouping"
  | "completed"
  | "cancelled"
  | "failed";

export interface ScanProgress {
  scanId: string;
  phase: ScanPhase;
  filesDiscovered: number;
  filesProcessed: number;
  filesToHash: number;
  bytesDiscovered: number;
  bytesHashed: number;
  currentFile: string | null;
  duplicatesFound: number;
  groupsFound: number;
  errors: number;
  elapsedMs: number;
  message: string | null;
}

export interface ScanOptions {
  paths: string[];
  excludedPaths: string[];
  excludedDirNames: string[];
  extensions: string[];
  minSize: number | null;
  maxSize: number | null;
  followSymlinks: boolean;
  includeSystemDirs: boolean;
  useHashCache: boolean;
}

export interface ScanStats {
  filesScanned: number;
  filesHashed: number;
  bytesScanned: number;
  duplicateGroups: number;
  duplicateFiles: number;
  wastedBytes: number;
  errors: number;
}

export interface ScanErrorEntry {
  path: string;
  message: string;
}

export interface ScanResult {
  scanId: string;
  startedAt: number;
  finishedAt: number;
  options: ScanOptions;
  stats: ScanStats;
  groups: DuplicateGroup[];
  errors: ScanErrorEntry[];
}

export interface ScanRecord {
  id: number;
  scanId: string;
  startedAt: number;
  finishedAt: number | null;
  status: string;
  paths: string[];
  filesScanned: number;
  filesHashed: number;
  duplicateGroups: number;
  duplicateFiles: number;
  wastedBytes: number;
  errors: number;
}

export interface DashboardStats {
  totalScans: number;
  lastScan: ScanRecord | null;
  filesScanned: number;
  duplicateFiles: number;
  duplicateGroups: number;
  wastedBytes: number;
  deletedFiles: number;
  deletedBytes: number;
  recentScans: ScanRecord[];
}

export interface DriveInfo {
  name: string;
  mountPoint: string;
  fileSystem: string;
  totalBytes: number;
  availableBytes: number;
  usedBytes: number;
  isRemovable: boolean;
  isReadOnly: boolean;
}

export interface CategoryTotal {
  category: string;
  bytes: number;
  count: number;
}

export interface LargeFile {
  path: string;
  name: string;
  size: number;
  extension: string;
  modifiedMs: number | null;
  category: string;
}

export interface LargeFilesResult {
  scannedFiles: number;
  files: LargeFile[];
  categoryTotals: CategoryTotal[];
  elapsedMs: number;
  errors: number;
}

export interface DiskNode {
  name: string;
  path: string;
  size: number;
  isDir: boolean;
  percent: number;
  category?: string;
  children: DiskNode[];
  fileCount: number;
}

export interface DiskMapResult {
  root: string;
  totalBytes: number;
  scannedFiles: number;
  scannedDirs: number;
  freeBytes: number;
  capacityBytes: number;
  categories: CategoryTotal[];
  tree: DiskNode;
  elapsedMs: number;
  errors: number;
}

export interface SimilarImage {
  file: FileEntry;
  phash: string;
  distance: number;
  thumbnail: string | null;
}

export interface SimilarGroup {
  id: number;
  referencePath: string;
  maxDistance: number;
  images: SimilarImage[];
}

export interface SimilarPhotosResult {
  scanId: string;
  groups: SimilarGroup[];
  imagesScanned: number;
  imagesHashed: number;
  errors: number;
  threshold: number;
  elapsedMs: number;
}

export interface SimilarProgress {
  scanId: string;
  phase: string;
  imagesFound: number;
  imagesToHash: number;
  imagesHashed: number;
  currentFile: string | null;
  groupsFound: number;
  errors: number;
  elapsedMs: number;
}

export interface AppSettings {
  similarPhotosThreshold: number;
  minFileSize: number;
  hashConcurrency: number;
  excludedDirNames: string[];
  confirmDeletion: boolean;
  verifyHashBeforeDelete: boolean;
  useRecycleBin: boolean;
  historyLimit: number;
}

export interface DeletionEntry {
  path: string;
  size: number;
  name: string;
}

export interface DeletionOutcome {
  path: string;
  success: boolean;
  size: number;
  reason?: string;
}

export interface DeletionPlan {
  allowed: DeletionEntry[];
  rejected: DeletionOutcome[];
  totalBytes: number;
  usesRecycleBin: boolean;
}

export interface DeletionRequest {
  paths: string[];
  scanId: string | null;
  expectedHashes: Record<string, string>;
  groupMembers: string[][];
  confirmed: boolean;
}

export interface DeletionReport {
  deleted: number;
  failed: number;
  freedBytes: number;
  restoredToTrash: boolean;
  outcomes: DeletionOutcome[];
  durationMs: number;
}

export interface DeletionLogEntry {
  id: number;
  deletedAt: number;
  path: string;
  size: number;
  sha256: string | null;
  scanId: string | null;
  method: string;
  success: boolean;
  message: string | null;
}

export interface ActiveScanInfo {
  scanId: string;
  kind: string;
  paused: boolean;
}

export interface QuickFolder {
  label: string;
  path: string;
}

export interface FolderEstimate {
  files: number;
  bytes: number;
}

export type PageId =
  | "dashboard"
  | "scan"
  | "duplicates"
  | "similar"
  | "large"
  | "diskmap"
  | "history"
  | "settings";
