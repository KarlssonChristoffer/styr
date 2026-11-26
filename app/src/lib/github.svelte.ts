// The GitHub repositories of the account `gh` is signed in as, for the folder pickers: the project
// picker in the new-thread box and the "Folder or repo" list under +. The core of each machine
// signs in on its own (`github_repos.rs`); the list is the account's, so one signed-in machine is
// enough and it is asked from the first one that is (this one first). A machine whose hubd is
// older than feature `github.repos` answers "not found": it is marked `old`, never an error.
import { api } from "./api";
import type { GithubConnect } from "./bindings/GithubConnect";
import type { GithubRepo } from "./bindings/GithubRepo";

/** Where one machine stands: what `/api/github/connect` says, or that its Hub is too old / failed to answer. */
export type MachineState = Partial<GithubConnect> & { state: string };

export type Gh = {
  /** Every repository of the account, newest push first. */
  repos: GithubRepo[];
  login: string | null;
  /** `unknown` before the first look, `loading` while looking for the first time. */
  state: "unknown" | "loading" | "ready" | "none" | "missing" | "old";
  /** Why the list is empty, from `gh`. */
  message: string | null;
  /** By machine ("" for this one). */
  machines: Record<string, MachineState>;
};

export const gh = $state<Gh>({ repos: [], login: null, state: "unknown", message: null, machines: {} });

const hk = (h: string | null | undefined) => h ?? "";
const isOld = (e: unknown) => /not found/i.test((e as Error)?.message ?? "") || (e as { status?: number })?.status === 404;
let loadedAt = 0;
let running: Promise<void> | null = null;

/** The key a local clone has (`github.com/owner/name`, lower case) for a GitHub repository. */
export const keyOf = (r: GithubRepo) => `github.com/${r.full}`.toLowerCase();

/** `https://github.com/owner/name.git`, what a clone is made from. */
export const cloneUrlOf = (r: GithubRepo) => `https://github.com/${r.full}.git`;

/** Looks at these machines (null is this one), then lists the repositories from the first signed in. */
export function load(hosts: (string | null)[], fresh = false): Promise<void> {
  if (running) return running;
  if (!fresh && gh.state !== "unknown" && gh.state !== "loading" && Date.now() - loadedAt < 60_000) return Promise.resolve();
  running = (async () => {
    if (gh.state === "unknown") gh.state = "loading";
    const states = await Promise.all(
      hosts.map(async (h): Promise<[string | null, MachineState]> => {
        try {
          return [h, await api.githubConnect(h)];
        } catch (e) {
          return [h, isOld(e) ? { state: "old" } : { state: "unknown" }];
        }
      }),
    );
    for (const [h, s] of states) gh.machines[hk(h)] = s;
    const ready = states.find(([, s]) => s.state === "ready");
    if (ready) {
      try {
        const list = await api.githubRepos(ready[0], fresh);
        gh.repos = list.repos;
        gh.login = list.login ?? ready[1].login ?? null;
        gh.message = list.message ?? null;
        gh.state = "ready";
      } catch (e) {
        gh.message = (e as Error).message;
        gh.state = gh.repos.length ? "ready" : "none";
      }
    } else if (states.length && states.every(([, s]) => s.state === "old")) {
      gh.state = "old";
      gh.message = null;
    } else if (states.length && states.every(([, s]) => s.state === "missing" || s.state === "old")) {
      gh.state = "missing";
      gh.message = states.find(([, s]) => s.state === "missing")?.[1].message ?? null;
    } else if (gh.state !== "ready") {
      gh.state = "none";
      gh.message = null;
    }
    loadedAt = Date.now();
  })().finally(() => (running = null));
  return running;
}

/** Looks again at one machine, e.g. while a sign-in waits for its code to be typed. */
export async function look(host: string | null): Promise<MachineState> {
  try {
    const s = await api.githubConnect(host);
    gh.machines[hk(host)] = s;
    return s;
  } catch (e) {
    return (gh.machines[hk(host)] = isOld(e) ? { state: "old" } : { state: "unknown" });
  }
}

/** Starts the sign-in on a machine: its state is `waiting` with the code, or why it could not. */
export async function connect(host: string | null): Promise<MachineState> {
  try {
    return (gh.machines[hk(host)] = await api.githubConnectStart(host));
  } catch (e) {
    return (gh.machines[hk(host)] = isOld(e) ? { state: "old" } : { state: "failed", message: (e as Error).message });
  }
}

/** Signs a machine out of GitHub. A hubd without feature `github.account` answers "not found": `old`. */
export async function signOut(host: string | null): Promise<MachineState> {
  try {
    return (gh.machines[hk(host)] = await api.githubDisconnect(host));
  } catch (e) {
    return (gh.machines[hk(host)] = isOld(e) ? { state: "old" } : { state: "failed", message: (e as Error).message });
  }
}

export async function giveUp(host: string | null): Promise<void> {
  try {
    gh.machines[hk(host)] = await api.githubConnectCancel(host);
  } catch {
    gh.machines[hk(host)] = { state: "unknown" };
  }
}

/** A signed-in machine says so: the list is asked again at once. */
export const refresh = (hosts: (string | null)[]) => load(hosts, true);
