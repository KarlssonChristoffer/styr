// The last answer to a slow question, kept in the webview so a view can show it at once and
// swap in the new one when it comes. Losing it only means waiting once.
const PREFIX = "hub.last.";

export function recall<T>(key: string): T | null {
  try {
    const text = localStorage.getItem(PREFIX + key);
    return text ? (JSON.parse(text) as T) : null;
  } catch {
    return null;
  }
}

export function remember(key: string, value: unknown) {
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(value));
  } catch {
    // Full or blocked storage; the view just waits next time.
  }
}
