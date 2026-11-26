// Turns files the user picks, drops or pastes into attachments for a message.
import type { Attachment } from "./bindings/Attachment";

/** Larger files are refused: they would bloat the log and most engines reject them anyway. */
export const MAX_BYTES = 20 * 1024 * 1024;
const MAX_TEXT_BYTES = 2 * 1024 * 1024;

const TEXT_EXTENSIONS =
  /\.(txt|md|markdown|json|jsonc|ya?ml|toml|ini|cfg|conf|env|csv|tsv|log|xml|html?|css|scss|less|svg|js|mjs|cjs|jsx|ts|tsx|svelte|vue|rs|go|py|rb|php|java|kt|swift|c|h|cc|cpp|hpp|cs|fs|sh|bash|zsh|ps1|psm1|bat|cmd|sql|graphql|proto|lua|dart|ex|exs|erl|hs|ml|r|jl|tf|gradle|lock|gitignore|dockerfile)$/i;

export async function fromFile(file: File): Promise<Attachment> {
  if (file.size > MAX_BYTES) throw new Error(`${file.name} is larger than ${MAX_BYTES / 1024 / 1024} MB`);
  const mime = file.type || guessMime(file.name);
  const name = file.name || (mime.startsWith("image/") ? `pasted.${mime.split("/")[1] ?? "png"}` : "pasted");
  if (!mime.startsWith("image/") && (mime.startsWith("text/") || TEXT_EXTENSIONS.test(name)) && file.size <= MAX_TEXT_BYTES) {
    const text = await file.text();
    if (!text.includes("\u0000")) return { name, mime: mime || "text/plain", text };
  }
  return { name, mime: mime || "application/octet-stream", data: await base64(file) };
}

function guessMime(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  const known: Record<string, string> = {
    png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp",
    pdf: "application/pdf", md: "text/markdown", json: "application/json",
  };
  return known[ext] ?? (TEXT_EXTENSIONS.test(name) ? "text/plain" : "");
}

async function base64(file: Blob): Promise<string> {
  const url: string = await new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
  return url.slice(url.indexOf(",") + 1);
}

export function preview(a: Attachment): string | null {
  return a.data && a.mime.startsWith("image/") ? `data:${a.mime};base64,${a.data}` : null;
}

export function size(a: Attachment): string {
  const bytes = a.text ? new Blob([a.text]).size : a.data ? Math.floor((a.data.length * 3) / 4) : 0;
  if (!bytes) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** An @-named Hub thread sent along with a message: a short note naming it, read on demand. */
export const THREAD_MIME = "text/x-hub-thread";

/** A big paste sent as a file: named for what it looks like, so the agent and you can tell. */
export function pastedFile(text: string, n: number): Attachment {
  return { name: `pasted-${n}.${pastedKind(text)}`, mime: "text/plain", text };
}

function pastedKind(text: string): string {
  const t = text.trim();
  if (/^[[{]/.test(t)) {
    try {
      JSON.parse(t);
      return "json";
    } catch {
      // Not JSON after all.
    }
  }
  if (/^(diff --git |--- \S|Index: )/m.test(t) && /^@@ /m.test(t)) return "diff";
  const lines = t.split("\n").slice(0, 200);
  const logged = lines.filter((l) => /^\s*(\[?\d{4}-\d\d-\d\d|\[?\d\d:\d\d:\d\d|\[?(TRACE|DEBUG|INFO|WARN|WARNING|ERROR|FATAL)\b|\s+at \S)/.test(l));
  return logged.length >= Math.max(3, lines.length * 0.3) ? "log" : "txt";
}

/** Lines in a text attachment, for its chip; 0 for anything else. */
export function lineCount(a: Attachment): number {
  if (!a.text || a.mime === THREAD_MIME) return 0;
  let n = 1;
  for (let i = a.text.indexOf("\n"); i !== -1 && i < a.text.length - 1; i = a.text.indexOf("\n", i + 1)) n++;
  return n;
}
