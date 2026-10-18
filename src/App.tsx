import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import type { IconName } from "./components/Icon";
import { Dialog, Toast } from "./components/Overlays";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TitleBar } from "./components/TitleBar";
import { api, type ScanErrorEvent, type ScanProgress, type ScanSummary } from "./lib/api";
import { formatCount, formatDuration } from "./lib/format";
import { rootLabel } from "./lib/scanText";
import { ComingSoon } from "./screens/ComingSoon";
import { Explore } from "./screens/Explore";
import { Files } from "./screens/Files";
import { Home } from "./screens/Home";
import { Settings } from "./screens/Settings";
import { useStore, type Screen } from "./store";

const LATER: Partial<Record<Screen, [string, IconName]>> = {
  dupes: ["M7", "dupe"],
  clean: ["M5", "clean"],
  apps: ["M7", "apps"],
  history: ["M4", "history"],
  search: ["M8", "search"],
};

function useScanEvents() {
  const { t } = useTranslation();
  useEffect(() => {
    const st = useStore.getState;
    const subs = [
      listen<ScanProgress>("scan-progress", (e) => st().onScanProgress(e.payload)),
      listen<ScanSummary>("scan-done", (e) => {
        st().onScanDone(e.payload);
        st().showToast(
          t("scan.doneToast", {
            root: rootLabel(e.payload.root),
            files: formatCount(e.payload.files),
            elapsed: formatDuration(e.payload.elapsedMs),
          }),
        );
      }),
      listen<ScanErrorEvent>("scan-error", (e) => {
        st().onScanEnded();
        if (e.payload.cancelled) st().showToast(t("scan.cancelled"));
        else st().showToast(t("scan.failed", { error: e.payload.message }), true);
      }),
    ];
    return () => subs.forEach((p) => p.then((off) => off()));
  }, [t]);
}

function useTheme() {
  const theme = useStore((s) => s.theme);
  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
    // The window theme drives the Mica tint and the system context menus.
    getCurrentWindow()
      .setTheme(theme === "system" ? null : theme)
      .catch(() => {});
  }, [theme]);
}

export default function App() {
  const { t } = useTranslation();
  const screen = useStore((s) => s.screen);
  const go = useStore((s) => s.go);

  useTheme();
  useScanEvents();
  useEffect(() => {
    const { loadInfo, loadDrives, loadCachedScans, showToast } = useStore.getState();
    loadInfo().catch((e) => showToast(String(e), true));
    loadDrives().catch((e) => showToast(String(e), true));
    loadCachedScans()
      .catch((e) => showToast(String(e), true))
      .then(() => api.takeStartupScan())
      .then((path) => {
        if (path) useStore.getState().startScan(path);
      })
      .catch(() => {});
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        go("search");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go]);

  const later = LATER[screen];
  return (
    <div className="app">
      <TitleBar />
      <div className="wbody">
        <Sidebar />
        {screen === "explore" && <Explore />}
        {screen === "files" && <Files />}
        {screen !== "explore" && screen !== "files" && (
          <main className="main">
            {screen === "home" && <Home />}
            {screen === "settings" && <Settings />}
            {later && <ComingSoon screen={t(`nav.${screen}`)} milestone={later[0]} icon={later[1]} />}
          </main>
        )}
      </div>
      <StatusBar />
      <Dialog />
      <Toast />
    </div>
  );
}
