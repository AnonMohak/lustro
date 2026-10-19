import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Dialog, Toast } from "./components/Overlays";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TitleBar } from "./components/TitleBar";
import { Tooltip } from "./components/Tooltip";
import { api, type ScanErrorEvent, type ScanProgress, type ScanSummary } from "./lib/api";
import { formatCount, formatDuration } from "./lib/format";
import { rootLabel } from "./lib/scanText";
import { autoUpdateOn, checkForUpdates } from "./lib/updates";
import { Apps } from "./screens/Apps";
import { Clean } from "./screens/Clean";
import { Dupes } from "./screens/Dupes";
import { Explore } from "./screens/Explore";
import { Files } from "./screens/Files";
import { History } from "./screens/History";
import { Search } from "./screens/Search";
import { Home } from "./screens/Home";
import { Settings } from "./screens/Settings";
import { useStore } from "./store";

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
      // A second start of Lustro (Explorer menu) asks to scan a folder.
      listen<string>("open-scan", (e) => st().startScan(e.payload)),
      // Live updates: the change journal changed a fast-scan tree.
      listen<ScanSummary>("tree-changed", (e) => st().afterDelete([e.payload])),
      listen<{ root: string }>("tree-stale", (e) => {
        st().showToast(t("live.stale", { root: rootLabel(e.payload.root) }));
        st().startScan(e.payload.root);
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

export default function App() {
  const screen = useStore((s) => s.screen);
  const go = useStore((s) => s.go);

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
    // Quietly look for a new version a few seconds after start (release builds only).
    const updateTimer = import.meta.env.PROD && autoUpdateOn() ? setTimeout(() => checkForUpdates(false), 5000) : undefined;
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        go("search");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(updateTimer);
      window.removeEventListener("keydown", onKey);
    };
  }, [go]);

  return (
    <div className="app">
      <TitleBar />
      <div className="wbody">
        <Sidebar />
        {screen === "explore" && <Explore />}
        {screen === "files" && <Files />}
        {screen === "clean" && <Clean />}
        {screen === "dupes" && <Dupes />}
        {screen === "apps" && <Apps />}
        {!["explore", "files", "clean", "dupes", "apps"].includes(screen) && (
          <main className="main">
            {screen === "home" && <Home />}
            {screen === "settings" && <Settings />}
            {screen === "history" && <History />}
            {screen === "search" && <Search />}
          </main>
        )}
      </div>
      <StatusBar />
      <Dialog />
      <Toast />
      <Tooltip />
    </div>
  );
}
