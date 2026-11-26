// Folds a thread's log into what the composer offers: settings, slash commands and usage.
import type { Event } from "./bindings/Event";
import type { SettingValue } from "./bindings/SettingValue";

/** `group` is the name of the group the engine listed it under, e.g. the provider Hub runs it on. */
export type Choice = { value: string; name: string; description?: string; group?: string };

export type ConfigOption = {
  id: string;
  name: string;
  description?: string;
  category?: string;
  type: "select" | "boolean";
  value: SettingValue;
  choices: Choice[];
};

export type SlashCommand = { name: string; description: string; hint?: string };

export type Usage = { used: number; size: number; cost?: { amount: number; currency: string } };

export type Session = {
  options: ConfigOption[];
  commands: SlashCommand[];
  usage?: Usage;
  images: boolean;
  files: boolean;
  /** False until the engine has reported its settings, i.e. while its session starts. */
  reported: boolean;
  /** Settings chosen since the engine last reported, by config id. */
  pending: Record<string, SettingValue>;
  /** The engine the options came from when the thread has switched engines; else the thread's own. */
  engine?: string;
};

type Json = Record<string, any>;

export function emptySession(): Session {
  return { options: [], commands: [], images: true, files: true, reported: false, pending: {} };
}

export function foldSession(events: Event[]): Session {
  const s = emptySession();
  // A setting chosen after the engine last reported its state is shown until the engine
  // confirms or refuses it.
  const chosen = new Map<string, { value: SettingValue; seq: number }>();
  let reported = 0;
  // The agent's modes, for agents that list them apart from their settings (Hermes does).
  let modes: Json | undefined;

  for (const e of events) {
    const d = e.data as Json;
    if (e.kind === "session_state") {
      if (d.config_options) s.options = parseOptions(d.config_options);
      if (d.modes) modes = d.modes;
      s.options = withModes(s.options, modes);
      if (d.prompt_capabilities) {
        s.images = !!d.prompt_capabilities.image;
        s.files = !!d.prompt_capabilities.embeddedContext;
      }
      reported = e.seq;
    } else if (e.kind === "engine_switch") {
      // A new engine: what the old one offered and was told no longer applies.
      Object.assign(s, { ...emptySession(), pending: {}, engine: d.to });
      chosen.clear();
      reported = 0;
      modes = undefined;
    } else if (e.kind === "setting") {
      chosen.set(d.config_id, { value: d.value, seq: e.seq });
    } else if (e.kind === "update") {
      const u = d.sessionUpdate as string;
      if (u === "config_option_update" && d.configOptions) {
        // The agent lists its own settings; Hub's provider pick and the agent's modes stay.
        const route = s.options.find((o) => o.id === ROUTE);
        const fresh = parseOptions(d.configOptions);
        if (route) {
          const rest = fresh.filter((o) => o.category !== "model" && o.id !== ROUTE);
          s.options = withModes([route, ...rest], modes);
        } else s.options = withModes(fresh, modes);
        reported = e.seq;
      } else if (u === "current_mode_update") {
        if (modes) modes = { ...modes, currentModeId: d.currentModeId };
        const mode = s.options.find((o) => o.id === "mode");
        if (mode) mode.value = d.currentModeId;
      } else if (u === "available_commands_update") {
        s.commands = (d.availableCommands ?? []).map((c: Json) => ({
          name: c.name,
          description: c.description ?? "",
          hint: c.input?.hint,
        }));
      } else if (u === "usage_update") {
        s.usage = { used: d.used, size: d.size, cost: d.cost ?? undefined };
      }
    }
  }
  for (const [id, c] of chosen) {
    if (c.seq <= reported) continue;
    s.pending[id] = c.value;
    const option = s.options.find((o) => o.id === id);
    if (option) option.value = c.value;
  }
  s.reported = reported > 0;
  return s;
}

/**
 * Until a thread's engine reports its settings, offer what the engine offered last time, with
 * the thread's own choices on top. Choices made now are applied when the session is up.
 */
export function withKnownOptions(s: Session, engine: string): Session {
  if (s.reported) return s;
  const options = knownOptions(engine);
  for (const o of options) {
    if (o.id in s.pending) o.value = s.pending[o.id];
  }
  return { ...s, options };
}

/** A two-way On/Off select, shown as a toggle like a boolean setting. */
export function isToggle(o: ConfigOption): boolean {
  if (o.type === "boolean") return true;
  const values = o.choices.map((c) => c.name.toLowerCase()).sort().join();
  return values === "off,on";
}

export function isOn(o: ConfigOption): boolean {
  if (o.type === "boolean") return o.value === true;
  return o.choices.find((c) => c.value === o.value)?.name.toLowerCase() === "on";
}

export function toggled(o: ConfigOption): SettingValue {
  if (o.type === "boolean") return !o.value;
  const want = isOn(o) ? "off" : "on";
  return o.choices.find((c) => c.name.toLowerCase() === want)?.value ?? o.value;
}

function parseOptions(raw: Json[]): ConfigOption[] {
  return raw.map((o) => ({
    id: o.id,
    name: o.name,
    description: o.description ?? undefined,
    category: o.category ?? undefined,
    type: o.type === "boolean" ? "boolean" : "select",
    value: o.currentValue,
    // Grouped options are flattened; each keeps its group's name for the picker's headings.
    choices: (o.options ?? [])
      .flatMap((c: Json) => (c.options ? c.options.map((x: Json) => ({ ...x, group: c.name })) : [c]))
      .map((c: Json) => ({
        value: c.value,
        name: c.name,
        description: c.description ?? undefined,
        group: c.group ?? undefined,
      })),
  }));
}

/** Hub's own provider and model pick (`routes.rs`), listed among the agent's settings. */
export const ROUTE = "hub_route";

/**
 * The agent's modes as one more setting, when its settings have no mode of their own. Hub adds
 * its provider pick to the settings, so an agent that has modes but no settings (Hermes) still
 * gets its mode shown next to it.
 */
function withModes(options: ConfigOption[], modes: Json | undefined): ConfigOption[] {
  if (!modes?.availableModes?.length || options.some((o) => o.id === "mode" || o.category === "mode")) return options;
  return [modeOption(modes), ...options];
}

function modeOption(modes: Json): ConfigOption {
  return {
    id: "mode",
    name: "Mode",
    category: "mode",
    type: "select",
    value: modes.currentModeId,
    choices: (modes.availableModes ?? []).map((m: Json) => ({ value: m.id, name: m.name, description: m.description })),
  };
}

const ORDER = ["mode", "model", "thought_level", "model_config"];

/** Mode first, then model and effort, then the rest in the engine's order. */
export function ordered(options: ConfigOption[]): ConfigOption[] {
  const rank = (o: ConfigOption) => {
    const i = ORDER.indexOf(o.category ?? "");
    return i < 0 ? ORDER.length : i;
  };
  return [...options].sort((a, b) => rank(a) - rank(b));
}

// The last options an engine offered, so a new thread can pick a model before it has a session.
const OPTIONS_KEY = "hub.engineOptions.";
const CHOICES_KEY = "hub.newThreadSettings.";

export function rememberOptions(engine: string, options: ConfigOption[]) {
  if (!options.length) return;
  try {
    localStorage.setItem(OPTIONS_KEY + engine, JSON.stringify(options));
  } catch {
    // Only a convenience.
  }
}

export function forgetOptions(engine: string) {
  try {
    localStorage.removeItem(OPTIONS_KEY + engine);
  } catch {
    // Only a convenience.
  }
}

export function knownOptions(engine: string): ConfigOption[] {
  try {
    return JSON.parse(localStorage.getItem(OPTIONS_KEY + engine) ?? "[]");
  } catch {
    return [];
  }
}

export function savedChoices(engine: string): Record<string, SettingValue> {
  try {
    return JSON.parse(localStorage.getItem(CHOICES_KEY + engine) ?? "{}");
  } catch {
    return {};
  }
}

export function saveChoices(engine: string, choices: Record<string, SettingValue>) {
  try {
    localStorage.setItem(CHOICES_KEY + engine, JSON.stringify(choices));
  } catch {
    // Only a convenience.
  }
}

export function tokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(n >= 10_000_000 ? 0 : 1).replace(/\.0$/, "") + "M";
  if (n >= 1000) return Math.round(n / 1000) + "k";
  return String(n);
}

/**
 * A model's name as the menus show it: "Opus 5.5", and OpenAI's named models the same way, so
 * gpt-6.1-sol reads "Sol 6.1". "(recommended)" is dropped; the exact id is in the tooltip.
 */
export function modelLabel(c: Choice): string {
  const name = c.name.replace(/\s*\(recommended\)$/i, "").trim();
  const named =
    /^gpt-(\d+(?:\.\d+)*)-([a-z]+)$/i.exec(String(c.value)) ?? /^(?:gpt[- ]?)?(\d+(?:\.\d+)*)[- ]([a-z]+)$/i.exec(name);
  if (!named || VARIANTS.includes(named[2].toLowerCase())) return name;
  return `${named[2][0].toUpperCase()}${named[2].slice(1).toLowerCase()} ${named[1]}`;
}

/** Suffixes that make a variant of a model rather than name one, as in gpt-5.4-mini. */
const VARIANTS = ["codex", "mini", "nano", "pro", "max", "chat", "latest", "preview"];

const CLAUDE = ["opus", "fable", "sonnet", "haiku"];
/** The families the menus lead with, strongest first: Claude's, then OpenAI's named models. Each
 *  provider keeps this order. */
const POWER = ["fable", "opus", "sonnet", "haiku", "astra", "sol", "terra", "luna"];
const LEADING = POWER.filter((f) => CLAUDE.includes(f));
/** OpenAI's named models from this generation on lead the menu (Luna, Sol); older GPTs do not. */
const CURRENT_GPT = 6;

/**
 * The model menu leads with the current models only: the newest of each leading family, strongest
 * first (Fable, Opus, Sonnet, Haiku; Astra, Sol, Terra, Luna). Older models and "Default" go under
 * More models, and search finds them all. The current model
 * always shows at the top.
 */
export function splitModels(o: ConfigOption): { top: Choice[]; more: Choice[] } {
  // Models grouped by provider are few and all shown, group by group.
  if (o.choices.some((c) => c.group)) return { top: o.choices, more: [] };
  const newest = new Map<string, { choice: Choice; version: number[] }>();
  for (const c of o.choices) {
    const m = /^(\w+)\s+(\d+(?:\.\d+)*)$/.exec(modelLabel(c));
    if (!m) continue;
    const family = m[1].toLowerCase();
    const version = m[2].split(".").map(Number);
    const leads = CLAUDE.includes(family) ? LEADING.includes(family) : version[0] >= CURRENT_GPT;
    if (!leads) continue;
    const best = newest.get(family);
    if (!best || compare(version, best.version) > 0) newest.set(family, { choice: c, version });
  }
  const rank = (f: string) => (POWER.includes(f) ? POWER.indexOf(f) : POWER.length);
  const top = [...newest]
    .sort(([a, x], [b, y]) => rank(a) - rank(b) || compare(y.version, x.version) || a.localeCompare(b))
    .map(([, v]) => v.choice);
  if (!top.length) return { top: o.choices, more: [] };
  const current = o.choices.find((c) => c.value === o.value);
  if (current && !top.includes(current)) top.push(current);
  return { top, more: o.choices.filter((c) => !top.includes(c)) };
}

function compare(a: number[], b: number[]): number {
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const d = (a[i] ?? 0) - (b[i] ?? 0);
    if (d) return d;
  }
  return 0;
}

/** Effort levels as Claude Code names them; "default" is left to the engine. */
/**
 * The levels of an effort setting. With `real`, the levels the provider's models really have
 * (DeepSeek: Off, Low, High, Max), so a harness that lists more (Hermes lists nine) shows those
 * only, named as the provider names them.
 */
export function effortLevels(
  o: ConfigOption,
  real: { value: string; name: string; what: string; default: boolean }[] = [],
): (Choice & { recommended?: boolean; costly?: boolean })[] {
  if (real.length) {
    const offered = real.filter((r) => o.choices.some((c) => c.value === r.value));
    if (offered.length > 1) return offered.map((r) => ({ value: r.value, name: r.name, description: r.what, recommended: r.default }));
  }
  const levels = o.choices.filter((c) => c.value !== "default");
  const recommended = levels.find((c) => c.value === "medium")?.value ?? levels[0]?.value;
  return levels.map((c) => ({
    ...c,
    name: c.value === "xhigh" ? "Extra" : c.name,
    recommended: c.value === recommended,
    costly: c.value === "max",
  }));
}
