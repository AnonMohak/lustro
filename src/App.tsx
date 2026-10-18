import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import type { IconName } from "./components/Icon";
import { Dialog, Toast } from "./components/Overlays";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TitleBar } from "./components/TitleBar";
import { ComingSoon } from "./screens/ComingSoon";
import { Home } from "./screens/Home";
import { Settings } from "./screens/Settings";
import { useStore, type Screen } from "./store";

const LATER: Partial<Record<Screen, [string, IconName]>> = {
  explore: ["M3", "map"],
  files: ["M3", "bigfile"],
  dupes: ["M7", "dupe"],
  clean: ["M5", "clean"],
  apps: ["M7", "apps"],
  history: ["M4", "history"],
  search: ["M8", "search"],
};

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
  useEffect(() => {
    const { loadInfo, loadDrives, showToast } = useStore.getState();
    loadInfo().catch((e) => showToast(String(e), true));
    loadDrives().catch((e) => showToast(String(e), true));
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
        <main className="main">
          {screen === "home" && <Home />}
          {screen === "settings" && <Settings />}
          {later && <ComingSoon screen={t(`nav.${screen}`)} milestone={later[0]} icon={later[1]} />}
        </main>
      </div>
      <StatusBar />
      <Dialog />
      <Toast />
    </div>
  );
}
