// Disk Map: treemap visualisation of folder sizes + category breakdown.

import { useEffect, useMemo, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  ChevronLeft,
  Database,
  Folder,
  FolderOpen,
  HardDrive,
  Loader2,
  RefreshCw,
} from "lucide-react";
import type { DiskMapResult, DiskNode, DriveInfo } from "../types";
import { api, errorMessage, isTauri } from "../services/api";
import {
  CATEGORY_COLORS,
  categoryLabel,
  driveLabel,
  formatBytes,
  formatNumber,
  percent,
} from "../services/format";
import { localizeBackend, tc, useT } from "../i18n";
import { Badge, Button, Card, EmptyState, SectionTitle, Spinner, useToast } from "../components/ui";

const DEPTH_COLORS = [
  ["#4f8cff", "#3a6fd8", "#2f5fb8", "#254e98"],
  ["#8b6cff", "#7457e0", "#5f45c4", "#4b36a3"],
  ["#38c7f5", "#2aa4cc", "#2183a6", "#19637f"],
];

export function DiskMapPage() {
  const toast = useToast();
  const { t } = useT();
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [result, setResult] = useState<DiskMapResult | null>(null);
  const [loading, setLoading] = useState<string | null>(null);
  const [current, setCurrent] = useState<DiskNode[] | null>(null);
  const [history, setHistory] = useState<DiskNode[]>([]);

  const loadDrives = async () => {
    if (!isTauri()) return;
    try {
      setDrives(await api.getDrives());
    } catch {
      /* ignore */
    }
  };

  // Fetch drives as soon as the page mounts so the tiles are always present.
  useEffect(() => {
    void loadDrives();
  }, []);

  const analyze = async (root: string) => {
    setLoading(root);
    setResult(null);
    setHistory([]);
    try {
      const analysis = await api.analyzeDiskUsage(root);
      setResult(analysis);
      setCurrent(analysis.tree.children);
      toast.push(
        "success",
        t("disk.ready"),
        t("disk.readyBody", {
          files: tc("file", analysis.scannedFiles),
          size: formatBytes(analysis.totalBytes),
        }),
      );
    } catch (error) {
      toast.push("error", t("disk.failed"), localizeBackend(errorMessage(error)));
    } finally {
      setLoading(null);
    }
  };

  const pickFolder = async () => {
    if (!isTauri()) return;
    const picked = await openDialog({ directory: true, title: t("disk.pickTitle") });
    if (typeof picked === "string") void analyze(picked);
  };

  const drillInto = (node: DiskNode) => {
    if (!node.children || node.children.length === 0) return;
    setHistory((current) => [...current, node]);
    setCurrent(node.children);
  };

  const goBack = () => {
    if (!result) return;
    const parent = history[history.length - 1];
    if (!parent) {
      setCurrent(result.tree.children);
      return;
    }
    setHistory((current) => current.slice(0, -1));
    setCurrent(parent.children.length > 0 ? parent.children : result.tree.children);
  };

  const defaultDriveRoot = "/";

  const nodes = useMemo(() => {
    const list = current ?? [];
    const max = Math.max(...list.map((node) => node.size), 1);
    return [...list].sort((a, b) => b.size - a.size).map((node) => ({ node, weight: node.size / max }));
  }, [current]);

  // The number under the title must describe the folder currently open, not
  // the whole volume — otherwise drilling into `var` (49 MB) still claims
  // "18.5 GB analysed".
  const activeNode = history.length > 0 ? history[history.length - 1] : undefined;
  const activeTitle = activeNode?.name ?? result?.root ?? "/";
  const activeSize = activeNode ? activeNode.size : (result?.totalBytes ?? 0);
  const activeFiles = activeNode ? activeNode.fileCount : (result?.scannedFiles ?? 0);
  const activeSubtitle = result
    ? `${formatBytes(activeSize)} · ${tc("file", activeFiles)}` +
      (history.length === 0
        ? t("disk.analysedIn", { seconds: (result.elapsedMs / 1000).toFixed(1) })
        : "")
    : "";

  return (
    <div className="mx-auto max-w-[1180px] space-y-5">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-[26px] font-semibold tracking-tight text-text-primary">{t("disk.title")}</h1>
          <p className="mt-1 text-[13px] text-text-secondary">
            {t("disk.subtitle")}
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="secondary" icon={<FolderOpen size={14} />} onClick={() => void pickFolder()}>
            {t("disk.chooseFolder")}
          </Button>
          <Button
            variant="primary"
            icon={loading ? <Loader2 size={15} className="animate-spin" /> : <HardDrive size={15} />}
            onClick={() => {
              const first = drives[0];
              if (first) {
                void analyze(first.mountPoint);
              } else {
                void loadDrives().then(() => {
                  // state updates asynchronously — start with the known prefix
                  void analyze(defaultDriveRoot);
                });
              }
            }}
            disabled={loading !== null}
          >
            {t("disk.analyzeSystem")}
          </Button>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {drives.map((drive) => (
          <button
            key={drive.mountPoint}
            onClick={() => void analyze(drive.mountPoint)}
            disabled={loading !== null}
            className="card card-hover flex items-center gap-3 p-4 text-left disabled:opacity-60"
          >
            <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-accent/12 text-accent">
              <HardDrive size={16} />
            </span>
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
            {loading === drive.mountPoint && <Spinner size={15} />}
          </button>
        ))}
        {drives.length === 0 && (
          <Card className="col-span-full p-4 text-center text-[12.5px] text-text-tertiary">
            {t("disk.noDrives")}
          </Card>
        )}
      </div>

      {loading && !result && (
        <Card className="flex items-center gap-3 p-6 text-[13px] text-text-secondary">
          <Spinner /> {t("disk.walking")}
        </Card>
      )}

      {result && (
        <>
          <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
            <Stat label={t("disk.statTotal")} value={formatBytes(result.totalBytes)} />
            <Stat label={t("disk.statFiles")} value={formatNumber(result.scannedFiles)} />
            <Stat label={t("disk.statFolders")} value={formatNumber(result.scannedDirs)} />
            <Stat
              label={t("disk.statFree")}
              value={result.capacityBytes > 0 ? formatBytes(result.freeBytes) : "—"}
              hint={
                result.capacityBytes > 0
                  ? t("disk.statFreeHint", {
                      percent: percent(result.freeBytes, result.capacityBytes).toFixed(0),
                      size: formatBytes(result.capacityBytes),
                    })
                  : undefined
              }
            />
          </div>

          <Card className="p-5">
            <div className="mb-3 flex flex-wrap items-center gap-2">
              <SectionTitle
                title={activeTitle}
                subtitle={activeSubtitle}
              />
              <div className="ml-auto flex items-center gap-2">
                <Button size="sm" variant="ghost" icon={<ChevronLeft size={14} />} onClick={goBack} disabled={history.length === 0}>
                  {t("common.up")}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<RefreshCw size={13} />}
                  onClick={() => void analyze(result.root)}
                >
                  {t("common.rescan")}
                </Button>
              </div>
            </div>

            <div className="flex h-[320px] flex-wrap gap-1.5 overflow-hidden rounded-xl bg-surface-1 p-1.5">
              {nodes.map(({ node, weight }, index) => {
                const palette = DEPTH_COLORS[index % DEPTH_COLORS.length]?.[0] ?? "#4f8cff";
                const flexBasis = Math.max(8, weight * 100);
                return (
                  <button
                    key={node.path}
                    onClick={() => drillInto(node)}
                    className="group relative flex min-w-[110px] flex-col justify-between overflow-hidden rounded-lg p-2.5 text-left transition-transform duration-150 hover:scale-[0.99] hover:brightness-110"
                    style={{
                      flex: `${flexBasis} 1 0%`,
                      minHeight: node.size > result.totalBytes * 0.15 ? "100%" : "52px",
                      background: `linear-gradient(160deg, ${palette}38, ${palette}18)`,
                      border: `1px solid ${palette}55`,
                    }}
                    title={`${node.name} — ${formatBytes(node.size)} (${tc("file", node.fileCount)})`}
                  >
                    <span className="flex items-center gap-1.5 truncate text-[12px] font-medium text-text-primary">
                      <Folder size={12} style={{ color: palette }} />
                      <span className="truncate">{node.name}</span>
                    </span>
                    <span className="mt-1 flex items-baseline justify-between gap-2">
                      <span className="text-[10.5px] text-text-secondary">{formatBytes(node.size)}</span>
                      <span className="text-[10px] text-text-tertiary">{node.percent.toFixed(1)}%</span>
                    </span>
                    <span
                      className="absolute bottom-0 left-0 h-[2px] transition-all"
                      style={{ width: `${Math.min(100, weight * 100)}%`, background: palette }}
                    />
                  </button>
                );
              })}
              {nodes.length === 0 && (
                <div className="flex w-full items-center justify-center text-[13px] text-text-tertiary">
                  {t("disk.onlyFiles")}
                </div>
              )}
            </div>
          </Card>

          <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
            <Card className="p-5">
              <SectionTitle title={t("disk.largestDirs")} subtitle={t("disk.largestDirsSubtitle")} />
              <div className="space-y-2.5">
                {(current ?? []).slice(0, 8).map((node) => (
                  <button
                    key={node.path}
                    onClick={() => drillInto(node)}
                    className="w-full text-left"
                  >
                    <div className="mb-1 flex items-center justify-between gap-3 text-[12px]">
                      <span className="flex min-w-0 items-center gap-1.5 text-text-primary">
                        <Folder size={12} className="shrink-0 text-text-tertiary" />
                        <span className="truncate">{node.name}</span>
                      </span>
                      <span className="shrink-0 text-text-tertiary">{formatBytes(node.size)}</span>
                    </div>
                    <div className="h-1.5 overflow-hidden rounded-full bg-white/[0.06]">
                      <div
                        className="h-full rounded-full bg-accent/80"
                        style={{ width: `${Math.min(100, node.percent)}%` }}
                      />
                    </div>
                  </button>
                ))}
              </div>
            </Card>

            <Card className="p-5">
              <SectionTitle title={t("disk.byCategory")} subtitle={t("disk.byCategorySubtitle")} />
              <div className="space-y-3">
                {result.categories.map((total) => (
                  <div key={total.category}>
                    <div className="mb-1 flex items-center justify-between text-[12px]">
                      <span className="flex items-center gap-2 text-text-primary">
                        <span
                          className="h-2 w-2 rounded-full"
                          style={{ background: CATEGORY_COLORS[total.category] ?? "#6b7079" }}
                        />
                        {categoryLabel(total.category)}
                        <span className="text-text-tertiary">({formatNumber(total.count)})</span>
                      </span>
                      <span className="text-text-tertiary">{formatBytes(total.bytes)}</span>
                    </div>
                    <div className="h-1.5 overflow-hidden rounded-full bg-white/[0.06]">
                      <div
                        className="h-full rounded-full"
                        style={{
                          width: `${percent(total.bytes, result.totalBytes)}%`,
                          background: CATEGORY_COLORS[total.category] ?? "#6b7079",
                        }}
                      />
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          </div>
        </>
      )}

      {!result && loading === null && (
        <Card>
          <EmptyState
            icon={<Database size={26} />}
            title={t("disk.emptyTitle")}
            description={t("disk.emptyBody")}
          />
        </Card>
      )}

      <div className="flex items-center gap-2 text-[11px] text-text-tertiary">
        <Badge tone="accent">{t("disk.tip")}</Badge> {t("disk.tipBody")}
      </div>
    </div>
  );
}

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <Card className="p-4">
      <div className="text-[20px] font-semibold tracking-tight text-text-primary">{value}</div>
      <div className="mt-0.5 text-[11.5px] text-text-secondary">{label}</div>
      {hint && <div className="text-[10.5px] text-text-tertiary">{hint}</div>}
    </Card>
  );
}
