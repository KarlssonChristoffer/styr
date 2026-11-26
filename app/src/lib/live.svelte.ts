// Live mode: one button on the rail switches the main window between the installed pages and
// dev mode's live ones, which are main with every open pull request that changes only the pages
// merged on top (`dev.rs`, preview `live`), served by Vite so a push shows within seconds. The
// shell (`app/src-tauri/src/live.rs`) moves the window and sends it back when the live pages stop
// showing signs of life; a live page that breaks says so itself, naming the pull request whose
// files the error points at when it can. Storage goes along both ways (`carry.ts`).
import { quiet } from "./quiet";

import { api } from "./api";
import { NOTE_KEY, leaveBroken, showingLive, storage, windowLabel } from "./carry";
import { hubdHas } from "./compat";
import { home } from "./home";
import { keepPlace } from "./history.svelte";
import type { MenuEntry } from "./menu.svelte";
import { openUrl } from "./native";
import { poll } from "./poll";
import { hub } from "./state.svelte";
import type { DevStatus } from "./bindings/DevStatus";
import type { DevPreviewPr } from "./bindings/DevPreviewPr";
import type { DevPreviewCheck } from "./bindings/DevPreviewCheck";

export { showingLive };

/** How long going live may take: the first time it clones Hub and installs its packages. */
const SET_UP_FOR = 10 * 60_000;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

class Live {
  /** The button is offered: the desktop app's main window, on this computer's core, with a shell
   *  and a core that have Live mode. */
  offered = $state(false);
  /** The shell or the core is too old for it: the button says it needs the next version. */
  needsUpdate = $state(false);
  /** Going live: setting dev mode up, starting Vite, or waiting for the first preview. */
  going = $state<string | null>(null);
  /** Dev mode as last read, for the menu. */
  dev = $state<DevStatus | null>(null);
  /** Why the window came back from the live pages, shown once (`LiveNote.svelte`). */
  note = $state<string | null>(null);

  async start(): Promise<() => void> {
    if (windowLabel() !== "main" || home) return () => {};
    try {
      this.note = sessionStorage.getItem(NOTE_KEY);
      sessionStorage.removeItem(NOTE_KEY);
    } catch {
      // No note without storage.
    }
    const shell = await invoke<boolean>("live_beat").then(
      () => true,
      () => false,
    );
    const core = await hubdHas("dev.live"); // compat-until: 2026-12-01
    this.offered = true;
    this.needsUpdate = !shell || !core;
    if (!showingLive) return () => {};
    return this.watch();
  }

  /** While live: a sign of life every few seconds, as long as the app is drawn, and a way back
   *  when an error leaves the page empty. */
  private watch() {
    const drawn = () => !!document.querySelector("#app")?.childElementCount;
    const beat = setInterval(() => {
      if (drawn()) quiet(invoke("live_beat"), "invoke");
    }, 3000);
    let checking = false;
    const broke = (what: unknown) => {
      if (checking) return;
      checking = true;
      // Most errors leave the page working; only one that leaves it empty sends it back.
      setTimeout(() => {
        checking = false;
        if (!drawn()) quiet(this.broke(what), "broke");
      }, 1500);
    };
    const onError = (e: ErrorEvent) => broke(e.error ?? e.message);
    const onRejection = (e: PromiseRejectionEvent) => broke(e.reason);
    addEventListener("error", onError);
    addEventListener("unhandledrejection", onRejection);
    quiet(this.refresh(), "refresh");
    // Asking for the status is what tells the core the pages are being watched: it looks for new
    // pushes often then, and rarely otherwise. Once a minute while the window is up front, and at
    // once when it comes back to the front.
    const watched = () => document.visibilityState === "visible" && document.hasFocus();
    const look = () => {
      if (watched()) quiet(this.refresh(), "refresh");
    };
    // `poll` also looks at once when the window is seen again.
    const looking = poll(look, 60_000);
    addEventListener("focus", look);
    return () => {
      looking();
      removeEventListener("focus", look);
      clearInterval(beat);
      removeEventListener("error", onError);
      removeEventListener("unhandledrejection", onRejection);
    };
  }

  async refresh() {
    this.dev = await api.dev();
  }

  /** The open pull requests the live pages show. */
  get included(): DevPreviewPr[] {
    return this.dev?.preview?.live ? this.dev.preview.included : [];
  }

  /** Left click on the rail button. */
  toggle() {
    if (this.needsUpdate) return hub.tell("Live mode comes with the next full update of Hub");
    if (showingLive) return this.leave().catch((e) => hub.fail(e));
    if (!this.going) this.go().catch((e) => hub.fail(e));
  }

  async go() {
    this.note = null;
    this.going = "Starting the live pages";
    try {
      // A refresh now, so the pages are current; files changed in the clone are kept on a branch.
      let status = await api.devPreview(true, null, true, true);
      const until = Date.now() + SET_UP_FOR;
      let serving: number | null = null;
      // Up when Vite answers and the first live preview is built.
      while (!(status.state === "running" && status.preview?.live && status.preview.refreshed_at)) {
        if (status.state === "failed") throw new Error(status.message ?? "Dev mode could not start");
        if (Date.now() > until) throw new Error("The live pages did not come up; see Labs > Work on Hub");
        // Vite is up but the preview is not: after a while its message (GitHub not answering,
        // say) is the reason.
        if (status.state === "running") serving ??= Date.now();
        if (serving && Date.now() - serving > 60_000 && status.preview?.message && !status.preview.refreshing) throw new Error(status.preview.message);
        this.going = status.state === "setting_up" ? (status.message ?? "Setting up dev mode") : status.preview?.message && !status.preview.refreshed_at ? status.preview.message : "Starting the live pages";
        await new Promise((r) => setTimeout(r, 1000));
        status = await api.dev();
      }
      this.dev = status;
      keepPlace(hub.route);
      await invoke("live_go", { url: status.url, parcel: JSON.stringify({ storage: storage() }) });
    } finally {
      this.going = null;
    }
  }

  async leave() {
    keepPlace(hub.route);
    await invoke("live_leave", { parcel: JSON.stringify({ storage: storage() }) });
  }

  /** "Look for new pushes now": asks for a look at GitHub and says what it found, instead of
   *  leaving the menu click without an answer. An older core does the look but does not report it;
   *  then all there is to say is that it was asked. */
  async look() {
    const before = this.dev?.preview?.check?.at ?? 0;
    hub.tell("Looking for new pushes…");
    this.dev = await api.devPreview(true, null, false, true);
    const t0 = Date.now();
    while (Date.now() - t0 < 60_000) {
      const p = this.dev?.preview;
      const check = p?.check;
      if (check && check.at !== before && !p?.refreshing) return hub.tell(this.found(check));
      if (!this.dev?.preview?.live) break;
      await new Promise((r) => setTimeout(r, 700));
      this.dev = await api.dev();
      // A core without `check` shows its refresh by the time it was made.
      if (!this.dev.preview?.check && this.dev.preview?.refreshed_at && !this.dev.preview.refreshing && Date.now() - t0 > 5000) return hub.tell("Asked GitHub for new pushes");
    }
    hub.tell("Still looking; GitHub is slow to answer");
  }

  private found(check: DevPreviewCheck): string {
    if (check.error) return `Could not look: ${check.error}`;
    const n = check.pushed.length;
    if (n) return `Found ${n} new push${n === 1 ? "" : "es"}: ${check.pushed.map((x) => `#${x}`).join(", ")}`;
    if (check.base_moved) return "Found something merged; it is in the live pages now";
    return "Checked, nothing new";
  }

  /** Back to the installed pages, saying what broke and, from the files in the error, which pull
   *  request it most likely came from. */
  private async broke(what: unknown) {
    const text = what instanceof Error ? `${what.message}\n${what.stack ?? ""}` : String(what);
    await quiet(this.refresh(), "refresh");
    const blamed = this.included.filter((pr) => pr.files?.some((f) => f.startsWith("app/") && text.includes(f.slice("app/".length))));
    const first = text.split("\n")[0].slice(0, 200);
    const who = blamed.length === 1 ? `#${blamed[0].number} ${blamed[0].title} broke the live pages` : "The live pages broke";
    await leaveBroken(`${who}: ${first}`);
  }

  /** Right click on the rail button: every open pull request, and whether it shows live. */
  menu(): MenuEntry[] {
    if (this.needsUpdate) return [{ label: "Live mode comes with the next full update of Hub", disabled: true }];
    const p = this.dev?.preview?.live ? this.dev.preview : null;
    const open = p?.open ?? [];
    const skipped = new Map((p?.skipped ?? []).map((s) => [s.number, s.why ?? "left out"]));
    const shown = new Set((p?.included ?? []).map((s) => s.number));
    return [
      { label: showingLive ? "Live: the open pull requests" : "Live is off", heading: true },
      ...open.map((pr) => ({
        label: `#${pr.number} ${pr.title}`,
        hint: shown.has(pr.number) ? "live" : (skipped.get(pr.number) ?? "waiting for the next refresh"),
        checked: shown.has(pr.number),
        run: () => openUrl(pr.url),
      })),
      // Merged, and the update not installed yet: the live pages already have them (they are in main).
      ...(p?.merged ?? []).map((pr) => ({
        label: `#${pr.number} ${pr.title}`,
        hint: pr.why ?? "merged, being built",
        checked: true,
        run: () => openUrl(pr.url),
      })),
      !open.length && !p?.merged?.length && { label: p ? (p.refreshed_at ? "No open pull requests" : "Reading the open pull requests") : "Turn it on to see the open pull requests", disabled: true },
      p?.message && { label: p.message, disabled: true },
      null,
      showingLive
        ? { label: "Back to the installed pages", icon: "back", run: () => this.leave().catch((e) => hub.fail(e)) }
        : { label: "Show the live pages", icon: "live", run: () => this.toggle() },
      p && { label: "Look for new pushes now", icon: "swap", run: () => this.look().catch((e) => hub.fail(e)) },
      { label: "Dev mode in Labs", run: () => hub.show({ view: "labs" }) },
    ];
  }
}

export const live = new Live();
