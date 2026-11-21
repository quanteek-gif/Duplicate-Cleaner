// App shell: title bar + sidebar + routed page area.

import type { ReactNode } from "react";
import type { PageId, ScanProgress } from "../types";
import { Sidebar } from "../components/Sidebar";
import { TitleBar } from "../components/TitleBar";
import { ScanActivityBar } from "../components/ScanActivityBar";

export function AppShell({
  page,
  onNavigate,
  duplicateCount,
  progress,
  paused,
  onPauseScan,
  onResumeScan,
  onCancelScan,
  children,
}: {
  page: PageId;
  onNavigate: (page: PageId) => void;
  duplicateCount: number;
  progress: ScanProgress | null;
  paused: boolean;
  onPauseScan: () => void;
  onResumeScan: () => void;
  onCancelScan: () => void;
  children: ReactNode;
}) {
  const running = progress !== null && !isFinished(progress);
  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-surface-0">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <Sidebar
          active={page}
          onNavigate={onNavigate}
          duplicateCount={duplicateCount}
          scanRunning={running}
        />
        <main className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
          <ScanActivityBar
            progress={progress}
            paused={paused}
            isScanPage={page === "scan" || page === "duplicates"}
            onPause={onPauseScan}
            onResume={onResumeScan}
            onCancel={onCancelScan}
            onOpen={() => onNavigate("scan")}
          />
          <div className="min-h-0 flex-1 overflow-y-auto">
            <div className="animate-fade-in px-8 py-7">{children}</div>
          </div>
        </main>
      </div>
    </div>
  );
}

function isFinished(progress: ScanProgress): boolean {
  return progress.phase === "completed" || progress.phase === "cancelled" || progress.phase === "failed";
}
