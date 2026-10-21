import { describe, expect, it } from "vitest";
import { baseName, detectPlatform, dirName, isDriveRoot, pathParts } from "./platform";

describe("detectPlatform", () => {
  it("reads the WebView user agent", () => {
    expect(detectPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/131.0")).toBe("windows");
    expect(detectPlatform("Mozilla/5.0 (X11; Ubuntu; Linux x86_64) AppleWebKit/605.1.15")).toBe("linux");
    expect(detectPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15")).toBe("macos");
    expect(detectPlatform("Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36")).toBe("android");
    expect(detectPlatform("")).toBe("windows");
  });
});

describe("path helpers", () => {
  it("split Windows paths", () => {
    expect(baseName("C:\\Users\\me\\a.mp4", "windows")).toBe("a.mp4");
    expect(baseName("C:\\Users\\me\\", "windows")).toBe("me");
    expect(dirName("C:\\Users\\me\\a.mp4", "windows")).toBe("C:\\Users\\me");
    expect(pathParts("C:\\Users\\me", "windows")).toEqual(["C:", "Users", "me"]);
  });

  it("split Unix paths", () => {
    expect(baseName("/home/me/a.mp4", "linux")).toBe("a.mp4");
    expect(baseName("/home/me/", "linux")).toBe("me");
    expect(baseName("/", "linux")).toBe("/");
    expect(dirName("/home/me/a.mp4", "linux")).toBe("/home/me");
    expect(dirName("/a.mp4", "linux")).toBe("/");
    expect(pathParts("/home/me", "linux")).toEqual(["home", "me"]);
  });

  it("find drive roots", () => {
    expect(isDriveRoot("C:\\", [], "windows")).toBe(true);
    expect(isDriveRoot("C:\\Users\\", [], "windows")).toBe(false);
    expect(isDriveRoot("/", [], "linux")).toBe(true);
    expect(isDriveRoot("/home/", ["/", "/home/"], "linux")).toBe(true);
    expect(isDriveRoot("/home/me/", ["/", "/home/"], "linux")).toBe(false);
  });
});
