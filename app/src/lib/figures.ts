// Formulas and diagrams in replies, after T3 Code's math and Mermaid rendering. Both libraries are
// big, so they load the first time a reply needs them. Only `$$…$$`, `\[…\]`, `\(…\)` and fenced
// `math` blocks are formulas: a lone `$` is far more often money than maths.

import { marked } from "marked";

let katex: Promise<typeof import("katex")> | undefined;
let mermaid: Promise<typeof import("mermaid").default> | undefined;
let figure = 0;

const esc = (t: string) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

// Markdown would eat the backslashes and the underscores of a formula, so formulas are lifted out
// whole while parsing, as a span that shows its source until it is typeset.
marked.use({
  extensions: [
    {
      name: "tex",
      level: "inline",
      start: (src: string) => src.match(/\$\$|\\\[|\\\(/)?.index,
      tokenizer(src: string) {
        const m = /^(?:\$\$([\s\S]+?)\$\$|\\\[([\s\S]+?)\\\]|\\\(([\s\S]+?)\\\))/.exec(src);
        if (!m) return;
        return { type: "tex", raw: m[0], tex: (m[1] ?? m[2] ?? m[3]).trim(), display: m[3] == null };
      },
      renderer: (t) => `<span class="tex" data-tex="${esc(t.tex)}" data-display="${t.display ? 1 : 0}">${esc(t.raw)}</span>`,
    },
  ],
});

/** Typesets formulas and draws ```mermaid blocks inside `root`, once each. */
export async function figures(root: HTMLElement, math: boolean) {
  const diagrams = [...root.querySelectorAll<HTMLElement>("pre > code.language-mermaid")].filter((c) => !c.dataset.drawn);
  if (diagrams.length) await draw(diagrams);
  if (!math) return;
  const blocks = [...root.querySelectorAll<HTMLElement>("pre > code.language-math")];
  const spans = [...root.querySelectorAll<HTMLElement>("span.tex[data-tex]")];
  if (!blocks.length && !spans.length) return;
  const k = (await (katex ??= import("katex/dist/katex.min.css").then(() => import("katex")))).default;
  for (const code of blocks) {
    const box = document.createElement("div");
    box.className = "math-block";
    box.innerHTML = k.renderToString(code.textContent ?? "", { displayMode: true, throwOnError: false });
    code.parentElement!.replaceWith(box);
  }
  for (const span of spans) {
    const display = span.dataset.display === "1";
    span.className = display ? "math-block" : "math";
    span.innerHTML = k.renderToString(span.dataset.tex ?? "", { displayMode: display, throwOnError: false });
    delete span.dataset.tex;
  }
}
async function draw(blocks: HTMLElement[]) {
  const m = await (mermaid ??= import("mermaid").then(({ default: m }) => {
    m.initialize({ startOnLoad: false, theme: "dark", securityLevel: "strict", fontFamily: "Geist Variable, sans-serif" });
    return m;
  }));
  for (const code of blocks) {
    const source = code.textContent ?? "";
    code.dataset.drawn = "1";
    try {
      const { svg } = await m.render(`hub-figure-${++figure}`, source);
      const box = document.createElement("div");
      box.className = "diagram";
      box.innerHTML = svg;
      box.dataset.tip = "Diagram (Mermaid)";
      code.parentElement!.replaceWith(box);
    } catch {
      // Half written while it streams, or not valid: it stays as code.
      document.getElementById(`dhub-figure-${figure}`)?.remove();
      delete code.dataset.drawn;
    }
  }
}
