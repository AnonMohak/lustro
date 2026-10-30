import { describe, expect, it } from "vitest";
import en from "./en.json";
import enLinux from "./en-linux.json";
import enMacos from "./en-macos.json";
import { mergeTexts, textsFor } from "./index";

type Tree = { [key: string]: string | Tree };

function keys(t: Tree, prefix = ""): string[] {
  return Object.entries(t).flatMap(([k, v]) => (typeof v === "object" ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`]));
}

describe("platform texts", () => {
  it("replace only texts that exist", () => {
    const all = new Set(keys(en));
    expect(keys(enLinux).filter((k) => !all.has(k))).toEqual([]);
    expect(keys(enMacos).filter((k) => !all.has(k))).toEqual([]);
  });

  it("macOS uses the Unix texts and its own", () => {
    const m = textsFor("macos") as typeof en;
    expect(m.del.recycle).toBe("Move to Trash");
    expect(m.explore.reveal).toBe("Show in Finder");
    expect(m.del.permanent).toBe(en.del.permanent);
    expect(textsFor("windows")).toBe(en);
  });

  it("merge into the base texts", () => {
    const m = mergeTexts(en, enLinux) as typeof en;
    expect(m.del.recycle).toBe("Move to Trash");
    expect(m.del.permanent).toBe(en.del.permanent);
  });
});
