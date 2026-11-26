// The tools that take a thread's work ("Hand to Knegarn"), per machine, for the thread menu.
// The Workers view refreshes it whenever it reads the tools; the app reads it once at start.
import { api } from "./api";
import type { OwnTool } from "./bindings/OwnTool";
import { hub } from "./state.svelte";

export type Taker = { host: string | null; folder: string; name: string; label: string; about: string | null; canBuild: boolean };

export const takers = $state<{ list: Taker[] }>({ list: [] });

/** Keeps the tools of `host` that take handoffs. */
export function noteTools(host: string | null, tools: OwnTool[]) {
  const others = takers.list.filter((t) => t.host !== host);
  const these = tools
    .filter((t) => t.handoff && !t.error)
    .map((t) => ({ host, folder: t.folder, name: t.name, label: t.handoff!.label, about: t.handoff!.about, canBuild: !!t.handoff!.can_build }));
  takers.list = [...others, ...these];
}

export async function loadTakers() {
  const remotes = hub.hosts.filter((h) => !h.local && h.state === "online").map((h) => h.name);
  await Promise.all(
    [null, ...remotes].map((host) =>
      api.ownTools(host).then(
        (tools) => noteTools(host, tools),
        () => undefined,
      ),
    ),
  );
}

/** The thread whose work is being handed over, and to which tool. */
export const handing = $state<{ to: { thread: string; taker: Taker } | null }>({ to: null });
