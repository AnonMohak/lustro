import { useEffect, useRef, useState } from "react";
import type { Kind, TreemapNode } from "../lib/api";
import { formatBytes } from "../lib/format";
import { squarify, type Rect } from "../lib/squarify";

interface Tile extends Rect {
  node: TreemapNode;
  group: boolean;
  depth: number;
}

const HEADER = 20;
const PAD = 3;
const MAX_DEPTH = 10;

/** Lays out the nested treemap. Folders that are big enough show their children inside. */
export function layoutTiles(root: TreemapNode, w: number, h: number): Tile[] {
  const tiles: Tile[] = [];
  const place = (nodes: TreemapNode[], rect: Rect, depth: number) => {
    for (const p of squarify(nodes, (n) => n.size, rect)) {
      if (p.w < 2 || p.h < 2) continue;
      const kids = p.item.children ?? [];
      const group = kids.length > 0 && p.w > 80 && p.h > 54 && depth < MAX_DEPTH;
      tiles.push({ x: p.x, y: p.y, w: p.w, h: p.h, node: p.item, group, depth });
      if (group) place(kids, { x: p.x + PAD, y: p.y + HEADER + 1, w: p.w - PAD * 2, h: p.h - HEADER - PAD - 1 }, depth + 1);
    }
  };
  place(root.children ?? [], { x: 0, y: 0, w, h }, 0);
  return tiles;
}

function cssVar(el: Element, name: string): string {
  return getComputedStyle(el).getPropertyValue(name).trim() || "#888";
}

interface Props {
  data: TreemapNode | null;
  selected: number | null;
  /** Ids of protected nodes get a striped pattern. */
  isProtected?: (id: number) => boolean;
  onSelect: (id: number) => void;
  onZoom: (id: number) => void;
  /** Changes when the theme changes, so colors are read again. */
  themeKey: string;
}

export function Treemap({ data, selected, isProtected, onSelect, onZoom, themeKey }: Props) {
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const tilesRef = useRef<Tile[]>([]);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [hover, setHover] = useState<{ tile: Tile; x: number; y: number } | null>(null);

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    const c = canvas.current;
    if (!c || !data || size.w === 0 || size.h === 0) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(size.w * dpr);
    c.height = Math.round(size.h * dpr);
    const ctx = c.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size.w, size.h);

    const colors = {} as Record<Kind, string>;
    for (const k of ["other", "video", "image", "doc", "app", "game", "archive", "system", "dev"] as Kind[])
      colors[k] = cssVar(c, `--c-${k}`);
    const fg = cssVar(c, "--fg");
    const tileFg = cssVar(c, "--tile-fg");
    const font = getComputedStyle(c).fontFamily;

    const tiles = layoutTiles(data, size.w, size.h);
    tilesRef.current = tiles;

    for (const t of tiles) {
      const x = t.x + 1;
      const y = t.y + 1;
      const w = t.w - 2;
      const h = t.h - 2;
      if (w < 1 || h < 1) continue;
      const color = colors[t.node.kind];
      ctx.save();
      ctx.beginPath();
      ctx.roundRect(x, y, w, h, 3);
      ctx.clip();
      if (t.group) {
        ctx.fillStyle = color;
        ctx.globalAlpha = 0.16;
        ctx.fillRect(x, y, w, h);
        ctx.globalAlpha = 0.5;
        ctx.strokeStyle = color;
        ctx.lineWidth = 1;
        ctx.strokeRect(x + 0.5, y + 0.5, w - 1, h - 1);
        ctx.globalAlpha = 1;
      } else {
        ctx.fillStyle = color;
        ctx.fillRect(x, y, w, h);
        // Cushion highlight.
        const g = ctx.createRadialGradient(x + w * 0.25, y + h * 0.15, 0, x + w * 0.25, y + h * 0.15, Math.max(w, h) * 1.1);
        g.addColorStop(0, "rgba(255,255,255,0.38)");
        g.addColorStop(0.6, "rgba(255,255,255,0)");
        ctx.fillStyle = g;
        ctx.fillRect(x, y, w, h);
        if (isProtected?.(t.node.id)) {
          ctx.strokeStyle = "rgba(0,0,0,0.08)";
          ctx.lineWidth = 6;
          for (let s = -h; s < w; s += 12) {
            ctx.beginPath();
            ctx.moveTo(x + s, y + h);
            ctx.lineTo(x + s + h, y);
            ctx.stroke();
          }
        }
        ctx.strokeStyle = "rgba(0,0,0,0.10)";
        ctx.lineWidth = 1;
        ctx.strokeRect(x + 0.5, y + 0.5, w - 1, h - 1);
      }
      // Label.
      if (w > 56 && h > (t.group ? 18 : 30)) {
        ctx.font = `600 11.5px ${font}`;
        ctx.fillStyle = t.group ? fg : tileFg;
        ctx.textBaseline = "top";
        const name = t.node.name;
        const sizeText = formatBytes(t.node.size);
        ctx.fillText(fit(ctx, name, w - 12), x + 6, y + 4);
        ctx.font = `400 11.5px ${font}`;
        ctx.globalAlpha = 0.75;
        if (t.group) {
          const nw = ctx.measureText(fit(ctx, name, w - 12)).width;
          if (nw + ctx.measureText(sizeText).width + 20 < w) ctx.fillText(sizeText, x + 12 + nw, y + 4);
        } else if (h > 34) {
          ctx.fillText(fit(ctx, sizeText, w - 12), x + 6, y + 19);
        }
        ctx.globalAlpha = 1;
      }
      ctx.restore();
      if (t.node.id === selected) {
        ctx.strokeStyle = fg;
        ctx.lineWidth = 2;
        ctx.strokeRect(x + 1, y + 1, w - 2, h - 2);
      }
    }
  }, [data, size, selected, isProtected, themeKey]);

  const hit = (e: React.MouseEvent): Tile | null => {
    const r = canvas.current!.getBoundingClientRect();
    const px = e.clientX - r.left;
    const py = e.clientY - r.top;
    let found: Tile | null = null;
    for (const t of tilesRef.current) {
      if (px >= t.x && px < t.x + t.w && py >= t.y && py < t.y + t.h) found = t; // deepest wins (drawn last)
    }
    return found;
  };

  return (
    <div className="tm" ref={wrap}>
      <canvas
        ref={canvas}
        style={{ width: "100%", height: "100%", display: "block" }}
        onClick={(e) => {
          const t = hit(e);
          if (t) onSelect(t.node.id);
        }}
        onDoubleClick={(e) => {
          const t = hit(e);
          if (t && t.node.isDir) onZoom(t.node.id);
        }}
        onMouseMove={(e) => {
          const t = hit(e);
          const r = wrap.current!.getBoundingClientRect();
          setHover(t ? { tile: t, x: e.clientX - r.left, y: e.clientY - r.top } : null);
        }}
        onMouseLeave={() => setHover(null)}
      />
      {hover && (
        <div
          className="tm-tip"
          style={{
            left: Math.min(hover.x + 14, size.w - 230),
            top: hover.y + 16 > size.h - 50 ? hover.y - 46 : hover.y + 16,
          }}
        >
          <b>{hover.tile.node.name}</b>
          <span className="num">{formatBytes(hover.tile.node.size)}</span>
        </div>
      )}
    </div>
  );
}

function fit(ctx: CanvasRenderingContext2D, text: string, max: number): string {
  if (ctx.measureText(text).width <= max) return text;
  let lo = 0;
  let hi = text.length;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (ctx.measureText(text.slice(0, mid) + "…").width <= max) lo = mid;
    else hi = mid - 1;
  }
  return lo > 0 ? text.slice(0, lo) + "…" : "";
}
