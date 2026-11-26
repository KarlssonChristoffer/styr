// Looks for text that overlaps other text or is cut off by its box, in every view, at every text
// size and a few zoom levels. Needs the dev server (npm run dev), a running core and Playwright.
//
//   node scripts/check-layout.cjs [--sizes default,largest] [--zooms 1,1.25] [--shots DIR] [--only place]
//
// Text gets a little extra letter spacing (--slack, in em) so layouts that only just fit fail here
// rather than on a machine whose fonts render a little wider.
//
// Text cut off on purpose (a scrolling box, or a long name with an ellipsis) is fine. A short
// text (SHORT characters or fewer: a label, a time, a plan name) cut by an ellipsis is not: that
// is a column too narrow for its own words, as in "Weekly · all m…" or "10…". Exits 1 when it
// finds anything.
const { chromium } = require("playwright");
const fs = require("fs");
const path = require("path");

const arg = (name, fallback) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : fallback;
};
const SIZES = arg("sizes", "default,larger,large,largest").split(",");
const ZOOMS = arg("zooms", "1,1.25,1.75").split(",").map(Number);
const SHOTS = arg("shots", "");
const ONLY = arg("only", "");
const SLACK = Number(arg("slack", "0.03"));
const SHORT = Number(arg("short", "28"));
// Names the user or an agent wrote (thread titles) are as long as they are; cutting them is the
// point of an ellipsis. These are the places that show one on its own.
const NAMES = ".t.grow, .when.link, .crumb, .fpath, .ref-label, .title .link";
const URL = arg("url", "http://127.0.0.1:1420/");
// The window in screen pixels; zoom leaves less room in CSS pixels.
const WIDTH = Number(arg("width", "1920"));
const HEIGHT = Number(arg("height", "1080"));

const SETTINGS = ["account", "appearance", "notifications", "dictation", "chat", "engines", "safety", "customs", "projects",
  "memory", "extensions", "apis", "vault", "machines", "access", "updates", "whatsnew", "privacy", "about"];

// Each place: a name and what to do in the page to get there (runs with `hub` from state.svelte.ts).
// A Home scene: open Home and click its tab (the check runs inside the page, so no closures).
const scene = (tab) => new Function(`return (async () => { hub.show({ view: "home" }); await new Promise((r) => setTimeout(r, 400)); [...document.querySelectorAll(".scenes .tabs button")].find((b) => b.textContent.trim() === ${JSON.stringify(tab)})?.click(); })()`);
// Simulating: the same Home page with the made-up threads running for a few ticks.
const simulating = new Function(`return (async () => { hub.show({ view: "home" }); await new Promise((r) => setTimeout(r, 400)); document.querySelector(".scenes .tabs .sim")?.click(); await new Promise((r) => setTimeout(r, 4500)); })()`);
const PLACES = [
  ["home", () => hub.show({ view: "home" })],
  ["home, Simulating", simulating],
  ...["Needs you", "Live", "While you were away", "Projects", "Today", "Custom"].map((t) => [`home, ${t}`, scene(t)]),
  ["new thread", () => hub.goNew()],
  ["thread", () => hub.threads[0] && hub.open(hub.threads[0].id)],
  ["overview", () => hub.show({ view: "overview" })],
  ["workers", () => hub.show({ view: "workers" })],
  ["machines", () => hub.show({ view: "machines" })],
  ["services", () => hub.show({ view: "services" })],
  ["routines", () => hub.show({ view: "routines" })],
  ["labs", () => hub.show({ view: "labs" })],
  ["library", () => hub.show({ view: "library", doc: null })],
  ["idea inbox", () => (hub.ideaOpen = true)],
  ["project", () => {
    const t = hub.threads[0];
    if (t) hub.show({ view: "project", project: t.cwd, host: null, tab: "overview" });
  }],
  // The name menu opens from the usage strip at the sidebar's foot, or, with the sidebar shut,
  // from the avatar on the rail; each for both subscriptions.
  ["user menu", "foot:"],
  ["user menu, Codex", "foot:codex"],
  ["user menu, rail", "rail:"],
  ["user menu, rail, Codex", "rail:codex"],
  ...SETTINGS.map((s) => [`settings/${s}`, `settings:${s}`]),
  // A key on its own page: the first one the vault lists.
  ["settings/vault key", () => {
    hub.openSettings("vault");
    setTimeout(() => document.querySelector(".settings .body .conn")?.click(), 300);
  }],
];

// Limits for two subscriptions, so the name menu's card has something to show whoever runs this.
const now = Date.now();
const plan = (name, a, b) => ({ plan: name, checked_at: now, limits: [
  { label: "5-hour limit", percent: a, resets: null, resets_at: now + 3 * 3600e3 },
  { label: "Weekly limit", percent: b, resets: null, resets_at: now + 4 * 86400e3 },
] });
const USAGE = { plan: plan("max", 24, 61), plan_error: null, refreshing: false, threads: [],
  others: [{ engine: "codex", label: "Codex", plan: plan("plus", 0, 3), note: null }] };
const ACCOUNTS = { keys: [], engines: [
  { engine: "claude", label: "Claude", command: null, logged_in: true, method: "subscription", plan: "max", email: null, organization: null, source: null },
  { engine: "codex", label: "Codex", command: null, logged_in: true, method: "subscription", plan: "plus", email: null, organization: null, source: null },
] };

// Runs in the page: returns what is wrong, one line per problem.
function inspect([SHORT, NAMES]) {
  const problems = [];
  const label = (el) => {
    const cls = typeof el.className === "string" && el.className.trim() ? "." + el.className.trim().split(/\s+/).filter((c) => !c.startsWith("svelte-")).join(".") : "";
    return `${el.tagName.toLowerCase()}${cls} "${(el.textContent || "").trim().replace(/\s+/g, " ").slice(0, 40)}"`;
  };
  const visible = (el) => {
    const s = getComputedStyle(el);
    return s.visibility !== "hidden" && s.display !== "none" && Number(s.opacity) > 0.05;
  };
  // Cut off: the box hides overflow, has no ellipsis and is not meant to scroll.
  for (const el of document.querySelectorAll("body *")) {
    if (!visible(el) || !el.firstChild) continue;
    if (el instanceof SVGElement) continue; // drawn in the picture's own units
    const ownText = [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
    if (!ownText) continue;
    const s = getComputedStyle(el);
    if (s.textOverflow === "ellipsis" && el.scrollWidth > el.clientWidth + 1) {
      const text = (el.textContent || "").trim();
      if (text.length <= SHORT && !el.closest(NAMES)) problems.push(`short text cut by an ellipsis: ${label(el)} (${el.scrollWidth}px in ${el.clientWidth}px)`);
    }
    if (s.textOverflow === "ellipsis" || s.webkitLineClamp !== "none" || /auto|scroll/.test(s.overflowX + s.overflowY)) continue;
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) continue;
    const hidden = /hidden|clip/.test(s.overflowX + s.overflowY);
    if (hidden && (el.scrollWidth > el.clientWidth + 1 || el.scrollHeight > el.clientHeight + 2))
      problems.push(`cut off: ${label(el)} (${el.scrollWidth}x${el.scrollHeight} in ${el.clientWidth}x${el.clientHeight})`);
    // Spills out: text wider than its box runs into the next thing or out of the box around it.
    else if (!hidden && s.display !== "inline" && el.clientWidth > 20 && el.scrollWidth > el.clientWidth + 2
      && (r.left + el.scrollWidth > (el.parentElement?.getBoundingClientRect().right ?? Infinity) + 1
        || (el.nextElementSibling && r.left + el.scrollWidth > el.nextElementSibling.getBoundingClientRect().left + 1)))
      problems.push(`spills out: ${label(el)} (${el.scrollWidth}px in ${el.clientWidth}px)`);
  }
  // Overlap: the line boxes of two texts cross where both are the top thing on screen.
  const texts = [];
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let n; (n = walker.nextNode()); ) {
    if (!n.textContent.trim() || !n.parentElement || !visible(n.parentElement)) continue;
    const range = document.createRange();
    range.selectNodeContents(n);
    // Only the part of the text that shows: boxes that hide overflow clip it.
    let clip = { left: -Infinity, top: -Infinity, right: Infinity, bottom: Infinity };
    let clipper = null;
    for (let a = n.parentElement; a && a !== document.body; a = a.parentElement) {
      if (!/hidden|clip|auto|scroll/.test(getComputedStyle(a).overflow)) continue;
      clipper ??= a;
      const b = a.getBoundingClientRect();
      clip = { left: Math.max(clip.left, b.left), top: Math.max(clip.top, b.top), right: Math.min(clip.right, b.right), bottom: Math.min(clip.bottom, b.bottom) };
    }
    for (const q of range.getClientRects()) {
      // Text with part of it outside a box that hides overflow (no ellipsis, no scrolling): "Nc".
      if (q.width > 4 && q.height > 4 && clipper && getComputedStyle(clipper).textOverflow !== "ellipsis"
        && getComputedStyle(n.parentElement).textOverflow !== "ellipsis" && !/auto|scroll/.test(getComputedStyle(clipper).overflow)) {
        const vis = Math.min(q.right, clip.right) - Math.max(q.left, clip.left);
        const rect = clipper.getBoundingClientRect();
        if (vis > 0 && vis < q.width - 2 && rect.width > 0 && q.top >= 0 && q.bottom <= innerHeight)
          problems.push(`clipped: ${label(n.parentElement)} loses ${Math.round(q.width - vis)}px of ${Math.round(q.width)}px to ${label(clipper)}`);
      }
      const r = { left: Math.max(q.left, clip.left), top: Math.max(q.top, clip.top), right: Math.min(q.right, clip.right), bottom: Math.min(q.bottom, clip.bottom) };
      r.width = r.right - r.left;
      r.height = r.bottom - r.top;
      if (r.width > 1 && r.height > 1) texts.push({ el: n.parentElement, r });
    }
  }
  const top = (el, x, y) => {
    const hit = document.elementFromPoint(x, y);
    return !!hit && (hit === el || el.contains(hit) || hit.contains(el));
  };
  // The nearest box that floats above the page: a menu, popover, dialog or fixed bar.
  const layer = (el) => {
    for (let a = el; a && a !== document.body; a = a.parentElement) {
      const s = getComputedStyle(a);
      if (s.position === "fixed" || (s.position === "absolute" && s.zIndex !== "auto") || a.matches("[role=dialog],[role=menu],[popover]")) return a;
    }
    return document.body;
  };
  const seen = new Set();
  for (let i = 0; i < texts.length; i++) {
    for (let j = i + 1; j < texts.length; j++) {
      const a = texts[i], b = texts[j];
      if (a.el === b.el || a.el.contains(b.el) || b.el.contains(a.el)) continue;
      const w = Math.min(a.r.right, b.r.right) - Math.max(a.r.left, b.r.left);
      const h = Math.min(a.r.bottom, b.r.bottom) - Math.max(a.r.top, b.r.top);
      if (w < 3 || h < Math.min(a.r.height, b.r.height) * 0.4) continue;
      const x = (Math.max(a.r.left, b.r.left) + Math.min(a.r.right, b.r.right)) / 2;
      const ya = (Math.max(a.r.top, b.r.top) + Math.min(a.r.bottom, b.r.bottom)) / 2;
      if (x < 0 || ya < 0 || x > innerWidth || ya > innerHeight) continue;
      // A menu or dialog over the page may cover it; within one layer nothing may cover text.
      if (layer(a.el) !== layer(b.el)) continue;
      if (!top(a.el, x, ya) && !top(b.el, x, ya)) continue;
      const key = label(a.el) + "|" + label(b.el);
      if (seen.has(key)) continue;
      seen.add(key);
      problems.push(`overlap: ${label(a.el)} and ${label(b.el)}`);
    }
  }
  return problems;
}

(async () => {
  const browser = await chromium.launch();
  let total = 0;
  for (const size of SIZES) {
    for (const zoom of ZOOMS) {
      // The app's zoom is the webview's: fewer CSS pixels, each drawn bigger. The same here.
      const page = await browser.newPage({ viewport: { width: Math.round(WIDTH / zoom), height: Math.round(HEIGHT / zoom) }, deviceScaleFactor: zoom });
      await page.addInitScript(([z, t]) => {
        localStorage.setItem("hub.zoom", "1");
        const prefs = JSON.parse(localStorage.getItem("hub.prefs") || "{}");
        localStorage.setItem("hub.prefs", JSON.stringify({ ...prefs, textSize: t, reduceMotion: true }));
      }, [zoom, size]);
      await page.route(/\/api\/usage/, (r) => r.fulfill({ json: USAGE }));
      await page.route(/\/api\/prefs\/accounts$/, (r) => r.fulfill({ json: ACCOUNTS }));
      await page.goto(URL);
      // Text a little wider than here, as other systems' font rendering can be, so near misses show.
      if (SLACK) await page.addStyleTag({ content: `body * { letter-spacing: ${SLACK}em !important; }` });
      await page.waitForFunction(() => window.hub && window.hub.threads !== undefined);
      await page.waitForTimeout(1500);
      for (const [name, go] of PLACES.filter(([n]) => !ONLY || n.startsWith(ONLY))) {
        await page.keyboard.press("Escape");
        await page.evaluate(() => ((hub.settingsOpen = false), (hub.ideaOpen = false)));
        if (typeof go === "function") await page.evaluate(go);
        else if (typeof go === "string" && go.startsWith("settings:")) await page.evaluate((s) => hub.openSettings(s), go.slice(9));
        else {
          const [where, engine] = go.split(":");
          await page.evaluate((e) => {
            const prefs = JSON.parse(localStorage.getItem("hub.prefs") || "{}");
            localStorage.setItem("hub.prefs", JSON.stringify({ ...prefs, menuAccount: e ?? "" }));
          }, engine);
          if (engine) await page.reload(), await page.waitForFunction(() => window.hub), await page.waitForTimeout(1200);
          await page.evaluate((w) => ((hub.sidebarOpen = w === "foot"), (hub.meMenu = w)), where);
        }
        await page.waitForTimeout(700);
        const problems = await page.evaluate(inspect, [SHORT, NAMES]);
        if (ONLY && SHOTS) await page.screenshot({ path: path.join(SHOTS, `only-${size}-${zoom}-${name.replace(/\W+/g, "-")}.png`) });
        if (problems.length) {
          total += problems.length;
          console.log(`\n${size} text, ${Math.round(zoom * 100)}% zoom, ${name}:`);
          for (const p of problems) console.log(`  ${p}`);
          if (SHOTS) {
            fs.mkdirSync(SHOTS, { recursive: true });
            await page.screenshot({ path: path.join(SHOTS, `${size}-${zoom}-${name.replace(/\W+/g, "-")}.png`) });
          }
        }
        if (name.startsWith("user menu")) await page.evaluate(() => ((hub.meMenu = null), (hub.sidebarOpen = true)));
      }
      await page.close();
    }
  }
  await browser.close();
  console.log(total ? `\n${total} problems.` : "No overlaps or cut-off text.");
  process.exit(total ? 1 : 0);
})();
