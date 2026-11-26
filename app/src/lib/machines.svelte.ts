// State behind the Machines page: the list of machines and their live status.
import { api } from "./api";
import { quiet } from "./quiet";
import { inTauri, isDevWindow, poppedViewing, relaunch } from "./native";
import { keepPlace } from "./history.svelte";
import { confirmPages } from "./rollback";
import { recall, remember } from "./lastSeen";
import { prefs, setPref } from "./prefs.svelte";
import type { AccountUsage } from "./bindings/AccountUsage";
import type { HostInfo } from "./bindings/HostInfo";
import type { PlanUsage } from "./bindings/PlanUsage";
import type { ReleaseNotes } from "./bindings/ReleaseNotes";
import type { Tailnet } from "./bindings/Tailnet";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import type { Route } from "./viewTypes";

/** What the machines and the updater need from the rest of `hub`. */
export interface MachinesLink {
  hosts: HostInfo[];
  problemsNeedYou: string[];
  route: Route;
  fail(e: unknown): void;
}

/** The tailnet and the machines found on it, the version updater, and the subscriptions' limits.
 *  `hub` passes its fields of the same names on to this, so nothing that reads `hub.update` or
 *  `hub.plan` has to know. */
export class Machines {
  constructor(private link: MachinesLink) {}

  /** The tailnet, where new machines show up; null until read. */
  tailnet = $state<Tailnet | null>(null);
  update = $state<UpdateStatus | null>(null);
  /** The window is on its way to a new version: `InstallBox` covers the page until it reloads. */
  installing = $state(false);
  /** Claude's plan limits. The last ones seen show at once; fresh ones replace them quietly. */
  plan = $state<PlanUsage | null>(recall("plan"));
  /** The other subscriptions' limits, Codex's for one, read from their own logs. */
  others = $state<AccountUsage[]>(recall("others") ?? []);
  /** Patch notes of the versions the updater built, newest first. */
  notes = $state<ReleaseNotes[]>([]);
  /** The version you pressed Install for; it goes on once no thread is working. */
  installAsked = $state<string | null>(null);
  /** A change Show me on the Updates page took you to, shown in a small card until you close it. */
  tour = $state<{ text: string; version: string } | null>(null);
  /** A machine being set up that wants its SSH password once, to take this machine's key. */
  machineAsk = $state<{ name: string; ssh: string; question: string } | null>(null);

  /** Takes new limits and keeps them for the next time the window opens. */
  setUsage(u: { plan?: PlanUsage | null; others?: AccountUsage[] }) {
    if (u.plan) remember("plan", (this.plan = u.plan));
    if (u.others) remember("others", (this.others = u.others));
  }

  /** Every subscription with limits to look at: Claude's from `/usage`, then the others. */
  get subscriptions(): AccountUsage[] {
    const claude: AccountUsage = { engine: "claude", label: "Claude", plan: this.plan, note: null, balance: null, spent_today: null };
    return [claude, ...this.others];
  }

  /** Patch notes of the running version that you have not opened yet. */
  freshNotes = $derived(
    this.notes.find((n) => n.version === this.update?.version && n.version !== prefs.seenNotes) ?? null,
  );
  /** A built version waiting for your Install, for the sidebar's update row and the avatar's dot.
   *  Null when there is none, after Install, or once you closed the row for this version. */
  updateReady = $derived(
    this.update?.state === "ready" && this.update.target && this.installAsked !== this.update.target && prefs.laterUpdate !== this.update.target
      ? this.update.target
      : null,
  );

  /** Installs the ready version here: now, or once the threads working are done. */
  async installUpdate() {
    const target = this.update?.target;
    if (!target) return;
    try {
      await api.installUpdate();
      this.installAsked = target;
      if (prefs.laterUpdate) setPref("laterUpdate", "");
    } catch (e) {
      this.link.fail(e);
    }
  }

  /** Linux machines and Macs that joined the tailnet and are not in Hub yet: ones to offer setting up. */
  newMachines = $derived((this.tailnet?.peers ?? []).filter((p) => p.supported && p.online && !p.machine));

  /** Reads which problems need the user on this machine and every online one, the way the Problems
   *  page does, so the sidebar and the page count the same thing. A machine that does not answer
   *  keeps what it had. */
  async refreshProblemsNeedYou() {
    const hosts: (string | null)[] = [null, ...this.link.hosts.filter((h) => !h.local && h.state === "online").map((h) => h.name)];
    const got = await Promise.all(
      hosts.map((host) =>
        api.problems({ view: "needs_you", limit: 2000 }, host).then(
          (ps) => ({ host: host ?? "", keys: ps.map((p) => `${host ?? ""}:${p.id}`) }),
          () => null,
        ),
      ),
    );
    const answered = new Set(got.filter((g) => g != null).map((g) => g!.host));
    const online = new Set(hosts.map((h) => h ?? ""));
    // Keep a machine's last answer only while it is online and just failed to answer this time.
    const kept = this.link.problemsNeedYou.filter((k) => {
      const host = k.slice(0, k.lastIndexOf(":"));
      return online.has(host) && !answered.has(host);
    });
    this.link.problemsNeedYou = [...kept, ...got.flatMap((g) => g?.keys ?? [])];
  }

  async refreshTailnet() {
    try {
      this.tailnet = await api.tailnet();
    } catch {
      // An older core has no tailnet list; nothing to offer then.
    }
  }

  /** Adds a machine found on the tailnet or the home network; the core installs hubd there when
   * it has none. An empty name names it after its host name. */
  async setUpMachine(target: { name: string; ssh: string }, password?: string) {
    const answer = await api.setUpHost({ name: target.name, ssh: target.ssh, password });
    if (answer.password) {
      this.machineAsk = { ...target, question: answer.password };
      return false;
    }
    this.machineAsk = null;
    if (answer.host && !this.link.hosts.some((h) => h.name === answer.host!.name)) this.link.hosts.push(answer.host);
    await this.refreshTailnet();
    return true;
  }

  async dismissMachine(name: string) {
    await api.dismissTailnet(name);
    await this.refreshTailnet();
  }

  onUpdate(status: UpdateStatus) {
    const was = this.update?.state;
    this.update = status;
    // Pages that got this far show the version on probation is alive.
    quiet(confirmPages(status, (v) => api.updateAlive(v)), "update alive");
    // A version is on offer: fetch its patch notes for the prompt.
    if (status.state === "ready" && status.target !== null && !this.notes.some((n) => n.version === status.target)) {
      quiet(api.notes().then((n) => (this.notes = n)), "notes");
    }
    // The core is about to restart into a new version; the window follows it, in place when only
    // the pages changed. Should the app itself restart, it comes back to this page.
    if (status.state === "restarting" && was !== "restarting") {
      // Only the main window's place: a popped-out viewer or the dev window has its own.
      if (!poppedViewing() && !isDevWindow) keepPlace(this.link.route);
      // The shell reloads this page when the new version is up; until then the box says so. A
      // window that is not the app's (a browser tab, the dev window) has no one to reload it.
      if (inTauri && !isDevWindow) this.installing = true;
      relaunch().then((moved) => { if (!moved) this.installing = false; }, () => (this.installing = false));
    }
    // A version that did not come up leaves the old core running, with no reload to come.
    if (status.state === "failed") this.installing = false;
  }
}
