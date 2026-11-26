// Code as it reads best: coloured by its language, named in a small label. Used by the docs
// tab's code blocks and the files tab's reader. The colours are Hub's own (`.hljs-*` in app.css).
import hljs from "highlight.js/lib/common";
import dockerfile from "highlight.js/lib/languages/dockerfile";
import powershell from "highlight.js/lib/languages/powershell";

hljs.registerLanguage("dockerfile", dockerfile);
hljs.registerLanguage("powershell", powershell);

/** File endings highlight.js does not know by name. */
const BY_EXT: Record<string, string> = {
  rs: "rust", ts: "typescript", tsx: "typescript", mts: "typescript", cts: "typescript", js: "javascript", mjs: "javascript", cjs: "javascript", jsx: "javascript",
  svelte: "xml", vue: "xml", html: "xml", htm: "xml", svg: "xml", xml: "xml", py: "python", rb: "ruby", go: "go", java: "java", kt: "kotlin", cs: "csharp",
  c: "c", h: "c", cpp: "cpp", hpp: "cpp", cc: "cpp", swift: "swift", php: "php", lua: "lua", sql: "sql", sh: "bash", bash: "bash", zsh: "bash",
  ps1: "powershell", psm1: "powershell", bat: "dos", cmd: "dos", json: "json", jsonc: "json", json5: "json", yml: "yaml", yaml: "yaml", toml: "ini", ini: "ini",
  cfg: "ini", env: "ini", css: "css", scss: "scss", less: "less", md: "markdown", diff: "diff", patch: "diff", graphql: "graphql", gql: "graphql", r: "r", pl: "perl",
};
const BY_NAME: Record<string, string> = { dockerfile: "dockerfile", makefile: "makefile", procfile: "bash", "cargo.lock": "ini", ".gitignore": "bash" };

/** What people call the languages in a label. */
const LABEL: Record<string, string> = {
  typescript: "TypeScript", javascript: "JavaScript", xml: "HTML", python: "Python", rust: "Rust", bash: "Shell", powershell: "PowerShell", dos: "Batch",
  json: "JSON", yaml: "YAML", ini: "TOML", css: "CSS", scss: "SCSS", markdown: "Markdown", csharp: "C#", cpp: "C++", sql: "SQL", go: "Go", diff: "Diff",
  dockerfile: "Dockerfile", makefile: "Makefile", plaintext: "Text",
};

/** The language of a file, from its name, or null when it is plain text. */
export function languageOf(path: string): string | null {
  const name = path.split(/[\\/]/).pop()!.toLowerCase();
  if (BY_NAME[name]) return BY_NAME[name];
  const ext = name.includes(".") ? name.slice(name.lastIndexOf(".") + 1) : "";
  return BY_EXT[ext] ?? (hljs.getLanguage(ext) ? ext : null);
}

export function labelOf(lang: string | null): string {
  if (!lang) return "Text";
  const name = hljs.getLanguage(lang)?.name;
  return LABEL[lang] ?? LABEL[name?.toLowerCase() ?? ""] ?? name ?? lang;
}

const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/**
 * The code as highlighted HTML. `lang` is a fence's info string or a language from `languageOf`;
 * unknown or missing, small snippets are guessed and big ones stay plain.
 */
export function highlight(code: string, lang: string | null): { html: string; lang: string | null } {
  const want = lang?.trim().split(/\s+/)[0].toLowerCase() || null;
  const known = want && (hljs.getLanguage(want) ? want : BY_EXT[want] ?? null);
  try {
    if (known) return { html: hljs.highlight(code, { language: known, ignoreIllegals: true }).value, lang: known };
    if (!want && code.length < 20_000) {
      const guess = hljs.highlightAuto(code);
      if (guess.relevance >= 6 && guess.language) return { html: guess.value, lang: guess.language };
    }
  } catch {
    // Plain text below.
  }
  return { html: escape(code), lang: want };
}
