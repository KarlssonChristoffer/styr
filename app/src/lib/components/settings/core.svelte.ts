// What the pages of Settings share: the core's settings, what the about and account calls say, and
// the few helpers more than one page uses. Used by Settings.svelte and the pages next to it.
import { api } from "../../api";
import { openUrl } from "../../native";
import { hub } from "../../state.svelte";
import { reloadEngines } from "../../providers.svelte";
import type { About } from "../../bindings/About";
import type { Accounts } from "../../bindings/Accounts";
import type { CoreSettings } from "../../bindings/CoreSettings";
import type { GuardPrefs } from "../../bindings/GuardPrefs";

/** Read once when Settings opens, and again when the core comes back after a restart. */
export const sc = $state<{ about: About | null; accounts: Accounts | null; core: CoreSettings | null; restarting: boolean }>({
  about: null,
  accounts: null,
  core: null,
  restarting: false,
});

/** The calls every page may want read: about, accounts and the core's settings. */
export function loadShared() {
  api.about().then((a) => (sc.about = a)).catch((e) => hub.fail(e));
  api.accounts().then((a) => (sc.accounts = a)).catch((e) => hub.fail(e));
  api.coreSettings().then((c) => (sc.core = c)).catch((e) => hub.fail(e));
}

/** Changes one setting of the core; what comes back is the settings as saved. */
export async function edit(key: string, value: string | number | boolean | null) {
  try {
    sc.core = await api.editConfig({ key, value });
  } catch (e) {
    hub.fail(e);
  }
}

/** Reads the core's settings again after the engines changed, and what the picker offers. */
export function reloadCore() {
  api.coreSettings().then((c) => (sc.core = c)).catch((e) => hub.fail(e));
  reloadEngines();
}

export async function restart() {
  try {
    await api.restartCore();
    sc.restarting = true;
  } catch (e) {
    hub.fail(e);
  }
}

export const open = (url: string) => openUrl(url).catch((e) => hub.fail(e));
export const openFolder = (path: string, how: "files" | "editor" | "terminal" = "files") =>
  api.openFolder(path, how).catch((e) => hub.fail(e));
export const parent = (path: string) => path.replace(/[\\/][^\\/]*$/, "");

export const cap = (s: string) => s[0].toUpperCase() + s.slice(1);

/** A percent typed as text, handed on only when it is between 1 and 100. */
export function pct(v: string, then: (n: number) => void) {
  const n = Number(v.replace("%", "").trim());
  if (Number.isFinite(n) && n >= 1 && n <= 100) then(n);
  else hub.fail(new Error("Type a percent between 1 and 100"));
}

/** Saves the quota guard's settings. */
export async function guard(patch: Partial<GuardPrefs>) {
  if (!hub.guard) return;
  try {
    hub.guard = await api.setGuard({ ...hub.guard, ...patch });
  } catch (e) {
    hub.fail(e);
  }
}
