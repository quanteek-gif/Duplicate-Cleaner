// Application root: page routing + providers.

import { useEffect, useState } from "react";
import type { PageId } from "./types";
import { AppShell } from "./layouts/AppShell";
import { ToastProvider } from "./components/ui";
import { ScanProvider, useScan } from "./hooks/ScanContext";
import { DashboardPage } from "./pages/Dashboard";
import { ScanPage } from "./pages/ScanPage";
import { DuplicatesPage } from "./pages/DuplicatesPage";
import { SimilarPhotosPage } from "./pages/SimilarPhotosPage";
import { LargeFilesPage } from "./pages/LargeFilesPage";
import { DiskMapPage } from "./pages/DiskMapPage";
import { HistoryPage } from "./pages/HistoryPage";
import { SettingsPage } from "./pages/SettingsPage";
import { ALL_NAV_ITEMS } from "./components/Sidebar";

const PAGE_KEY = "duplicate-cleaner:page";

function isPageId(value: string): value is PageId {
  return ALL_NAV_ITEMS.some((item) => item.id === value);
}

function Router() {
  const { progress, paused, pauseScan, resumeScan, cancelScan, result } = useScan();
  const [scanRequest, setScanRequest] = useState(0);
  const [page, setPage] = useState<PageId>(() => {
    const stored = localStorage.getItem(PAGE_KEY);
    return stored && isPageId(stored) ? stored : "dashboard";
  });

  useEffect(() => {
    localStorage.setItem(PAGE_KEY, page);
  }, [page]);

  const duplicateCount = result?.stats.duplicateFiles ?? 0;

  return (
    <AppShell
      page={page}
      onNavigate={setPage}
      duplicateCount={duplicateCount}
      progress={progress}
      paused={paused}
      onPauseScan={() => void pauseScan()}
      onResumeScan={() => void resumeScan()}
      onCancelScan={() => void cancelScan()}
    >
      {page === "dashboard" && <DashboardPage onNavigate={setPage} />}
      {page === "scan" && (
        <ScanPage
          onNavigate={setPage}
          autoStart={scanRequest}
          onAutoStartHandled={() => setScanRequest(0)}
        />
      )}
      {page === "duplicates" && (
        <DuplicatesPage
          onNavigate={setPage}
          onOpenScan={() => {
            // "Scan again" means scan again: queue a start for the Scan page.
            setScanRequest((value) => value + 1);
            setPage("scan");
          }}
        />
      )}
      {page === "similar" && <SimilarPhotosPage />}
      {page === "large" && <LargeFilesPage />}
      {page === "diskmap" && <DiskMapPage />}
      {page === "history" && <HistoryPage onNavigate={setPage} />}
      {page === "settings" && <SettingsPage />}
    </AppShell>
  );
}

export default function App() {
  return (
    <ToastProvider>
      <ScanProvider>
        <Router />
      </ScanProvider>
    </ToastProvider>
  );
}
