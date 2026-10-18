import type { IconName } from "../components/Icon";
import { KINDS, type Kind } from "./api";

export const KIND_ICON: Record<Kind, IconName> = {
  other: "file",
  video: "video",
  image: "image",
  doc: "doc",
  app: "app",
  game: "game",
  archive: "archive",
  system: "windows",
  dev: "code",
};

export const kindColor = (k: Kind) => `var(--c-${k})`;

/** Breakdown array (indexed like KINDS) as [kind, bytes] pairs, biggest first, zeros removed. */
export function sortedBreakdown(b: number[]): [Kind, number][] {
  return KINDS.map((k, i) => [k, b[i] ?? 0] as [Kind, number])
    .filter(([, v]) => v > 0)
    .sort((a, c) => c[1] - a[1]);
}
