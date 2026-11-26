// Pictures of the app's main places, the same every run, to hold a before against an after. Report
// only: it never fails a build and decides nothing; it says which pictures changed and by how much.
// The app has no JS tests, so this is the check for a change that must not alter what it looks like
// (moving code between files, say).
//
//   npm run dev                                   (in app/, the dev server on :1420)
//   node scripts/shots.cjs --save before          (pictures in <temp>/hub-shots/before)
//   ... change the app ...
//   node scripts/shots.cjs --save after --against before
//
// No core is needed: scripts/shots/mock-core.cjs answers every call with the same made-up data and
// the clock stands still. Other flags: --only <prefix> (places whose name starts so),
// --url, --width, --height, --threshold (share of changed pixels, in percent, that counts as "changed"; default 0.001, to ignore a stray pixel),
// --strict (exit 1 when something changed; off by default). Pictures and diffs live in the system
// temp folder (hub-shots/); delete it when done: `node scripts/shots.cjs --clean`.
// Needs Playwright (NODE_PATH=/opt/node22/lib/node_modules in the cloud container). Described in docs/ARCHITECTURE.md.
const { chromium } = require("playwright");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { NOW, answer, known } = require("./shots/mock-core.cjs");

const arg = (name, fallback) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : fallback;
};
const flag = (name) => process.argv.includes(`--${name}`);
const ROOT = path.join(os.tmpdir(), "hub-shots");
if (flag("clean")) {
  fs.rmSync(ROOT, { recursive: true, force: true });
  console.log(`Removed ${ROOT}`);
  process.exit(0);
}
const SAVE = arg("save", "now");
const AGAINST = arg("against", "");
const ONLY = arg("only", "");
const URL = arg("url", "http://127.0.0.1:1420/");
const WIDTH = Number(arg("width", "1600"));
const HEIGHT = Number(arg("height", "1000"));
const THRESHOLD = Number(arg("threshold", "0.001"));
for (const [flagName, v] of [["save", SAVE], ["against", AGAINST]]) {
  if (v && !/^[\w-][\w.-]*$/.test(v)) { console.log(`--${flagName} wants a plain name (letters, digits, - _ .), not "${v}"`); process.exit(1); }
}
if (AGAINST && AGAINST === SAVE) { console.log("--save and --against name the same folder; the new pictures would replace the ones to compare with"); process.exit(1); }
const OUT = path.join(ROOT, SAVE);

const SETTINGS = ["account", "spend", "appearance", "notifications", "dictation", "chat", "engines", "handoffs", "personality", "safety",
  "customs", "projects", "extensions", "apis", "vault", "machines", "access", "updates", "whatsnew", "privacy", "about"];

const wait = (page, ms = 500) => page.waitForTimeout(ms);
const openThread = (page) => page.evaluate(() => hub.open("t-1"));
const typeInBox = async (page, text) => {
  await page.locator("textarea").first().click();
  await page.keyboard.type(text, { delay: 20 });
};

const attach = async (page) => {
  await page.evaluate(() => hub.open("t-1"));
  await wait(page);
  await page.locator("textarea").first().click();
  await page.evaluate(() => {
    const png = Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg=="), (c) => c.charCodeAt(0));
    const data = new DataTransfer();
    data.items.add(new File(["first line\nsecond line\nthird line\n"], "notes.txt", { type: "text/plain" }));
    data.items.add(new File([png], "dot.png", { type: "image/png" }));
    document.querySelector("textarea").dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
  });
  await wait(page, 600);
};

// [name, what to do once the page is at rest and nothing is open]
const PLACES = [
  ["home", (p) => p.evaluate(() => hub.show({ view: "home" }))],
  ["new-thread", (p) => p.evaluate(() => hub.goNew())],
  ["thread", openThread],
  ["overview", (p) => p.evaluate(() => hub.show({ view: "overview" }))],
  ["workers", (p) => p.evaluate(() => hub.show({ view: "workers" }))],
  ["machines", (p) => p.evaluate(() => hub.show({ view: "machines" }))],
  // One machine's page and one project's page, with their tabs.
  ["machine", (p) => p.evaluate(() => hub.openMachine("this computer"))],
  ["project", (p) => p.evaluate(() => hub.openProject("/work/hub"))],
  ["project-library", (p) => p.evaluate(() => { hub.openProject("/work/hub"); hub.projectTab = "library"; })],
  ["project-github", (p) => p.evaluate(() => { hub.openProject("/work/hub"); hub.projectTab = "github"; })],
  ["project-files", (p) => p.evaluate(() => { hub.openProject("/work/hub"); hub.projectTab = "files"; })],
  ["project-services", (p) => p.evaluate(() => { hub.openProject("/work/hub"); hub.projectTab = "services"; })],
  ["project-tests", (p) => p.evaluate(() => { hub.openProject("/work/hub"); hub.projectTab = "tests"; })],
  ["project-rules", (p) => p.evaluate(() => { hub.openProject("/work/hub"); hub.projectTab = "rules"; })],
  ["routines", (p) => p.evaluate(() => hub.show({ view: "routines" }))],
  ["library", (p) => p.evaluate(() => hub.show({ view: "library", doc: null }))],
  ...SETTINGS.map((s) => [`settings-${s}`, (p) => p.evaluate((x) => hub.openSettings(x), s)]),
  ["settings-apis-connection", async (p) => { await p.evaluate(() => { hub.openSettings("apis"); hub.apiOpen = "jev"; }); }],
  // The same page scrolled to its end, where Hub's own use of the connection is.
  ["settings-apis-connection-use", async (p) => {
    await p.evaluate(() => { hub.openSettings("apis"); hub.apiOpen = "jev"; });
    await p.waitForTimeout(500);
    await p.evaluate(() => { for (const el of document.querySelectorAll("*")) if (el.scrollHeight > el.clientHeight + 40 && /auto|scroll/.test(getComputedStyle(el).overflowY)) el.scrollTop = el.scrollHeight; });
  }],
  // The message box, in a thread and in a new thread.
  ["composer-thread", async (p) => { await openThread(p); await typeInBox(p, "Move the helpers first,\nthen run the check"); }],
  ["composer-new", async (p) => { await p.evaluate(() => hub.goNew()); await wait(p); await typeInBox(p, "Start from a clean branch"); }],
  // The folder picker of a new thread: open, searched, and browsing by a typed path.
  ["folder-picker", async (p) => { await p.evaluate(() => hub.goNew()); await wait(p); await p.locator(".picker .field").first().click(); await wait(p, 800); }],
  ["folder-picker-search", async (p) => { await p.evaluate(() => hub.goNew()); await wait(p); await p.locator(".picker .field").first().click(); await wait(p, 400); await p.keyboard.type("sa"); await wait(p, 600); }],
  ["folder-picker-path", async (p) => { await p.evaluate(() => hub.goNew()); await wait(p); await p.locator(".picker .field").first().click(); await wait(p, 400); await p.keyboard.type("~/"); await wait(p, 800); }],
  ["folder-picker-connect", async (p) => { await p.evaluate(() => hub.goNew()); await wait(p); await p.locator(".picker .field").first().click(); await wait(p, 400); await p.keyboard.type("sa"); await wait(p, 600); await p.locator(".ghnote .go-on").click(); await wait(p, 900); }],
  ["model-picker", async (p) => { await openThread(p); await wait(p); await p.locator(".model-pill").click(); }],
  ["effort-menu", async (p) => { await openThread(p); await wait(p); await p.locator("button.pill", { hasText: /medium/i }).click(); }],
  ["mode-menu", async (p) => { await openThread(p); await wait(p); await p.locator("button.pill", { hasText: /default/i }).click(); }],
  ["provider-menu", async (p) => { await openThread(p); await wait(p); await p.locator(".mark-btn").click(); }],
  ["at-suggest", async (p) => { await openThread(p); await wait(p); await typeInBox(p, "look at @sa"); await wait(p, 800); }],
  ["slash-suggest", async (p) => { await openThread(p); await wait(p); await p.locator("textarea").first().click(); await p.keyboard.press("Control+A"); await p.keyboard.press("Backspace"); await typeInBox(p, "/"); await wait(p, 800); }],
  ["mic-menu", async (p) => { await openThread(p); await wait(p); await p.locator(".mic-pick").click(); }],
  // A pasted file (a text and a picture) as chips above the box, and the look at the text.
  ["attachments", attach],
  ["attachment-peek", async (p) => { await attach(p); await p.locator(".chip .look").first().click(); await wait(p, 300); }],
  ["stash-list", async (p) => { await openThread(p); await wait(p); await p.locator(".tab").first().click(); }],
];

// Runs in the page: puts two pictures through a canvas and counts the pixels that differ.
async function diffInPage(page, a, b) {
  return page.evaluate(async ([a, b]) => {
    const load = (src) => new Promise((res, rej) => { const i = new Image(); i.onload = () => res(i); i.onerror = rej; i.src = src; });
    const [x, y] = await Promise.all([load(a), load(b)]);
    if (x.width !== y.width || x.height !== y.height) return { size: true };
    const draw = (img) => { const c = document.createElement("canvas"); c.width = img.width; c.height = img.height; const g = c.getContext("2d"); g.drawImage(img, 0, 0); return g.getImageData(0, 0, c.width, c.height); };
    const [da, db] = [draw(x), draw(y)];
    const out = new ImageData(x.width, x.height);
    let changed = 0;
    for (let i = 0; i < da.data.length; i += 4) {
      const d = Math.abs(da.data[i] - db.data[i]) + Math.abs(da.data[i + 1] - db.data[i + 1]) + Math.abs(da.data[i + 2] - db.data[i + 2]);
      if (d > 12) { changed++; out.data.set([255, 40, 90, 255], i); } else out.data.set([da.data[i] >> 2, da.data[i + 1] >> 2, da.data[i + 2] >> 2, 255], i);
    }
    const c = document.createElement("canvas"); c.width = x.width; c.height = x.height; c.getContext("2d").putImageData(out, 0, 0);
    return { changed, total: x.width * x.height, diff: c.toDataURL("image/png") };
  }, [a, b]);
}

(async () => {
  try {
  fs.rmSync(OUT, { recursive: true, force: true });
  fs.mkdirSync(OUT, { recursive: true });
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: WIDTH, height: HEIGHT }, deviceScaleFactor: 1 });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message.split("\n")[0] + " " + (e.stack||"").split("\n").slice(1,3).join(" ")));
  await page.clock.setFixedTime(NOW);
  await page.addInitScript((stash) => {
    localStorage.setItem("hub.zoom", "1");
    const prefs = JSON.parse(localStorage.getItem("hub.prefs") || "{}");
    localStorage.setItem("hub.prefs", JSON.stringify({ ...prefs, reduceMotion: true }));
    localStorage.setItem("hub.stash", JSON.stringify(stash));
    // Microphones: the same two everywhere, whatever the machine has.
    const devices = [{ deviceId: "default", kind: "audioinput", label: "Default microphone", groupId: "a" }, { deviceId: "usb", kind: "audioinput", label: "USB microphone", groupId: "b" }];
    if (navigator.mediaDevices) navigator.mediaDevices.enumerateDevices = async () => devices;
  }, [{ text: "Check the Safari export once more", attachments: [], at: NOW - 3600e3 }, { text: "Ask about the merge queue", attachments: [], at: NOW - 7200e3 }]);
  // The stream says nothing; every call gets the made-up answer.
  await page.routeWebSocket(/\/api\/stream/, () => {});
  await page.route(/\/api\//, (r) => {
    const req = r.request();
    if (flag("debug") && !known(req.method(), new globalThis.URL(req.url()))) console.log(`  unanswered: ${req.method()} ${new globalThis.URL(req.url()).pathname}`);
    r.fulfill({ json: answer(req.method(), new globalThis.URL(req.url())) });
  });
  await page.goto(URL);
  await page.waitForFunction(() => window.hub && window.hub.threads !== undefined);
  // Nothing moves, nothing blinks: the same picture every time.
  await page.addStyleTag({ content: "*, *::before, *::after { animation: none !important; transition: none !important; caret-color: transparent !important; scroll-behavior: auto !important; }" });
  await wait(page, 1200);

  const names = [];
  let seen = errors.length;
  for (const [name, go] of PLACES.filter(([n]) => !ONLY || n.startsWith(ONLY))) {
    await page.keyboard.press("Escape");
    await page.evaluate(() => { hub.settingsOpen = false; hub.ideaOpen = false; hub.show({ view: "home" }); });
    await wait(page, 300);
    try {
      await go(page);
      await wait(page, 700);
      await page.screenshot({ path: path.join(OUT, `${name}.png`) });
      names.push(name);
      if (errors.length > seen) console.log(`  ${name}: page error, ${errors[seen]}`);
      seen = errors.length;
    } catch (e) {
      console.log(`${name}: could not get there (${String(e.message).split("\n")[0]})`);
    }
  }
  console.log(`${names.length} pictures in ${OUT}`);
  if (errors.length) console.log(`The page reported ${errors.length} errors, first: ${errors[0]}`);

  let changed = 0;
  let unchecked = 0;
  if (AGAINST) {
    const base = path.join(ROOT, AGAINST);
    fs.mkdirSync(path.join(OUT, "diff"), { recursive: true });
    const url = (f) => `data:image/png;base64,${fs.readFileSync(f).toString("base64")}`;
    console.log(`\nAgainst ${base}:`);
    for (const name of names) {
      const before = path.join(base, `${name}.png`);
      if (!fs.existsSync(before)) { unchecked++; console.log(`  ${name}: no picture before`); continue; }
      const d = await diffInPage(page, url(before), url(path.join(OUT, `${name}.png`)));
      if (d.size) { changed++; console.log(`  ${name}: size changed (no diff picture)`); continue; }
      const pct = (100 * d.changed) / d.total;
      if (d.changed && pct >= THRESHOLD) {
        changed++;
        fs.writeFileSync(path.join(OUT, "diff", `${name}.png`), Buffer.from(d.diff.split(",")[1], "base64"));
        console.log(`  ${name}: ${d.changed} pixels differ (${pct.toFixed(3)}%), diff/${name}.png`);
      }
    }
    const total = PLACES.filter(([n]) => !ONLY || n.startsWith(ONLY)).length;
    unchecked += total - names.length;
    if (unchecked) console.log(`\n${unchecked} places could not be compared (no picture before, or no picture now).`);
    console.log(changed ? `\n${changed} of ${names.length} places differ.` : `\n${names.length - unchecked} compared places look the same.`);
    if (unchecked) changed += unchecked;
  }
  await browser.close();
  process.exit(changed && flag("strict") ? 1 : 0);
  } catch (e) {
    console.log(`Could not take the pictures (is the dev server running on ${URL}?): ${String(e.message).split("\n")[0]}`);
    process.exit(flag("strict") ? 1 : 0);
  }
})();
