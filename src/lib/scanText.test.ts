import { describe, expect, it } from "vitest";
import { normalizeRoot, rootLabel } from "./scanText";

describe("normalizeRoot", () => {
  it("matches the Rust rule", () => {
    expect(normalizeRoot("c:")).toBe("C:\\");
    expect(normalizeRoot("C:\\")).toBe("C:\\");
    expect(normalizeRoot("D:/Photos/")).toBe("D:\\Photos\\");
    expect(normalizeRoot("C:\\Users\\me\\projects")).toBe("C:\\Users\\me\\projects\\");
  });
});

describe("rootLabel", () => {
  it("shows the drive or the folder name", () => {
    expect(rootLabel("C:\\")).toBe("C:");
    expect(rootLabel("C:\\Users\\me\\projects\\")).toBe("projects");
  });
});

describe("Unix roots", () => {
  it("keep one trailing slash and show the folder name", () => {
    expect(normalizeRoot("/", "linux")).toBe("/");
    expect(normalizeRoot("/home/me/", "linux")).toBe("/home/me/");
    expect(normalizeRoot("/home/me", "linux")).toBe("/home/me/");
    expect(rootLabel("/", "linux")).toBe("/");
    expect(rootLabel("/home/me/code/", "linux")).toBe("code");
  });
});
