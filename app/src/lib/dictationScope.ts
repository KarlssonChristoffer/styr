// Which message box Ctrl D belongs to. The key is caught on the window, so every Composer that is
// mounted hears it; `ownsDictationKey` says whether this one should act. The box that has the
// focus does; one that is already recording does too (focus may have moved to the send button);
// with nothing focused, the one that is on screen. Another dialog that took the key already
// (the idea box) has called preventDefault, and then none of them act.
export interface DictationScope {
  /** The key event already handled by something nearer the focus. */
  handled: boolean;
  /** This composer is recording right now. */
  recording: boolean;
  /** The focused element is inside this composer. */
  focusInside: boolean;
  /** Something other than the page itself has the focus (an input, a button, a dialog). */
  focusElsewhere: boolean;
  /** This composer is visible. */
  shown: boolean;
}

export function ownsDictationKey(s: DictationScope): boolean {
  if (s.handled) return false;
  if (s.focusInside || s.recording) return true;
  return !s.focusElsewhere && s.shown;
}
