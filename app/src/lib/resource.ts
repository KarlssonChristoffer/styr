// One way to read something from hubd and keep it fresh: `resource(key, fetch, { poll, invalidateOn })`.
// Everyone who asks for the same key shares one value and one fetch; it is polled with `poll()`
// while anyone is subscribed and read again when a stream message says it is stale (a
// reconnect always does). The value follows Svelte's store contract, so a component reads
// `$r.data`, `$r.error` and `$r.loading`. Move a page onto it when the page is touched anyway.
import { poll } from "./poll";
import type { StreamItem } from "./api";

export type ResourceState<T> = { data: T | undefined; error: string | undefined; loading: boolean };

export type ResourceOptions = {
  /** Read again every this many ms while someone is subscribed (and the window is seen). */
  poll?: number;
  /** Read again when this stream message arrives. A `resync` (the stream came back) always does. */
  invalidateOn?: (msg: StreamItem) => boolean;
};

export type Resource<T> = {
  subscribe(run: (s: ResourceState<T>) => void): () => void;
  /** Read it now. Resolves when that read is over; a failure sets `error`, it does not throw. */
  refresh(): Promise<void>;
};

type Entry = {
  state: ResourceState<unknown>;
  listeners: Set<(s: ResourceState<unknown>) => void>;
  fetch: () => Promise<unknown>;
  opts: ResourceOptions;
  stopPoll?: () => void;
  /** A read in flight, so two callers do not ask twice at once. */
  inflight?: Promise<void>;
  /** Set when a read was asked for while one ran: that one is stale, read once more. */
  again: boolean;
};

const entries = new Map<string, Entry>();

function set(e: Entry, patch: Partial<ResourceState<unknown>>) {
  e.state = { ...e.state, ...patch };
  for (const l of e.listeners) l(e.state);
}

function read(e: Entry): Promise<void> {
  if (e.inflight) {
    e.again = true;
    return e.inflight;
  }
  set(e, { loading: true });
  e.inflight = e
    .fetch()
    .then(
      (data) => set(e, { data, error: undefined, loading: false }),
      (err: unknown) => set(e, { error: err instanceof Error ? err.message : String(err), loading: false }),
    )
    .finally(() => {
      e.inflight = undefined;
      if (e.again && e.listeners.size) {
        e.again = false;
        void read(e);
      }
    });
  return e.inflight;
}

/** The shared resource for `key`. The first caller's `fetch` and options win while anyone holds it. */
export function resource<T>(key: string, fetch: () => Promise<T>, opts: ResourceOptions = {}): Resource<T> {
  const entry = (): Entry => {
    let e = entries.get(key);
    if (!e) {
      e = { state: { data: undefined, error: undefined, loading: false }, listeners: new Set(), fetch, opts, again: false };
      entries.set(key, e);
    }
    return e;
  };
  return {
    subscribe(run) {
      const e = entry();
      e.listeners.add(run as (s: ResourceState<unknown>) => void);
      run(e.state as ResourceState<T>);
      if (e.listeners.size === 1) {
        void read(e);
        if (e.opts.poll) e.stopPoll = poll(() => read(e), e.opts.poll);
      }
      return () => {
        e.listeners.delete(run as (s: ResourceState<unknown>) => void);
        if (e.listeners.size === 0) {
          e.stopPoll?.();
          entries.delete(key);
        }
      };
    },
    refresh() {
      // Nobody is subscribed: nothing to keep fresh, and no entry to leave behind.
      const e = entries.get(key);
      return e ? read(e) : Promise.resolve();
    },
  };
}

/** Tell the resources a stream message arrived; `stream.svelte.ts` calls this for every message
 *  and, for a (re)connect, with `{ type: "resync" }` from `Stream.resync()`. */
export function notifyResources(msg: StreamItem) {
  for (const e of entries.values()) {
    if (e.listeners.size && (msg.type === "resync" || e.opts.invalidateOn?.(msg))) void read(e);
  }
}
