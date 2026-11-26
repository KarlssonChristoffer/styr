// Files read as what they are, not only as text: Markdown rendered, JSON as a tree, CSV as a table,
// pictures, PDFs, sound and video as themselves, spreadsheets as one table per sheet, databases as their tables, notebooks as cells, logs by level, env files with
// their values hidden. `FilePreview.svelte` draws each kind; the reply's file chips, file paths in
// replies, the viewer and the project's Files tab all use it.
import { quiet } from "./quiet";
import type { FileChange } from "./transcript";
import { languageOf } from "./code";

export type FileKind =
  | "markdown"
  | "json"
  | "jsonl"
  | "csv"
  | "tsv"
  | "image"
  | "svg"
  | "pdf"
  | "audio"
  | "video"
  | "html"
  | "sheet"
  | "database"
  | "notebook"
  | "log"
  | "env"
  | "code"
  | "text";

const EXT: Record<string, FileKind> = {
  md: "markdown", markdown: "markdown", mdx: "markdown",
  json: "json", jsonc: "json", json5: "json", geojson: "json", webmanifest: "json",
  jsonl: "jsonl", ndjson: "jsonl",
  csv: "csv", tsv: "tsv", tab: "tsv",
  png: "image", jpg: "image", jpeg: "image", gif: "image", webp: "image", avif: "image", bmp: "image", ico: "image",
  svg: "svg",
  pdf: "pdf",
  mp3: "audio", wav: "audio", ogg: "audio", oga: "audio", m4a: "audio", flac: "audio",
  mp4: "video", m4v: "video", webm: "video", mov: "video",
  html: "html", htm: "html",
  xlsx: "sheet", xlsm: "sheet", xls: "sheet", ods: "sheet",
  db: "database", sqlite: "database", sqlite3: "database",
  ipynb: "notebook",
  log: "log", out: "log",
};

const nameOf = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** What a file is, from its name. "text" when Hub has nothing better than plain text for it. */
export function fileKind(path: string): FileKind {
  const name = nameOf(path).toLowerCase();
  if (name === ".env" || name.startsWith(".env.") || name.endsWith(".env")) return "env";
  const ext = name.includes(".") ? name.slice(name.lastIndexOf(".") + 1) : "";
  return EXT[ext] ?? (languageOf(name) ? "code" : "text");
}

/** What agents make that Hub has no viewer for yet (extension, then what it is called in words).
 *  Such a file shows as a path and nothing more; when the chat meets one, `viewerGaps.ts` tells
 *  Problems, so Guard can build the viewer. Anything in `EXT` is not here: it has a viewer. */
const NO_VIEWER: Record<string, string> = {
  docx: "Word document", doc: "Word document", odt: "document", rtf: "document",
  pptx: "PowerPoint deck", ppt: "PowerPoint deck", odp: "presentation", key: "Keynote deck",
  blend: "Blender file", glb: "3D model", gltf: "3D model", obj: "3D model", stl: "3D model", fbx: "3D model", usdz: "3D model",
  psd: "Photoshop file", ai: "Illustrator file", fig: "Figma file", sketch: "Sketch file",
  heic: "picture", tif: "picture", tiff: "picture", exr: "picture", hdr: "picture",
  mkv: "video", avi: "video", wmv: "video", flv: "video", aac: "sound", wma: "sound", aiff: "sound", mid: "music",
  zip: "archive", tar: "archive", gz: "archive", tgz: "archive", "7z": "archive", rar: "archive",
  duckdb: "database", parquet: "data table", feather: "data table", arrow: "data table",
  epub: "book", mobi: "book", tex: "LaTeX document", bib: "bibliography",
  gpx: "route", kml: "map", shp: "map", gcode: "print file", dxf: "drawing", dwg: "drawing", step: "3D model", stp: "3D model",
  pkl: "data", npy: "data", npz: "data", h5: "data", onnx: "model", pt: "model", safetensors: "model",
};

/** The kind of file, in words, when Hub has no viewer for it; null when it has one (or cannot tell). */
export function missingViewer(path: string): { ext: string; what: string } | null {
  const name = nameOf(path).toLowerCase();
  const ext = name.includes(".") ? name.slice(name.lastIndexOf(".") + 1) : "";
  const what = NO_VIEWER[ext];
  return what && !EXT[ext] ? { ext, what } : null;
}

/** Kinds that are the point of a file an agent made, so a reply that writes one shows it in the chat
 *  as a Viewkit card instead of leaving it behind a chip: a table, a drawing, a page, a document (Markdown). */
export const showsInChat = (k: FileKind) => k === "csv" || k === "tsv" || k === "svg" || k === "html" || k === "markdown";

/** Kinds that are shown from the file's bytes rather than its text. */
export const isBinary = (k: FileKind) => k === "image" || k === "svg" || k === "pdf" || k === "audio" || k === "video";

/** Kinds that read differently from their text, so the reply's chip opens them as they read. */
export const hasReader = (k: FileKind) => k !== "code" && k !== "text";

/** Whether a path names a file Hub can show (rather than a folder or something unknown). */
export function showable(path: string): boolean {
  const name = nameOf(path);
  if (!name.includes(".") || name.endsWith(".")) return false;
  return fileKind(path) !== "text" || /\.(txt|text|conf|lock|properties|gitignore|editorconfig)$/i.test(name);
}

/** Where to ask the core for a file: an absolute path is read from its own folder; a relative one
 *  from the thread's. The core reads files from any folder it is given, so this is no wider. */
export function placeOf(path: string, cwd: string): { root: string; rel: string } {
  const p = path.replace(/\\/g, "/");
  if (/^(?:[A-Za-z]:\/|\/)/.test(p)) {
    const base = cwd.replace(/\\/g, "/").replace(/\/+$/, "");
    if (base && p.toLowerCase().startsWith(base.toLowerCase() + "/")) return { root: cwd, rel: p.slice(base.length + 1) };
    const at = p.lastIndexOf("/");
    return { root: p.slice(0, at) || "/", rel: p.slice(at + 1) };
  }
  return { root: cwd, rel: p.replace(/^\.\//, "") };
}

/** The file's text as the reply left it, from its edits alone: known only when the reply wrote the
 *  whole file first (the first edit has nothing before it); later edits are replayed on top. */
export function fromEdits(f: FileChange): string | null {
  const [first, ...rest] = f.edits;
  if (!first || first.old !== "") return null;
  let text = first.new;
  for (const e of rest) {
    if (e.old === "") text = e.new;
    else if (text.includes(e.old)) text = text.replace(e.old, e.new);
    else return null;
  }
  return text;
}

/** Splits YAML front matter from the body, so it can show small above the rendered text. */
export function frontMatter(text: string): { meta: string | null; body: string } {
  const m = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/.exec(text);
  return m ? { meta: m[1], body: text.slice(m[0].length) } : { meta: null, body: text };
}

/** CSV or TSV as rows of cells (RFC 4180: quoted cells may hold the separator, quotes and line
 *  breaks). The separator is guessed for CSV: comma, semicolon (Swedish Excel) or tab. */
export function parseDelimited(text: string, sep?: string): string[][] {
  const s = text.replace(/^﻿/, "");
  if (!sep) {
    const head = s.slice(0, s.indexOf("\n") >= 0 ? s.indexOf("\n") : s.length);
    const count = (c: string) => head.split(c).length - 1;
    sep = [",", ";", "\t"].reduce((a, b) => (count(b) > count(a) ? b : a), ",");
  }
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (quoted) {
      if (c === '"' && s[i + 1] === '"') (cell += '"'), i++;
      else if (c === '"') quoted = false;
      else cell += c;
    } else if (c === '"' && cell === "") quoted = true;
    else if (c === sep) row.push(cell), (cell = "");
    else if (c === "\n" || c === "\r") {
      if (c === "\r" && s[i + 1] === "\n") i++;
      row.push(cell), rows.push(row), (row = []), (cell = "");
    } else cell += c;
  }
  if (cell !== "" || row.length) row.push(cell), rows.push(row);
  return rows;
}

/** JSON Lines: each line a value. Lines that do not parse are counted, not dropped silently. */
export function parseJsonl(text: string): { values: unknown[]; bad: number } {
  const values: unknown[] = [];
  let bad = 0;
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    try {
      values.push(JSON.parse(line));
    } catch {
      bad++;
    }
  }
  return { values, bad };
}

/** JSON with comments and trailing commas (tsconfig, VS Code settings) parsed as JSON. */
export function parseLooseJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch (first) {
    // Strip comments outside strings, then trailing commas.
    let out = "";
    let inString = false;
    for (let i = 0; i < text.length; i++) {
      const c = text[i];
      if (inString) {
        out += c;
        if (c === "\\") out += text[++i] ?? "";
        else if (c === '"') inString = false;
      } else if (c === '"') (inString = true), (out += c);
      else if (c === "/" && text[i + 1] === "/") while (i < text.length && text[i] !== "\n") i++;
      else if (c === "/" && text[i + 1] === "*") {
        i += 2;
        while (i < text.length && !(text[i] === "*" && text[i + 1] === "/")) i++;
        i++;
      } else out += c;
    }
    try {
      return JSON.parse(out.replace(/,(\s*[}\]])/g, "$1"));
    } catch {
      throw first;
    }
  }
}

/** Rows of objects as a table: the keys in the order they first appear (at most `max`). */
export function tableOf(values: unknown[], max = 40): { columns: string[]; rows: string[][] } | null {
  if (!values.length || !values.every((v) => v && typeof v === "object" && !Array.isArray(v))) return null;
  const columns: string[] = [];
  for (const v of values) for (const k of Object.keys(v as object)) if (!columns.includes(k) && columns.length < max) columns.push(k);
  const cell = (x: unknown) => (x == null ? "" : typeof x === "object" ? JSON.stringify(x) : String(x));
  return { columns, rows: values.map((v) => columns.map((c) => cell((v as Record<string, unknown>)[c]))) };
}

export type LogLevel = "error" | "warn" | "info" | "debug" | null;

/** The level a log line is at, from the usual ways of writing it. */
export function logLevel(line: string): LogLevel {
  const m = /\b(FATAL|CRIT(?:ICAL)?|ERR(?:OR)?|PANIC|WARN(?:ING)?|INFO|NOTICE|DEBUG|TRACE)\b/i.exec(line.slice(0, 200));
  if (!m) return /^\s*(Traceback|at |Caused by:|thread '.*' panicked)/.test(line) ? "error" : null;
  const w = m[1].toUpperCase();
  return w.startsWith("ERR") || w === "FATAL" || w.startsWith("CRIT") || w === "PANIC" ? "error" : w.startsWith("WARN") ? "warn" : w === "INFO" || w === "NOTICE" ? "info" : "debug";
}

/** An env file's lines, with each value marked so it can be hidden until asked for. */
export function envLines(text: string): { key: string | null; value: string; raw: string }[] {
  return text.split(/\r?\n/).map((raw) => {
    const m = /^\s*(?:export\s+)?([A-Za-z_][\w.-]*)\s*=\s*(.*)$/.exec(raw);
    return m ? { key: m[1], value: m[2], raw } : { key: null, value: "", raw };
  });
}

/** A size in bytes the way people say it. */
export function sizeOf(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10240 ? 1 : 0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Folders a reply mentions (`outputs/visningstest/`, or the folder of a file it names by its path),
 *  to look in for a file the reply names by its bare name. */
export function dirHints(text: string): string[] {
  const out = new Set<string>();
  for (const m of text.replace(/\\/g, "/").matchAll(/(?:\.\/)?[\w@.-]+(?:\/[\w@.-]+)+\/?/g)) {
    const parts = m[0].replace(/^\.\//, "").split("/").filter(Boolean);
    const last = parts[parts.length - 1];
    const dir = m[0].endsWith("/") || !/\.\w{1,6}$/.test(last) ? parts : parts.slice(0, -1);
    if (dir.length && !dir.includes("..")) out.add(dir.join("/"));
  }
  return [...out];
}

/** Where a file a reply names really is. A script writes it somewhere under the folder and the reply
 *  says only `order.csv`, so a name with no folder is looked for in the folders the reply mentions,
 *  then down the folder tree (a few levels, the first match wins). Anything else is as written. */
export async function findFile(
  list: (rel: string) => Promise<{ name: string; rel: string; dir: boolean }[]>,
  rel: string,
  hints: string[] = [],
): Promise<string> {
  const clean = rel.replace(/\\/g, "/").replace(/^\.\//, "");
  const name = clean.split("/").pop()!.toLowerCase();
  const has = async (dir: string) => (await quiet(list(dir), "list", [])).find((e) => !e.dir && e.name.toLowerCase() === name)?.rel;
  const dir0 = clean.includes("/") ? clean.slice(0, clean.lastIndexOf("/")) : "";
  const here = await has(dir0);
  if (here) return here;
  for (const h of hints) {
    const hit = await has(h);
    if (hit) return hit;
  }
  let level = [dir0];
  let calls = 0;
  for (let depth = 0; depth < 3 && level.length && calls < 60; depth++) {
    const next: string[] = [];
    for (const d of level) {
      calls++;
      const entries = await quiet(list(d), "list", []);
      const hit = entries.find((e) => !e.dir && e.name.toLowerCase() === name);
      if (hit) return hit.rel;
      next.push(...entries.filter((e) => e.dir && !/^(node_modules|target|\.git|dist|build)$/.test(e.name)).map((e) => e.rel));
    }
    level = next;
  }
  return clean;
}
