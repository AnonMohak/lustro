import { describe, expect, it } from "vitest";
import en from "./en.json";
import enLinux from "./en-linux.json";
import { mergeTexts } from "./index";

type Tree = { [key: string]: string | Tree };

function keys(t: Tree, prefix = ""): string[] {
  return Object.entries(t).flatMap(([k, v]) => (typeof v === "object" ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`]));
}

describe("platform texts", () => {
  it("replace only texts that exist", () => {
    const all = new Set(keys(en));
    expect(keys(enLinux).filter((k) => !all.has(k))).toEqual([]);
  });

  it("merge into the base texts", () => {
    const m = mergeTexts(en, enLinux) as typeof en;
    expect(m.del.recycle).toBe("Move to Trash");
    expect(m.del.permanent).toBe(en.del.permanent);
  });
});
