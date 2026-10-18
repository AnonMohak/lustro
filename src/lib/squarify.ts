/** Squarified treemap layout (Bruls, Huizing, van Wijk). */

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Placed<T> extends Rect {
  item: T;
}

function worst(areas: number[], side: number): number {
  let sum = 0;
  let max = 0;
  let min = Infinity;
  for (const a of areas) {
    sum += a;
    if (a > max) max = a;
    if (a < min) min = a;
  }
  const s2 = side * side;
  const sum2 = sum * sum;
  return Math.max((s2 * max) / sum2, sum2 / (s2 * min));
}

/**
 * Lays out items in a rectangle. Each item gets an area proportional to `value(item)`.
 * Items with a value of 0 or less are skipped. Sort items biggest first for the best result.
 */
export function squarify<T>(items: T[], value: (item: T) => number, rect: Rect): Placed<T>[] {
  const list = items.filter((i) => value(i) > 0);
  const total = list.reduce((a, i) => a + value(i), 0);
  const out: Placed<T>[] = [];
  if (total <= 0 || rect.w <= 0 || rect.h <= 0) return out;

  const scale = (rect.w * rect.h) / total;
  let { x, y, w, h } = rect;
  let i = 0;
  while (i < list.length) {
    const side = Math.min(w, h);
    const row: T[] = [list[i]];
    const areas = [value(list[i]) * scale];
    i++;
    while (i < list.length) {
      const next = value(list[i]) * scale;
      if (worst([...areas, next], side) > worst(areas, side)) break;
      row.push(list[i]);
      areas.push(next);
      i++;
    }
    const rowArea = areas.reduce((a, b) => a + b, 0);
    if (w >= h) {
      const cw = rowArea / h;
      let yy = y;
      row.forEach((item, k) => {
        const rh = areas[k] / cw;
        out.push({ item, x, y: yy, w: cw, h: rh });
        yy += rh;
      });
      x += cw;
      w -= cw;
    } else {
      const ch = rowArea / w;
      let xx = x;
      row.forEach((item, k) => {
        const rw = areas[k] / ch;
        out.push({ item, x: xx, y, w: rw, h: ch });
        xx += rw;
      });
      y += ch;
      h -= ch;
    }
  }
  return out;
}
