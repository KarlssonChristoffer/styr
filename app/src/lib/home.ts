// Which Hub this window uses. Normally the core on this computer; on a laptop or a work computer
// it can be the Hub on another machine instead (the PC, or a server that is always on), reached
// over the tailnet with a token of its own, so every computer shows the same threads, projects and
// settings. As in T3 Code (github.com/pingdotgg/t3code, MIT), the token is only shown once the
// machine at that address has proved it is the one paired with (`/api/health` names its id), so a
// different machine that answers there later never gets it.
import { quiet } from "./quiet";

const KEY = "hub.home";
export const LOCAL = (import.meta.env.VITE_HUB_URL as string | undefined) ?? "http://127.0.0.1:7878";

export type Home = { url: string; token: string; name: string; machine: string | null };

function read(): Home | null {
  try {
    const home = JSON.parse(localStorage.getItem(KEY) ?? "null");
    return home && typeof home.url === "string" && typeof home.token === "string" ? home : null;
  } catch {
    return null;
  }
}

/** The other machine's Hub this window uses, or null for this computer's own. */
export const home = read();
export const BASE = home?.url ?? LOCAL;
/** The token this window shows its core: the paired one for another machine's Hub, or this
 * computer's connection token once `ownToken` has read it. Until then the window gets in by its
 * origin, as every window did before. */
export let TOKEN: string | null = home?.token ?? null;

/** Reads this computer's connection token, so the window shows it like every other client and a
 * program here cannot get in by sending the app's `Origin` alone. The desktop shell reads it from
 * the data folder of the core at `LOCAL` (`core_token`, the dev core's for dev mode's window). A
 * shell from before `core_token`, or a plain browser, has none and goes by its origin, which only
 * works until a window has shown the core its token (`auth::WINDOW_SHOWS_TOKEN`); after that a
 * browser tab on Vite needs another machine's pairing or the token. Called once, before the app
 * starts (`main.ts`). */
export async function ownToken(): Promise<void> {
  if (home) return;
  const token = await shellToken();
  if (!token) return;
  TOKEN = token;
  const plain = window.fetch.bind(window);
  window.fetch = (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (!url.startsWith(LOCAL)) return plain(input, init);
    const headers = new Headers(init?.headers ?? (input instanceof Request ? input.headers : undefined));
    if (!headers.has("authorization")) headers.set("authorization", `Bearer ${token}`);
    return plain(input, { ...init, headers });
  };
}

async function shellToken(): Promise<string | null> {
  if (!("__TAURI_INTERNALS__" in window)) return null;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const token = await invoke<string | null>("core_token", { core: LOCAL });
    return typeof token === "string" && token ? token : null;
  } catch {
    // A shell without the command: the window goes by its origin.
    return null;
  }
}

type Health = { machine?: string | null; name?: string };

async function health(url: string, fetcher: typeof fetch): Promise<Health> {
  const res = await fetcher(`${url}/api/health`, { signal: AbortSignal.timeout(10_000) });
  if (!res.ok) throw new Error(`${url} answered ${res.status}`);
  return res.json();
}

/** Whether the machine answering at the home address is the one paired with. Asked once; until
 * it answers, requests wait, and a different machine gets no token. */
let proved: Promise<boolean> | null = null;

if (home) {
  const plain = window.fetch.bind(window);
  const prove = () => (proved ??= health(home.url, plain).then((h) => !home.machine || h.machine === home.machine).catch(() => {
    // Not reachable now: ask again next time instead of deciding.
    proved = null;
    return false;
  }));
  window.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (!url.startsWith(home.url) || url.startsWith(`${home.url}/api/health`)) return plain(input, init);
    if (!(await prove())) return plain(input, init);
    const headers = new Headers(init?.headers ?? (input instanceof Request ? input.headers : undefined));
    headers.set("authorization", `Bearer ${home.token}`);
    return plain(input, { ...init, headers });
  };
}

/** The stream's address, with the token (browsers cannot set headers on a WebSocket). */
export function streamUrl(): string {
  return socketUrl("/api/stream");
}

/** Any WebSocket on the core, with the token when the window has one. */
export function socketUrl(path: string): string {
  const url = BASE.replace(/^http/, "ws") + path;
  return TOKEN ? `${url}?token=${encodeURIComponent(TOKEN)}` : url;
}

/** Turns what was typed into an address: `pc`, `pc.tail1234.ts.net` and `https://pc…/` all work. */
export function address(typed: string): string {
  let url = typed.trim().replace(/\/+$/, "");
  if (!/^https?:\/\//.test(url)) url = `https://${url}`;
  return url;
}

function deviceName(): string {
  const ua = navigator.userAgent;
  if (/Mac/.test(ua)) return "Mac";
  if (/Windows/.test(ua)) return "Windows computer";
  if (/Linux/.test(ua)) return "Linux computer";
  return "Computer";
}

/** Pairs this window with the Hub at `typed` using a code made there (Settings > Machines > Make a
 * code, or `hubd pair`), and keeps using it from now on. The caller reloads. */
export async function useHubAt(typed: string, code: string): Promise<Home> {
  const url = address(typed);
  const before = await health(url, fetch).catch(() => {
    throw new Error(`Nothing answers at ${url}. Is Hub running there, and does Tailscale serve it (hubd pair --tailscale)?`);
  });
  const res = await fetch(`${url}/api/pair/redeem`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ code: code.trim(), name: deviceName() }),
  });
  if (!res.ok) throw new Error((await quiet(res.json(), "error body", null))?.error ?? `The code did not work (${res.status})`);
  const paired: { token: string; machine: string } = await res.json();
  const chosen: Home = { url, token: paired.token, name: before.name ?? paired.machine, machine: before.machine ?? null };
  localStorage.setItem(KEY, JSON.stringify(chosen));
  await localCore(false);
  return chosen;
}

/** Goes back to this computer's own Hub. The caller reloads. */
export async function useOwnHub(): Promise<void> {
  localStorage.removeItem(KEY);
  await localCore(true);
}

/** Tells the desktop shell whether to keep a core running on this computer. */
async function localCore(on: boolean) {
  if (!("__TAURI_INTERNALS__" in window)) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await quiet(invoke("set_local_core", { on }), "invoke");
}
