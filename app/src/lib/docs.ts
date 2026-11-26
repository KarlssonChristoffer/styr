// Markdown the way the docs tab reads it: Obsidian's `[[Doc]]`, `[[Doc#Heading]]`,
// `[[Doc|text]]` and `#tags` on top of GitHub markdown, headings with anchors, and links to other
// markdown files turned into doc links. Where a link leads comes from the core (`DocPage.links`).
import DOMPurify from "dompurify";
import { Marked, type Tokens } from "marked";
import type { DocLink } from "./bindings/DocLink";
import { highlight, labelOf } from "./code";

/** The anchor of a heading; the same as the core's `vault::slug`. */
export function slug(text: string): string {
  let out = "";
  for (const c of text.toLowerCase()) {
    if (/[\p{L}\p{N}]/u.test(c)) out += c;
    else if (out && !out.endsWith("-")) out += "-";
  }
  return out.replace(/-+$/, "");
}

/** A heading as it reads: links show their text, emphasis and code marks go. */
export function plainHeading(text: string): string {
  return text
    .replace(/\[\[([^\]|]*)\|?([^\]]*)\]\]/g, (_, t, a) => a || t)
    .replace(/[*`]/g, "")
    .replace(/__/g, "")
    .trim();
}

/** `a/b/../c.md` → `a/c.md`. */
function normalize(path: string): string {
  const out: string[] = [];
  for (const part of path.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") out.pop();
    else out.push(part);
  }
  return out.join("/");
}

const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** Where a doc link points: the doc (a path, or a name the core resolves) and the heading. */
export type DocTarget = { doc: string; heading: string | null };

/** `docs/a.md#Heading` → its parts. */
export function splitTarget(target: string): DocTarget {
  const at = target.indexOf("#");
  return at < 0 ? { doc: target, heading: null } : { doc: target.slice(0, at), heading: target.slice(at + 1) || null };
}

/**
 * Renders a doc's markdown. `rel` is the doc itself, for relative links; `links` says where each
 * `[[link]]` resolved, so links to docs not written yet look different; null when Hub has not
 * looked (a file shown outside the docs tab), and then no link looks missing.
 */
export function renderDoc(body: string, rel: string, links: DocLink[] | null): string {
  const resolved = new Map((links ?? []).map((l) => [l.target.toLowerCase(), l.to]));
  const dir = rel.includes("/") ? rel.slice(0, rel.lastIndexOf("/")) : "";
  const marked = new Marked({ gfm: true, breaks: false });
  const docLink = (to: string | null, target: string, heading: string | null, text: string, embed = false) => {
    const href = (to ?? target) + (heading ? `#${heading}` : "");
    const known = to || !links;
    const cls = ["wl", known ? "" : "miss", embed ? "embed" : ""].filter(Boolean).join(" ");
    const title = to ? to : links ? `${target} is not written yet` : target;
    return `<a href="#" class="${cls}" data-doc="${escape(href)}" data-tip="${escape(title)}">${text}</a>`;
  };
  marked.use({
    extensions: [
      {
        name: "wikilink",
        level: "inline",
        start: (src: string) => {
          const i = src.indexOf("[[");
          return i < 0 ? undefined : i > 0 && src[i - 1] === "!" ? i - 1 : i;
        },
        tokenizer(src: string) {
          const m = /^(!?)\[\[([^[\]\n]+?)\]\]/.exec(src);
          if (!m) return;
          const [inner, alias] = m[2].includes("|") ? [m[2].slice(0, m[2].indexOf("|")), m[2].slice(m[2].indexOf("|") + 1)] : [m[2], ""];
          const { doc, heading } = splitTarget(inner.trim());
          return { type: "wikilink", raw: m[0], doc: doc.trim(), heading: heading?.replace(/^\^/, "") ?? null, alias: alias.trim(), embed: !!m[1] };
        },
        renderer(t: Tokens.Generic) {
          const shown = t.alias || (t.doc ? t.doc.split("/").pop() : "") + (t.heading ? (t.doc ? " › " : "") + t.heading : "");
          if (!t.doc) return `<a href="#${escape(slug(t.heading ?? ""))}" class="wl">${escape(shown)}</a>`;
          return docLink(resolved.get(t.doc.toLowerCase()) ?? null, t.doc, t.heading, escape(shown), t.embed);
        },
      },
      {
        name: "tag",
        level: "inline",
        start: (src: string) => {
          const m = /(^|[\s(])#[\p{L}_-]/u.exec(src);
          return m ? m.index + m[1].length : undefined;
        },
        tokenizer(src: string) {
          const m = /^#([\p{L}\p{N}_\-/]*[\p{L}_\-][\p{L}\p{N}_\-/]*)/u.exec(src);
          if (!m) return;
          return { type: "tag", raw: m[0], tag: m[1].replace(/\/+$/, "") };
        },
        renderer(t: Tokens.Generic) {
          return `<a href="#" class="tag" data-tag="${escape(t.tag)}">#${escape(t.tag)}</a>`;
        },
      },
    ],
    renderer: {
      // A code block with its language and a copy button (the reader handles the click).
      code({ text, lang }: Tokens.Code) {
        const { html, lang: found } = highlight(text, lang ?? null);
        const lines = text.split("\n").length;
        return `<div class="codeblock"><div class="cb-head"><span class="cb-lang">${escape(labelOf(found))}</span>${lines > 1 ? `<span class="cb-lines">${lines} lines</span>` : ""}<button type="button" class="cb-copy">Copy</button></div><pre><code class="hljs">${html}</code></pre></div>\n`;
      },
      heading(this: { parser: { parseInline: (t: Tokens.Generic[]) => string } }, { tokens, depth, text }: Tokens.Heading) {
        return `<h${depth} id="h-${escape(slug(plainHeading(text)))}">${this.parser.parseInline(tokens)}</h${depth}>\n`;
      },
      link(this: { parser: { parseInline: (t: Tokens.Generic[]) => string } }, { href, title, tokens }: Tokens.Link) {
        const text = this.parser.parseInline(tokens);
        const external = /^[a-z][a-z0-9+.-]*:/i.test(href);
        if (!external && /\.md(#|$)/i.test(href)) {
          const { doc, heading } = splitTarget(decodeURIComponent(href));
          const path = normalize(doc.startsWith("/") ? doc : `${dir}/${doc}`);
          const to = resolved.has(doc.toLowerCase()) ? resolved.get(doc.toLowerCase())! : path;
          return docLink(to, path, heading, text);
        }
        if (href.startsWith("#")) return `<a href="#${escape(slug(href.slice(1)))}" class="wl">${text}</a>`;
        return `<a href="${escape(href)}" class="ext"${title ? ` data-tip="${escape(title)}"` : ""}>${text}</a>`;
      },
    },
  });
  const html = marked.parse(body, { async: false });
  return DOMPurify.sanitize(html);
}
