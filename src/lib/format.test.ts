import { describe, expect, it } from "vitest";
import { formatBytes, formatDuration, percent, timeAgo } from "./format";

describe("formatBytes", () => {
  it("shows small values in bytes", () => {
    expect(formatBytes(0)).toBe("0 bytes");
    expect(formatBytes(1)).toBe("1 byte");
    expect(formatBytes(1023)).toBe("1023 bytes");
  });

  it("uses binary units like File Explorer", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 ** 3)).toBe("5.0 GB");
    expect(formatBytes(476.9 * 1024 ** 3)).toBe("477 GB");
    expect(formatBytes(2 * 1024 ** 4)).toBe("2.0 TB");
  });

  it("rejects invalid input", () => {
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
  });
});

describe("percent", () => {
  it("clamps to 0..100", () => {
    expect(percent(50, 200)).toBe(25);
    expect(percent(5, 0)).toBe(0);
    expect(percent(300, 200)).toBe(100);
  });
});


describe("formatDuration", () => {
  it("picks a readable unit", () => {
    expect(formatDuration(850)).toBe("850 ms");
    expect(formatDuration(11_400)).toBe("11.4 s");
    expect(formatDuration(161_000)).toBe("2 min 41 s");
    expect(formatDuration(3_900_000)).toBe("1 h 5 min");
  });
});

describe("timeAgo", () => {
  const now = 1_800_000_000_000;
  it("describes the past", () => {
    expect(timeAgo(now - 5_000, now)).toBe("just now");
    expect(timeAgo(now - 14 * 60_000, now)).toBe("14 min ago");
    expect(timeAgo(now - 3 * 3_600_000, now)).toBe("3 h ago");
    expect(timeAgo(now - 30 * 3_600_000, now)).toBe("yesterday");
    expect(timeAgo(now - 72 * 3_600_000, now)).toBe("3 days ago");
    expect(timeAgo(now - 100 * 86_400_000, now)).toBe("3 months ago");
    expect(timeAgo(now - 500 * 86_400_000, now)).toBe("1 yr 4 mo ago");
    expect(timeAgo(now - 731 * 86_400_000, now)).toBe("2 yr ago");
  });
});
