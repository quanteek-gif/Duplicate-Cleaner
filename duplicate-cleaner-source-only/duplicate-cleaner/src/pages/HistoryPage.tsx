// History: previous scans + deletion log.

import { useCallback, useEffect, useState } from "react";
import { Clock, Eraser, FileClock, FolderOpen, RefreshCw, RotateCcw } from "lucide-react";
import type { DeletionLogEntry, PageId, ScanRecord } from "../types";
import { api, errorMessage, isTauri } from "../services/api";
import {
  formatBytes,
  formatDate,
  formatRelative,
} from "../services/format";
import { localizeBackend, tc, useT } from "../i18n";
import { statusLabel } from "./Dashboard";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  SectionTitle,
  Spinner,
  Tabs,
  useToast,
} from "../components/ui";
import { useScan } from "../hooks/ScanContext";
import { shortenPath } from "../utils/formatPath";

export function HistoryPage({ onNavigate }: { onNavigate: (page: PageId) => void }) {
  const toast = useToast();
  const { t } = useT();
  const { loadScan, refreshRecent } = useScan();
  const [tab, setTab] = useState<"scans" | "deletions">("scans");
  const [scans, setScans] = useState<ScanRecord[]>([]);
  const [deletions, setDeletions] = useState<DeletionLogEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      const [scanList, deletionList] = await Promise.all([
        api.listRecentScans(100),
        api.listDeletions(300),
      ]);
      setScans(scanList);
      setDeletions(deletionList);
      await refreshRecent();
    } catch (error) {
      toast.push("error", t("history.loadFailed"), localizeBackend(errorMessage(error)));
    } finally {
      setLoading(false);
    }
  }, [refreshRecent, toast]);

  useEffect(() => {
    void load();
  }, [load]);

  const removeScan = async (scanId: string) => {
    setBusy(true);
    try {
      await api.deleteScan(scanId);
      setScans((current) => current.filter((scan) => scan.scanId !== scanId));
      await refreshRecent();
    } catch (error) {
      toast.push("error", t("history.deleteFailed"), localizeBackend(errorMessage(error)));
    } finally {
      setBusy(false);
    }
  };

  const clearDeletions = async () => {
    setBusy(true);
    try {
      await api.clearDeletionHistory();
      setDeletions([]);
      toast.push("success", t("history.cleared"));
    } catch (error) {
      toast.push("error", t("history.clearFailed"), localizeBackend(errorMessage(error)));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto max-w-[1180px] space-y-5">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-[26px] font-semibold tracking-tight text-text-primary">{t("history.title")}</h1>
          <p className="mt-1 text-[13px] text-text-secondary">
            {t("history.subtitle")}
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="secondary" icon={<RefreshCw size={14} />} loading={loading && !busy} onClick={() => void load()}>
            {t("common.refresh")}
          </Button>
          {tab === "deletions" && deletions.length > 0 && (
            <Button variant="ghost" icon={<Eraser size={14} />} loading={busy} onClick={() => void clearDeletions()}>
              {t("history.clearLog")}
            </Button>
          )}
        </div>
      </div>

      <Tabs
        value={tab}
        onChange={(value) => setTab(value as "scans" | "deletions")}
        tabs={[
          { id: "scans", label: t("history.tabScans", { n: scans.length }), icon: <Clock size={13} /> },
          {
            id: "deletions",
            label: t("history.tabDeletions", { n: deletions.length }),
            icon: <FileClock size={13} />,
          },
        ]}
      />

      {loading ? (
        <div className="flex items-center gap-3 py-16 text-[13px] text-text-secondary">
          <Spinner /> {t("history.loading")}
        </div>
      ) : tab === "scans" ? (
        scans.length === 0 ? (
          <Card>
            <EmptyState
              icon={<Clock size={26} />}
              title={t("history.emptyScans")}
              description={t("history.emptyScansBody")}
              action={<Button onClick={() => onNavigate("scan")}>{t("history.runScan")}</Button>}
            />
          </Card>
        ) : (
          <Card className="divide-y divide-line-soft/70">
            {scans.map((scan) => (
              <div key={scan.scanId} className="flex flex-wrap items-center gap-3 px-4 py-3">
                <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-white/[0.05] text-text-secondary">
                  <Clock size={15} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="truncate text-[13px] text-text-primary">
                      {scan.paths.join(", ") || t("common.unknownLocation")}
                    </span>
                    <Badge tone={scan.status === "completed" ? "success" : scan.status === "cancelled" ? "warning" : "danger"}>
                      {statusLabel(scan.status, t)}
                    </Badge>
                  </div>
                  <div className="text-[11px] text-text-tertiary">
                    {formatDate(scan.startedAt)} ({formatRelative(scan.startedAt)}) ·{" "}
                    {tc("file", scan.filesScanned)} · {tc("duplicateFile", scan.duplicateFiles)} ·{" "}
                    {tc("group", scan.duplicateGroups)}
                  </div>
                </div>
                <span className="text-right text-[12px] text-success">{formatBytes(scan.wastedBytes)}</span>
                <div className="flex gap-1">
                  <Button
                    size="sm"
                    variant="ghost"
                    icon={<FolderOpen size={13} />}
                    disabled={scan.duplicateGroups === 0}
                    title={
                      scan.duplicateGroups === 0 ? t("history.noGroupsTitle") : t("history.openResultsTitle")
                    }
                    onClick={() => {
                      void loadScan(scan.scanId).then(() => onNavigate("duplicates"));
                    }}
                  >
                    {t("history.viewResults")}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    icon={<RotateCcw size={13} />}
                    disabled={busy}
                    onClick={() => void removeScan(scan.scanId)}
                  >
                    {t("history.forget")}
                  </Button>
                </div>
              </div>
            ))}
          </Card>
        )
      ) : deletions.length === 0 ? (
        <Card>
          <EmptyState
            icon={<FileClock size={26} />}
            title={t("history.emptyDeletions")}
            description={t("history.emptyDeletionsBody")}
          />
        </Card>
      ) : (
        <Card className="overflow-hidden">
          <table className="w-full text-[12.5px]">
            <thead>
              <tr className="border-b border-line-soft text-left text-[10.5px] uppercase tracking-wider text-text-tertiary">
                <th className="px-4 py-2.5 font-medium">{t("history.colWhen")}</th>
                <th className="px-2 py-2.5 font-medium">{t("history.colPath")}</th>
                <th className="px-2 py-2.5 font-medium">{t("history.colMethod")}</th>
                <th className="px-2 py-2.5 text-right font-medium">{t("history.colSize")}</th>
                <th className="px-4 py-2.5 font-medium">{t("history.colResult")}</th>
              </tr>
            </thead>
            <tbody>
              {deletions.map((entry) => (
                <tr key={entry.id} className="border-b border-line-soft/60 last:border-0 hover:bg-white/[0.03]">
                  <td className="whitespace-nowrap px-4 py-2 text-text-tertiary">{formatDate(entry.deletedAt)}</td>
                  <td className="w-[38%] max-w-0 px-2 py-2">
                    {/* CSS `truncate` cuts the end — exactly the part that names
                        the file. Show the tail explicitly instead; the full path
                        stays in the tooltip. `direction:rtl` would also work but
                        it reorders the slashes, which reads as a different path. */}
                    <div className="truncate text-text-primary" title={entry.path}>
                      {shortenPath(entry.path)}
                    </div>
                    {entry.sha256 && (
                      <div className="truncate font-mono text-[10px] text-text-tertiary">
                        {entry.sha256.slice(0, 16)}…
                      </div>
                    )}
                  </td>
                  <td className="whitespace-nowrap px-2 py-2 text-text-tertiary">
                    {entry.method === "recycle_bin" ? t("history.recycleBin") : t("history.permanent")}
                  </td>
                  <td className="whitespace-nowrap px-2 py-2 text-right text-text-secondary">{formatBytes(entry.size)}</td>
                  <td className="px-4 py-2">
                    {entry.success ? (
                      <Badge tone="success">{t("history.deleted")}</Badge>
                    ) : (
                      <span title={localizeBackend(entry.message) || undefined}>
                        <Badge tone="danger">{t("history.failed")}</Badge>
                      </span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}

      <SectionTitle title="" />
    </div>
  );
}
