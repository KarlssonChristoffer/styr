// Who is behind an engine and a model, so the composer can say it plainly with a name and a mark:
// Claude, OpenAI, DeepSeek. Engines are whatever hub.json configures; the provider is read from
// the model's name first (a Claude-compatible engine can serve DeepSeek), then the engine's.
import { api } from "./api";
import { hub } from "./state.svelte";
import { forgetOptions, knownOptions, ROUTE, type Choice, type ConfigOption } from "./session";
import type { SettingValue } from "./bindings/SettingValue";
import type { ReadyCheck } from "./bindings/ReadyCheck";
import type { ReadyFix } from "./bindings/ReadyFix";
import type { Effort } from "./bindings/Effort";
import { quietly } from "./quiet";

export type ProviderId = "anthropic" | "openai" | "deepseek" | "google" | "other";

export type Provider = { id: ProviderId; name: string };

const PROVIDERS: Record<ProviderId, Provider> = {
  anthropic: { id: "anthropic", name: "Claude" },
  openai: { id: "openai", name: "OpenAI" },
  deepseek: { id: "deepseek", name: "DeepSeek" },
  google: { id: "google", name: "Gemini" },
  other: { id: "other", name: "Agent" },
};

const PATTERNS: [ProviderId, RegExp][] = [
  ["deepseek", /deepseek/i],
  ["openai", /openai|codex|\bgpt|\bo[1-9]\b|o[1-9]-/i],
  ["google", /gemini|google/i],
  ["anthropic", /claude|anthropic|opus|sonnet|haiku|fable/i],
];

function match(text: string | undefined): ProviderId | null {
  if (!text) return null;
  return PATTERNS.find(([, re]) => re.test(text))?.[0] ?? null;
}

/** The provider of `engine`, or of `model` on it when the model's name gives it away. */
export function providerOf(engine: string, model?: string): Provider {
  const id = match(model) ?? match(engine) ?? match(engines.commands[engine]) ?? "other";
  return named(engine, PROVIDERS[id]);
}

/** An engine Hub has no mark for goes by what it was called when it was added, e.g. "OpenCode · Kimi". */
function named(engine: string, p: Provider): Provider {
  return p.id === "other" && engines.labels[engine] ? { ...p, name: engines.labels[engine] } : p;
}

/** The provider an engine stands for, ignoring the model picked on it. */
export function engineProvider(engine: string): Provider {
  return named(engine, PROVIDERS[match(engine) ?? match(engines.commands[engine]) ?? "other"]);
}

/** Providers Hub knows how to show but that no configured engine serves yet. */
export function missingProviders(): Provider[] {
  const served = new Set(engines.names.map((e) => engineProvider(e).id));
  return (["anthropic", "openai", "deepseek"] as ProviderId[]).filter((p) => !served.has(p)).map((p) => PROVIDERS[p]);
}

// ---- Configured engines ----

export const engines = $state({
  names: [] as string[],
  commands: {} as Record<string, string>,
  labels: {} as Record<string, string>,
  default: "claude",
  loaded: false,
});

let loading: Promise<void> | null = null;

/** Reads the engines the running core can start, once. */
/** Engine choices for a `Select`: Hub's default, every engine, and `current` if it is no longer known. */
export function engineOptions(current?: string): { value: string | undefined; label: string }[] {
  const out: { value: string | undefined; label: string }[] = [{ value: undefined, label: `Default (${engines.default})` }];
  for (const name of engines.names) out.push({ value: name, label: name });
  if (current && !engines.names.includes(current)) out.push({ value: current, label: current });
  return out;
}

export function loadEngines(): Promise<void> {
  loading ??= api
    .coreSettings()
    .then((c) => {
      const commands = Object.fromEntries(Object.entries(c.running.engines).filter((e): e is [string, string] => !!e[1]));
      engines.commands = commands;
      engines.labels = Object.fromEntries(Object.entries(c.running.engine_labels ?? {}).filter((e): e is [string, string] => !!e[1]));
      // Claude first, then the rest by name, as the rail lists them.
      engines.names = Object.keys(commands).sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
      engines.default = c.running.default_engine;
      engines.loaded = true;
      for (const e of engines.names) if (strayModels(e)) forgetOptions(e);
    })
    .catch(() => {
      loading = null;
    });
  return loading;
}

/** The engines another machine's hubd can start, by machine; read once per machine. */
export const hostEngines: Record<string, string[]> = $state({});
const hostLoading = new Set<string>();

/** Reads the engines on `host`, so a thread there offers only what that machine can switch to. */
export function loadHostEngines(host: string): void {
  if (hostEngines[host] || hostLoading.has(host)) return;
  hostLoading.add(host);
  api
    .coreSettings(host)
    .then((c) => {
      const names = Object.entries(c.running.engines).filter((e) => !!e[1]).map(([name]) => name);
      hostEngines[host] = names.sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
    })
    .catch(quietly("host engines"))
    .finally(() => hostLoading.delete(host));
}

/** Why an engine cannot start on a machine now, said short enough for the picker, and what fixes it. */
export type Unready = {
  /** "Sign-in expired", "Not signed in", "Not installed". */
  short: string;
  /** The machine's own words, e.g. "the sign-in ran out 34 min ago: Authentication required". */
  detail: string;
  /** The button that fixes it right there, when one can. */
  action: string | null;
  fix: ReadyFix | null;
};

/** Engines that cannot start on a machine now (not installed, not signed in), by machine; "" is
 *  this one. Read from the machine's "Ready for work" checks. */
export const notReady: Record<string, Record<string, Unready>> = $state({});
const readyAt = new Map<string, number>();

function unready(c: ReadyCheck): Unready {
  const fix = c.fix;
  const signIn = fix?.kind === "sign_in" || fix?.kind === "keep_signed_in";
  const short = / not found$/.test(c.detail)
    ? "Not installed"
    : /ran out|stopped working/.test(c.detail)
      ? "Sign-in expired"
      : signIn
        ? "Not signed in"
        : "Not ready";
  return { short, detail: c.detail, action: signIn ? "Sign in" : fix?.kind === "shell" ? "Open terminal" : null, fix };
}

/** Reads which engines are not ready on `host` (or here), at most every half minute. */
export function loadReady(host?: string | null, again = false): void {
  const key = host ?? "";
  if (!again && Date.now() - (readyAt.get(key) ?? 0) < 30_000) return;
  readyAt.set(key, Date.now());
  api
    .machineReady(host)
    .then((r) => {
      const out: Record<string, Unready> = {};
      for (const c of r.checks) if (c.key.startsWith("engine:") && c.state === "missing") out[c.key.slice("engine:".length)] = unready(c);
      notReady[key] = out;
    })
    .catch(() => readyAt.delete(key));
}

/** Fixes an engine that is not ready, as the machine's page would: a terminal on `machine` (named
 *  as this machine knows it; `host` is its `notReady` key) that signs it in, or a shell for what
 *  has no button. Answers what to tell the user: what to type when no window opens, or the hint. */
export async function fixEngine(host: string | null | undefined, machine: string, u: Unready): Promise<string | null> {
  const f = u.fix;
  if (!f) return null;
  const open =
    f.kind === "keep_signed_in"
      ? { what: "sign_in" as const, engine: f.engine }
      : f.kind === "sign_in"
        ? { what: "engine" as const, engine: f.engine }
        : f.kind === "shell"
          ? { what: "shell" as const, engine: null }
          : null;
  if (!open) return null;
  await hub.openTerminal(machine, open);
  // Check again soon, so the picker turns the engine back on once it is signed in.
  for (const at of [20_000, 60_000, 120_000]) setTimeout(() => loadReady(host, true), at);
  return f.kind === "shell" ? f.hint : null;
}

/** Reads the engines again, e.g. after one was added and the core restarted. */
export function reloadEngines(): Promise<void> {
  loading = null;
  return loadEngines();
}

const ORDER: ProviderId[] = ["anthropic", "openai", "deepseek", "google", "other"];
const rank = (engine: string) => ORDER.indexOf(engineProvider(engine).id);

/**
 * Whether the models remembered for `engine` all belong to another provider, as when an older
 * Hub saved one engine's options under the engine a thread had just switched to.
 */
function strayModels(engine: string): boolean {
  const own = engineProvider(engine).id;
  if (own === "other") return false;
  const named = (knownModels(engine)?.choices ?? []).map((c) => match(`${c.name} ${c.value}`)).filter((p) => p !== null);
  // A Claude-compatible engine can serve DeepSeek, so only Claude and OpenAI count as stray.
  return named.length > 0 && named.every((p) => p !== own && (p === "anthropic" || p === "openai"));
}

// ---- What the harnesses and providers can do (Settings > Engines' catalog) ----

/** Per engine its harness, per provider its real thinking levels, per harness whether Hub has settings for it. */
export const harnessFacts = $state({
  harnessOf: {} as Record<string, string>,
  /** Per engine its harness's name ("Hermes", "Claude Code"), for the picker's group heads. */
  labels: {} as Record<string, string>,
  /** Per engine that runs on a provider Hub knows, the models to offer before it has ever run. */
  models: {} as Record<string, ConfigOption>,
  efforts: {} as Record<string, Effort[]>,
  settings: {} as Record<string, boolean>,
  loaded: false,
});
let factsLoading: Promise<void> | null = null;

/** Reads the catalog once; the composer narrows thinking levels with it. */
export function loadHarnessFacts(): Promise<void> {
  factsLoading ??= api
    .engineCatalog()
    .then((c) => {
      harnessFacts.harnessOf = Object.fromEntries(c.engines.map((e) => [e.name, e.harness ?? e.name]));
      harnessFacts.efforts = Object.fromEntries(c.providers.filter((p) => p.efforts.length).map((p) => [p.id, p.efforts]));
      harnessFacts.settings = Object.fromEntries(c.harnesses.map((h) => [h.id, h.settings]));
      const label = Object.fromEntries(c.harnesses.map((h) => [h.id, h.label]));
      harnessFacts.labels = Object.fromEntries(c.engines.map((e) => [e.name, label[e.harness ?? e.name] ?? e.name]));
      // An engine on a provider's key lists that provider's models from the start, as the agent
      // would once it has run. `pairs` is missing from a hubd older than `engines.pairs`.
      const routed = (e: (typeof c.engines)[number]) => {
        const h = c.harnesses.find((x) => x.id === (e.harness ?? e.name));
        return !!h && !!e.provider && (h.providers.includes(e.provider) || ((h as { pairs?: string[] }).pairs ?? []).includes(e.provider));
      };
      harnessFacts.models = {};
      for (const e of c.engines) {
        const p = c.providers.find((x) => x.id === e.provider);
        if (!p || !p.has_key || !routed(e)) continue;
        const plain = (e.model ?? "").replace(new RegExp(`^${p.id}[:/]`), "") || p.models[0];
        harnessFacts.models[e.name] = {
          id: ROUTE,
          name: "Model",
          category: "model",
          type: "select",
          value: `${p.id}:${plain}`,
          choices: p.models.map((m) => ({ value: `${p.id}:${m}`, name: m, description: p.label, group: p.label })),
        };
      }
      harnessFacts.loaded = true;
    })
    .catch(() => {
      factsLoading = null;
    });
  return factsLoading;
}

/** The thinking levels of the provider a Hub provider pick (`deepseek:deepseek-flash`) runs on. */
export function routeEfforts(route: unknown): Effort[] {
  if (typeof route !== "string" || !route.includes(":")) return [];
  return harnessFacts.efforts[route.slice(0, route.indexOf(":"))] ?? [];
}

/** The model setting an engine offered last time, if it has run on this computer. */
export function knownModels(engine: string): ConfigOption | undefined {
  return knownOptions(engine).find((o) => o.category === "model");
}

/** What the picker lists for an engine: its models as last offered, else (an engine on a provider's
 *  key that has not run here yet) the provider's from the catalog. */
export function modelsFor(engine: string): ConfigOption | undefined {
  return knownModels(engine) ?? harnessFacts.models[engine];
}

/** Which harness runs `engine`, as its name reads in a list: "Hermes", "Claude Code". */
export function harnessLabel(engine: string): string {
  return harnessFacts.labels[engine] ?? engines.labels[engine]?.split(" · ")[0] ?? engine;
}

/** The picker's tabs: one per provider, so a model that two harnesses run (DeepSeek in Hermes and
 *  in Claude Code) is one tab with a head per harness. An engine Hub has no mark for has its own. */
export type Tab = { key: string; engines: string[] };
const HARNESS_ORDER = ["hermes", "opencode", "claude", "codex", "gemini"];
export function tabsOf(names: string[]): Tab[] {
  const tabs: Tab[] = [];
  for (const e of names) {
    const id = engineProvider(e).id;
    const into = id === "other" ? undefined : tabs.find((t) => engineProvider(t.key).id === id);
    if (into) into.engines.push(e);
    else tabs.push({ key: e, engines: [e] });
  }
  const at = (e: string) => {
    const i = HARNESS_ORDER.indexOf(harnessFacts.harnessOf[e] ?? e);
    return i < 0 ? HARNESS_ORDER.length : i;
  };
  for (const t of tabs) t.engines.sort((a, b) => at(a) - at(b) || a.localeCompare(b));
  return tabs;
}

// The engine new threads start with, as last picked on Home or a project page.
const ENGINE_KEY = "hub.newThreadEngine";

export function lastEngine(): string | null {
  try {
    return localStorage.getItem(ENGINE_KEY);
  } catch {
    return null;
  }
}

export function rememberEngine(engine: string) {
  try {
    localStorage.setItem(ENGINE_KEY, engine);
  } catch {
    // Only a convenience.
  }
}

// ---- What new threads start with (Settings > Engines > New threads) ----

const START_KEY = "hub.startDefaults";
const PROJECT_ENGINES_KEY = "hub.projectEngines";

function readMap<T>(key: string): Record<string, T> {
  try {
    return JSON.parse(localStorage.getItem(key) ?? "{}");
  } catch {
    return {};
  }
}

/** Where a project's new threads start differently: another engine, and/or its own model and
 * effort (by option id, for the engine they start on). */
export type ProjectStart = { engine?: string; options?: Record<string, SettingValue> };

function readProjects(): Record<string, ProjectStart> {
  // Kept as just the engine's name before projects could pin a model and effort too.
  const raw = readMap<string | ProjectStart>(PROJECT_ENGINES_KEY);
  return Object.fromEntries(Object.entries(raw).map(([path, v]) => [path, typeof v === "string" ? { engine: v } : v]));
}

/**
 * Per engine, the model and effort (by option id) every new thread starts on. An engine with none
 * starts on what its last thread used, as before. `projects` are the exceptions, by folder.
 */
export const startDefaults = $state<{ by: Record<string, Record<string, SettingValue>>; projects: Record<string, ProjectStart> }>({
  by: readMap(START_KEY),
  projects: readProjects(),
});

export function setStartDefault(engine: string, option: string, value: SettingValue | undefined) {
  const mine = { ...(startDefaults.by[engine] ?? {}) };
  if (value === undefined) delete mine[option];
  else mine[option] = value;
  const by = { ...startDefaults.by, [engine]: mine };
  if (!Object.keys(mine).length) delete by[engine];
  startDefaults.by = by;
  try {
    localStorage.setItem(START_KEY, JSON.stringify(by));
  } catch {
    // Only a convenience.
  }
}

/** How a project's new threads start, by its folder; undefined (or nothing set) drops it. */
export function setProjectStart(folder: string, start: ProjectStart | undefined) {
  const projects = { ...startDefaults.projects };
  const options = Object.fromEntries(Object.entries(start?.options ?? {}).filter(([, v]) => v !== undefined));
  const tidy: ProjectStart = { ...(start?.engine ? { engine: start.engine } : {}), ...(Object.keys(options).length ? { options } : {}) };
  if (Object.keys(tidy).length) projects[folder] = tidy;
  else delete projects[folder];
  startDefaults.projects = projects;
  try {
    localStorage.setItem(PROJECT_ENGINES_KEY, JSON.stringify(projects));
  } catch {
    // Only a convenience.
  }
}

/** Whether `folder` is `project` or inside it, and how near: the project's length, else -1. */
export function folderIn(folder: string, project: string): number {
  const f = folder.replace(/[\\/]+$/, "").replace(/\\/g, "/").toLowerCase();
  const p = project.replace(/[\\/]+$/, "").replace(/\\/g, "/").toLowerCase();
  return p && (f === p || f.startsWith(p + "/")) ? p.length : -1;
}

/** How the project `folder` is in starts its threads, the nearest one when projects nest. */
export function projectStart(folder: string): ProjectStart | undefined {
  let best: [number, ProjectStart] | undefined;
  for (const [path, start] of Object.entries(startDefaults.projects)) {
    const n = folderIn(folder, path);
    if (n >= 0 && (!best || n > best[0])) best = [n, start];
  }
  return best?.[1];
}

/** The engine set for the project `folder` is in, if it has its own. */
export const projectEngine = (folder: string): string | undefined => projectStart(folder)?.engine;

// ---- Favorites ----

const FAVORITES_KEY = "hub.favoriteModels";

function readFavorites(): string[] {
  try {
    return JSON.parse(localStorage.getItem(FAVORITES_KEY) ?? "[]");
  } catch {
    return [];
  }
}

export const favorites = $state({ keys: readFavorites() });

export const favoriteKey = (engine: string, c: Choice) => `${engine}:${c.value}`;

export function toggleFavorite(engine: string, c: Choice) {
  const key = favoriteKey(engine, c);
  favorites.keys = favorites.keys.includes(key) ? favorites.keys.filter((k) => k !== key) : [...favorites.keys, key];
  try {
    localStorage.setItem(FAVORITES_KEY, JSON.stringify(favorites.keys));
  } catch {
    // Only a convenience.
  }
}

/** A machine's name short enough to sit under a lamp: "christoffer's macbook pro" reads "macbook". */
export function machineShort(m: string): string {
  const words = m.replace(/^.*'s\s+/i, "").split(/\s+/);
  return words[0].length > 2 ? words[0] : words.slice(0, 2).join(" ");
}
