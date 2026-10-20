import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import type { TFunction } from "../i18n/useLocale";

/** The window has no native title bar; these sit at the end of the app's own header. */
const inDesktopApp = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function WindowControls({ t }: { t: TFunction }) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!inDesktopApp) {
      return;
    }
    const appWindow = getCurrentWindow();
    const sync = () => void appWindow.isMaximized().then(setMaximized).catch(() => undefined);
    sync();
    const unlisten = appWindow.onResized(sync);
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  if (!inDesktopApp) {
    return null;
  }

  const appWindow = getCurrentWindow();
  return (
    <span className="window-controls">
      <button type="button" aria-label={t("window.minimize")} title={t("window.minimize")} onClick={() => void appWindow.minimize()}>
        <Minus size={15} strokeWidth={1.6} aria-hidden="true" />
      </button>
      <button
        type="button"
        aria-label={maximized ? t("window.restore") : t("window.maximize")}
        title={maximized ? t("window.restore") : t("window.maximize")}
        onClick={() => void appWindow.toggleMaximize()}
      >
        {maximized ? (
          <Copy size={13} strokeWidth={1.6} aria-hidden="true" />
        ) : (
          <Square size={12} strokeWidth={1.6} aria-hidden="true" />
        )}
      </button>
      <button
        type="button"
        className="close"
        aria-label={t("window.close")}
        title={t("window.close")}
        onClick={() => void appWindow.close()}
      >
        <X size={16} strokeWidth={1.6} aria-hidden="true" />
      </button>
    </span>
  );
}
