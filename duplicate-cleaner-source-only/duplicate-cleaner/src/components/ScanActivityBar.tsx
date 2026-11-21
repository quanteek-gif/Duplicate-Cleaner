// Compact banner above the page content showing the running scan:
// phase, current file, progress and pause/cancel controls.

import { AlertTriangle, Pause, Play, Square } from "lucide-react";
import type { ScanProgress } from "../types";
import { formatNumber } from "../services/format";
import { Button, ProgressBar } from "./ui";
import { useT } from "../i18n";
import type { MessageKey } from "../i18n/en";

export const PHASE_KEYS: Record<string, MessageKey> = {
  discovering: "phase.discovering",
  sizing: "phase.sizing",
  hashing: "phase.hashing",
  grouping: "phase.grouping",
};

export function ScanActivityBar({
  progress,
  paused,
  isScanPage,
  onPause,
  onResume,
  onCancel,
  onOpen,
}: {
  progress: ScanProgress | null;
  paused: boolean;
  isScanPage: boolean;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onOpen: () => void;
}) {
  const { t } = useT();
  if (!progress) return null;
  const finished =
    progress.phase === "completed" ||
    progress.phase === "cancelled" ||
    progress.phase === "failed";
  if (finished && progress.phase !== "failed") return null;
  if (isScanPage && !finished) {
    // The Scan page renders its own full-size progress panel.
    return null;
  }
  if (finished) return null;

  const percent = computePercent(progress);
  const phaseText = t(PHASE_KEYS[progress.phase] ?? "activity.scanning");

  return (
    <div className="flex shrink-0 items-center gap-4 border-b border-line-soft bg-surface-2/70 px-6 py-2.5 backdrop-blur-xl">
      <div className="flex min-w-0 flex-1 items-center gap-3">
        <span
          className={[
            "h-2 w-2 shrink-0 rounded-full",
            paused ? "bg-warning" : "animate-pulse-soft bg-success",
          ].join(" ")}
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="text-[12px] font-medium text-text-primary">
              {paused ? t("activity.paused") : phaseText}
            </span>
            <span className="text-[11px] text-text-tertiary">
              {t("activity.files", {
                n: formatNumber(progress.filesProcessed || progress.filesDiscovered),
              })}{" "}
              · {percent.toFixed(0)}%
            </span>
          </div>
          <div className="mt-1 flex items-center gap-2">
            <ProgressBar value={percent} className="h-1 max-w-[420px]" />
          </div>
        </div>
      </div>
      <div className="flex items-center gap-1">
        <Button size="sm" variant="ghost" onClick={onOpen}>
          {t("activity.details")}
        </Button>
        {paused ? (
          <Button size="sm" variant="subtle" icon={<Play size={13} />} onClick={onResume}>
            {t("activity.resume")}
          </Button>
        ) : (
          <Button size="sm" variant="subtle" icon={<Pause size={13} />} onClick={onPause}>
            {t("activity.pause")}
          </Button>
        )}
        <Button size="sm" variant="ghost" icon={<Square size={12} />} onClick={onCancel}>
          {t("activity.cancel")}
        </Button>
      </div>
    </div>
  );
}

export function computePercent(progress: ScanProgress): number {
  switch (progress.phase) {
    case "discovering":
      return progress.filesDiscovered > 0 ? 8 : 2;
    case "sizing":
      return 14;
    case "hashing":
      if (progress.filesToHash === 0) return 90;
      return 18 + 74 * (progress.filesProcessed / progress.filesToHash);
    case "grouping":
      return 96;
    case "completed":
      return 100;
    default:
      return 0;
  }
}

export function ScanErrorNotice({ progress }: { progress: ScanProgress }) {
  const { t } = useT();
  if (progress.errors === 0) return null;
  return (
    <div className="flex items-center gap-2 text-[11px] text-warning">
      <AlertTriangle size={12} />
      {t("activity.skipped", { n: formatNumber(progress.errors) })}
    </div>
  );
}
