// Custom title bar with Windows 11 style window controls.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X, Copy } from "lucide-react";
import { useEffect, useState } from "react";
import { isTauri } from "../services/api";
import { useT } from "../i18n";

export function TitleBar() {
  const { t } = useT();
  const [maximized, setMaximized] = useState(false);
  const [appWindow, setAppWindow] = useState<ReturnType<typeof getCurrentWindow> | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    try {
      const win = getCurrentWindow();
      setAppWindow(win);
      void win.isMaximized().then(setMaximized);
      const unlisten = win.onResized(() => {
        void win.isMaximized().then(setMaximized);
      });
      return () => {
        void unlisten.then((f) => f());
      };
    } catch {
      return;
    }
  }, []);

  const minimize = () => void appWindow?.minimize();
  const toggleMaximize = async () => {
    await appWindow?.toggleMaximize();
    setMaximized((await appWindow?.isMaximized()) ?? false);
  };
  const close = () => void appWindow?.close();

  return (
    <div className="drag-region flex h-10 shrink-0 items-center justify-between border-b border-line-soft bg-surface-0/80 pl-4 backdrop-blur-xl">
      <div className="flex items-center gap-2.5">
        <img src="/app-icon.svg" alt="" className="h-4 w-4" draggable={false} />
        <span className="text-[12px] font-medium tracking-wide text-text-secondary">
          {t("app.brand")}
        </span>
      </div>

      <div className="no-drag flex h-full items-center">
        <button
          onClick={minimize}
          aria-label={t("titlebar.minimize")}
          className="flex h-full w-12 items-center justify-center text-text-secondary transition-colors hover:bg-white/[0.07] hover:text-text-primary"
        >
          <Minus size={15} strokeWidth={1.8} />
        </button>
        <button
          onClick={() => void toggleMaximize()}
          aria-label={maximized ? t("titlebar.restore") : t("titlebar.maximize")}
          className="flex h-full w-12 items-center justify-center text-text-secondary transition-colors hover:bg-white/[0.07] hover:text-text-primary"
        >
          {maximized ? (
            <Copy size={13} strokeWidth={1.8} className="-scale-x-100" />
          ) : (
            <Square size={12} strokeWidth={1.8} />
          )}
        </button>
        <button
          onClick={close}
          aria-label={t("titlebar.close")}
          className="flex h-full w-12 items-center justify-center text-text-secondary transition-colors hover:bg-[#c42b1c] hover:text-white"
        >
          <X size={16} strokeWidth={1.8} />
        </button>
      </div>
    </div>
  );
}
