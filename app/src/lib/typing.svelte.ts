// When you last typed in a message box, after T3 Code's composer typing guard. A question that
// shows up while you type is held until you have stopped for a moment, so a key or a click meant
// for your message does not answer it. Anything that answers from the keyboard asks `typingNow`.

export const QUIET_MS = 1500;

export const typing = $state({ at: 0, focused: false });

export function typed() {
  typing.at = Date.now();
}

/** True while a message box has the keyboard and was typed in less than `QUIET_MS` ago. */
export function typingNow(now = Date.now()): boolean {
  return typing.focused && now - typing.at < QUIET_MS;
}
