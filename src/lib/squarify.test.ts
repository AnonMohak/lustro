import { describe, expect, it } from "vitest";
import { squarify } from "./squarify";

const rect = { x: 0, y: 0, w: 600, h: 400 };

describe("squarify", () => {
  it("gives each item an area proportional to its value", () => {
    const values = [6, 6, 4, 3, 2, 2, 1];
    const total = values.reduce((a, b) => a + b, 0);
    const out = squarify(values, (v) => v, rect);
    expect(out).toHaveLength(values.length);
    for (const p of out) {
      expect(p.w * p.h).toBeCloseTo((p.item / total) * rect.w * rect.h, 6);
    }
  });

  it("fills the rectangle without overlap", () => {
    const out = squarify([5, 4, 3, 2, 1, 1, 1], (v) => v, rect);
    const area = out.reduce((a, p) => a + p.w * p.h, 0);
    expect(area).toBeCloseTo(rect.w * rect.h, 6);
    for (const p of out) {
      expect(p.x).toBeGreaterThanOrEqual(-1e-9);
      expect(p.y).toBeGreaterThanOrEqual(-1e-9);
      expect(p.x + p.w).toBeLessThanOrEqual(rect.w + 1e-6);
      expect(p.y + p.h).toBeLessThanOrEqual(rect.h + 1e-6);
    }
    for (let i = 0; i < out.length; i++)
      for (let j = i + 1; j < out.length; j++) {
        const a = out[i];
        const b = out[j];
        const ox = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
        const oy = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
        expect(ox > 1e-6 && oy > 1e-6).toBe(false);
      }
  });

  it("skips empty items and empty rectangles", () => {
    expect(squarify([0, -1], (v) => v, rect)).toEqual([]);
    expect(squarify([1, 2], (v) => v, { x: 0, y: 0, w: 0, h: 10 })).toEqual([]);
    expect(squarify([0, 3], (v) => v, rect)).toHaveLength(1);
  });

  it("keeps tiles close to square", () => {
    const out = squarify([10, 10, 10, 10], (v) => v, { x: 0, y: 0, w: 400, h: 400 });
    for (const p of out) expect(Math.max(p.w / p.h, p.h / p.w)).toBeLessThan(1.01);
  });
});
