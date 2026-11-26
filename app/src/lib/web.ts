// A web search, shown as one: your message with a globe, one "Searching the web…" row for the
// agent's searches and the pages it reads, and the sources as cards under the answer (sketch
// "Webbsökning på desktop"). The phone has the same rules in ios/Hub/Web.swift; the instruction
// must stay word for word the same in both, so a search asked on one shows right on the other.
// Older wordings stay in EARLIER, so searches sent with them still show as searches.
import type { ToolItem } from "./transcript";

export const WEB_PREFIX = "Search the web for this. Open and read the pages you rely on rather than guessing from search snippets, then answer briefly and give the sources as links: ";
const EARLIER = ["Search the web for this, answer briefly and give the sources as links: "];

/** What Hub sends for a web search of `message`. */
export function webAsk(message: string): string {
  return WEB_PREFIX + message;
}

/** Your own words of a web search, null for any other message. */
export function webAsked(text: string): string | null {
  const p = [WEB_PREFIX, ...EARLIER].find((p) => text.startsWith(p));
  return p ? text.slice(p.length) : null;
}

/** Whether a thread is a web search chat: its first message of yours was one. Its box then starts
 *  with web search on, so a follow-up searches too. */
export function startedAsWeb(items: { kind: string; text?: string; fromHub?: unknown }[]): boolean {
  const first = items.find((i) => i.kind === "user" && !i.fromHub);
  return !!first?.text && webAsked(first.text) !== null;
}

/** A tool call that searched the web or read a page from it. */
export function isWebStep(step: { kind: string; title?: string; toolKind?: string }): boolean {
  if (step.kind !== "tool") return false;
  if (step.toolKind === "fetch") return true;
  const t = (step.title ?? "").toLowerCase();
  return /web ?search|web_search/.test(t) || t.startsWith("fetch http") || t.startsWith("searching for");
}

export type WebPage = { url: string; host: string; title?: string };

/** What a web step looked for, or the page it read: a query in quotes, or a link. */
export function webLine(step: ToolItem): { query?: string; page?: WebPage } {
  const title = step.title.replace(/`/g, "").trim();
  const link = urls(title)[0] ?? (step.command && /^https?:\/\//.test(step.command) ? step.command : undefined);
  if (link) return { page: page(link) };
  const quoted = title.match(/^"(.+)"/)?.[1];
  const query = quoted ?? title.replace(/^(searching for|search(ed)? the web( for)?|web ?search)\s*:?\s*/i, "") ?? step.command;
  return { query: query || step.command || title };
}

/** The pages a run of web steps found or read, one per site, in the order they came. */
export function webPages(steps: ToolItem[]): WebPage[] {
  const out: WebPage[] = [];
  const seen = new Set<string>();
  const add = (p: WebPage) => {
    if (!real(p.host) || seen.has(p.host)) return;
    seen.add(p.host);
    out.push(p);
  };
  for (const s of steps) {
    const { page: read } = webLine(s);
    if (read) add(read);
    // A search's results: Claude's WebSearch lists them as {"title": …, "url": …}.
    for (const m of (s.detail ?? "").matchAll(/"title"\s*:\s*"((?:[^"\\]|\\.)*)"\s*,\s*"url"\s*:\s*"([^"]+)"/g)) add({ ...page(m[2]), title: m[1] });
    if (!read) for (const u of urls(s.detail ?? "")) add(page(u));
  }
  return out;
}

/** An answer to a web search: its text without the list of sources it ends on, and the links it
 *  gives (that list's and those in the text), which show as cards instead. */
export function webAnswer(text: string): { body: string; links: WebPage[] } {
  const links: WebPage[] = [];
  const seen = new Set<string>();
  // Markdown links first, so a card has the page's name.
  for (const m of text.matchAll(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g)) {
    const p = page(m[2]);
    if (real(p.host) && !seen.has(p.url)) {
      seen.add(p.url);
      links.push({ ...p, title: /^https?:/.test(m[1]) ? undefined : m[1] });
    }
  }
  for (const u of urls(text)) {
    const p = page(u);
    if (real(p.host) && !seen.has(p.url)) {
      seen.add(p.url);
      links.push(p);
    }
  }
  if (!links.length) return { body: text, links };
  const lines = text.split("\n");
  let cut = false;
  while (lines.length) {
    const line = lines[lines.length - 1].trim();
    if (!line || onlyLinks(line)) {
      lines.pop();
      if (line) cut = true;
      continue;
    }
    if (cut && /^(#+\s*)?\**\s*(sources?|källor|references?)\s*:?\s*\**\s*:?$/i.test(line)) lines.pop();
    break;
  }
  const body = cut ? lines.join("\n").trim() : text;
  return { body: body || text, links };
}

/** A line that is nothing but a link or two: "- [SCB](https://scb.se)", "1. https://…". */
function onlyLinks(line: string): boolean {
  if (!urls(line).length) return false;
  const rest = line
    .replace(/\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/<?https?:\/\/[^\s"'`<>)\]]+>?/g, "")
    .replace(/^\s*([-*•]|\d+[.)])\s*/, "")
    .replace(/^\s*\*{0,2}(sources?|källor|källa|references?)\*{0,2}\s*:?\*{0,2}/i, "")
    .replace(/[\s\p{P}]+/gu, "");
  return rest.length <= 24;
}

function urls(text: string): string[] {
  return [...text.matchAll(/https?:\/\/[^\s"'`<>)\]]+/g)].map((m) => m[0].replace(/[.,;:]+$/, ""));
}

function page(url: string): WebPage {
  let host = url;
  try {
    host = new URL(url).hostname;
  } catch {
    /* not a link after all: its text stands as the name */
  }
  return { url, host: host.replace(/^www\./, "") };
}

function real(host: string): boolean {
  return !!host && host !== "localhost" && !/^(127\.|192\.168\.|100\.|10\.)/.test(host) && host.includes(".");
}

/** The tint of a site's letter, in place of its icon: the same site keeps its colour. */
export function siteTint(host: string): string {
  const hues = [210, 100, 40, 300, 170, 10];
  let h = 0;
  for (const c of host) h = (h * 31 + c.charCodeAt(0)) & 0xffff;
  return `hsl(${hues[h % hues.length]} 45% 78%)`;
}
