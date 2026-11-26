// The look at a Mac's iPhone simulator (feature `sim.view`, docs/ARCHITECTURE.md "The simulator
// view"). A thread whose machine has a booted simulator gets a phone button in its header; the
// phone floats over the chat without a frame and can be enlarged into the right panel.
import { quiet } from "./quiet";

import { api } from "./api";
import { tooOld } from "./compat";
import type { SimView } from "./bindings/SimView";
import { hub } from "./state.svelte";
import { poll } from "./poll";

/** Where the phone is shown: loose over the chat, or big in the right panel. */
type Shown = { host: string | null; udid: string; where: "float" | "panel"; thread: string };

const key = (host: string | null | undefined) => host ?? "";

class Sim {
  /** What each machine last said, by host name ("" is this machine). */
  views = $state<Record<string, SimView>>({});
  shown = $state<Shown | null>(null);
  /** Machines that cannot show a simulator (not a Mac, or a Hub before the feature): not asked again. */
  private none = new Set<string>();
  private watching = new Map<string, number>();

  view(host: string | null | undefined): SimView | undefined {
    return this.views[key(host)];
  }

  /** Machines whose hubd has no touch yet (it answered 404): the screen stays a picture there. */
  noTouch = $state<Record<string, boolean>>({});

  /** Whether a machine takes touches. A hubd from before `sim.touch` is found out by its first try. */
  canTouch(host: string | null | undefined): boolean {
    return !this.noTouch[key(host)];
  }

  /** Sends touches or a button to the watched simulator. Never throws: a dropped touch is not worth a toast. */
  async touch(host: string | null | undefined, udid: string, input: { touches?: { phase: "begin" | "move" | "end"; x: number; y: number }[]; button?: string }) {
    try {
      await api.simInput(udid, input, host);
    } catch (e) {
      if (tooOld(e)) this.noTouch[key(host)] = true;
    }
  }

  /** Machines whose hubd has no `sim.app` yet: the Hub app menu says it needs the next version. */
  noApp = $state<Record<string, boolean>>({});

  /** Starts Hub's own app in the simulator, or opens a thread or a tab in it. */
  async app(host: string | null | undefined, udid: string, app: { action: "start" | "thread" | "tab"; thread?: string; tab?: number }) {
    try {
      await api.simApp(udid, app, host);
    } catch (e) {
      if (tooOld(e)) this.noApp[key(host)] = true;
      else hub.fail(e);
    }
  }

  /** The simulator being started from the GUI, by machine: starting takes up to a minute. */
  starting = $state<Record<string, string>>({});
  /** Machines whose hubd cannot shut a simulator down yet (404 once). */
  noShutdown = $state<Record<string, boolean>>({});

  /** Whether a Mac has simulators to start though none runs: a phone button that offers them. */
  canStart(host: string | null | undefined): boolean {
    const v = this.view(host);
    return !!v?.available && v.devices.length > 0;
  }

  /** Starts a simulator and shows the phone for the thread once it is up. */
  async start(thread: string, host: string | null, udid: string) {
    const k = key(host);
    this.starting[k] = udid;
    try {
      this.views[k] = await api.simBoot(udid, host);
      await this.open(thread, host, udid);
    } catch (e) {
      hub.fail(e);
    } finally {
      delete this.starting[k];
    }
  }

  /** Shuts a simulator down and closes the phone. */
  async powerOff(host: string | null, udid: string) {
    const s = this.shown;
    if (s?.udid === udid) {
      this.shown = null;
      if (hub.viewing?.kind === "sim") hub.viewing = null;
    }
    try {
      this.views[key(host)] = await api.simShutdown(udid, host);
    } catch (e) {
      if (tooOld(e)) this.noShutdown[key(host)] = true;
      else hub.fail(e);
    }
  }

  /** Whether this machine has a simulator running (or being watched) worth a button. */
  has(host: string | null | undefined): boolean {
    const v = this.view(host);
    return !!v?.available && (v.devices.some((d) => d.booted) || v.live.phase !== "off");
  }

  /** The running simulator to look at: the one watched, else the first booted. */
  pick(host: string | null | undefined): string | null {
    const v = this.view(host);
    return v?.live.udid ?? v?.devices.find((d) => d.booted)?.udid ?? null;
  }

  async refresh(host: string | null | undefined) {
    const k = key(host);
    if (this.none.has(k)) return;
    try {
      const v = await api.simView(host);
      if (!v.available) this.none.add(k);
      this.views[k] = v;
    } catch (e) {
      if (tooOld(e)) this.none.add(k);
    }
  }

  /** Asks a machine now and every few seconds while `use` is held. Returns what ends it. */
  watch(host: string | null | undefined): () => void {
    const k = key(host);
    const n = (this.watching.get(k) ?? 0) + 1;
    this.watching.set(k, n);
    void this.refresh(host);
    const timer = poll(() => void this.refresh(host), 4000);
    return () => {
      timer();
      this.watching.set(k, (this.watching.get(k) ?? 1) - 1);
    };
  }

  /** Shows the phone: starts the live picture of the running simulator and floats it. */
  async open(thread: string, host: string | null, udid?: string | null) {
    const id = udid ?? this.pick(host);
    if (!id) return;
    this.shown = { host, udid: id, where: "float", thread };
    try {
      this.views[key(host)] = await api.simLive(id, true, host);
    } catch (e) {
      hub.fail(e);
      this.shown = null;
    }
  }

  async switchTo(udid: string) {
    const s = this.shown;
    if (!s || s.udid === udid) return;
    this.shown = { ...s, udid };
    try {
      this.views[key(s.host)] = await api.simLive(udid, true, s.host);
    } catch (e) {
      hub.fail(e);
    }
  }

  /** Floating or in the panel. The panel is a viewer of the thread (`kind: "sim"`). */
  place(where: "float" | "panel") {
    const s = this.shown;
    if (!s) return;
    this.shown = { ...s, where };
    if (where === "panel") hub.viewing = { thread: s.thread, kind: "sim", id: s.udid };
    else if (hub.viewing?.kind === "sim") hub.viewing = null;
  }

  async close() {
    const s = this.shown;
    this.shown = null;
    if (hub.viewing?.kind === "sim") hub.viewing = null;
    if (s) await quiet(api.simLive(null, false, s.host).then((v) => (this.views[key(s.host)] = v)), "sim live");
  }
}

export const sim = new Sim();
