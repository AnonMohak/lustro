/** The platform that Lustro runs on, and its path rules. */
export type Platform = "windows" | "linux" | "macos" | "android";

/** The WebView tells the platform in its user agent. Tests (Node) count as Windows. */
export function detectPlatform(ua: string): Platform {
  if (/Android/i.test(ua)) return "android";
  if (/Macintosh|Mac OS X/i.test(ua)) return "macos";
  if (/Linux|X11/i.test(ua)) return "linux";
  return "windows";
}

export const PLATFORM: Platform = detectPlatform(typeof navigator === "undefined" ? "" : navigator.userAgent);
export const IS_WINDOWS = PLATFORM === "windows";

/** The path separator of the platform. */
export function sepOf(p: Platform = PLATFORM): string {
  return p === "windows" ? "\\" : "/";
}

/** The parts of a path, without empty parts. */
export function pathParts(path: string, p: Platform = PLATFORM): string[] {
  return path.split(sepOf(p)).filter(Boolean);
}

/** The last part of a path: the file or folder name. */
export function baseName(path: string, p: Platform = PLATFORM): string {
  const sep = sepOf(p);
  const trimmed = path.length > 1 ? path.replace(sep === "\\" ? /\\+$/ : /\/+$/, "") : path;
  return trimmed.slice(trimmed.lastIndexOf(sep) + 1) || trimmed;
}

/** The folder that holds a path ("/" for an item in the root of a Unix file system). */
export function dirName(path: string, p: Platform = PLATFORM): string {
  const sep = sepOf(p);
  const trimmed = path.replace(sep === "\\" ? /\\+$/ : /\/+$/, "");
  const i = trimmed.lastIndexOf(sep);
  if (i < 0) return "";
  return i === 0 ? sep : trimmed.slice(0, i);
}

/** True for a whole drive: `C:\` on Windows, or the root of a listed drive (for example `/` or `/home/`). */
export function isDriveRoot(root: string, driveRoots: string[] = [], p: Platform = PLATFORM): boolean {
  if (p === "windows" && /^[A-Za-z]:\\$/.test(root)) return true;
  return root === "/" || driveRoots.includes(root);
}
