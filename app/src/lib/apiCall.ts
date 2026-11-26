// The fetch helpers every call of the app's API goes through, and the query pieces they share.
import { quiet } from "./quiet";
import { BASE } from "./home";
import { ApiError } from "./compat";

export async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(BASE + path, {
    method,
    headers: body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new ApiError(err.error ?? res.statusText, res.status);
  }
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

/** Sends a file's bytes as they are, for the core to save; it answers with where. */
export async function upload(path: string, file: Blob): Promise<{ path: string }> {
  const res = await fetch(BASE + path, { method: "POST", headers: { "content-type": "application/octet-stream" }, body: file });
  if (!res.ok) throw new Error((await quiet(res.json(), "error body", null))?.error ?? res.statusText);
  return res.json();
}

/** For requests that carry a client id, so the core carries each out once however often it
 * arrives: while the core restarts (an update every few minutes) the request is tried again
 * for up to a minute instead of failing. */
export async function retried<T>(method: string, path: string, body: unknown): Promise<T> {
  const until = Date.now() + 60_000;
  for (let wait = 500; ; wait = Math.min(wait * 2, 4_000)) {
    try {
      return await call<T>(method, path, body);
    } catch (err) {
      // fetch throws a TypeError when nothing answers; an answer with an error is final.
      if (!(err instanceof TypeError) || Date.now() + wait > until) throw err;
      await new Promise((r) => setTimeout(r, wait));
    }
  }
}

/** `&host=` for requests about another machine; nothing for this one. */
export const at = (host?: string | null) => (host ? `&host=${encodeURIComponent(host)}` : "");
/** The same as the only query. */
export const on = (host?: string | null) => (host ? `?host=${encodeURIComponent(host)}` : "");
export const p = (path: string) => `path=${encodeURIComponent(path)}`;
