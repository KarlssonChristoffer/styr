// The rows of the composer's model picker: pure, so the picker component only draws them.
import { splitModels, type Choice, type ConfigOption } from "./session";
import { favorites, harnessLabel, providerOf } from "./providers.svelte";

/** `head` is the heading above the row: the harness when the tab has several, else the provider group. */
export type ModelRow = { engine: string; choice: Choice | null; head?: string };

export type RowsFrom = {
  query: string;
  /** The provider tab, or "favorites". */
  rail: string;
  showOld: boolean;
  engineNames: string[];
  modelsOf: (engine: string) => ConfigOption | undefined;
  tabEngines: (engine: string) => string[];
};

export function modelRows({ query, rail, showOld, engineNames, modelsOf, tabEngines }: RowsFrom): ModelRow[] {
  const q = query.trim().toLowerCase();
  if (q) {
    return engineNames.flatMap((e) =>
      (modelsOf(e)?.choices ?? [])
        .filter((c) => `${c.name} ${c.value} ${c.description ?? ""} ${providerOf(e, c.name).name}`.toLowerCase().includes(q))
        .map((choice) => ({ engine: e, choice })),
    );
  }
  if (rail === "favorites") {
    return favorites.keys.flatMap((k) => {
      const at = k.indexOf(":");
      const e = k.slice(0, at);
      const choice = engineNames.includes(e) ? modelsOf(e)?.choices.find((c) => c.value === k.slice(at + 1)) : undefined;
      return choice ? [{ engine: e, choice }] : [];
    });
  }
  const group = tabEngines(rail);
  const several = group.length > 1;
  return group.flatMap((e): ModelRow[] => {
    const o = modelsOf(e);
    if (!o?.choices.length) return [{ engine: e, choice: null, head: several ? harnessLabel(e) : undefined }];
    const { top, more } = splitModels(o);
    const list = [...top, ...(showOld ? more : [])];
    const groups = new Set(list.map((c) => c.group).filter(Boolean));
    let last: string | undefined;
    return list.map((choice, i) => {
      // Several harnesses: a head for each, with the provider's group only where it has more than one.
      const part = several && groups.size > 1 && choice.group ? ` · ${choice.group}` : "";
      const head = several
        ? i === 0 || (part && choice.group !== last)
          ? harnessLabel(e) + part
          : undefined
        : choice.group && choice.group !== last
          ? choice.group
          : undefined;
      last = choice.group;
      return { engine: e, choice, head };
    });
  });
}

/** How many older models the tab keeps behind "More models". */
export function olderCount({ query, rail, modelsOf, tabEngines }: Pick<RowsFrom, "query" | "rail" | "modelsOf" | "tabEngines">): number {
  if (query.trim() || rail === "favorites") return 0;
  return tabEngines(rail).reduce((n, e) => {
    const o = modelsOf(e);
    return n + (o ? splitModels(o).more.length : 0);
  }, 0);
}

// Said wherever a pick would change provider: the new model is not the one that knew the thread.
export const SWITCH_WARNING = "Tool output and files stay behind.";
