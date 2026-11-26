// One way to let a call fail without telling the person: `quiet(promise, "what it was")` gives
// back the value, or the fallback (undefined when none) if the call failed, and leaves one
// `console.debug` line saying what failed. Use it for a refresh or a nice-to-have where an
// error has nothing to show; a failure the person must hear about is `try`/`catch` into the UI.
// A bare empty catch hides what went wrong from the console too, so use this instead.
export function quiet<T>(promise: PromiseLike<T>, what: string): Promise<T | undefined>;
export function quiet<T, F>(promise: PromiseLike<T>, what: string, fallback: F): Promise<T | F>;
export function quiet<T, F>(promise: PromiseLike<T>, what: string, fallback?: F): Promise<T | F | undefined> {
  return Promise.resolve(promise).then(
    (v) => v,
    (err: unknown) => {
      console.debug(`quiet: ${what}`, err);
      return fallback;
    },
  );
}

/** For a chain that already ends in `.catch`: `p.then(...).catch(quietly("what"))`. Same line in the console as `quiet`. */
export function quietly(what: string): (err: unknown) => void {
  return (err) => console.debug(`quiet: ${what}`, err);
}
