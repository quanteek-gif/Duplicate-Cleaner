// Confirmation dialog: shows exactly what will be moved to the Recycle Bin,
// calls `preview_deletion` (backend validation) and only then `delete_files`.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, FolderInput, Recycle, ShieldCheck, Trash2 } from "lucide-react";
import type { DeletionPlan, DeletionReport, DeletionRequest } from "../types";
import { api, errorMessage, isTauri } from "../services/api";
import { formatBytes, formatNumber } from "../services/format";
import { localizeBackend, tc, useT } from "../i18n";
import { Badge, Button, Modal, Spinner, useToast } from "./ui";

export interface DeletionTarget {
  paths: string[];
  expectedHashes: Record<string, string>;
  groupMembers: string[][];
  scanId: string | null;
}

export function DeletionDialog({
  target,
  open,
  onClose,
  onCompleted,
}: {
  target: DeletionTarget | null;
  open: boolean;
  onClose: () => void;
  onCompleted?: (report: DeletionReport) => void;
}) {
  const toast = useToast();
  const { t } = useT();
  const [confirmEnabled, setConfirmEnabled] = useState(true);

  const [plan, setPlan] = useState<DeletionPlan | null>(null);
  const [loading, setLoading] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const request = useMemo<DeletionRequest | null>(() => {
    if (!target) return null;
    return {
      paths: target.paths,
      scanId: target.scanId,
      expectedHashes: target.expectedHashes,
      groupMembers: target.groupMembers,
      confirmed: confirmEnabled,
    };
  }, [target, confirmEnabled]);

  useEffect(() => {
    if (!open || !request) {
      setPlan(null);
      setError(null);
      return;
    }
    if (!isTauri()) {
      setError(t("common.desktopBackendRequired"));
      return;
    }
    setLoading(true);
    setError(null);
    void api
      .getSettings()
      .then((settings) => setConfirmEnabled(settings.confirmDeletion))
      .catch(() => undefined);
    void api
      .previewDeletion(request)
      .then(setPlan)
      .catch((reason: unknown) => setError(errorMessage(reason)))
      .finally(() => setLoading(false));
  }, [open, request]);

  /**
   * `userConsent` is a call parameter rather than component state on purpose:
   * the click handler would set the state and call `confirm()` in the same
   * turn, so a state read inside `confirm` would still see the old value.
   *
   * Consent comes from an explicit click on this dialog. The "ask before
   * deleting" setting only decides whether the dialog stops for the user; when
   * it is off the dialog auto-confirms after showing the preview.
   */
  const confirm = useCallback(async (userConsent: boolean) => {
    if (!request) return;
    const payload = { ...request, confirmed: userConsent || !confirmEnabled };
    setDeleting(true);
    try {
      const report = await api.deleteFiles(payload);
      onCompleted?.(report);
      if (report.deleted > 0) {
        toast.push(
          "success",
          t("delete.moved", { files: tc("file", report.deleted) }),
          t("delete.freed", { size: formatBytes(report.freedBytes) }),
        );
      }
      if (report.failed > 0) {
        const firstFailure = report.outcomes.find((outcome) => !outcome.success && outcome.reason);
        toast.push(
          "error",
          t("delete.notDeleted", { items: tc("item", report.failed) }),
          localizeBackend(firstFailure?.reason),
        );
      }
      onClose();
    } catch (reason) {
      toast.push("error", t("delete.failed"), localizeBackend(errorMessage(reason)));
    } finally {
      setDeleting(false);
    }
  }, [request, confirmEnabled, onCompleted, onClose, toast]);

  const autoRan = useRef(false);
  useEffect(() => {
    if (!open || loading || !plan || confirmEnabled || autoRan.current) return;
    if (plan.allowed.length === 0) return;
    autoRan.current = true;
    void confirm(true);
  }, [open, loading, plan, confirmEnabled, confirm]);
  useEffect(() => {
    if (!open) autoRan.current = false;
  }, [open]);

  const allowed = plan?.allowed ?? [];
  const rejected = plan?.rejected ?? [];

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t("delete.title")}
      description={t("delete.description")}
      width={620}
      dismissable={!deleting}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={deleting}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="danger"
            icon={<Trash2 size={15} />}
            onClick={() => {
              void confirm(true);
            }}
            loading={deleting}
            disabled={loading || allowed.length === 0}
          >
            {t("delete.confirm", {
              items: allowed.length > 0 ? tc("item", allowed.length) : "",
            })}
          </Button>
        </>
      }
    >
      {loading && (
        <div className="flex items-center gap-3 py-6 text-[13px] text-text-secondary">
          <Spinner /> {t("delete.verifying")}
        </div>
      )}

      {error && (
        <div className="flex items-start gap-2.5 rounded-xl border border-danger/30 bg-danger/10 px-4 py-3 text-[13px] text-danger">
          <AlertTriangle size={16} className="mt-0.5 shrink-0" />
          <span>{error}</span>
        </div>
      )}

      {!loading && !error && plan && (
        <div className="space-y-4">
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone="accent">
              <Recycle size={11} /> {plan.usesRecycleBin ? t("delete.recycleBin") : t("delete.permanent")}
            </Badge>
            <Badge tone="success">
              <ShieldCheck size={11} /> {t("delete.verified", { n: formatNumber(allowed.length) })}
            </Badge>
            {rejected.length > 0 && (
              <Badge tone="warning">
                <AlertTriangle size={11} /> {t("delete.protected", { n: formatNumber(rejected.length) })}
              </Badge>
            )}
            <span className="ml-auto text-[13px] font-semibold text-text-primary">
              {t("delete.toFree", { size: formatBytes(plan.totalBytes) })}
            </span>
          </div>

          <div>
            <div className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-text-tertiary">
              {t("delete.willMove")}
            </div>
            <div className="max-h-56 space-y-1 overflow-y-auto rounded-xl border border-line-soft bg-surface-1 p-1.5">
              {allowed.length === 0 && (
                <div className="px-3 py-4 text-center text-[13px] text-text-tertiary">
                  {t("delete.nothing")}
                </div>
              )}
              {allowed.map((entry) => (
                <div
                  key={entry.path}
                  className="flex items-center gap-2.5 rounded-lg px-2.5 py-1.5 hover:bg-white/[0.04]"
                >
                  <FolderInput size={13} className="shrink-0 text-text-tertiary" />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[12.5px] text-text-primary">{entry.name}</div>
                    <div className="truncate text-[11px] text-text-tertiary" title={entry.path}>
                      {entry.path}
                    </div>
                  </div>
                  <span className="shrink-0 text-[11px] text-text-secondary">
                    {formatBytes(entry.size)}
                  </span>
                </div>
              ))}
            </div>
          </div>

          {rejected.length > 0 && (
            <div>
              <div className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-warning">
                {t("delete.willProtect")}
              </div>
              <div className="max-h-36 space-y-1 overflow-y-auto rounded-xl border border-warning/20 bg-warning/[0.05] p-1.5">
                {rejected.map((entry) => (
                  <div key={entry.path} className="rounded-lg px-2.5 py-1.5">
                    <div className="truncate text-[12px] text-text-secondary">{entry.path}</div>
                    <div className="text-[11px] text-warning/90">{localizeBackend(entry.reason)}</div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </Modal>
  );
}
