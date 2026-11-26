// How Hub starts every agent (`agent_prefs.rs`): when it compacts and how it writes, everywhere
// and per project. One copy for Settings > Engines and Settings > Personality.
import { api } from "./api";
import type { AgentPrefs } from "./bindings/AgentPrefs";
import type { ProjectPrefs } from "./bindings/ProjectPrefs";
import { hub } from "./state.svelte";

export const agent = $state<{ prefs: AgentPrefs | null }>({ prefs: null });

let loading: Promise<void> | null = null;
/** Reads them once; later calls wait for the same read. */
export function loadAgentPrefs(): Promise<void> {
  loading ??= api
    .agentPrefs()
    .then((p) => void (agent.prefs = p))
    .catch((e) => {
      loading = null;
      hub.fail(e);
    });
  return loading;
}

export async function saveAgentPrefs(change: Partial<AgentPrefs>) {
  if (!agent.prefs) return;
  const before = agent.prefs;
  agent.prefs = { ...before, ...change };
  try {
    agent.prefs = await api.setAgentPrefs(agent.prefs);
  } catch (e) {
    agent.prefs = before;
    hub.fail(e);
  }
}

/** Changes one project's exception; a project left with nothing set is dropped (by the core too). */
export function setProjectPrefs(path: string, change: Partial<ProjectPrefs> | null) {
  if (!agent.prefs) return;
  const projects = { ...agent.prefs.projects };
  const next: ProjectPrefs = change === null ? { compact: null, output_style: null } : { compact: null, output_style: null, ...projects[path], ...change };
  if (next.compact == null && next.output_style == null) delete projects[path];
  else projects[path] = next;
  return saveAgentPrefs({ projects });
}

export const COMPACT = [
  { value: "engine", label: "Auto", hint: "When the agent finds its context nearly full" },
  { value: "60", label: "60", hint: "At 60% of the context" },
  { value: "70", label: "70", hint: "At 70% of the context" },
  { value: "80", label: "80", hint: "At 80% of the context" },
  { value: "90", label: "90", hint: "At 90% of the context" },
  { value: "never", label: "Off", hint: "Only when you press Compact by Send" },
];

/** Everywhere's compaction as one of COMPACT's values. */
export const compactWord = (p: AgentPrefs | null): string =>
  !p ? "engine" : p.auto_compact_off ? "never" : p.compact_at ? String(p.compact_at) : "engine";

export const STYLES = [
  { value: "", label: "Short", hint: "Does the work and says little: the agent's own default." },
  { value: "Explanatory", label: "Explains", hint: "Says why it chose what it did as it goes." },
  { value: "Learning", label: "Teaches", hint: "Explains, and leaves small parts for you to write." },
];
