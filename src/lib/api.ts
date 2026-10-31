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
  /** True when the change journal can keep this scan current (fast scans, admin). */
  live: boolean;
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
  /** A protected app folder in Program Files that an admin may delete after a warning. */
  appFolder: boolean;
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
  /** A protected app folder in Program Files that an admin may delete after a warning. */
  appFolder: boolean;
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

export type HistoryAction =
  "recycle" | "permanent" | "cleaner" | "blocked" | "linked" | "moved" | "cloud" | "compressed";

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
  group: "sys" | "cache" | "web" | "dev" | "docker" | "ide";
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
  /** A short name for the row (for example the app of a cache folder). */
  label?: string;
  bytes: number;
  files: number;
  ageDays: number | null;
  selected: boolean;
}

/** A clean rule of the user: delete folders with these names in untouched projects under `root`. */
export interface CustomRule {
  /** Empty for a new rule; the backend gives it an id. */
  id: string;
  name: string;
  root: string;
  /** "folders": targets are folder names; "files": targets are file patterns like *.iso. */
  kind: "folders" | "files";
  targets: string[];
  ageDays: number;
  schedule: "off" | "start" | "daily" | "weekly";
  /** Unix ms of the last run, or 0. */
  lastRun: number;
}

export interface CustomOutcome {
  freedBytes: number;
  folders: number;
  errors: number;
}

export interface CleanOutcome {
  id: string;
  name: string;
  freedBytes: number;
  deletedFiles: number;
  skippedFiles: number;
  skipped: string | null;
}

export interface DupeFile {
  root: string;
  id: number;
  path: string;
  modified: number;
  size: number;
  /** Name groups: files with the same number have the same content; 0 when no other file has it. */
  same: number;
}

export interface DupeGroup {
  /** Exact groups: the size of each copy. Name groups: the biggest file. */
  size: number;
  /** Exact groups: the content hash. Name groups: the shared name. */
  hash: string;
  /** "folder": `files` are the copies of a folder. */
  kind: "exact" | "name" | "similar" | "folder";
  /** The number of files in each copy (1, or the files of a duplicate folder). */
  items: number;
  files: DupeFile[];
}

export type DupeKind = "image" | "video" | "doc" | "archive" | "other";

export interface DupeOptions {
  minSize: number;
  includeAppData: boolean;
  mode: "exact" | "name" | "similar";
  /** Empty: all types. */
  kinds: DupeKind[];
  /** Empty: all scans. */
  onlyIn: string[];
  notIn: string[];
}

export interface DupeProgress {
  totalBytes: number;
  hashedBytes: number;
  groups: number;
}

export interface InstalledApp {
  id: string;
  name: string;
  publisher: string;
  version: string;
  source: "desktop" | "store";
  size: number | null;
  sizeFrom: "registry" | "folder" | "none";
  installed: number;
  lastUsed: number;
  location: string;
}

export interface SearchHit {
  root: string;
  id: number;
  name: string;
  path: string;
  folder: string;
  size: number;
  modified: number;
  kind: Kind;
  isDir: boolean;
  protected: boolean;
  /** A protected app folder in Program Files that an admin may delete after a warning. */
  appFolder: boolean;
}

export interface SearchResult {
  hits: SearchHit[];
  total: number;
  searched: number;
  elapsedMs: number;
}

export interface SearchArgs {
  text: string;
  kinds?: Kind[];
  minSize?: number;
  folders?: boolean;
  limit?: number;
}

export interface BackendSettings {
  keepInTray: boolean;
  lowSpaceAlert: boolean;
  lowSpacePercent: number;
  startWithWindows: boolean;
  explorerMenu: boolean;
  allowAppFolders: boolean;
  /** Folders that Lustro never suggests to delete. */
  ignore: string[];
  /** Ids of "Your rules" that run when their drive is low on space. */
  lowSpaceRules: string[];
  /** Ids of built-in clean rules that run when the home drive is low on space. */
  lowSpaceClean: string[];
  /** Rescan each scan when it is this many days old (0: never). */
  rescanDays: number;
}

/** The change between the last two scans of a drive or folder. */
export interface GrowthReport {
  root: string;
  /** Unix seconds of the two scans. */
  from: number;
  to: number;
  before: number;
  after: number;
  changes: { path: string; before: number; after: number }[];
}

export interface DriveHistory {
  root: string;
  points: { day: number; used: number; total: number }[];
  /** Days until full at the recent growth, or null when the drive does not grow. */
  daysLeft: number | null;
}

/** A built-in clean rule without its size. */
export interface CleanRuleInfo {
  id: string;
  group: CleanRule["group"];
  name: string;
  safe: boolean;
  admin: boolean;
  warn: boolean;
}

export interface FavoriteHit {
  root: string;
  id: number;
  size: number;
}

export interface Favorite {
  path: string;
  /** The scans that hold the folder, the smallest scan first. Empty when no scan holds it. */
  hits: FavoriteHit[];
}

/** The `delete-progress` event of a delete or a clean run. */
export interface DeleteProgress {
  done: number;
  doneBytes: number;
  total: number;
  /** 0 for a clean run: the cleaner measures while it runs. */
  totalBytes: number;
  /** The path (delete) or rule name (clean) that is in work now. Empty between items. */
  current: string;
}

export const ROOT_ID = 0;

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  listDrives: () => invoke<DriveInfo[]>("list_drives"),
  restartAsAdmin: () => invoke<void>("restart_as_admin"),
  startScan: (root: string) => invoke<void>("start_scan", { root }),
  cancelScan: () => invoke<void>("cancel_scan"),
  forgetScan: (root: string) => invoke<void>("forget_scan", { root }),
  loadCachedScans: () => invoke<ScanSummary[]>("load_cached_scans"),
  listChildren: (root: string, id: number, limit?: number) => invoke<NodeDto[]>("list_children", { root, id, limit }),
  nodeDetail: (root: string, id: number) => invoke<NodeDetail>("node_detail", { root, id }),
  treemap: (root: string, id: number, maxNodes?: number) => invoke<TreemapNode>("treemap", { root, id, maxNodes }),
  findFiles: (query: FindFilesQuery) => invoke<FileHit[]>("find_files", { query }),
  getSettings: () => invoke<BackendSettings>("get_settings"),
  setSettings: (settings: BackendSettings) => invoke<BackendSettings>("set_settings", { settings }),
  saveExport: (kind: "csv" | "html", fileName: string, contents: string) =>
    invoke<string | null>("save_export", { kind, fileName, contents }),
  search: (args: SearchArgs) => invoke<SearchResult>("search", { args }),
  favorites: () => invoke<Favorite[]>("favorites"),
  setFavorite: (path: string, on: boolean) => invoke<Favorite[]>("set_favorite", { path, on }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  takeStartupScan: () => invoke<string | null>("take_startup_scan"),
  /** `appFolders`: the user confirmed the app-folder warning. */
  deleteItems: (items: DeleteTarget[], permanent: boolean, appFolders = false) =>
    invoke<DeleteResult>("delete_items", { items, permanent, appFolders }),
  cancelDelete: () => invoke<void>("cancel_delete"),
  history: () => invoke<HistoryEntry[]>("history"),
  restore: (id: number) => invoke<void>("restore", { id }),
  dupesStart: (options: DupeOptions) => invoke<void>("dupes_start", { options }),
  dupesCancel: () => invoke<void>("dupes_cancel"),
  dupesResult: () => invoke<DupeGroup[] | null>("dupes_result"),
  /** Replaces copies with links to the kept file (exact groups of the last result only). */
  dupesLink: (sets: { keep: string; copies: string[] }[]) =>
    invoke<{ linked: number; freedBytes: number; errors: number; firstError: string | null }>("dupes_link", { sets }),
  /** A small preview of a photo in the last duplicate result, as a data: URL. */
  photoThumbnail: (path: string) => invoke<string | null>("photo_thumbnail", { path }),
  appsList: () => invoke<InstalledApp[]>("apps_list"),
  appUninstall: (id: string) => invoke<void>("app_uninstall", { id }),
  cleanScan: (devAgeDays: number) => invoke<CleanRule[]>("clean_scan", { devAgeDays }),
  cleanRun: (ids: string[], items: Record<string, string[]>, devAgeDays: number) =>
    invoke<CleanOutcome[]>("clean_run", { ids, items, devAgeDays }),
  customRules: () => invoke<CustomRule[]>("custom_rules"),
  growthReports: () => invoke<GrowthReport[]>("growth_reports"),
  /** Big folders with no change for a year that compress well. */
  oldFolders: () => invoke<{ root: string; id: number; path: string; size: number; newest: number }[]>("old_folders"),
  /** Windows: NTFS compression in place. macOS and Linux: a .tar.gz archive, then the folder goes to the Trash. */
  compressFolder: (root: string, id: number) =>
    invoke<{ freedBytes: number; archive: string | null; summary: ScanSummary | null }>("compress_folder", {
      root,
      id,
    }),
  /** Big old files in OneDrive or iCloud Drive that can become online-only. */
  cloudCandidates: () =>
    invoke<{ provider: string; root: string; files: { path: string; size: number }[] }[]>("cloud_candidates"),
  cloudFree: () =>
    invoke<{ files: number; freedBytes: number; errors: number; firstError: string | null }>("cloud_free"),
  /** Moves a scanned folder into `dest` and leaves a link in its old place. Events: move-progress. */
  moveFolder: (root: string, id: number, dest: string, appFolders = false) =>
    invoke<{ files: number; bytes: number; note: string | null; summary: ScanSummary | null }>("move_folder", {
      root,
      id,
      dest,
      appFolders,
    }),
  /** Installers and disk images in Downloads; the old ones are selected. */
  installersScan: () => invoke<CleanItem[]>("installers_scan"),
  installersClean: () => invoke<CustomOutcome>("installers_clean"),
  spaceHistory: () => invoke<DriveHistory[]>("space_history"),
  cleanRulesList: () => invoke<CleanRuleInfo[]>("clean_rules_list"),
  /** Saves the whole list and returns it with the ids of new rules. */
  setCustomRules: (rules: CustomRule[]) => invoke<CustomRule[]>("set_custom_rules", { rules }),
  customRuleScan: (id: string) => invoke<CleanItem[]>("custom_rule_scan", { id }),
  customRuleRun: (id: string) => invoke<CustomOutcome>("custom_rule_run", { id }),
};
