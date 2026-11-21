// Scan configuration: drives/folders, filters, live progress with pause/cancel.

import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  FolderOpen,
  Gauge,
  HardDrive,
  Info,
  Pause,
  Play,
  Plus,
  ScanSearch,
  Settings2,
  Square,
  X,
} from "lucide-react";
import type { DriveInfo, PageId, ScanOptions, ScanProgress, ScanResult } from "../types";
import { usePersistentPaths, addUniquePath } from "../hooks/usePersistentPaths";
import { api, errorMessage, isTauri } from "../services/api";
import {
  driveLabel,
  formatBytes,
  formatDuration,
  formatNumber,
  percent,
} from "../services/format";
import { localizeBackend, tc, useT } from "../i18n";
import type { MessageKey } from "../i18n/en";
import {
  Badge,
  Button,
  Card,
  IconButton,
  ProgressBar,
  SectionTitle,
  TextInput,
  Toggle,
  useToast,
} from "../components/ui";
import { useScan } from "../hooks/ScanContext";
import { computePercent } from "../components/ScanActivityBar";

const SIZE_PRESETS = [
  { labelKey: "scan.sizeAny" as MessageKey, min: null as number | null },
  { labelKey: "scan.size1kb" as MessageKey, min: 1024 },
  { labelKey: "scan.size100kb" as MessageKey, min: 100 * 1024 },
  { labelKey: "scan.size1mb" as MessageKey, min: 1024 * 1024 },
  { labelKey: "scan.size10mb" as MessageKey, min: 10 * 1024 * 1024 },
];

const PHASE_TEXT: Record<string, MessageKey> = {
  discovering: "phase.discovering",
  sizing: "phase.scanSize",
  hashing: "phase.scanHash",
  grouping: "phase.grouping",
  completed: "phase.completed",
  cancelled: "phase.cancelled",
  failed: "phase.failed",
};

const TARGETS_KEY = "duplicate-cleaner.scan-targets";

export function ScanPage({
  onNavigate,
  autoStart = 0,
  onAutoStartHandled,
}: {
  onNavigate: (page: PageId) => void;
  /** Incremented by callers that want a scan to start immediately (e.g. "Scan again"). */
  autoStart?: number;
  /** Called once the request has been consumed, so it cannot fire again on remount. */
  onAutoStartHandled?: () => void;
}) {
  const toast = useToast();
  const { t } = useT();
  const { startScan, cancelScan, pauseScan, resumeScan, progress, running, paused, result } =
    useScan();

  const [paths, setPaths] = usePersistentPaths(TARGETS_KEY);
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [quickFolders, setQuickFolders] = useState<{ label: string; path: string }[]>([]);
  const [extensions, setExtensions] = useState("");
  const [minSize, setMinSize] = useState<number | null>(null);
  const [maxSize, setMaxSize] = useState<number | null>(null);
  const [followSymlinks, setFollowSymlinks] = useState(false);
  const [includeSystemDirs, setIncludeSystemDirs] = useState(false);
  const [useHashCache, setUseHashCache] = useState(true);
  const [manualPath, setManualPath] = useState("");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [starting, setStarting] = useState(false);
  const [estimate, setEstimate] = useState<{ files: number; bytes: number } | null>(null);
  const lastProgress = useRef<ScanProgress | null>(null);

  useEffect(() => {
    if (progress) lastProgress.current = progress;
  }, [progress]);

  useEffect(() => {
    if (!isTauri()) return;
    void api.getDrives().then(setDrives).catch(() => undefined);
    void api.getQuickFolders().then(setQuickFolders).catch(() => undefined);
    void api
      .getSettings()
      .then((settings) => {
        setUseHashCache(true);
        if (settings.minFileSize > 0) setMinSize(settings.minFileSize);
      })
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!isTauri() || paths.length === 0) {
      setEstimate(null);
      return;
    }
    let cancelled = false;
    void api
      .estimateFolder(paths)
      .then((value) => {
        if (!cancelled) setEstimate(value);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [paths]);

  const addPath = useCallback(
    (path: string) => setPaths((current) => addUniquePath(current, path)),
    [setPaths],
  );

  const pickFolder = async () => {
    if (!isTauri()) {
      toast.push("info", t("scan.pickUnavailable"), t("scan.pickUnavailableBody"));
      return;
    }
    try {
      const selected = await openDialog({ directory: true, multiple: true, title: t("scan.pickTitle") });
      if (!selected) return;
      const list = Array.isArray(selected) ? selected : [selected];
      list.forEach(addPath);
    } catch (error) {
      toast.push("error", t("scan.pickFailed"), localizeBackend(errorMessage(error)));
    }
  };

  const options: ScanOptions = {
    paths,
    excludedPaths: [],
    excludedDirNames: [],
    extensions: extensions
      .split(/[,\s]+/)
      .map((value) => value.trim().replace(/^\./, "").toLowerCase())
      .filter(Boolean),
    minSize,
    maxSize,
    followSymlinks,
    includeSystemDirs,
    useHashCache,
  };

  const onStart = useCallback(async () => {
    if (paths.length === 0) {
      toast.push("error", t("scan.noFolders"), t("scan.noFoldersBody"));
      return;
    }
    setStarting(true);
    const scan = await startScan(options);
    setStarting(false);
    if (scan) {
      toast.push(
        "success",
        t("scan.complete"),
        t("scan.completeBody", {
          files: tc("duplicateFile", scan.stats.duplicateFiles),
          groups: tc("group", scan.stats.duplicateGroups),
        }),
      );
      onNavigate("duplicates");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- options is rebuilt each render
  }, [paths, startScan, toast, onNavigate, extensions, minSize, maxSize, followSymlinks, includeSystemDirs, useHashCache]);

  // "Scan again" from the Duplicates page should scan, not just navigate. The
  // request is consumed here and cleared by the parent: a request that survived
  // in component state would re-fire every time this page mounts.
  useEffect(() => {
    if (autoStart === 0) return;
    onAutoStartHandled?.();
    if (running || starting || paths.length === 0) return;
    void onStart();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- one start per request
  }, [autoStart]);

  const activeProgress = progress ?? (result ? completedProgress(result) : null);
  const finished =
    activeProgress &&
    (activeProgress.phase === "completed" || activeProgress.phase === "cancelled" || activeProgress.phase === "failed");

  return (
    <div className="mx-auto max-w-[1180px] space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-[26px] font-semibold tracking-tight text-text-primary">{t("scan.title")}</h1>
          <p className="mt-1 text-[13px] text-text-secondary">
            {t("scan.subtitle")}
          </p>
        </div>
        {running ? (
          <div className="flex gap-2">
            {paused ? (
              <Button variant="secondary" icon={<Play size={15} />} onClick={() => void resumeScan()}>
                {t("activity.resume")}
              </Button>
            ) : (
              <Button variant="secondary" icon={<Pause size={15} />} onClick={() => void pauseScan()}>
                {t("activity.pause")}
              </Button>
            )}
            <Button variant="danger" icon={<Square size={13} />} onClick={() => void cancelScan()}>
              {t("scan.cancelScan")}
            </Button>
          </div>
        ) : (
          <Button
            variant="primary"
            size="lg"
            icon={<ScanSearch size={16} />}
            loading={starting}
            onClick={() => void onStart()}
          >
            {t("scan.start")}
          </Button>
        )}
      </div>

      {(running || activeProgress) && (
        <Card className="p-5">
          <div className="flex items-center justify-between gap-4">
            <div className="flex items-center gap-2.5">
              <span
                className={`h-2.5 w-2.5 rounded-full ${
                  finished
                    ? activeProgress?.phase === "completed"
                      ? "bg-success"
                      : "bg-danger"
                    : paused
                      ? "bg-warning"
                      : "animate-pulse-soft bg-accent"
                }`}
              />
              <span className="text-[14px] font-medium text-text-primary">
                {t(PHASE_TEXT[activeProgress?.phase ?? "discovering"] ?? "activity.scanning")}
                {paused && !finished ? t("phase.pausedSuffix") : ""}
              </span>
              {!finished && <Badge tone="accent">{formatDuration(activeProgress?.elapsedMs ?? 0)}</Badge>}
            </div>
            <span className="text-[13px] font-semibold text-text-primary">
              {(activeProgress ? computePercent(activeProgress) : 0).toFixed(0)}%
            </span>
          </div>

          <ProgressBar
            value={activeProgress ? computePercent(activeProgress) : 0}
            indeterminate={running && !paused && activeProgress?.phase === "discovering"}
            className="mt-3 h-2"
            tone={finished && activeProgress?.phase !== "completed" ? "danger" : "accent"}
          />

          <div className="mt-4 grid grid-cols-2 gap-4 md:grid-cols-4">
            <MiniStat label={t("scan.statDiscovered")} value={formatNumber(activeProgress?.filesDiscovered ?? 0)} />
            <MiniStat
              label={t("scan.statHashed")}
              value={`${formatNumber(activeProgress?.filesProcessed ?? 0)}${
                activeProgress?.filesToHash ? ` / ${formatNumber(activeProgress.filesToHash)}` : ""
              }`}
            />
            <MiniStat label={t("scan.statGroups")} value={formatNumber(activeProgress?.groupsFound ?? 0)} />
            <MiniStat
              label={t("scan.statSkipped")}
              value={formatNumber(activeProgress?.errors ?? 0)}
              tone={activeProgress && activeProgress.errors > 0 ? "warning" : undefined}
            />
          </div>

          <div className="mt-3 flex items-center gap-2 rounded-lg bg-surface-1 px-3 py-2 text-[11.5px] text-text-tertiary">
            <Info size={12} className="shrink-0" />
            <span className="truncate font-mono">
              {activeProgress?.currentFile ||
                (finished
                  ? t("scan.finishedLine", {
                      phase: t(PHASE_TEXT[activeProgress?.phase ?? "completed"] ?? "phase.finished"),
                      files: formatNumber(activeProgress?.filesDiscovered ?? 0),
                      groups: formatNumber(activeProgress?.groupsFound ?? 0),
                    })
                  : t("phase.waiting"))}
            </span>
          </div>

          {finished && activeProgress?.message && (
            <div className="mt-3 flex items-center gap-2 text-[12.5px] text-text-secondary">
              <AlertTriangle size={13} className="text-warning" />
              {localizeBackend(activeProgress.message)}
              {activeProgress.phase !== "completed" && t("scan.openResults")}
            </div>
          )}
        </Card>
      )}

      <div className="grid grid-cols-1 gap-5 xl:grid-cols-[1.4fr_1fr]">
        {/* locations */}
        <Card className="p-5">
          <SectionTitle
            title={t("scan.locations")}
            subtitle={`${tc("item", paths.length)}${
              estimate
                ? t("scan.estimate", {
                    files: formatNumber(estimate.files),
                    size: formatBytes(estimate.bytes),
                  })
                : ""
            }`}
            action={
              <Button size="sm" variant="secondary" icon={<FolderOpen size={13} />} onClick={() => void pickFolder()}>
                {t("scan.addFolder")}
              </Button>
            }
          />

          {paths.length === 0 && (
            <div className="mb-4 rounded-xl border border-dashed border-line px-4 py-6 text-center text-[13px] text-text-tertiary">
              {t("scan.noLocations")}
            </div>
          )}

          <div className="space-y-1.5">
            {paths.map((path) => (
              <div
                key={path}
                className="flex items-center gap-2.5 rounded-xl border border-line-soft bg-surface-1 px-3 py-2"
              >
                <FolderOpen size={15} className="shrink-0 text-text-tertiary" />
                <span className="min-w-0 flex-1 truncate text-[12.5px] text-text-primary" title={path}>
                  {path}
                </span>
                <IconButton label={t("common.remove")} onClick={() => setPaths((current) => current.filter((p) => p !== path))}>
                  <X size={14} />
                </IconButton>
              </div>
            ))}
          </div>

          <div className="mt-3 flex gap-2">
            <TextInput
              className="flex-1"
              placeholder={t("scan.pathPlaceholder")}
              value={manualPath}
              onChange={(event) => setManualPath(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && manualPath.trim()) {
                  addPath(manualPath.trim());
                  setManualPath("");
                }
              }}
            />
            <Button
              variant="subtle"
              icon={<Plus size={14} />}
              onClick={() => {
                if (manualPath.trim()) {
                  addPath(manualPath.trim());
                  setManualPath("");
                }
              }}
            >
              {t("common.add")}
            </Button>
          </div>

          {drives.length > 0 && (
            <>
              <div className="mt-5 mb-2 text-[11px] font-semibold uppercase tracking-wider text-text-tertiary">
                {t("scan.drives")}
              </div>
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                {drives.map((drive) => {
                  const selected = paths.includes(drive.mountPoint);
                  return (
                    <button
                      key={drive.mountPoint}
                      onClick={() =>
                        selected
                          ? setPaths((current) => current.filter((p) => p !== drive.mountPoint))
                          : addPath(drive.mountPoint)
                      }
                      className={`flex items-center gap-3 rounded-xl border px-3 py-2.5 text-left transition-colors ${
                        selected
                          ? "border-accent/50 bg-accent/10"
                          : "border-line-soft bg-surface-1 hover:border-line hover:bg-surface-3"
                      }`}
                    >
                      <HardDrive size={17} className={selected ? "text-accent" : "text-text-tertiary"} />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[12.5px] text-text-primary">
                          {driveLabel(drive.mountPoint, drive.name)}
                        </span>
                        <span className="block text-[10.5px] text-text-tertiary">
                          {t("dashboard.freeOf", {
                            free: formatBytes(drive.availableBytes),
                            total: formatBytes(drive.totalBytes),
                          })}
                        </span>
                      </span>
                      <span className="h-8 w-8 shrink-0">
                        <MiniRing value={percent(drive.usedBytes, drive.totalBytes)} />
                      </span>
                    </button>
                  );
                })}
              </div>
            </>
          )}

          {quickFolders.length > 0 && (
            <>
              <div className="mt-5 mb-2 text-[11px] font-semibold uppercase tracking-wider text-text-tertiary">
                {t("scan.quickAdd")}
              </div>
              <div className="flex flex-wrap gap-1.5">
                {quickFolders.map((folder) => (
                  <Button
                    key={folder.path}
                    size="sm"
                    variant="ghost"
                    icon={<Plus size={12} />}
                    onClick={() => addPath(folder.path)}
                  >
                    {folder.label}
                  </Button>
                ))}
              </div>
            </>
          )}
        </Card>

        {/* filters */}
        <div className="space-y-5">
          <Card className="p-5">
            <SectionTitle title={t("scan.filters")} subtitle={t("scan.filtersSubtitle")} />
            <label className="mb-1.5 block text-[12px] text-text-secondary">{t("scan.extensions")}</label>
            <TextInput
              placeholder={t("scan.extensionsPlaceholder")}
              value={extensions}
              onChange={(event) => setExtensions(event.target.value)}
            />

            <div className="mt-4 mb-1.5 text-[12px] text-text-secondary">{t("scan.minSize")}</div>
            <div className="flex flex-wrap gap-1.5">
              {SIZE_PRESETS.map((preset) => (
                <button
                  key={preset.labelKey}
                  onClick={() => setMinSize(preset.min)}
                  className={`rounded-lg px-2.5 py-1.5 text-[11.5px] transition-colors ${
                    minSize === preset.min
                      ? "bg-accent/15 text-accent"
                      : "bg-white/[0.04] text-text-secondary hover:bg-white/[0.08]"
                  }`}
                >
                  {t(preset.labelKey)}
                </button>
              ))}
            </div>

            <div className="mt-4 grid grid-cols-2 gap-3">
              <div>
                <label className="mb-1.5 block text-[12px] text-text-secondary">{t("scan.maxSizeMb")}</label>
                <TextInput
                  type="number"
                  min={0}
                  placeholder={t("scan.unlimited")}
                  value={maxSize ? String(Math.round(maxSize / (1024 * 1024))) : ""}
                  onChange={(event) => {
                    const value = Number(event.target.value);
                    setMaxSize(event.target.value === "" || !Number.isFinite(value) ? null : Math.max(0, value) * 1024 * 1024);
                  }}
                />
              </div>
              <div>
                <label className="mb-1.5 block text-[12px] text-text-secondary">{t("scan.minSizeMb")}</label>
                <TextInput
                  type="number"
                  min={0}
                  placeholder={t("scan.auto")}
                  value={minSize ? String(+(minSize / (1024 * 1024)).toFixed(3)) : ""}
                  onChange={(event) => {
                    const value = Number(event.target.value);
                    setMinSize(event.target.value === "" || !Number.isFinite(value) ? null : Math.max(0, value) * 1024 * 1024);
                  }}
                />
              </div>
            </div>
          </Card>

          <Card className="p-5">
            <SectionTitle
              title={t("scan.engine")}
              subtitle={t("scan.engineSubtitle")}
              action={
                <IconButton label={t("scan.advanced")} active={advancedOpen} onClick={() => setAdvancedOpen((open) => !open)}>
                  <Settings2 size={15} />
                </IconButton>
              }
            />
            <Toggle
              checked={useHashCache}
              onChange={setUseHashCache}
              label={t("scan.hashCache")}
              description={t("scan.hashCacheBody")}
            />
            <Toggle
              checked={followSymlinks}
              onChange={setFollowSymlinks}
              label={t("scan.symlinks")}
              description={t("scan.symlinksBody")}
            />
            <Toggle
              checked={includeSystemDirs}
              onChange={setIncludeSystemDirs}
              label={t("scan.systemDirs")}
              description={t("scan.systemDirsBody")}
            />
            {advancedOpen && (
              <div className="mt-3 rounded-xl border border-line-soft bg-surface-1 p-3 text-[11.5px] leading-relaxed text-text-tertiary">
                <div className="flex items-center gap-1.5 font-medium text-text-secondary">
                  <Gauge size={12} /> {t("scan.performance")}
                </div>
                {t("scan.performanceBody")}
              </div>
            )}
          </Card>
        </div>
      </div>
    </div>
  );
}

function MiniStat({ label, value, tone }: { label: string; value: string; tone?: "warning" }) {
  return (
    <div className="rounded-xl border border-line-soft bg-surface-1 px-3 py-2.5">
      <div className={`text-[15px] font-semibold ${tone === "warning" ? "text-warning" : "text-text-primary"}`}>
        {value}
      </div>
      <div className="text-[10.5px] uppercase tracking-wider text-text-tertiary">{label}</div>
    </div>
  );
}

function MiniRing({ value }: { value: number }) {
  const radius = 14;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference - (Math.min(100, value) / 100) * circumference;
  const color = value > 92 ? "#f56c6c" : value > 78 ? "#f5b533" : "#4f8cff";
  return (
    <svg viewBox="0 0 34 34" className="h-8 w-8 -rotate-90">
      <circle cx="17" cy="17" r={radius} fill="none" stroke="rgba(255,255,255,0.08)" strokeWidth="4" />
      <circle
        cx="17"
        cy="17"
        r={radius}
        fill="none"
        stroke={color}
        strokeWidth="4"
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={offset}
      />
    </svg>
  );
}

/**
 * Progress for a scan that already finished. It used to report zeros, so a
 * reopened Scan page claimed "0 discovered, 0 groups" for a scan that had data —
 * the panel has to describe the result it is showing.
 */
function completedProgress(result: ScanResult): ScanProgress {
  const { stats, startedAt, finishedAt } = result;
  return {
    scanId: result.scanId,
    phase: "completed",
    filesDiscovered: stats.filesScanned,
    filesProcessed: stats.filesHashed,
    filesToHash: stats.filesHashed,
    bytesDiscovered: stats.bytesScanned,
    bytesHashed: 0,
    currentFile: null,
    duplicatesFound: stats.duplicateFiles,
    groupsFound: stats.duplicateGroups,
    errors: stats.errors,
    elapsedMs: Math.max(0, finishedAt - startedAt),
    message: null,
  };
}
