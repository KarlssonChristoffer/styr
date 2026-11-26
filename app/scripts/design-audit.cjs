// Design audit: walks every view, settings page, menu and card Hub has in both themes and counts what
// strays from design/DESIGN.md: corner radii off the scale, icon line weights, text under 4.5 : 1, and
// the font sizes in use. A report, not a gate: it exits 0.
//
//   (needs the dev server on :1420 and a core, like check-layout.cjs)
//   node scripts/design-audit.cjs [--themes dark,cappuccino] [--shots DIR] [--url http://127.0.0.1:1420/]
const path = require("path");
const { chromium } = require("playwright");
const fs = require("fs");
const arg = (n, f) => { const i = process.argv.indexOf(`--${n}`); return i > 0 ? process.argv[i + 1] : f; };
const SHOTS = arg("shots", "");
const URL = arg("url", "http://127.0.0.1:1420/");
const THEMES = arg("themes", "dark,cappuccino").split(",");
const now = Date.now();
const plan = (name, a, b) => ({ plan: name, checked_at: now, limits: [
  { label: "5-hour limit", percent: a, resets: null, resets_at: now + 3 * 3600e3 },
  { label: "Weekly limit", percent: b, resets: null, resets_at: now + 4 * 86400e3 } ] });
const USAGE = { plan: plan("max", 24, 61), plan_error: null, refreshing: false, threads: [],
  others: [{ engine: "codex", label: "Codex", plan: plan("plus", 0, 3), note: null }] };
const ACCOUNTS = { keys: [], engines: [
  { engine: "claude", label: "Claude", command: null, logged_in: true, method: "subscription", plan: "max", email: null, organization: null, source: null },
  { engine: "codex", label: "Codex", command: null, logged_in: true, method: "subscription", plan: "plus", email: null, organization: null, source: null } ] };
const F = (body) => new Function(`return (async () => { ${body} })()`);
const SETTINGS = ["account","spend","appearance","notifications","dictation","chat","engines","handoffs","personality","safety","customs","projects","memory","extensions","apis","vault","machines","access","updates","whatsnew","privacy","about"];
const SCENES = [
  ["home", F(`hub.show({ view: "home" })`)],
  ...["Needs you","Live","While you were away","Projects","Today","Custom"].map((t)=>["home-"+t, F(`hub.show({ view: "home" }); await new Promise(r=>setTimeout(r,400)); [...document.querySelectorAll(".scenes .tabs button")].find(b=>b.textContent.trim()===${JSON.stringify(t)})?.click();`)]),
  ["new", F(`hub.goNew()`)],
  ["thread", F(`hub.threads[0] && hub.open(hub.threads[0].id)`)],
  ...["services","workers","machines","labs","overview","problems","routines","recipes"].map((v)=>[v, F(`hub.show({ view: ${JSON.stringify(v)} })`)]),
  ["library", F(`hub.show({ view: "library", doc: null })`)],
  ["plugins", F(`hub.show({ view: "plugins", repo: null })`)],
  ...["overview","library","github","docs","files","services"].map((t)=>["project-"+t, F(`const t=hub.threads[0]; if (t) hub.show({ view: "project", project: t.cwd, host: null, tab: ${JSON.stringify(t)} })`)]),
  ...SETTINGS.map((s)=>["settings-"+s, F(`hub.openSettings(${JSON.stringify(s)})`)]),
  ["idea-inbox", F(`hub.ideaOpen = true`)],
  ["user-menu", F(`hub.sidebarOpen = true; hub.meMenu = "foot:"`)],
  ["palette", async (page) => { await page.keyboard.press("Control+k"); }],
  ["ctx-thread-row", async (page) => { await page.evaluate(`hub.threads[0] && hub.open(hub.threads[0].id)`); await page.waitForTimeout(500); const r = page.locator(".thread").first(); await r.click({ button: "right", timeout: 2000 }); }],
  ["composer-model-menu", async (page) => { await page.evaluate(`hub.goNew()`); await page.waitForTimeout(500); await page.locator(".composer button", { hasText: /Claude|default|Sonnet|Opus/ }).first().click({ timeout: 2000 }); }],
  ["composer-plus", async (page) => { await page.evaluate(`hub.goNew()`); await page.waitForTimeout(500); await page.locator(".composer .plus, .composer button[aria-label*='dd'], .composer button:has-text('+')").first().click({ timeout: 2000 }); }],
  ["thread-more", async (page) => { await page.evaluate(`hub.threads[0] && hub.open(hub.threads[0].id)`); await page.waitForTimeout(600); await page.locator("[data-tip='Thread actions']").first().click({ timeout: 2000 }); }],
  ["new-project", F(`hub.newProjectOpen = true`)],
  ["ctx-message", async (page) => { await page.evaluate(`hub.threads[0] && hub.open(hub.threads[0].id)`); await page.waitForTimeout(600); await page.locator(".bubble").first().click({ button: "right", timeout: 2000 }); }],
  ["ctx-sidebar-project", async (page) => { await page.locator(".sidebar .project, aside .project").first().click({ button: "right", timeout: 2000 }); }],
  ["select-open-appearance", async (page) => { await page.evaluate(`hub.openSettings("appearance")`); await page.waitForTimeout(500); await page.locator(".select button, button.select, [role=combobox]").first().click({ timeout: 2000 }); }],
  ["select-open-chat", async (page) => { await page.evaluate(`hub.openSettings("chat")`); await page.waitForTimeout(500); await page.locator(".select button, button.select, [role=combobox]").first().click({ timeout: 2000 }); }],
  ["composer-effort-menu", async (page) => { await page.evaluate(`hub.goNew()`); await page.waitForTimeout(500); await page.locator(".composer button").nth(2).click({ timeout: 2000 }); }],
  ["thread-panel-collapsed", F(`hub.threads[0] && hub.open(hub.threads[0].id); hub.panelOpen = true`)],
  ["find-bar", async (page) => { await page.evaluate(`hub.threads[0] && hub.open(hub.threads[0].id)`); await page.waitForTimeout(500); await page.keyboard.press("Control+f"); }],
  ["sidebar-hover", async (page) => { await page.locator(".thread").nth(1).hover({ timeout: 2000 }); }],
];
function audit() {
  const parse = (c) => { const m = c.match(/rgba?\(([^)]+)\)/); if (!m) return null; const p = m[1].split(/[ ,\/]+/).filter(Boolean).map(Number); return { r: p[0], g: p[1], b: p[2], a: p[3] === undefined ? 1 : p[3] }; };
  const lum = ({ r, g, b }) => { const f = (x) => { x /= 255; return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const blend = (top, bot) => ({ r: top.r * top.a + bot.r * (1 - top.a), g: top.g * top.a + bot.g * (1 - top.a), b: top.b * top.a + bot.b * (1 - top.a), a: 1 });
  const bgOf = (el) => { const chain = []; for (let e = el; e; e = e.parentElement) { const c = parse(getComputedStyle(e).backgroundColor); if (c && c.a > 0) { chain.push(c); if (c.a >= 1) break; } } let base = { r: 20, g: 19, b: 18, a: 1 }; for (const c of chain.reverse()) base = blend(c, base); return base; };
  const cls = (el) => el.tagName.toLowerCase() + (typeof el.className === "string" ? "." + el.className.split(" ").filter((c) => c && !c.startsWith("svelte")).join(".") : "");
  const visible = (el) => { const r = el.getBoundingClientRect(); const s = getComputedStyle(el); return r.width > 0 && r.height > 0 && s.display !== "none" && s.visibility !== "hidden" && +s.opacity > 0.05; };
  const out = { radii: {}, strokes: {}, contrast: [], sizes: {}, weights: {} };
  for (const el of document.querySelectorAll("body *")) {
    if (!visible(el)) continue;
    const s = getComputedStyle(el); const r = el.getBoundingClientRect();
    if (!(el instanceof SVGElement)) {
      const v = [s.borderTopLeftRadius, s.borderTopRightRadius, s.borderBottomRightRadius, s.borderBottomLeftRadius].map((x) => (x.includes("%") ? 9999 : parseFloat(x) || 0));
      const half = Math.min(r.width, r.height) / 2;
      for (const x of v) { if (x > 0 && x < half - 0.6 && ![1,1.5,2,3,3.5,4,6,8,12].includes(x)) (out.radii[x] ||= []).push(cls(el)); }
      const own = [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
      if (own) {
        const fg = parse(s.color), bg = bgOf(el); if (fg) { const f = blend(fg, bg); const L1 = lum(f), L2 = lum(bg); const ratio = (Math.max(L1, L2) + 0.05) / (Math.min(L1, L2) + 0.05); const size = parseFloat(s.fontSize);
          out.sizes[size] = (out.sizes[size] || 0) + 1; out.weights[s.fontWeight] = (out.weights[s.fontWeight] || 0) + 1;
          if (ratio < 4.5 && !s.opacity.startsWith("0.") ) out.contrast.push({ ratio: +ratio.toFixed(2), sel: cls(el), text: el.textContent.trim().slice(0, 30), size }); } }
    } else if (el.tagName === "svg") {
      const kids = [...el.querySelectorAll("path,line,polyline,circle,rect")]; const st = kids.map((k) => getComputedStyle(k)).filter((c) => c.stroke !== "none" && c.fill === "none" || c.fill === "rgba(0, 0, 0, 0)");
      const ws = [...new Set(st.map((c) => parseFloat(c.strokeWidth)))];
      const vb = el.viewBox?.baseVal; const scale = vb && vb.width ? r.width / vb.width : 1;
      for (const w of ws) { const key = w + "@" + (vb?.width || "-"); (out.strokes[key] ||= []).push(cls(el.parentElement || el)); }
    }
  }
  return out;
}
(async () => {
  const browser = await chromium.launch();
  const all = {};
  for (const theme of THEMES) {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
    await page.addInitScript((t) => { localStorage.setItem("hub.zoom", "1"); const prefs = JSON.parse(localStorage.getItem("hub.prefs") || "{}"); localStorage.setItem("hub.prefs", JSON.stringify({ ...prefs, reduceMotion: true, theme: t })); }, theme);
    await page.route(/\/api\/usage/, (r) => r.fulfill({ json: USAGE }));
    await page.route(/\/api\/prefs\/accounts$/, (r) => r.fulfill({ json: ACCOUNTS }));
    for (const [name, go] of SCENES) {
      try {
        await page.goto(URL);
        await page.waitForFunction(() => window.hub && window.hub.threads !== undefined);
        await page.waitForTimeout(900);
        if (go.length === 1) await go(page); else await page.evaluate(go);
        await page.waitForTimeout(700);
        const res = await page.evaluate(audit);
        all[`${theme}/${name}`] = res;
        if (SHOTS) { fs.mkdirSync(SHOTS, { recursive: true }); await page.screenshot({ path: `${SHOTS}/${theme}-${name}.png` }); }
      } catch (e) { all[`${theme}/${name}`] = { error: String(e).slice(0, 120) }; }
      await page.keyboard.press("Escape");
    }
    await page.close();
  }
  await browser.close();
  summarize(all);
})();

// The source scan: every .svelte and .css file, whether or not a view shows it.
function scanSource() {
  const root = path.join(__dirname, "..", "src");
  const files = [];
  (function walk(d) { for (const e of fs.readdirSync(d, { withFileTypes: true })) { const f = path.join(d, e.name); if (e.isDirectory()) walk(f); else if (/\.(svelte|css)$/.test(e.name)) files.push(f); } })(root);
  const bad = { radius: {}, half: {}, strokes: {} };
  const okRadius = new Set(["0", "1", "1.5", "2", "3", "4", "6", "8", "12", "999", "99"]);
  for (const f of files) {
    const rel = path.relative(root, f);
    for (const line of fs.readFileSync(f, "utf8").split("\n")) {
      if (rel.includes("ApiConnections") && line.includes(".look-")) continue;
      for (const m of line.matchAll(/border(?:-[a-z]+)*-radius:\s*([^;}"]+)/g)) for (const n of m[1].matchAll(/(\d+(?:\.\d+)?)px/g)) if (!okRadius.has(n[1])) (bad.radius[n[1]] ||= new Set()).add(rel);
      if (!line.includes("--keep-size")) for (const m of line.matchAll(/font(?:-size)?:\s*(\d+\.\d+)px/g)) (bad.half[m[1]] ||= new Set()).add(rel);
    }
  }
  const fmt = (o) => Object.keys(o).length ? Object.entries(o).map(([k, v]) => `${k}px in ${v.size} file${v.size > 1 ? "s" : ""}`).join(", ") : "none";
  console.log(`Source scan (${files.length} files): radii off the scale: ${fmt(bad.radius)}; half-pixel font sizes: ${fmt(bad.half)}\n`);
}

function summarize(all) {
  scanSource();
  const scenes = Object.keys(all);
  const failed = scenes.filter((k) => all[k].error);
  console.log(`${scenes.length - failed.length} of ${scenes.length} views audited` + (failed.length ? `; could not open: ${[...new Set(failed.map((k) => k.split("/")[1]))].join(", ")}` : ""));
  const radii = {}, strokes = {}, contrast = {}, sizes = {};
  for (const v of Object.values(all)) {
    for (const [r, tags] of Object.entries(v.radii || {})) for (const t of tags) ((radii[r] ||= {})[t] = ((radii[r] || {})[t] || 0) + 1);
    for (const [w, tags] of Object.entries(v.strokes || {})) strokes[w] = (strokes[w] || 0) + tags.length;
    for (const c of v.contrast || []) { const e = (contrast[c.sel] ||= { n: 0, min: 9, text: c.text }); e.n++; e.min = Math.min(e.min, c.ratio); }
    for (const [s, n] of Object.entries(v.sizes || {})) sizes[s] = (sizes[s] || 0) + n;
  }
  const top = (o, n = 8) => Object.entries(o).sort((a, b) => b[1] - a[1]).slice(0, n);
  console.log("\nRadii off the scale (1-3, 4, 6, 8, 12, pills are fine):", Object.keys(radii).length ? "" : "none");
  for (const [r, c] of Object.entries(radii)) console.log(`  ${r}px:`, top(c, 5).map(([t, n]) => `${t} x${n}`).join(", "));
  console.log("\nIcon line weights (width@viewBox; the rule is 1.75@24):");
  for (const [w, n] of top(strokes, 10)) console.log(`  ${w}  x${n}`);
  const bad = Object.entries(contrast).sort((a, b) => b[1].n - a[1].n);
  console.log(`\nText under 4.5 : 1: ${bad.length} styles`);
  for (const [sel, e] of bad.slice(0, 15)) console.log(`  ${e.min}  ${sel.slice(0, 40)}  "${e.text}"  x${e.n}`);
  console.log("\nFont sizes in use (px, count):", Object.entries(sizes).sort((a, b) => a[0] - b[0]).map(([s, n]) => `${+(+s).toFixed(1)}:${n}`).join(" "));
}
