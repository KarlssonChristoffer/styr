// A made-up core for shots.cjs: the same answers every run, so a picture only changes when the app
// does. Anything not listed answers an empty list (GET) or {} (the rest). Times hang off NOW, which
// shots.cjs also freezes as the page's clock.
const NOW = Date.parse("2026-11-06T12:00:00Z");
const MIN = 60e3;
const thread = (n, title, status, ago, more = {}) => ({
  id: `t-${n}`, title, cwd: "/work/hub", engine: "claude", status, engine_session_id: null, title_locked: true, archived: false,
  created_at: NOW - ago * MIN - 600e3, updated_at: NOW - ago * MIN, returned_at: status === "running" ? null : NOW - ago * MIN,
  seen_at: null, settled_at: null, sent_at: NOW - ago * MIN - 60e3, pinned_at: null, snoozed_until: null, auto_settle: false,
  parent: null, reading: null, owner: null, ...more,
});
const threads = [
  thread(1, "Split the app's giant files", "running", 2),
  thread(2, "Which model for the review?", "waiting_on_you", 9),
  thread(3, "Fix the Safari export button", "idle", 35),
  thread(4, "Tidy the sidebar spacing", "idle", 120, { seen_at: NOW - 100 * MIN }),
  thread(5, "Wire up the GitHub account row", "error", 300, { cwd: "/work/knegarn" }),
];

const ev = (n, seq, kind, data, ago) => ({ thread_id: `t-${n}`, seq, ts: NOW - ago * MIN, kind, data });
const chunk = (text) => ({ sessionUpdate: "agent_message_chunk", content: { type: "text", text } });
const events = {
  "t-1": [
    ev(1, 1, "user_message", { text: "Split state.svelte.ts into slices, no behaviour change." }, 12),
    ev(1, 2, "update", chunk("I'll start with the types and the pure helpers, then move the derived lists."), 11),
    ev(1, 3, "update", { sessionUpdate: "tool_call", toolCallId: "c1", title: "Read state.svelte.ts", status: "completed", kind: "read" }, 10),
    ev(1, 4, "update", chunk("The first slice is in place. `hub` stays the facade, so no importer changes."), 3),
    ev(1, 5, "session_state", { config_options: [
      { id: "mode", name: "Mode", category: "mode", type: "select", currentValue: "default", options: [
        { value: "default", name: "default", description: "Asks before it edits or runs a command" },
        { value: "acceptEdits", name: "acceptEdits", description: "Edits files without asking" },
        { value: "plan", name: "plan", description: "Plans, changes nothing" },
        { value: "bypassPermissions", name: "bypassPermissions", description: "Never asks" } ] },
      { id: "model", name: "Model", category: "model", type: "select", currentValue: "sonnet", options: [
        { value: "sonnet", name: "Sonnet" }, { value: "opus", name: "Opus" } ] },
      { id: "effort", name: "Effort", category: "thought_level", type: "select", currentValue: "medium", options: [
        { value: "low", name: "low", description: "Quick" }, { value: "medium", name: "medium", description: "Balanced" }, { value: "high", name: "high", description: "Thorough" } ] },
    ] }, 2),
  ],
  "t-3": [
    ev(3, 1, "user_message", { text: "The export button does nothing on Safari." }, 40),
    ev(3, 2, "update", chunk("Fixed: Safari needs the click handler on the button, not on the icon."), 36),
    ev(3, 3, "turn_end", { stopReason: "end_turn" }, 35),
  ],
};

const plan = (name, a, b) => ({ plan: name, checked_at: NOW, limits: [
  { label: "5-hour limit", percent: a, resets: null, resets_at: NOW + 3 * 3600e3 },
  { label: "Weekly limit", percent: b, resets: null, resets_at: NOW + 4 * 86400e3 },
] });
const caps = (engine, value, name, extra = {}) => ({ engine, value, name, catalog: null, found: true, corrected: false, context: 200000,
  image_in: true, pdf_in: true, tools: true, reasoning: true, web_search: false, cost: null, ...extra });
const coreValues = { default_engine: "claude", engines: { claude: "claude-agent-acp", codex: "codex-acp" }, engine_labels: { claude: "Claude", codex: "Codex" },
  checkpoints: true, tidy_model: null, voice_language: "en", voice_mute_key: null, voice_polish: true, update_enabled: true, update_branch: "main",
  update_ask: false, update_target_dir: null, update_interval_secs: 300, update_build_mode: "quiet", update_build_merges: 1,
  update_build_notice_at: [], update_build_max_wait_mins: 10, update_build_times: [], editor: null, terminal: null };

// path (no query) -> answer. A function gets the request's { method, url }.
const answers = {
  "GET /api/health": { ok: true, machine: "mock", name: "this computer" },
  "GET /api/threads": threads,
  "GET /api/projects": [{ path: "/work/hub", name: "hub", created_at: NOW - 30 * 86400e3 }, { path: "/work/knegarn", name: "knegarn", created_at: NOW - 20 * 86400e3 }],
  "GET /api/usage": { plan: plan("max", 24, 61), plan_error: null, refreshing: false, threads: [],
    others: [{ engine: "codex", label: "Codex", plan: plan("plus", 0, 3), note: null }] },
  "GET /api/prefs/accounts": { keys: [], engines: [
    { engine: "claude", label: "Claude", command: null, logged_in: true, method: "subscription", plan: "max", email: null, organization: null, source: null },
    { engine: "codex", label: "Codex", command: null, logged_in: true, method: "subscription", plan: "plus", email: null, organization: null, source: null },
  ] },
  "GET /api/hosts": [{ name: "this computer", local: true, address: "127.0.0.1", state: "online", version: "0.0.0", error: null, removable: false, setup: null, join: null, sending: null }],
  "GET /api/host": { hostname: "mock", os: "linux", cpu_percent: 12, mem_used: 6e9, mem_total: 16e9, listeners: [] },
  "GET /api/version": { version: "0.0.0", state: "idle", target: null, message: null, building: null, step: null, checked_at: NOW },
  "GET /api/halt": null,
  "GET /api/mergeq": { mode: "off", note: null, step: null, checked_at: NOW, log: null, rows: [] },
  "GET /api/tailnet": { running: false, error: null, me: null, peers: [] },
  "GET /api/looks": { machines: {}, projects: {}, pinned: [] },
  "GET /api/prefs/config": { path: "/mock/config.toml", running: coreValues, saved: coreValues, default_engines: {}, busy: false },
  "GET /api/prefs/guard": { warn_at: 80, switch_at: null, fallback_model: "sonnet", review_model: null, proof: true, leak_guard: true,
    clash_warn: true, customs: true, resume: true, limit_switch: "ask", focus_check: true, docs_check: true },
  "GET /api/apis": { prefs: { connections: [{ name: "jev", kind: "jev", label: "Jev", url: "https://jev.example", secret: "jev_key", model: null, test_path: "/health", docs: null, daily_tokens: 0, projects: [], auth: { type: "bearer" }, client_id: null }], helper: "jev", off: ["focus"], sure: {} },
    catalog: [{ kind: "jev", label: "Jev", what: "A small decision model", url: "https://jev.example", secret: "jev_key", model: null, test_path: "/health", docs: null, keys_at: null, asks: true, auth: { type: "bearer" }, paste: "", setup: "" }],
    features: [{ name: "auto", what: "Reads a reply and says whether to go on" }, { name: "focus", what: "Checks a thread stays on its task" }],
    status: [{ name: "jev", key: "…a1b2", today: { calls: 12, input_tokens: 3400, output_tokens: 90, errors: 0 }, month: { calls: 210, input_tokens: 61000, output_tokens: 1800, errors: 1 }, last_error: null, last_error_at: null, check: { ok: true, message: "ok", at: 1 } }],
    redirect: "http://127.0.0.1:7878/oauth" },
  "GET /api/apis/decisions": [{ at: 1, feature: "auto", connection: "jev", thread: null, text: "Should I go ahead and merge it?", hub: "ask", said: "asks", confidence: 0.82, agree: true, ms: 310, acted: true }, { at: 2, feature: "focus", connection: "jev", thread: null, text: "Rewrites the build script", hub: "none", said: "none", confidence: 0.4, agree: false, ms: 280, acted: false }],
  "GET /api/prefs/threads": { settle_after_days: 3, settle_on_merge: true, watch_prs: true },
  "GET /api/prefs/permissions": { level: "ask", allow: [], deny: [], protected: [] },
  "GET /api/prefs/about": { version: "0.0.0", update_enabled: true, remote: "origin", branch: "main", interval_secs: 300, repo_url: null,
    data_dir: "/mock/data", config_path: "/mock/config.toml", config_exists: true, config_problem: null, os: "linux", user: "mock", changelog: [] },
  "GET /api/prefs/ssh": { dir: "/mock/.ssh", keys: [], hosts: [], known_hosts: 0 },
  "GET /api/projects/defaults": { root: "/work", github: false },
  "GET /api/stats": { days: 7, answers: 0, tokens: 0, cost: 0, models: [], projects: [], threads: [] },
  "GET /api/connections": { engines: [] },
  "GET /api/connections/prefs": { off: {}, account_off: [], plugins_off: {} },
  "GET /api/customs/prefs": { auto_review: true, daily_check: true, monthly_update: true, registry: false },
  "GET /api/customs/registry": { base: "", env: [], cargo_config: "" },
  "GET /api/engines": { harnesses: [], providers: [], engines: [] },
  "GET /api/keys": { kept_in: "the system keychain", keys: [], elsewhere: [] },
  "GET /api/agent-prefs": { auto_compact_off: false, compact_at: null, output_style: null, projects: {} },
  "GET /api/prefs/disk": { items: [], data_free: 5e11, build_free: 5e11 },
  "GET /api/checkup": { checked_at: NOW, projects: [] },
  "GET /api/updates": { hubd: "0.0.0", follow: true, machines: [], at: NOW },
  "GET /api/github/connect": { state: "off", login: null, code: null, url: null, message: null },
  "GET /api/library": { root: "/mock/library" },
  "GET /api/machine/ready": { checks: [{ key: "claude", label: "Claude", state: "ok", detail: "Signed in", fix: null }, { key: "git", label: "Git", state: "warn", detail: "No identity", fix: { kind: "git_identity" } }, { key: "codex", label: "Codex", state: "missing", detail: "Not signed in", fix: { kind: "sign_in", engine: "codex" } }], git: null },
  "GET /api/cleanup": null,
  "GET /api/pulse": { moments: [], pulls: [], longest_thread: null, notes: [] },
  "GET /api/pulse/lines": { days: [], diffed: 0, commits: 0 },
  "GET /api/project/vault": { docs: [], tags: [], skipped: 0 },
  "GET /api/workers": { host: "this computer", threads: [], hub: [], containers: [] },
  "GET /api/customs/activity": { entries: [], reviewing: [] },
  "GET /api/desks": { ids: [] },
  "GET /api/models": { source: "mock", fetched_at: NOW, known: 6, error: null, overrides: "", engines: [
    { engine: "claude", draws: true, fast: null, models: [caps("claude", "opus", "Opus 5.5"), caps("claude", "sonnet", "Sonnet 5.5"), caps("claude", "haiku", "Haiku 5.5")] },
    { engine: "codex", draws: true, fast: null, models: [caps("codex", "gpt-5", "GPT-5"), caps("codex", "gpt-5-mini", "GPT-5 mini")] },
  ] },
};

/** Events of a thread, after `after`; null when the route is not one of those. */
function eventsFor(url) {
  const m = /\/api\/threads\/([^/]+)\/events/.exec(url.pathname);
  if (!m) return null;
  const after = Number(url.searchParams.get("after") ?? 0);
  return (events[m[1]] ?? []).filter((e) => e.seq > after);
}

function answer(method, url) {
  const e = eventsFor(url);
  if (e) return e;
  const key = `${method} ${url.pathname}`;
  if (key in answers) return answers[key];
  return method === "GET" ? [] : {};
}

/** Whether the call has an answer of its own (--debug lists the ones that do not). */
function known(method, url) {
  return !!eventsFor(url) || `${method} ${url.pathname}` in answers;
}

module.exports = { NOW, answer, known, threads };
