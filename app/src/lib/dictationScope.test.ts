import { describe, expect, it } from "vitest";
import { ownsDictationKey } from "./dictationScope";

const base = { handled: false, recording: false, focusInside: false, focusElsewhere: false, shown: true };

describe("ownsDictationKey", () => {
  it("the focused box acts", () => expect(ownsDictationKey({ ...base, focusInside: true })).toBe(true));
  it("a box in another place stays out while focus is elsewhere", () =>
    expect(ownsDictationKey({ ...base, focusElsewhere: true })).toBe(false));
  it("a recording box stops even when focus moved", () =>
    expect(ownsDictationKey({ ...base, recording: true, focusElsewhere: true })).toBe(true));
  it("with nothing focused the visible box acts, a hidden one does not", () => {
    expect(ownsDictationKey(base)).toBe(true);
    expect(ownsDictationKey({ ...base, shown: false })).toBe(false);
  });
  it("a key another dialog handled is left alone", () =>
    expect(ownsDictationKey({ ...base, focusInside: true, handled: true })).toBe(false));
});
