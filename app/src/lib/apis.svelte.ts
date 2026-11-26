// API connections as last loaded, shared by Settings' list, its sidebar and the project page.
import { quiet } from "./quiet";
import { api } from "./api";
import type { ApiConnection } from "./bindings/ApiConnection";
import type { ApiState } from "./bindings/ApiState";
import type { ApiStatus } from "./bindings/ApiStatus";
import type { TuneState } from "./bindings/TuneState";
import type { TuneSuggestion } from "./bindings/TuneSuggestion";
import { hubdHas } from "./compat";
import { poll } from "./poll";

export type Health = "working" | "failing" | "unchecked" | "nokey";

class Apis {
  data = $state<ApiState | null>(null);
  /** The decision model's latest background tune; null on a core without one (`apis.tune.run`). */
  tune = $state<TuneState | null>(null);
  private polling: (() => void) | null = null;

  /** Suggestions not yet used or skipped. */
  get open(): TuneSuggestion[] {
    return this.tune?.suggestions.filter((s) => !s.answer) ?? [];
  }

  async loadTune() {
    if (!(await quiet(hubdHas("apis.tune.run"), "hubd has", false))) return (this.tune = null); // compat-until: 2026-12-03
    this.tune = await api.tuneState();
    this.follow();
    return this.tune;
  }

  /** Starts a tune; it reads in the background and the state is looked at until it is done. */
  async runTune() {
    this.tune = await api.tuneRun();
    this.follow();
  }

  async answer(id: string, use: boolean) {
    this.tune = await api.tuneAnswer(id, use);
    if (use) await this.load();
  }

  private follow() {
    if (this.tune?.running && !this.polling) {
      this.polling = poll(async () => {
        try {
          this.tune = await api.tuneState();
        } catch {
          // Looked at again on the next tick.
        }
        if (!this.tune?.running && this.polling) {
          this.polling();
          this.polling = null;
        }
      }, 3_000);
    }
  }

  async load() {
    this.data = await api.apis();
    return this.data;
  }

  status(c: ApiConnection): ApiStatus | undefined {
    return this.data?.status.find((s) => s.name === c.name);
  }

  health(c: ApiConnection): Health {
    const s = this.status(c);
    if (!s?.key) return "nokey";
    if (!s.check) return "unchecked";
    return s.check.ok ? "working" : "failing";
  }
}

export const apis = new Apis();

export const HEALTH: Record<Health, string> = {
  working: "Working",
  failing: "Failing",
  unchecked: "Not checked",
  nokey: "No key",
};
