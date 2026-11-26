import { describe, expect, it } from "vitest";
import { fileKind, showsInChat } from "./filePreview";

describe("showsInChat", () => {
  it("shows tables, drawings, pages and documents as cards", () => {
    for (const f of ["a.csv", "a.tsv", "a.svg", "a.html", "notes.md"]) expect(showsInChat(fileKind(f))).toBe(true);
  });
  it("leaves code and plain text behind a chip", () => {
    for (const f of ["a.ts", "a.rs", "a.txt", "a.json"]) expect(showsInChat(fileKind(f))).toBe(false);
  });
});
