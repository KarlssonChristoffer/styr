import { defineConfig, searchForWorkspaceRoot, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { TEXT_MIN, textGrow, type TextSize } from "./src/lib/textSize";

/** Text size (Settings > Appearance) without touching every stylesheet.
 *
 * - Each font size in px may grow to --text-min, less half the distance to 15px, so small print
 *   grows most and headings hardly at all.
 * - A px line height grows with the text of its rule, or by --text-grow (how much body text
 *   grew) when the rule sets no size.
 * - Lengths that hold text grow by --text-grow too, so text never outgrows its box: widths of
 *   100px and more (menus, columns, panels, the sidebar), or 40px in a rule that sets a font size
 *   (label columns), px grid columns from 24px, and heights from 16px (rows, buttons, chips).
 * - A box of fixed px width and height (avatars, badges, icon letters, dots) keeps its size.
 * - A rule with `--keep-size: 1;` is left as written: text drawn inside an SVG's viewBox (a clock's
 *   digits) sizes with the drawing, not with the text size, and growing it alone makes it collide.
 * - Media and container queries with px get a copy per step with the px grown by as much, under
 *   :root[data-text=step], so a layout gives way when its grown content no longer fits.
 *
 * A font size written as `calc(Npx * var(--x))` grows the same way, the variable applied on top.
 * At the default (--text-min 0px, --text-grow 1) every size is the one written. */
function textSize(): Plugin {
  const px = /(\d+(?:\.\d+)?)px/;
  const onlyPx = /^\d+(?:\.\d+)?px$/;
  const grown = (n: number) => {
    const d = (15 - n) / 2;
    const floor = d >= 0 ? `var(--text-min) - ${d}px` : `var(--text-min) + ${-d}px`;
    return `max(${n}px, calc(${floor}))`;
  };
  const scaled = (v: string) => `calc(${v} * var(--text-grow))`;
  const steps = (Object.keys(TEXT_MIN) as TextSize[]).filter((t) => t !== "default");
  const under = (selector: string, step: string) =>
    selector.split(",").map((one) => {
      const s = one.trim();
      const attr = `[data-text="${step}"]`;
      if (s.startsWith(":root")) return `:root${attr}${s.slice(5)}`;
      if (s.startsWith("html")) return `html${attr}${s.slice(4)}`;
      return `:root${attr} ${s}`;
    }).join(", ");
  const plugin = {
    postcssPlugin: "hub-text-size",
    AtRule(at: any) {
      if (!["media", "container"].includes(at.name) || at.params.includes("data-text") || !/\d+(\.\d+)?px/.test(at.params)) return;
      if (at.parent?.type === "atrule" && at.parent.name === at.name) return;
      if (at.__grown) return;
      // A copy made on an earlier pass (the CSS goes through PostCSS twice, and the mark above
      // does not survive) already is for one step; growing it again doubled its px.
      let copied = false;
      at.walkRules((r: any) => { if (r.selector.includes("[data-text=")) copied = true; });
      if (copied) return;
      at.__grown = true;
      for (const step of steps.slice().reverse()) {
        const copy = at.clone();
        copy.__grown = true;
        copy.params = at.params.replace(/(\d+(?:\.\d+)?)px/g, (_: string, n: string) => `${Math.round(Number(n) * textGrow(step))}px`);
        copy.walkRules((r: any) => {
          if (r.parent?.type === "atrule" && /keyframes/.test(r.parent.name)) return;
          r.selector = under(r.selector, step);
        });
        at.after(copy);
      }
    },
    Rule(rule: any) {
      const decls: any[] = [];
      rule.each((d: any) => { if (d.type === "decl" && !d.value.includes("--text-")) decls.push(d); });
      if (rule.some((d: any) => d.type === "decl" && d.prop === "--keep-size")) return;
      const fixed = (prop: string) => decls.some((d) => d.prop === prop && onlyPx.test(d.value.trim()));
      if (fixed("width") && fixed("height")) return;
      let size: number | undefined;
      for (const d of decls) {
        if (d.prop !== "font-size" && d.prop !== "font") continue;
        // A size that is a px times a variable (the chat's size in Settings > Appearance) grows like a bare px.
        const times = d.prop === "font-size" ? d.value.trim().match(/^calc\((\d+(?:\.\d+)?)px \* (var\(--[\w-]+\))\)$/) : null;
        if (times) {
          size = Number(times[1]);
          d.value = `calc(${grown(size)} * ${times[2]})`;
          continue;
        }
        const m = d.value.match(px);
        if (!m || (d.prop === "font-size" && d.value.trim() !== m[0])) continue;
        size = Number(m[1]);
        d.value = d.value.replace(px, grown(size));
      }
      for (const d of decls) {
        const v = d.value.trim();
        if (d.prop === "line-height" && onlyPx.test(v)) {
          d.value = size === undefined ? scaled(v) : `calc(${v} + 1em - ${size}px)`;
        } else if (["width", "min-width", "max-width", "flex-basis"].includes(d.prop) && onlyPx.test(v) && parseFloat(v) >= (size === undefined ? 100 : 40)) {
          d.value = scaled(v);
        } else if (["height", "min-height"].includes(d.prop) && onlyPx.test(v) && parseFloat(v) >= 16) {
          d.value = scaled(v);
        } else if (d.prop.startsWith("grid-template-columns")) {
          d.value = v.replace(/(?<![\w.-])(\d+(?:\.\d+)?px)/g, (m: string) => (parseFloat(m) >= 24 ? scaled(m) : m));
        }
      }
    },
  };
  return { name: "hub-text-size", config: () => ({ css: { postcss: { plugins: [plugin] } } }) };
}

// Tauri expects a fixed port and fails if it is not available.
export default defineConfig({
  plugins: [svelte(), textSize()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    // In dev mode a thread's worktree links to the clone's packages, which then live outside it.
    fs: { allow: [searchForWorkspaceRoot(process.cwd()), ...(process.env.HUB_DEV_MODULES ? [process.env.HUB_DEV_MODULES] : [])] },
  },
  build: { target: "es2023" },
});
