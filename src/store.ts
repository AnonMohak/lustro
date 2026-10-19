import { create } from "zustand";
import { api, type AppInfo, type DeleteProgress, type DriveInfo, type Favorite, type ScanProgress, type ScanSummary } from "./lib/api";
import { normalizeRoot } from "./lib/scanText";

export type Screen = "home" | "explore" | "files" | "dupes" | "clean" | "apps" | "history" | "search" | "settings";

export interface DialogSpec {
  title: string;
  body: string[];
  confirm: string;
  danger?: boolean;
  /** A warning in a red box under the body. */
  warn?: string;
  /** A checkbox label. The confirm button stays disabled until it is checked. */
  ack?: string;
  onConfirm: () => void;
}

export type JobKind = "recycle" | "permanent" | "clean";

/** One delete or clean run. `run` does the work and returns what it freed. */
export interface Job {
  kind: JobKind;
  items: number;
  bytes: number;
  run: () => Promise<{ freed: number; errors: number }>;
}

/** The progress block in the sidebar. It sums all queued jobs. */
export interface JobView {
  kind: JobKind;
  done: number;
  total: number;
  doneBytes: number;
  totalBytes: number;
  current: string;
  stopping: boolean;
  /** Set when the queue is empty; the block then shows the result for a few seconds. */
  finished: { freed: number; recycled: number; errors: number; stopped: boolean } | null;
}

interface Toast {
  message: string;
  error?: boolean;
}

interface State {
  screen: Screen;
  info: AppInfo | null;
  drives: DriveInfo[] | null;
  /** Finished scans by root path, for example "C:\". */
  scans: Record<string, ScanSummary>;
  /** The running scan. `progress` is null until the first progress event. */
  scanning: { root: string; progress: ScanProgress | null } | null;
  /** The drive or folder that Explore shows. */
  exploreRoot: string | null;
  /** A folder that Explore zooms into once it shows `root` (set by a favourite). */
  exploreTarget: { root: string; id: number } | null;
  favorites: Favorite[];
  dialog: DialogSpec | null;
  toast: Toast | null;
  /** Goes up after each delete, so views load the changed tree again. */
  treeVersion: number;
  deleteMode: "recycle" | "permanent";
  devAgeDays: number;
  go: (screen: Screen) => void;
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
  setDevAgeDays: (d: number) => void;
  /** Removes a scan from the list and deletes its saved cache. */
  forgetScan: (root: string) => Promise<void>;
  loadFavorites: () => Promise<void>;
  setFavorite: (path: string, on: boolean) => Promise<void>;
  /** Opens a favourite in Explore, in the smallest scan that holds it. */
  openFavorite: (f: Favorite) => void;
  /** Settings > Allow deleting app folders (a backend setting). With admin rights, it unlocks
   *  app folders in Program Files, behind a warning. */
  allowAppFolders: boolean;
  loadAllowAppFolders: () => Promise<void>;
  setAllowAppFolders: (on: boolean) => void;
  job: JobView | null;
  /** Queues a delete or clean run. Resolves when it is done (or dropped by Stop). */
  runJob: (job: Job) => Promise<void>;
  /** Stops the running job after its current item and drops the queued ones. */
  cancelJobs: () => void;
  onDeleteProgress: (p: DeleteProgress) => void;
}

const DELETE_KEY = "lustro.deleteMode";
const DEV_AGE_KEY = "lustro.devAgeDays";
function savedDevAge(): number {
  try {
    const v = Number(localStorage.getItem(DEV_AGE_KEY));
    return v > 0 ? v : 30;
  } catch {
    return 30;
  }
}

function savedDeleteMode(): "recycle" | "permanent" {
  try {
    return localStorage.getItem(DELETE_KEY) === "permanent" ? "permanent" : "recycle";
  } catch {
    return "recycle";
  }
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;

// The job queue. Jobs run one at a time; the sidebar block shows their sum.
const jobQueue: { job: Job; resolve: () => void }[] = [];
let jobCurrent: Job | null = null;
let jobRunning = false;
/** Items and bytes of the jobs that already ended. */
let jobBase = { done: 0, bytes: 0 };
let jobResult = { freed: 0, recycled: 0, errors: 0 };
let jobTimer: ReturnType<typeof setTimeout> | undefined;

async function pumpJobs() {
  jobRunning = true;
  while (jobQueue.length > 0) {
    const { job, resolve } = jobQueue.shift()!;
    jobCurrent = job;
    useStore.setState((s) => ({ job: s.job && { ...s.job, kind: job.kind, current: "" } }));
    try {
      const r = await job.run();
      jobResult.errors += r.errors;
      if (job.kind === "recycle") jobResult.recycled += r.freed;
      else jobResult.freed += r.freed;
    } catch (e) {
      jobResult.errors += 1;
      useStore.getState().showToast(String(e), true);
    }
    jobBase = { done: jobBase.done + job.items, bytes: jobBase.bytes + job.bytes };
    jobCurrent = null;
    resolve();
  }
  jobRunning = false;
  const v = useStore.getState().job;
  if (!v) return;
  const full = v.stopping ? {} : { done: v.total, doneBytes: v.totalBytes };
  useStore.setState({ job: { ...v, ...full, current: "", finished: { ...jobResult, stopped: v.stopping } } });
  jobTimer = setTimeout(() => useStore.setState({ job: null }), 4000);
}

export const useStore = create<State>((set, get) => ({
  screen: "home",
  info: null,
  drives: null,
  scans: {},
  scanning: null,
  exploreRoot: null,
  exploreTarget: null,
  favorites: [],
  allowAppFolders: false,
  loadAllowAppFolders: async () => set({ allowAppFolders: (await api.getSettings()).allowAppFolders }),
  setAllowAppFolders: (allowAppFolders) => set({ allowAppFolders }),
  job: null,
  runJob: (job) =>
    new Promise<void>((resolve) => {
      clearTimeout(jobTimer);
      let v = get().job;
      if (!v || v.finished) {
        jobBase = { done: 0, bytes: 0 };
        jobResult = { freed: 0, recycled: 0, errors: 0 };
        v = { kind: job.kind, done: 0, total: 0, doneBytes: 0, totalBytes: 0, current: "", stopping: false, finished: null };
      }
      set({ job: { ...v, total: v.total + job.items, totalBytes: v.totalBytes + job.bytes } });
      jobQueue.push({ job, resolve });
      if (!jobRunning) void pumpJobs();
    }),
  cancelJobs: () => {
    const v = get().job;
    if (!v || v.finished) return;
    // Drop the queued jobs and take them out of the totals.
    let items = 0;
    let bytes = 0;
    for (const q of jobQueue.splice(0)) {
      items += q.job.items;
      bytes += q.job.bytes;
      q.resolve();
    }
    set({ job: { ...v, total: v.total - items, totalBytes: v.totalBytes - bytes, stopping: true } });
    api.cancelDelete().catch(() => {});
  },
  onDeleteProgress: (p) => {
    const v = get().job;
    const j = jobCurrent;
    if (!v || !j || v.finished) return;
    // A clean run reports freed bytes, which can differ from the measured size.
    set({
      job: {
        ...v,
        done: jobBase.done + Math.min(p.done, j.items),
        doneBytes: jobBase.bytes + Math.min(p.doneBytes, j.bytes),
        current: p.current,
      },
    });
  },
  dialog: null,
  toast: null,
  treeVersion: 0,
  deleteMode: savedDeleteMode(),
  devAgeDays: savedDevAge(),
  go: (screen) => set({ screen }),
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
  forgetScan: async (root) => {
    await api.forgetScan(root);
    const scans = { ...get().scans };
    delete scans[root];
    const exploreRoot = get().exploreRoot === root ? (Object.keys(scans)[0] ?? null) : get().exploreRoot;
    set({ scans, exploreRoot });
  },
  loadFavorites: async () => set({ favorites: await api.favorites() }),
  setFavorite: async (path, on) => {
    try {
      set({ favorites: await api.setFavorite(path, on) });
    } catch (e) {
      get().showToast(String(e), true);
    }
  },
  openFavorite: (f) => {
    const hit = f.hits[0];
    if (hit) set({ exploreRoot: hit.root, exploreTarget: { root: hit.root, id: hit.id }, screen: "explore" });
    else get().explore(f.path);
  },
  setDevAgeDays: (devAgeDays) => {
    try {
      localStorage.setItem(DEV_AGE_KEY, String(devAgeDays));
    } catch {
      // Not saved; the choice still applies to this session.
    }
    set({ devAgeDays });
  },
}));

/** True when app folders in Program Files can be deleted: admin rights and the setting on. */
export const useAppsUnlocked = () => useStore((s) => (s.info?.isAdmin ?? false) && s.allowAppFolders);
