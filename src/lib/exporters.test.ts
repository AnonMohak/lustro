import { describe, expect, it } from "vitest";
import { csvField, toCsv } from "./exporters";

describe("csv", () => {
  it("quotes only when needed", () => {
    expect(csvField("plain")).toBe("plain");
    expect(csvField(42)).toBe("42");
    expect(csvField('say "hi"')).toBe('"say ""hi"""');
    expect(csvField("a,b")).toBe('"a,b"');
    expect(csvField("line\nbreak")).toBe('"line\nbreak"');
  });

  it("writes a BOM, a header and CRLF rows", () => {
    expect(toCsv(["Name", "Size"], [["a.txt", 5]])).toBe("﻿Name,Size\r\na.txt,5\r\n");
  });
});
