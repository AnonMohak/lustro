import i18n from "../i18n";
import type { ScanSummary } from "./api";
import { formatBytes, formatCount, formatDuration, timeAgo } from "./format";
import { PLATFORM, type Platform, baseName } from "./platform";

/** "1,284,330 files · 412 GB · Fast scan (MFT) in 11.4 s · 14 min ago" */
export function scanSummaryText(s: ScanSummary): string {
  return i18n.t("scan.summary", {
    files: formatCount(s.files),
    size: formatBytes(s.totalBytes),
    method: i18n.t(`scan.method.${s.method}`),
    elapsed: formatDuration(s.elapsedMs),
    ago: timeAgo(s.scannedAt),
  });
}

/** Display name of a scan root: "C:" for a drive, "/" for a Unix root, the folder name otherwise. */
export function rootLabel(root: string, p: Platform = PLATFORM): string {
  if (p !== "windows") return root === "/" ? "/" : baseName(root, p);
  const trimmed = root.replace(/\\+$/, "");
  return /^[A-Za-z]:$/.test(trimmed) ? trimmed : (trimmed.split("\\").pop() ?? trimmed);
}

/** Same rule as `normalize_root` in Rust. Windows: "c:" → "C:\", folders get one trailing
 *  backslash. Other platforms: folders get one trailing "/", and the root stays "/". */
export function normalizeRoot(root: string, p: Platform = PLATFORM): string {
  if (p !== "windows") return `${root.replace(/\/+$/, "")}/`;
  const w = root.replace(/\//g, "\\");
  if (/^[A-Za-z]:\\?$/.test(w)) return `${w[0].toUpperCase()}:\\`;
  return `${w.replace(/\\+$/, "")}\\`;
}

