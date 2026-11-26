// Warm start: asks for a scratch folder and a thread id ahead of time so Send starts faster.
import { api } from "./api";
import type { SettingValue } from "./bindings/SettingValue";
import { quietly } from "./quiet";

/** What a new thread would start as, if the message went now. */
export type Start = {
  engine: string;
  /** The folder; empty for "No project", which gets a fresh scratch folder (`scratch`). */
  cwd: string;
  /** The machine, when it is not this one. */
  host?: string | null;
  settings: Record<string, SettingValue>;
};

/**
 * Starts the engine for a new thread while its first message is typed, so it is up by the time
 * the message goes (`warm.rs` in the core). Call `typing` as text comes in and `take` when the
 * thread is created: it gives the id to create it with and, for "No project", the scratch folder
 * the engine started in. Only on this machine, and not on Auto, which picks the engine by the
 * message. When the composer changes (another engine, folder or provider) the next key starts
 * again for that.
 */
export function warmer(scratch: () => Promise<string>) {
  let key = "";
  let id: string | undefined;
  let folder: string | undefined;
  let asking: Promise<void> | undefined;
  const keyOf = (s: Start) => JSON.stringify([s.engine, s.cwd, s.settings["hub_route"] ?? null]);

  return {
    typing(s: Start) {
      if (s.host || s.engine === "auto" || keyOf(s) === key) return;
      key = keyOf(s);
      const asked = key;
      asking = (async () => {
        const cwd = s.cwd || (folder ??= await scratch());
        const got = await api.warmNew({ cwd, engine: s.engine, settings: s.settings });
        if (asked === key) id = got.id ?? undefined;
      })().catch(() => {
        if (asked === key) key = "";
      });
    },
    /** The id and folder to start the thread `s` with; forgets them, as they are used once. */
    async take(s: Start): Promise<{ warm?: string; scratch?: string }> {
      await asking?.catch(quietly("warm start"));
      const same = keyOf(s) === key && !s.host;
      const out = { warm: same ? id : undefined, scratch: same && !s.cwd ? folder : undefined };
      key = "";
      id = folder = asking = undefined;
      return out;
    },
  };
}
