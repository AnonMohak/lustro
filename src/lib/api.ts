import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  version: string;
  isAdmin: boolean;
}

export type DriveKind = "ssd" | "hdd" | "removable" | "unknown";

export interface DriveInfo {
  letter: string;
  root: string;
  label: string;
  fileSystem: string;
  kind: DriveKind;
  totalBytes: number;
  freeBytes: number;
  supportsFastScan: boolean;
}

export type ScanMethod = "mft" | "walk";

export interface ScanSummary {
  root: string;
  files: number;
  totalBytes: number;
  allocBytes: number;
  method: ScanMethod;
  scannedAt: number;
  elapsedMs: number;
  denied: number;
  /** Bytes for each kind, indexed like KINDS. */
  breakdown: number[];
}

export interface ScanProgress {
  root: string;
  files: number;
  dirs: number;
  bytes: number;
  current: string;
  fast: boolean;
}

export interface ScanErrorEvent {
  root: string;
  message: string;
  cancelled: boolean;
}

/** Same order as `Kind` in Rust (src-tauri/src/scan/kind.rs). */
export const KINDS = ["other", "video", "image", "doc", "app", "game", "archive", "system", "dev"] as const;
export type Kind = (typeof KINDS)[number];

export interface NodeDto {
  id: number;
  name: string;
  size: number;
  alloc: number;
  items: number;
  modified: number;
  kind: Kind;
  isDir: boolean;
  hasChildren: boolean;
  denied: boolean;
  link: boolean;
  hidden: boolean;
  system: boolean;
  onlineOnly: boolean;
}

export interface NodeDetail {
  node: NodeDto;
  path: string;
  /** From the root down to this node, including both. */
  ancestors: { id: number; name: string }[];
  breakdown: number[];
  protected: boolean;
}

export interface TreemapNode {
  id: number;
  name: string;
  size: number;
  kind: Kind;
  isDir: boolean;
  children?: TreemapNode[];
}

export interface FileHit {
  root: string;
  id: number;
  name: string;
  path: string;
  folder: string;
  size: number;
  modified: number;
  kind: Kind;
  protected: boolean;
}

export interface FindFilesQuery {
  /** "size": biggest first. "age": oldest first. */
  sort: "size" | "age";
  minSize?: number;
  /** Only files changed before this Unix time in milliseconds. */
  olderThan?: number;
  kinds?: Kind[];
  roots?: string[];
  limit?: number;
}

export interface DeleteTarget {
  root: string;
  id: number;
}

export interface DeleteResult {
  deleted: number;
  freedBytes: number;
  failed: { path: string; message: string }[];
  /** New totals of the scans that changed. */
  summaries: ScanSummary[];
}

export type HistoryAction = "recycle" | "permanent" | "cleaner" | "blocked";

export interface HistoryEntry {
  id: number;
  /** Unix time in milliseconds. */
  time: number;
  action: HistoryAction;
  path: string;
  size: number;
  ok: boolean;
  error?: string;
  /** True while the item can come back from the Recycle Bin. */
  restorable: boolean;
  restored: boolean;
}

export interface CleanRule {
  id: string;
  group: "sys" | "web" | "dev" | "docker" | "ide";
  name: string;
  desc: string;
  bytes: number;
  files: number;
  safe: boolean;
  admin: boolean;
  warn: boolean;
  running: string[];
  service: string | null;
  items: CleanItem[];
  unavailable: string | null;
}

export interface CleanItem {
  path: string;
  bytes: number;
  files: number;
  ageDays: number | null;
  selected: boolean;
}

export interface CleanOutcome {
  id: string;
  name: string;
  freedBytes: number;
  deletedFiles: number;
  skippedFiles: number;
  skipped: string | null;
}

export const ROOT_ID = 0;

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  listDrives: () => invoke<DriveInfo[]>("list_drives"),
  restartAsAdmin: () => invoke<void>("restart_as_admin"),
  startScan: (root: string) => invoke<void>("start_scan", { root }),
  cancelScan: () => invoke<void>("cancel_scan"),
  loadCachedScans: () => invoke<ScanSummary[]>("load_cached_scans"),
  listChildren: (root: string, id: number, limit?: number) =>
    invoke<NodeDto[]>("list_children", { root, id, limit }),
  nodeDetail: (root: string, id: number) => invoke<NodeDetail>("node_detail", { root, id }),
  treemap: (root: string, id: number, maxNodes?: number) => invoke<TreemapNode>("treemap", { root, id, maxNodes }),
  findFiles: (query: FindFilesQuery) => invoke<FileHit[]>("find_files", { query }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  takeStartupScan: () => invoke<string | null>("take_startup_scan"),
  deleteItems: (items: DeleteTarget[], permanent: boolean) =>
    invoke<DeleteResult>("delete_items", { items, permanent }),
  history: () => invoke<HistoryEntry[]>("history"),
  restore: (id: number) => invoke<void>("restore", { id }),
  cleanScan: (devAgeDays: number) => invoke<CleanRule[]>("clean_scan", { devAgeDays }),
  cleanRun: (ids: string[], items: Record<string, string[]>, devAgeDays: number) =>
    invoke<CleanOutcome[]>("clean_run", { ids, items, devAgeDays }),
};
