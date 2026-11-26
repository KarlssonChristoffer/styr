// What the composer's controls read out of a thread's settings: which option is the permission
// mode, which the model, which the thinking effort. Pure; used by Composer.svelte and ConfigControls.svelte.
import { isToggle, ordered, type ConfigOption, type Session } from "./session";
import type { SettingValue } from "./bindings/SettingValue";

/** Three controls, as in T3 Code: who answers (provider and model), how hard it thinks (effort and
 *  its toggles), and what it may do without asking (the permission mode). */
export function configOf(session: Session) {
  const options = ordered(session.options);
  const mode = options.find((o) => o.category === "mode" || o.id === "mode");
  const rest = options.filter((o) => o !== mode && (o.choices.length > 1 || isToggle(o) || o.category === "model"));
  const model = rest.find((o) => o.category === "model");
  const effort = rest.find((o) => o.category === "thought_level" && !isToggle(o));
  const toggles = rest.filter(isToggle);
  const others = rest.filter((o) => o !== model && o !== effort && !isToggle(o));
  return { options, mode, model, effort, toggles, others };
}

export function choiceName(o: ConfigOption) {
  return o.choices.find((c) => c.value === o.value)?.name ?? String(o.value);
}

/** Some engines name levels and modes in lower case; the controls read better capitalised. */
export const cap = (name: string) => name.charAt(0).toUpperCase() + name.slice(1);

export const isDanger = (value: SettingValue) => /bypass|full|yolo|danger/i.test(String(value));

export function modeIcon(value: SettingValue) {
  const v = String(value);
  if (isDanger(v)) return "unlock" as const;
  if (/plan/i.test(v)) return "plan" as const;
  if (/read/i.test(v)) return "eye" as const;
  if (/accept|edit/i.test(v)) return "pen" as const;
  if (/auto|dont/i.test(v)) return "auto" as const;
  return "lock" as const;
}
