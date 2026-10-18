import i18n from "../i18n";
import type { ScanSummary } from "./api";
import { formatBytes, formatCount, formatDuration, timeAgo } from "./format";

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

/** Display name of a scan root: "C:" for a drive, the folder name otherwise. */
export function rootLabel(root: string): string {
  const trimmed = root.replace(/\\+$/, "");
  return /^[A-Za-z]:$/.test(trimmed) ? trimmed : (trimmed.split("\\").pop() ?? trimmed);
}

/** Same rule as `normalize_root` in Rust: "c:" → "C:\", folders get one trailing backslash. */
export function normalizeRoot(root: string): string {
  const p = root.replace(/\//g, "\\");
  if (/^[A-Za-z]:\\?$/.test(p)) return `${p[0].toUpperCase()}:\\`;
  return `${p.replace(/\\+$/, "")}\\`;
}
