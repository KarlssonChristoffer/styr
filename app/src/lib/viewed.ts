// Files of a pull request you ticked off as read, after T3 Code's "mark files as viewed". Kept in
// this window per pull request, with a fingerprint of the file's diff: a file pushed to after you
// read it comes back as changed.

const KEY = "hub.viewed";
type Marks = Record<string, Record<string, string>>;

function load(): Marks {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}");
  } catch {
    return {};
  }
}

function save(marks: Marks) {
  try {
    localStorage.setItem(KEY, JSON.stringify(marks));
  } catch {
    // Kept until the window closes.
  }
}

/** A short fingerprint of a file's diff. */
export function fingerprint(text: string): string {
  let h = 5381;
  for (let i = 0; i < text.length; i++) h = ((h << 5) + h + text.charCodeAt(i)) | 0;
  return (h >>> 0).toString(36);
}

/** The pull request's ticks, path to fingerprint. */
export function viewedIn(pr: string): Record<string, string> {
  return load()[pr] ?? {};
}

export function setViewed(pr: string, path: string, print: string | null) {
  const marks = load();
  const mine = { ...(marks[pr] ?? {}) };
  if (print) mine[path] = print;
  else delete mine[path];
  marks[pr] = mine;
  save(marks);
}

/** Test files, by the usual folder and file names. */
export const isTest = (path: string) =>
  /(^|\/)(tests?|__tests__|spec|e2e)\//i.test(path) || /\.(test|spec)\.[cm]?[jt]sx?$/i.test(path) || /(_test\.(go|py|rs)|_spec\.rb|Tests?\.(cs|java|kt))$/.test(path);
