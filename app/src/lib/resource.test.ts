// Tests for resource(): sharing, polling, invalidation and errors.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { notifyResources, resource, type ResourceState } from "./resource";
import type { StreamItem } from "./api";

const flush = () => new Promise((r) => setTimeout(r, 0));
const msg = (type: string) => ({ type }) as unknown as StreamItem;

describe("resource", () => {
  const stops: Array<() => void> = [];
  const watch = <T>(r: { subscribe: (f: (s: ResourceState<T>) => void) => () => void }) => {
    let last!: ResourceState<T>;
    stops.push(r.subscribe((s) => (last = s)));
    return () => last;
  };
  afterEach(() => {
    while (stops.length) stops.pop()!();
    vi.useRealTimers();
  });

  it("reads once for everyone with the same key", async () => {
    const fetch = vi.fn(async () => 7);
    const a = watch(resource("k1", fetch));
    const b = watch(resource("k1", fetch));
    await flush();
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(a().data).toBe(7);
    expect(b().data).toBe(7);
    expect(a().loading).toBe(false);
  });

  it("keeps the failure as text and the old value", async () => {
    let n = 0;
    const fetch = async () => {
      if (n++ === 0) return "ok";
      throw new Error("down");
    };
    const r = resource("k2", fetch);
    const s = watch(r);
    await flush();
    await r.refresh();
    expect(s().data).toBe("ok");
    expect(s().error).toBe("down");
  });

  it("reads again when a message says so, and on every resync", async () => {
    const fetch = vi.fn(async () => 1);
    watch(resource("k3", fetch, { invalidateOn: (m) => m.type === "halt" }));
    await flush();
    notifyResources(msg("usage"));
    await flush();
    expect(fetch).toHaveBeenCalledTimes(1);
    notifyResources(msg("halt"));
    await flush();
    expect(fetch).toHaveBeenCalledTimes(2);
    notifyResources(msg("resync"));
    await flush();
    expect(fetch).toHaveBeenCalledTimes(3);
  });

  it("asks once more when it was told to read during a read", async () => {
    let release!: () => void;
    let calls = 0;
    const fetch = () => {
      calls++;
      return new Promise<number>((res) => (release = () => res(calls)));
    };
    const r = resource("k4", fetch);
    const s = watch(r);
    void r.refresh();
    expect(calls).toBe(1);
    release();
    await flush();
    await flush();
    expect(calls).toBe(2);
    release();
    await flush();
    expect(s().data).toBe(2);
  });

  it("does nothing on refresh without a subscriber and leaves no entry", async () => {
    const fetch = vi.fn(async () => 1);
    await resource("k6", fetch).refresh();
    expect(fetch).not.toHaveBeenCalled();
    watch(resource("k6", fetch));
    await flush();
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("forgets the value after the last subscriber leaves", async () => {
    const fetch = vi.fn(async () => 1);
    resource("k7", fetch).subscribe(() => {})();
    await flush();
    let last: ResourceState<number> | undefined;
    stops.push(resource("k7", fetch).subscribe((s) => (last = s)));
    expect(last?.data).toBeUndefined();
  });

  describe("polling", () => {
    beforeEach(() => vi.useFakeTimers());
    it("polls while subscribed and stops after the last one leaves", async () => {
      const fetch = vi.fn(async () => 1);
      const stop = resource("k5", fetch, { poll: 1000 }).subscribe(() => {});
      await vi.advanceTimersByTimeAsync(3100);
      expect(fetch).toHaveBeenCalledTimes(4);
      stop();
      await vi.advanceTimersByTimeAsync(5000);
      expect(fetch).toHaveBeenCalledTimes(4);
    });
  });
});
