import { create } from "zustand";
import { api, type AppInfo, type DriveInfo } from "./lib/api";

export type Screen = "home" | "explore" | "files" | "dupes" | "clean" | "apps" | "history" | "search" | "settings";
export type Theme = "system" | "light" | "dark";

export interface DialogSpec {
  title: string;
  body: string[];
  confirm: string;
  danger?: boolean;
  onConfirm: () => void;
}

interface Toast {
  message: string;
  error?: boolean;
}

interface State {
  screen: Screen;
  theme: Theme;
  info: AppInfo | null;
  drives: DriveInfo[] | null;
  dialog: DialogSpec | null;
  toast: Toast | null;
  go: (screen: Screen) => void;
  setTheme: (theme: Theme) => void;
  loadInfo: () => Promise<void>;
  loadDrives: () => Promise<void>;
  openDialog: (d: DialogSpec) => void;
  closeDialog: () => void;
  showToast: (message: string, error?: boolean) => void;
}

const THEME_KEY = "lustro.theme";

function savedTheme(): Theme {
  try {
    const t = localStorage.getItem(THEME_KEY);
    if (t === "light" || t === "dark" || t === "system") return t;
  } catch {
    // Storage can be unavailable. The default theme is fine then.
  }
  return "system";
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;

export const useStore = create<State>((set) => ({
  screen: "home",
  theme: savedTheme(),
  info: null,
  drives: null,
  dialog: null,
  toast: null,
  go: (screen) => set({ screen }),
  setTheme: (theme) => {
    try {
      localStorage.setItem(THEME_KEY, theme);
    } catch {
      // Not saved; the choice still applies to this session.
    }
    set({ theme });
  },
  loadInfo: async () => set({ info: await api.appInfo() }),
  loadDrives: async () => set({ drives: await api.listDrives() }),
  openDialog: (dialog) => set({ dialog }),
  closeDialog: () => set({ dialog: null }),
  showToast: (message, error) => {
    clearTimeout(toastTimer);
    set({ toast: { message, error } });
    toastTimer = setTimeout(() => set({ toast: null }), 4000);
  },
}));
