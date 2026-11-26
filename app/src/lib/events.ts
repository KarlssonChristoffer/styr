// The payload of an event, typed by its kind. The types are generated from the Rust structs
// (`bindings/EventPayloads.ts`, one per kind). The log is read as it was written and is not
// checked here, so a reader keeps its fallback (`?? ""`) where it had one.
import type { Event } from "./bindings/Event";
import type { EventPayloads } from "./bindings/EventPayloads";

/** What an event of kind `K` carries. A field every writer of the kind has written is required
 *  (the golden log in hub-core pins that); one an older log lacks is optional. */
export type Payload<K extends keyof EventPayloads> = EventPayloads[K];

/** An event's kind and payload as one pair, so a `switch` on `kind` knows what `d` holds. */
export type Typed = { [K in keyof EventPayloads]: { kind: K; d: Payload<K> } }[keyof EventPayloads] | { kind: "unknown"; d: Record<string, never> };

/** The event as its kind and typed payload. A payload that is not an object reads as empty. */
export function typed(e: Event): Typed {
  const d = e.data && typeof e.data === "object" && !Array.isArray(e.data) ? e.data : {};
  return { kind: e.kind, d } as Typed;
}

/** The payload of `e` when it is of kind `kind`, else null. */
export function payload<K extends keyof EventPayloads>(e: Event, kind: K): Payload<K> | null {
  const t = typed(e);
  return t.kind === kind ? (t.d as Payload<K>) : null;
}

/** A word from the log as the union the reader names. The log is not checked: a word a newer Hub
 *  wrote passes through as it is, and the reader's own fallback takes it. */
export function known<T extends string | undefined>(word: string | undefined): T {
  return word as T;
}
