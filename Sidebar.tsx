// Left navigation rail.

import {
  Clock,
  Copy,
  Files,
  HardDrive,
  Image,
  LayoutDashboard,
  ScanSearch,
  Settings,
} from "lucide-react";
import type { PageId } from "../types";
import type { MessageKey } from "../i18n/en";
import { useT } from "../i18n";

export interface NavItem {
  id: PageId;
  labelKey: MessageKey;
  icon: typeof Copy;
}

export const NAV_SECTIONS: { titleKey: MessageKey; items: NavItem[] }[] = [
  {
    titleKey: "nav.overview",
    items: [
      { id: "dashboard", labelKey: "nav.dashboard", icon: LayoutDashboard },
      { id: "scan", labelKey: "nav.scan", icon: ScanSearch },
    ],
  },
  {
    titleKey: "nav.results",
    items: [
      { id: "duplicates", labelKey: "nav.duplicates", icon: Copy },
      { id: "similar", labelKey: "nav.similar", icon: Image },
      { id: "large", labelKey: "nav.large", icon: Files },
      { id: "diskmap", labelKey: "nav.diskmap", icon: HardDrive },
    ],
  },
  {
    titleKey: "nav.system",
    items: [
      { id: "history", labelKey: "nav.history", icon: Clock },
      { id: "settings", labelKey: "nav.settings", icon: Settings },
    ],
  },
];

export const ALL_NAV_ITEMS: NavItem[] = NAV_SECTIONS.flatMap((section) => section.items);

export function Sidebar({
  active,
  onNavigate,
  duplicateCount,
  scanRunning,
}: {
  active: PageId;
  onNavigate: (page: PageId) => void;
  duplicateCount: number;
  scanRunning: boolean;
}) {
  const { t } = useT();
  return (
    <aside className="flex w-[236px] shrink-0 flex-col border-r border-line-soft bg-surface-1/70">
      <div className="flex items-center gap-3 px-5 pt-6 pb-5">
        <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-accent to-violet shadow-[0_8px_20px_-8px_rgba(79,140,255,0.8)]">
          <Copy size={18} className="text-white" strokeWidth={2.2} />
        </div>
        <div className="leading-tight">
          <div className="text-[13px] font-semibold text-text-primary">{t("app.brand")}</div>
          <div className="text-[10.5px] uppercase tracking-[0.14em] text-text-tertiary">
            {t("app.tagline")}
          </div>
        </div>
      </div>

      <nav className="flex-1 space-y-5 overflow-y-auto px-3">
        {NAV_SECTIONS.map((section) => (
          <div key={section.titleKey}>
            <div className="px-3 pb-1.5 text-[10.5px] font-semibold uppercase tracking-[0.12em] text-text-tertiary/80">
              {t(section.titleKey)}
            </div>
            <div className="space-y-0.5">
              {section.items.map((item) => {
                const Icon = item.icon;
                const isActive = item.id === active;
                return (
                  <button
                    key={item.id}
                    onClick={() => onNavigate(item.id)}
                    className={[
                      "group relative flex w-full items-center gap-2.5 rounded-[10px] px-3 py-2 text-left text-[13px] transition-all duration-150 focus-ring",
                      isActive
                        ? "bg-white/[0.08] font-medium text-text-primary"
                        : "text-text-secondary hover:bg-white/[0.04] hover:text-text-primary",
                    ].join(" ")}
                  >
                    <span
                      className={[
                        "absolute left-0 top-1/2 h-4 w-[3px] -translate-y-1/2 rounded-full bg-accent transition-opacity",
                        isActive ? "opacity-100" : "opacity-0",
                      ].join(" ")}
                    />
                    <Icon
                      size={16}
                      strokeWidth={1.9}
                      className={isActive ? "text-accent" : "text-text-tertiary group-hover:text-text-secondary"}
                    />
                    <span className="flex-1">{t(item.labelKey)}</span>
                    {item.id === "duplicates" && duplicateCount > 0 && (
                      <span className="rounded-md bg-accent/15 px-1.5 py-0.5 text-[10.5px] font-semibold text-accent">
                        {duplicateCount > 9999 ? "9999+" : duplicateCount}
                      </span>
                    )}
                    {item.id === "scan" && scanRunning && (
                      <span className="h-1.5 w-1.5 animate-pulse-soft rounded-full bg-success" />
                    )}
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </nav>

      <div className="px-4 pb-4">
        <div className="rounded-xl border border-line-soft bg-white/[0.03] px-3.5 py-3">
          <div className="text-[11px] font-medium text-text-secondary">
            {t("nav.localTitle")}
          </div>
          <div className="mt-1 text-[11px] leading-relaxed text-text-tertiary">
            {t("nav.localBody")}
          </div>
        </div>
      </div>
    </aside>
  );
}
