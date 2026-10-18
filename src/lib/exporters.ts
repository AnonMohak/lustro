import i18n from "../i18n";
import { useStore } from "../store";
import { api, KINDS, type FileHit, type NodeDto, type ScanSummary } from "./api";
import { formatBytes, formatCount } from "./format";
import { rootLabel } from "./scanText";

/** One CSV field: quoted when it holds a comma, quote or line break. */
export function csvField(v: string | number): string {
  const s = String(v);
  return /[",\r\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** CSV text with a byte order mark, so Excel reads UTF-8 names correctly. */
export function toCsv(header: string[], rows: (string | number)[][]): string {
  return "﻿" + [header, ...rows].map((r) => r.map(csvField).join(",")).join("\r\n") + "\r\n";
}

const isoDate = (ms: number) => (ms > 0 ? new Date(ms).toISOString().slice(0, 10) : "");

/** Asks for a file name and saves. Shows a toast with the result. */
export async function saveExport(kind: "csv" | "html", fileName: string, contents: string) {
  const { showToast } = useStore.getState();
  try {
    const path = await api.saveExport(kind, fileName, contents);
    if (path) showToast(i18n.t("export.saved", { path }));
  } catch (e) {
    showToast(String(e), true);
  }
}

export function filesCsv(rows: { name: string; path: string; size: number; modified: number; kind: string }[]): string {
  return toCsv(
    ["Name", "Path", "Size (bytes)", "Size", "Modified", "Type"],
    rows.map((r) => [r.name, r.path, r.size, formatBytes(r.size), isoDate(r.modified), r.kind]),
  );
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** A self-contained HTML report of one scan. */
export async function scanReportHtml(summary: ScanSummary): Promise<string> {
  const t = i18n.t.bind(i18n);
  const [top, files] = await Promise.all([
    api.listChildren(summary.root, 0, 30),
    api.findFiles({ sort: "size", roots: [summary.root], limit: 50 }),
  ]);
  const total = summary.totalBytes || 1;
  const kinds = KINDS.map((k, i) => [k, summary.breakdown[i] ?? 0] as const)
    .filter(([, v]) => v > 0)
    .sort((a, b) => b[1] - a[1]);
  const bar = (v: number) => `<span class="bar"><i style="width:${((v / total) * 100).toFixed(2)}%"></i></span>`;
  const folderRows = top
    .map((n: NodeDto) => `<tr><td>${esc(n.name)}</td><td class="r">${formatBytes(n.size)}</td><td>${bar(n.size)}</td><td class="r">${((n.size / total) * 100).toFixed(1)}%</td></tr>`)
    .join("");
  const fileRows = files
    .map((f: FileHit) => `<tr><td>${esc(f.name)}<div class="p">${esc(f.folder)}</div></td><td class="r">${formatBytes(f.size)}</td><td>${isoDate(f.modified)}</td><td>${esc(t(`kind.${f.kind}`))}</td></tr>`)
    .join("");
  const kindRows = kinds
    .map(([k, v]) => `<tr><td>${esc(t(`kind.${k}`))}</td><td class="r">${formatBytes(v)}</td><td>${bar(v)}</td><td class="r">${((v / total) * 100).toFixed(1)}%</td></tr>`)
    .join("");
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Lustro report: ${esc(rootLabel(summary.root))}</title>
<style>
:root{color-scheme:light dark;--fg:#191b20;--mut:#5c6169;--line:#e3e5ea;--bar:#3d57d6;--bg:#fff}
@media (prefers-color-scheme:dark){:root{--fg:#ebedf0;--mut:#a5aab3;--line:#2c2f35;--bar:#91a4ff;--bg:#16181c}}
body{margin:0;padding:32px max(16px,5vw);font:14px/1.5 "Segoe UI",system-ui,sans-serif;color:var(--fg);background:var(--bg)}
h1{font-size:24px;margin:0 0 4px}h2{font-size:16px;margin:28px 0 8px}.mut{color:var(--mut)}
table{border-collapse:collapse;width:100%;max-width:960px}td,th{padding:6px 10px;border-bottom:1px solid var(--line);text-align:left;vertical-align:top}
th{font-size:12px;color:var(--mut)}.r{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}
.bar{display:inline-block;width:160px;height:8px;border-radius:4px;background:var(--line);overflow:hidden}.bar i{display:block;height:100%;background:var(--bar)}
.p{font:12px Consolas,monospace;color:var(--mut);word-break:break-all}
</style></head><body>
<h1>${esc(rootLabel(summary.root))}</h1>
<div class="mut">${esc(summary.root)} · ${formatCount(summary.files)} files · ${formatBytes(summary.totalBytes)} · ${esc(t(`scan.method.${summary.method}`))} · ${new Date(summary.scannedAt).toLocaleString("en-GB")}</div>
<h2>${esc(t("export.byType"))}</h2><table><tr><th>${esc(t("files.type"))}</th><th class="r">${esc(t("files.size"))}</th><th></th><th class="r">%</th></tr>${kindRows}</table>
<h2>${esc(t("export.topFolders"))}</h2><table><tr><th>${esc(t("files.name"))}</th><th class="r">${esc(t("files.size"))}</th><th></th><th class="r">%</th></tr>${folderRows}</table>
<h2>${esc(t("export.topFiles"))}</h2><table><tr><th>${esc(t("files.name"))}</th><th class="r">${esc(t("files.size"))}</th><th>${esc(t("files.modified"))}</th><th>${esc(t("files.type"))}</th></tr>${fileRows}</table>
<p class="mut">${esc(t("export.footer"))}</p>
</body></html>`;
}
