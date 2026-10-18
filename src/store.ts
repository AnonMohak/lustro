import { create } from "zustand";
import { api, type AppInfo, type DriveInfo, type ScanProgress, type ScanSummary } from "./lib/api";
import { normalizeRoot } from "./lib/scanText";

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
  /** Finished scans by root path, for example "C:\". */
  scans: Record<string, ScanSummary>;
  /** The running scan. `progress` is null until the first progress event. */
  scanning: { root: string; progress: ScanProgress | null } | null;
  /** The drive or folder that Explore shows. */
  exploreRoot: string | null;
  dialog: DialogSpec | null;
  toast: Toast | null;
  /** Goes up after each delete, so views load the changed tree again. */
  treeVersion: number;
  deleteMode: "recycle" | "permanent";
  go: (screen: Screen) => void;
  setTheme: (theme: Theme) => void;
  loadInfo: () => Promise<void>;
  loadDrives: () => Promise<void>;
  loadCachedScans: () => Promise<void>;
  explore: (root: string) => void;
  startScan: (root: string) => Promise<void>;
  onScanProgress: (p: ScanProgress) => void;
  onScanDone: (s: ScanSummary) => void;
  onScanEnded: () => void;
  openDialog: (d: DialogSpec) => void;
  closeDialog: () => void;
  showToast: (message: string, error?: boolean) => void;
  afterDelete: (summaries: ScanSummary[]) => void;
  setDeleteMode: (m: "recycle" | "permanent") => void;
}

const THEME_KEY = "lustro.theme";
const DELETE_KEY = "lustro.deleteMode";

function savedDeleteMode(): "recycle" | "permanent" {
  try {
    return localStorage.getItem(DELETE_KEY) === "permanent" ? "permanent" : "recycle";
  } catch {
    return "recycle";
  }
}

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

export const useStore = create<State>((set, get) => ({
  screen: "home",
  theme: savedTheme(),
  info: null,
  drives: null,
  scans: {},
  scanning: null,
  exploreRoot: null,
  dialog: null,
  toast: null,
  treeVersion: 0,
  deleteMode: savedDeleteMode(),
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
  loadCachedScans: async () => {
    const list = await api.loadCachedScans();
    const scans = { ...get().scans };
    for (const s of list) scans[s.root] ??= s;
    set({ scans, exploreRoot: get().exploreRoot ?? list[0]?.root ?? null });
  },
  explore: (root) => set({ exploreRoot: normalizeRoot(root), screen: "explore" }),
  startScan: async (path) => {
    const root = normalizeRoot(path);
    set({ scanning: { root, progress: null }, exploreRoot: root, screen: "explore" });
    try {
      await api.startScan(root);
    } catch (e) {
      set({ scanning: null });
      get().showToast(String(e), true);
    }
  },
  onScanProgress: (p) => {
    const s = get().scanning;
    if (s) set({ scanning: { root: s.root, progress: p } });
  },
  onScanDone: (summary) => set({ scans: { ...get().scans, [summary.root]: summary }, scanning: null }),
  onScanEnded: () => set({ scanning: null }),
  openDialog: (dialog) => set({ dialog }),
  closeDialog: () => set({ dialog: null }),
  showToast: (message, error) => {
    clearTimeout(toastTimer);
    set({ toast: { message, error } });
    toastTimer = setTimeout(() => set({ toast: null }), 4000);
  },
  afterDelete: (summaries) => {
    const scans = { ...get().scans };
    for (const s of summaries) scans[s.root] = s;
    set({ scans, treeVersion: get().treeVersion + 1 });
  },
  setDeleteMode: (deleteMode) => {
    try {
      localStorage.setItem(DELETE_KEY, deleteMode);
    } catch {
      // Not saved; the choice still applies to this session.
    }
    set({ deleteMode });
  },
}));
