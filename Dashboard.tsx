// Dashboard: KPI cards, drive usage, quick start and recent scans.

import { useCallback, useEffect, useState } from "react";
import {
  ArrowRight,
  Clock,
  Copy,
  Files,
  FolderOpen,
  HardDrive,
  Plus,
  Sparkles,
  Trash2,
} from "lucide-react";
import type { DashboardStats, DriveInfo, PageId } from "../types";
import { api, errorMessage, isTauri } from "../services/api";
import {
  driveLabel,
  formatBytes,
  formatNumber,
  formatRelative,
  percent,
} from "../services/format";
import { localizeBackend, tc, useT } from "../i18n";
import { Badge, Button, Card, ProgressBar, SectionTitle, Spinner, useToast } from "../components/ui";
import { useScan } from "../hooks/ScanContext";

export function DashboardPage({ onNavigate }: { onNavigate: (page: PageId) => void }) {
  const toast = useToast();
  const { t } = useT();
  const { result, startScan, lastRecords } = useScan();
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [quickFolders, setQuickFolders] = useState<{ label: string; path: string }[]>([]);
  const [loading, setLoading] = useState(true);
  const [startingQuickScan, setStartingQuickScan] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      const [dashboard, driveList, folders] = await Promise.all([
        api.getDashboardStats(),
        api.getDrives().catch(() => [] as DriveInfo[]),
        api.getQuickFolders().catch(() => []),
      ]);
      setStats(dashboard);
      setDrives(driveList);
      setQuickFolders(folders);
    } catch (error) {
      toast.push("error", t("dashboard.loadFailed"), localizeBackend(errorMessage(error)));
    } finally {
      setLoading(false);
    }
  }, [toast]);

  useEffect(() => {
    void load();
  }, [load]);

  const quickScan = async (path: string, label: string) => {
    setStartingQuickScan(path);
    try {
      const scan = await startScan({
        paths: [path],
        excludedPaths: [],
        excludedDirNames: [],
        extensions: [],
        minSize: 1024,
        maxSize: null,
        followSymlinks: false,
        includeSystemDirs: false,
        useHashCache: true,
      });
      if (scan) {
        toast.push(
          "success",
          t("dashboard.scanned", { label }),
          t("dashboard.analysed", { files: tc("file", scan.stats.filesScanned) }),
        );
        onNavigate("duplicates");
      }
    } finally {
      setStartingQuickScan(null);
    }
  };

  const recent = lastRecords.length > 0 ? lastRecords : stats?.recentScans ?? [];
  const lastScan = result?.stats ?? null;
  const filesScanned = lastScan?.filesScanned ?? stats?.filesScanned ?? 0;
  const duplicatesFound = lastScan?.duplicateFiles ?? stats?.duplicateFiles ?? 0;
  const groups = lastScan?.duplicateGroups ?? stats?.duplicateGroups ?? 0;
  const wasted = lastScan?.wastedBytes ?? stats?.wastedBytes ?? 0;

  return (
    <div className="mx-auto max-w-[1180px] space-y-6">
      {/* header */}
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-[26px] font-semibold tracking-tight text-text-primary">{t("dashboard.title")}</h1>
          <p className="mt-1 text-[13px] text-text-secondary">
            {t("dashboard.subtitle")}
          </p>
        </div>
        <Button variant="primary" size="lg" icon={<Plus size={16} />} onClick={() => onNavigate("scan")}>
          {t("dashboard.newScan")}
        </Button>
      </div>

      {loading ? (
        <div className="flex items-center gap-3 py-16 text-[13px] text-text-secondary">
          <Spinner /> {t("dashboard.loading")}
        </div>
      ) : (
        <>
          {/* KPI cards */}
          <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
            <KpiCard
              icon={<Files size={18} />}
              tone="accent"
              label={t("dashboard.kpiFiles")}
              value={formatNumber(filesScanned)}
              hint={stats?.lastScan ? formatRelative(stats.lastScan.startedAt) : t("common.noScanYet")}
            />
            <KpiCard
              icon={<Copy size={18} />}
              tone="violet"
              label={t("dashboard.kpiDuplicates")}
              value={formatNumber(duplicatesFound)}
              hint={tc("group", groups)}
            />
            <KpiCard
              icon={<FolderOpen size={18} />}
              tone="cyan"
              label={t("dashboard.kpiGroups")}
              value={formatNumber(groups)}
              hint={groups > 0 ? t("dashboard.readyForReview") : t("dashboard.nothingToClean")}
            />
            <KpiCard
              icon={<Sparkles size={18} />}
              tone="success"
              label={t("dashboard.kpiReclaimable")}
              value={formatBytes(wasted)}
              hint={
                stats && stats.deletedBytes > 0
                  ? t("dashboard.alreadyFreed", { size: formatBytes(stats.deletedBytes) })
                  : t("dashboard.fromLastScan")
              }
            />
          </div>

          <div className="grid grid-cols-1 gap-5 xl:grid-cols-[1.35fr_1fr]">
            {/* recent scans */}
            <Card className="p-5">
              <SectionTitle
                title={t("dashboard.recentScans")}
                subtitle={t("dashboard.recentScansSubtitle")}
                action={
                  <Button size="sm" variant="ghost" onClick={() => onNavigate("history")}>
                    {t("dashboard.viewAll")} <ArrowRight size={13} />
                  </Button>
                }
              />
              {recent.length === 0 ? (
                <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-line py-10 text-center">
                  <Clock size={20} className="text-text-tertiary" />
                  <div className="text-[13px] text-text-secondary">{t("dashboard.noScans")}</div>
                  <Button size="sm" variant="subtle" onClick={() => onNavigate("scan")}>
                    {t("dashboard.runFirst")}
                  </Button>
                </div>
              ) : (
                <div className="space-y-1.5">
                  {recent.slice(0, 5).map((scan) => (
                    <button
                      key={scan.scanId}
                      onClick={() => onNavigate(scan.duplicateGroups > 0 ? "duplicates" : "history")}
                      className="flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left transition-colors hover:bg-white/[0.04]"
                    >
                      <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-white/[0.06] text-text-secondary">
                        <HardDrive size={15} />
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[13px] text-text-primary">
                          {scan.paths.join(", ") || t("common.unknownLocation")}
                        </span>
                        <span className="block text-[11px] text-text-tertiary">
                          {formatRelative(scan.startedAt)} ·{" "}
                          {tc("file", scan.filesScanned)} ·{" "}
                          {tc("duplicateFile", scan.duplicateFiles)}
                        </span>
                      </span>
                      <span className="shrink-0 text-right">
                        <span className="block text-[12px] font-medium text-success">
                          {formatBytes(scan.wastedBytes)}
                        </span>
                        <Badge
                          tone={
                            scan.status === "completed"
                              ? "success"
                              : scan.status === "running"
                                ? "accent"
                                : "danger"
                          }
                        >
                          {statusLabel(scan.status, t)}
                        </Badge>
                      </span>
                    </button>
                  ))}
                </div>
              )}
            </Card>

            {/* drives */}
            <Card className="p-5">
              <SectionTitle
                title={t("dashboard.diskUsage")}
                subtitle={t("dashboard.volumesDetected", { n: formatNumber(drives.length) })}
              />
              {drives.length === 0 ? (
                <div className="py-8 text-center text-[13px] text-text-tertiary">
                  {t("dashboard.noDrives")}
                </div>
              ) : (
                <div className="space-y-4">
                  {drives.slice(0, 4).map((drive) => {
                    const used = percent(drive.usedBytes, drive.totalBytes);
                    return (
                      <div key={drive.mountPoint}>
                        <div className="mb-1.5 flex items-baseline justify-between gap-3">
                          <span className="truncate text-[12.5px] text-text-primary">
                            {driveLabel(drive.mountPoint, drive.name)}
                          </span>
                          <span className="shrink-0 text-[11px] text-text-tertiary">
                            {t("dashboard.freeOf", {
                              free: formatBytes(drive.availableBytes),
                              total: formatBytes(drive.totalBytes),
                            })}
                          </span>
                        </div>
                        <ProgressBar
                          value={used}
                          tone={used > 92 ? "danger" : used > 78 ? "success" : "accent"}
                          className="h-2"
                        />
                      </div>
                    );
                  })}
                </div>
              )}
            </Card>
          </div>

          {/* quick start */}
          <Card className="p-5">
            <SectionTitle
              title={t("dashboard.quickScan")}
              subtitle={t("dashboard.quickScanSubtitle")}
            />
            <div className="flex flex-wrap gap-2">
              {quickFolders.length === 0 && (
                <div className="text-[13px] text-text-tertiary">
                  {t("dashboard.quickScanEmpty")}
                </div>
              )}
              {quickFolders.map((folder) => (
                <Button
                  key={folder.path}
                  variant="subtle"
                  size="md"
                  icon={<FolderOpen size={14} />}
                  loading={startingQuickScan === folder.path}
                  disabled={startingQuickScan !== null}
                  onClick={() => void quickScan(folder.path, folder.label)}
                >
                  {folder.label}
                </Button>
              ))}
              <Button variant="ghost" icon={<Plus size={14} />} onClick={() => onNavigate("scan")}>
                {t("dashboard.customScan")}
              </Button>
            </div>
          </Card>

          {stats && (stats.deletedFiles > 0 || stats.totalScans > 0) && (
            <Card className="flex flex-wrap items-center gap-6 p-5">
              <div className="flex items-center gap-3">
                <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-success/12 text-success">
                  <Trash2 size={16} />
                </span>
                <div>
                  <div className="text-[15px] font-semibold text-text-primary">
                    {t("dashboard.cleaned", { files: tc("file", stats.deletedFiles) })}
                  </div>
                  <div className="text-[11.5px] text-text-tertiary">
                    {t("dashboard.totals", {
                      size: formatBytes(stats.deletedBytes),
                      scans: tc("scan", stats.totalScans),
                    })}
                  </div>
                </div>
              </div>
              <Button size="sm" variant="ghost" className="ml-auto" onClick={() => onNavigate("history")}>
                {t("dashboard.deletionHistory")} <ArrowRight size={13} />
              </Button>
            </Card>
          )}
        </>
      )}
    </div>
  );
}

/** Scan status as shown in badges: completed / running / cancelled / failed. */
export function statusLabel(status: string, t: ReturnType<typeof useT>["t"]): string {
  switch (status) {
    case "completed":
      return t("phase.completed");
    case "running":
      return t("activity.scanning");
    case "cancelled":
      return t("phase.cancelled");
    case "failed":
      return t("phase.failed");
    default:
      return status;
  }
}

function KpiCard({
  icon,
  label,
  value,
  hint,
  tone,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  hint: string;
  tone: "accent" | "violet" | "cyan" | "success";
}) {
  const tones = {
    accent: "bg-accent/12 text-accent",
    violet: "bg-violet/12 text-violet",
    cyan: "bg-cyan/12 text-cyan",
    success: "bg-success/12 text-success",
  } as const;
  return (
    <Card className="card-hover p-4.5">
      <div className="flex items-start justify-between">
        <span className={`flex h-9 w-9 items-center justify-center rounded-xl ${tones[tone]}`}>
          {icon}
        </span>
      </div>
      <div className="mt-3 text-[24px] font-semibold leading-none tracking-tight text-text-primary">
        {value}
      </div>
      <div className="mt-1.5 text-[12px] text-text-secondary">{label}</div>
      <div className="mt-0.5 text-[11px] text-text-tertiary">{hint}</div>
    </Card>
  );
}
