// What Inspect mode hands a new thread: the marked elements and notes as text, and one
// screenshot with the marks drawn on it (`InspectCard.svelte`). Kept short on purpose: no whole
// DOM and no whole stylesheet, the agent has the code and finds the rest itself.
import type { Attachment } from "./bindings/Attachment";
import type { Mark } from "./inspect.svelte";
import { UI } from "./inspectInfo";

const MAX_W = 1400;

/** The page as a picture with the marks drawn over it, as a base64 PNG, or why it could not be made. */
export async function screenshot(marks: Mark[]): Promise<{ data: string; w: number; h: number } | { error: string }> {
  try {
    const { toCanvas } = await import("html-to-image");
    // The panel is left out of the picture, so the picture ends where it starts.
    const panel = document.querySelector<HTMLElement>("aside.inspector");
    const w = Math.round(panel ? panel.getBoundingClientRect().left : innerWidth);
    const h = innerHeight;
    // Frames are measured now, before the page is photographed, in the page's own pixels.
    const frames = marks.map((m) => (m.el.isConnected ? m.el.getBoundingClientRect() : null));
    const canvas = await toCanvas(document.body, {
      width: w,
      height: h,
      pixelRatio: 1,
      cacheBust: false,
      filter: (n) => !(n instanceof Element && (n.matches(UI) || n.closest(UI))),
    });
    const ctx = canvas.getContext("2d")!;
    const css = getComputedStyle(document.documentElement);
    const accent = css.getPropertyValue("--accent").trim() || "#a5abee";
    const ink = css.getPropertyValue("--bg").trim() || "#1a1918";
    ctx.lineWidth = 2;
    ctx.font = "700 12px sans-serif";
    marks.forEach((m, i) => {
      const r = frames[i];
      if (!r || r.width + r.height === 0) return;
      ctx.strokeStyle = accent;
      ctx.fillStyle = accent.startsWith("#") && accent.length === 7 ? accent + "26" : "rgba(165,171,238,0.15)";
      ctx.fillRect(r.x, r.y, r.width, r.height);
      ctx.strokeRect(r.x, r.y, r.width, r.height);
      const tag = m.group ? `${m.group}${i + 1}` : String(i + 1);
      const tw = Math.max(18, ctx.measureText(tag).width + 10);
      const bx = Math.max(0, r.x - 7), by = Math.max(0, r.y - 9);
      ctx.fillStyle = accent;
      ctx.beginPath();
      ctx.roundRect(bx, by, tw, 18, 9);
      ctx.fill();
      ctx.fillStyle = ink;
      ctx.fillText(tag, bx + (tw - ctx.measureText(tag).width) / 2, by + 13);
    });
    const scale = Math.min(1, MAX_W / canvas.width);
    let out = canvas;
    if (scale < 1) {
      out = document.createElement("canvas");
      out.width = Math.round(canvas.width * scale);
      out.height = Math.round(canvas.height * scale);
      out.getContext("2d")!.drawImage(canvas, 0, 0, out.width, out.height);
    }
    return { data: out.toDataURL("image/png").split(",")[1], w: out.width, h: out.height };
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
}

export function pngAttachment(data: string): Attachment {
  return { name: "inspect.png", mime: "image/png", data };
}

const q = (s: string) => s.replace(/`/g, "'");

/** The text of the message: where, what was marked and what was said, in that order. */
export function describeMarks(marks: Mark[], groups: Record<string, string>, view: string, extra: string, hasShot: boolean): string {
  const lines: string[] = [];
  const any = marks.some((m) => m.note.trim()) || Object.values(groups).some((n) => n.trim());
  lines.push(
    "I marked these in Hub's own interface with Inspect mode" +
      (hasShot ? "; the picture shows them with the same numbers." : ".") +
      (any ? "" : " I wrote no notes: look at what I marked and tell me what looks wrong or could be better.") +
      " The app's code is in `app/src` of this project; the file named for each element draws it.",
  );
  lines.push("", `View: ${view} · ${innerWidth}×${innerHeight} · ${extra}`);
  const groupsUsed = [...new Set(marks.map((m) => m.group).filter(Boolean))] as string[];
  for (const g of groupsUsed) {
    const ids = marks.map((m, i) => (m.group === g ? `${g}${i + 1}` : "")).filter(Boolean).join(", ");
    lines.push("", `## Group ${g}: ${ids}`, groups[g]?.trim() ? `Note: ${groups[g].trim()}` : "No note on the group.");
  }
  marks.forEach((m, i) => {
    const n = m.group ? `${m.group}${i + 1}` : String(i + 1);
    const f = m.info;
    lines.push("", `## ${n}  ${f.name || f.short}${f.file ? `  (app/src/${f.file})` : ""}`);
    if (m.note.trim()) lines.push(`Note: ${m.note.trim()}`);
    lines.push(`Element: \`${q(f.short)}\` · \`${q(f.path)}\``);
    lines.push(`Size and place: ${f.rect.w} × ${f.rect.h} at ${f.rect.x}, ${f.rect.y}`);
    if (f.text) lines.push(`Text: "${f.text}"`);
    const st = Object.entries(f.styles).map(([k, v]) => `${k}: ${v}`).join("; ");
    if (st) lines.push(`Styles: ${st}`);
    if (f.chain.length > 1) lines.push(`Inside: ${f.chain.join(" › ")}`);
  });
  return lines.join("\n");
}

const lensCache = new WeakMap<Element, { src: string; w: number; h: number }>();

/** The element drawn larger for the magnifier: three times, kept inside 440 × 320 on screen. */
export async function magnified(el: Element): Promise<{ src: string; w: number; h: number } | null> {
  const r = el.getBoundingClientRect();
  if (r.width < 2 || r.height < 2 || r.width > 1400 || r.height > 1000) return null;
  const hit = lensCache.get(el);
  if (hit) return hit;
  try {
    const { toCanvas } = await import("html-to-image");
    // Small things get more: a 14 px icon at three times is still small.
    const ratio = Math.min(8, Math.max(3, 160 / Math.max(r.width, r.height)));
    const canvas = await toCanvas(el as HTMLElement, {
      pixelRatio: ratio,
      cacheBust: false,
      filter: (n) => !(n instanceof Element && (n.matches(UI) || n.closest(UI))),
    });
    // Drawn at three times, shown at as much of that as fits.
    const k = Math.min(1, 440 / canvas.width, 320 / canvas.height);
    const out = { src: canvas.toDataURL("image/png"), w: Math.round(canvas.width * k), h: Math.round(canvas.height * k) };
    lensCache.set(el, out);
    return out;
  } catch {
    return null;
  }
}
