// The API calls about threads, messages and what a thread shows (part of `api`, see api.ts).
import type { UseHit } from "./bindings/UseHit";
import type { UseReport } from "./bindings/UseReport";
import type { KeyVault } from "./bindings/KeyVault";
import type { PackageSummary } from "./bindings/PackageSummary";
import type { TuneState } from "./bindings/TuneState";
import type { Looks } from "./bindings/Looks";
import type { ThreadReach } from "./bindings/ThreadReach";
import type { Attachment } from "./bindings/Attachment";
import type { WarmAsk } from "./bindings/WarmAsk";
import type { Warmed } from "./bindings/Warmed";
import type { EditFrom } from "./bindings/EditFrom";
import type { EmojiState } from "./bindings/EmojiState";
import type { React } from "./bindings/React";
import type { ModelTable } from "./bindings/ModelTable";
import type { Connections } from "./bindings/Connections";
import type { ThreadConnections } from "./bindings/ThreadConnections";
import type { ConnectionPrefs } from "./bindings/ConnectionPrefs";
import type { AgentPrefs } from "./bindings/AgentPrefs";
import type { HandoffPrefs } from "./bindings/HandoffPrefs";
import type { LocalModels } from "./bindings/LocalModels";
import type { DoctorReport } from "./bindings/DoctorReport";
import type { Stats } from "./bindings/Stats";
import type { Spend } from "./bindings/Spend";
import type { DevlogDay } from "./bindings/DevlogDay";
import type { ProjectDay } from "./bindings/ProjectDay";
import type { AwayReport } from "./bindings/AwayReport";
import type { HistoryHit } from "./bindings/HistoryHit";
import type { Routine } from "./bindings/Routine";
import type { Widget } from "./bindings/Widget";
import type { Problem } from "./bindings/Problem";
import type { ProblemDetail } from "./bindings/ProblemDetail";
import type { ProblemFilter } from "./bindings/ProblemFilter";
import type { ProblemPatch } from "./bindings/ProblemPatch";
import type { Dossier } from "./bindings/Dossier";
import type { VaktenStatus } from "./bindings/VaktenStatus";
import type { VaktenPrefs } from "./bindings/VaktenPrefs";
import type { ProblemRules } from "./bindings/ProblemRules";
import type { Rule } from "./bindings/Rule";
import type { SavedView } from "./bindings/SavedView";
import type { MetricSummary } from "./bindings/MetricSummary";
import type { Monitor } from "./bindings/Monitor";
import type { ProblemThread } from "./bindings/ProblemThread";
import type { ToolStats } from "./bindings/ToolStats";
import type { StatusShadow } from "./bindings/StatusShadow";
import type { ToolReview } from "./bindings/ToolReview";
import type { Report } from "./bindings/Report";
import type { Source } from "./bindings/Source";
import type { NewSource } from "./bindings/NewSource";
import type { RoutineDraft } from "./bindings/RoutineDraft";
import type { Routines } from "./bindings/Routines";
import type { Recipe } from "./bindings/Recipe";
import type { RecipeDraft } from "./bindings/RecipeDraft";
import type { Recipes } from "./bindings/Recipes";
import type { HealthReport } from "./bindings/HealthReport";
import type { AttemptGroup } from "./bindings/AttemptGroup";
import type { Event } from "./bindings/Event";
import type { GitInfo } from "./bindings/GitInfo";
import type { ThreadChange } from "./bindings/ThreadChange";
import type { FileMatch } from "./bindings/FileMatch";
import type { SettingValue } from "./bindings/SettingValue";
import type { UsageReport } from "./bindings/UsageReport";
import type { NewThread } from "./bindings/NewThread";
import type { Thread } from "./bindings/Thread";
import type { ThreadPeek } from "./bindings/ThreadPeek";
import type { ThreadPatch } from "./bindings/ThreadPatch";
import type { FixCard } from "./bindings/FixCard";
import type { ThreadPr } from "./bindings/ThreadPr";
import type { ThreadPrefs } from "./bindings/ThreadPrefs";
import type { ApiState } from "./bindings/ApiState";
import type { ApiPrefs } from "./bindings/ApiPrefs";
import type { ApiTest } from "./bindings/ApiTest";
import type { ProviderTest } from "./bindings/ProviderTest";
import type { ApiTry } from "./bindings/ApiTry";
import type { ApiAnswer } from "./bindings/ApiAnswer";
import type { ApiDecision } from "./bindings/ApiDecision";
import type { Tidied } from "./bindings/Tidied";
import type { Reach } from "./bindings/Reach";
import type { Project } from "./bindings/Project";
import type { NewProject } from "./bindings/NewProject";
import type { CreatedProject } from "./bindings/CreatedProject";
import type { ProjectDefaults } from "./bindings/ProjectDefaults";
import type { ProjectTemplates } from "./bindings/ProjectTemplates";
import type { ActivityReport } from "./bindings/ActivityReport";
import type { ActivityDetail } from "./bindings/ActivityDetail";
import type { Pulse } from "./bindings/Pulse";
import type { LinesReport } from "./bindings/LinesReport";
import type { UndoPreview } from "./bindings/UndoPreview";
import type { ReviewStarted } from "./bindings/ReviewStarted";
import type { ReviewRequest } from "./bindings/ReviewRequest";
import type { DesignReviewStarted } from "./bindings/DesignReviewStarted";
import type { Aside } from "./bindings/Aside";
import type { BagAdd } from "./bindings/BagAdd";
import type { BagItem } from "./bindings/BagItem";
import type { ContextDetail } from "./bindings/ContextDetail";
import { call, retried, at, on } from "./apiCall";

/** What the iPhone app asked this machine to send it (`ntfy::Notify`, `GET /api/phone/notify`). */
export type PhoneNotify = {
  server: string;
  topic: string;
  approvals: boolean;
  questions: boolean;
  failures: boolean;
  done: boolean;
  quiet_from: number | null;
  quiet_to: number | null;
  utc_offset_min: number;
  quiet_at_pc: boolean;
};

export const threadApi = {
  /** What the phone has set for its notices, and a test notice to it (feature `phone.notify`). */
  phoneNotify: () => call<PhoneNotify>("GET", "/api/phone/notify"),
  phoneNotifyTest: () => call<unknown>("POST", "/api/phone/notify/test"),
  /** Says which thread the window shows and whether you are at it, to the machine that runs the thread. */
  presence: (host: string | null, p: { client: string; active: boolean; thread: string | null; idle_ms: number }) =>
    call<void>("POST", `/api/presence${on(host)}`, p),
  threads: () => call<Thread[]>("GET", "/api/threads"),
  // A client id makes a retry of the same request harmless. `host` runs the thread on another
  // machine; leave it out for this one.
  createThread: (t: NewThread, host?: string) =>
    retried<Thread>("POST", `/api/threads${host ? `?host=${encodeURIComponent(host)}` : ""}`, { client_id: crypto.randomUUID(), ...t }),
  /** Starts a thread in the cloud (`threads.cloud`). The core runs the CLI of the engine's own subscription. */
  createCloudThread: (t: NewThread, host?: string) =>
    retried<Thread>("POST", `/api/threads${host ? `?host=${encodeURIComponent(host)}` : ""}`, { client_id: crypto.randomUUID(), ...t, cloud: true }),
  patchThread: (id: string, patch: ThreadPatch) => call<Thread>("PATCH", `/api/threads/${id}`, patch),
  /** Starts the thread's engine ahead of a message, if it stopped. */
  warm: (id: string) => call<void>("POST", `/api/threads/${id}/warm`),
  /** Starts an engine for a thread not created yet, while its first message is typed; create
   *  the thread with the id it gives (`warm`) and the message goes to that engine. */
  warmNew: (w: WarmAsk) => call<Warmed>("POST", "/api/threads/warm", w),
  branches: (host?: string) =>
    call<Record<string, string>>("GET", `/api/branches${host ? `?host=${encodeURIComponent(host)}` : ""}`),
  /** The pull request of each thread's branch on this machine. */
  prs: () => call<Record<string, ThreadPr>>("GET", "/api/prs"),
  /** Where each fix thread from Problems stands, by thread id, on one machine. */
  fixCards: (host?: string) => call<Record<string, FixCard>>("GET", `/api/problems/fixes${on(host)}`),
  /** Threads waiting for a plan limit to reset, and when they carry on. */
  waits: () => call<Record<string, number>>("GET", "/api/waits"),
  retitle: (id: string) => call<Thread>("POST", `/api/threads/${id}/retitle`),
  /** What a thread's hover card shows, from the log as it stands (`gist.rs`). */
  peek: (id: string) => call<ThreadPeek>("GET", `/api/threads/${id}/peek`),
  /** A fresh TL;DR for the hover card, by the small model; slow, so only when `peek` says stale. */
  gist: (id: string) => call<ThreadPeek>("POST", `/api/threads/${id}/gist`),
  threadPrefs: () => call<ThreadPrefs>("GET", "/api/prefs/threads"),
  setThreadPrefs: (prefs: ThreadPrefs) => call<ThreadPrefs>("PUT", "/api/prefs/threads", prefs),
  /** Icons picked for machines and projects (`looks.svelte.ts`). */
  looks: () => call<Looks>("GET", "/api/looks"),
  setLooks: (looks: Looks) => call<Looks>("PUT", "/api/looks", looks),
  testProvider: (id: string) => call<ProviderTest>("POST", `/api/providers/${encodeURIComponent(id)}/test`),
  setSecret: (name: string, value: string) => call<void>("PUT", `/api/secrets/${encodeURIComponent(name)}`, { value }),
  deleteSecret: (name: string) => call<void>("DELETE", `/api/secrets/${encodeURIComponent(name)}`),
  /** Settings > Key vault: each key by its end, its dates and what uses it. */
  keyVault: () => call<KeyVault>("GET", "/api/keys"),
  createKey: (name: string, value: string) => call<KeyVault>("POST", "/api/keys", { name, value }),
  rotateKey: (name: string, value: string) => call<KeyVault>("PUT", `/api/keys/${encodeURIComponent(name)}`, { value }),
  /** Whether agents in this thread may ask the decision model (`apis.agent`); false too when all agents are off. */
  threadJev: (id: string) => call<{ agents: boolean }>("GET", `/api/threads/${id}/jev`),
  setThreadJev: (id: string, agents: boolean) => call<{ agents: boolean }>("PUT", `/api/threads/${id}/jev`, { agents }),
  apis: () => call<ApiState>("GET", "/api/apis"),
  setApis: (prefs: ApiPrefs) => call<ApiPrefs>("PUT", "/api/apis", prefs),
  testApi: (name: string) => call<ApiTest>("POST", `/api/apis/${encodeURIComponent(name)}/test`),
  /** The address of the service's Allow page, for a connection that logs you in. */
  apiLogin: (name: string) => call<{ url: string }>("POST", `/api/apis/${encodeURIComponent(name)}/login`),
  tryApi: (name: string, ask: ApiTry) => call<ApiAnswer>("POST", `/api/apis/${encodeURIComponent(name)}/try`, ask),
  apiDecisions: () => call<ApiDecision[]>("GET", "/api/apis/decisions"),
  /** Writes a brief of the decision model's latest decisions and says what to ask a thread (`apis.tune`). */
  jevTune: () => call<{ path: string; prompt: string; decisions: number }>("POST", "/api/apis/tune"),
  /** The decision model's latest background tune and its suggestions (`apis.tune.run`). */
  tuneState: () => call<TuneState>("GET", "/api/apis/tune/state"),
  /** Starts a tune in the background unless one is reading; answers the state. */
  tuneRun: () => call<TuneState>("POST", "/api/apis/tune/run"),
  /** Uses or skips a suggestion; using a threshold sets it. */
  tuneAnswer: (id: string, use: boolean) => call<TuneState>("POST", "/api/apis/tune/answer", { id, use }),
  deleteThread: (id: string) => call<void>("DELETE", `/api/threads/${id}`),
  events: (id: string, after = 0) => call<Event[]>("GET", `/api/threads/${id}/events?after=${after}`),
  send: (id: string, text: string, attachments: Attachment[] = [], original?: string, steer = false) =>
    retried<void>("POST", `/api/threads/${id}/messages`, { text, attachments, original, steer, client_id: crypto.randomUUID() }),
  /** "Edit from here": `text` in place of user message `seq`; what followed is left behind. */
  editFrom: (id: string, seq: number, edit: EditFrom) => call<void>("POST", `/api/threads/${id}/messages/${seq}/edit`, edit),
  /** Puts an emoji on a message (a user message, or the turn end of an agent reply), or takes it away. */
  react: (id: string, r: React) => call<void>("POST", `/api/threads/${id}/react`, r),
  /** The user's own emojis with their pictures, and the ones they use most. */
  emoji: () => call<EmojiState>("GET", "/api/emoji"),
  putEmoji: (name: string, mime: string, data: string) => call<void>("PUT", `/api/emoji/${encodeURIComponent(name)}`, { mime, data }),
  deleteEmoji: (name: string) => call<void>("DELETE", `/api/emoji/${encodeURIComponent(name)}`),
  /** `context` has the core write the prompt knowing the project, folder, rules and thread (an older hubd only puts the text in order; compat-until: 2026-11-29). */
  tidy: (text: string, cwd?: string, thread?: { context?: boolean; threadId?: string }) =>
    call<Tidied>("POST", "/api/tidy", { text, cwd, context: thread?.context, thread_id: thread?.threadId }),
  /** `tz` is minutes east of UTC; days are counted in it. */
  activity: (days: number, tz: number) => call<ActivityReport>("GET", `/api/activity?days=${days}&tz=${tz}`),
  activityDay: (date: string, tz: number) => call<ActivityDetail>("GET", `/api/activity/day?date=${date}&tz=${tz}`),
  summarizeDay: (date: string, tz: number) => call<Tidied>("POST", `/api/activity/day?date=${date}&tz=${tz}`),
  useLog: (days: number) => call<UseReport>("GET", `/api/uselog?days=${days}`),
  useRecord: (hits: UseHit[]) => call<number>("POST", "/api/uselog", hits),
  useClear: () => call<number>("DELETE", "/api/uselog"),
  useOff: (off: boolean) => call<{ off: boolean }>("PUT", "/api/uselog/off", { off }),
  pulse: (days: number, tz: number) => call<Pulse>("GET", `/api/pulse?days=${days}&tz=${tz}`),
  pulseLines: (days: number, tz: number) => call<LinesReport>("GET", `/api/pulse/lines?days=${days}&tz=${tz}`),
  checkup: () => call<HealthReport>("GET", "/api/checkup"),
  runCheckup: () => call<HealthReport>("POST", "/api/checkup"),
  /** The context bag, newest first, without content. */
  bag: () => call<BagItem[]>("GET", "/api/bag"),
  /** One bag item with its content. */
  bagItem: (id: string) => call<BagItem>("GET", `/api/bag/${encodeURIComponent(id)}`),
  putInBag: (add: BagAdd) => call<BagItem>("POST", "/api/bag", add),
  /** A picture on the web, fetched by the core so it goes in the bag as the picture itself. */
  fetchPicture: (url: string) => call<Attachment>("POST", "/api/bag/picture", { url }),
  takeFromBag: (id: string) => call<void>("DELETE", `/api/bag/${encodeURIComponent(id)}`),
  emptyBag: () => call<{ removed: number }>("DELETE", "/api/bag"),
  problems: (f: ProblemFilter, host?: string | null) => {
    const q = new URLSearchParams();
    for (const [k, v] of Object.entries(f)) if (v != null && v !== "") q.set(k, String(v));
    if (host) q.set("host", host);
    return call<Problem[]>("GET", `/api/problems?${q}`);
  },
  problem: (id: number, host?: string | null) => call<ProblemDetail>("GET", `/api/problems/${id}${on(host)}`),
  changeProblem: (id: number, patch: ProblemPatch, host?: string | null) => call<Problem>("PATCH", `/api/problems/${id}${on(host)}`, patch),
  problemThread: (id: number, plan?: string | null, host?: string | null) =>
    call<ProblemThread>("POST", `/api/problems/${id}/thread${on(host)}`, { plan: plan ?? null }),
  /** Starts the fix thread for a problem on machine `host` on this machine, in the project of the same name. */
  problemFixHere: (id: number, plan: string | null | undefined, host: string) =>
    call<ProblemThread>("POST", "/api/problems/fix_here", { id, plan: plan ?? null, host }),
  problemDossier: (id: number, host?: string | null) => call<Dossier>("GET", `/api/problems/${id}/dossier${on(host)}`),
  vakten: (host?: string | null) => call<VaktenStatus>("GET", `/api/problems/vakten${on(host)}`),
  setVakten: (p: VaktenPrefs, host?: string | null) => call<VaktenStatus>("PUT", `/api/problems/vakten${on(host)}`, p),
  vaktenLook: (host?: string | null) => call<VaktenStatus>("POST", `/api/problems/vakten/look${on(host)}`),
  problemRules: () => call<ProblemRules>("GET", "/api/problems/rules"),
  setProblemRules: (global: string) => call<ProblemRules>("PUT", "/api/problems/rules", { global }),
  addProblemRule: (rule: Rule) => call<ProblemRules>("POST", "/api/problems/rules/add", rule),
  problemViews: () => call<SavedView[]>("GET", "/api/problems/views"),
  setProblemViews: (v: SavedView[]) => call<SavedView[]>("PUT", "/api/problems/views", v),
  problemMetrics: () => call<MetricSummary[]>("GET", "/api/problems/metrics"),
  problemMonitors: () => call<Monitor[]>("GET", "/api/problems/monitors"),
  /** Agents' tool calls and how many failed, per engine and tool, on one machine. */
  toolStats: (days = 7, host?: string | null) => call<ToolStats>("GET", `/api/problems/tools?days=${days}${host ? `&host=${encodeURIComponent(host)}` : ""}`),
  /** Vakten's latest review of the tools that fail most, kept on the machine whose Vakten wrote it. */
  toolReview: (host?: string | null) => call<ToolReview | null>("GET", `/api/problems/tools/review${on(host)}`),
  /** Shadow mode of the thread status (D3): how often the status the events imply differed from the thread's own. Needs feature `status.shadow`. */
  statusShadow: () => call<StatusShadow>("GET", "/api/status-shadow"),
  statusShadowClear: () => call<StatusShadow>("DELETE", "/api/status-shadow"),
  /** "Fix now" on a failure's card: a fix thread on it, in the background. `about` is the error's seq. */
  fixNow: (thread: string, about: number, host?: string | null) => call<Thread>("POST", `/api/problems/fix${on(host)}`, { thread, about }),
  reportProblem: (r: Report) => call<Problem>("POST", "/api/problems/report", r),
  problemSources: () => call<Source[]>("GET", "/api/problems/sources"),
  addProblemSource: (s: NewSource) => call<Source>("POST", "/api/problems/sources", s),
  removeProblemSource: (id: number) => call<void>("DELETE", `/api/problems/sources/${id}`),
  widgets: () => call<Widget[]>("GET", "/api/widgets"),
  createWidget: (text: string) => call<Widget>("POST", "/api/widgets", { text }),
  changeWidget: (id: string, text: string) => call<Widget>("POST", `/api/widgets/${encodeURIComponent(id)}/change`, { text }),
  deleteWidget: (id: string) => call<void>("DELETE", `/api/widgets/${encodeURIComponent(id)}`),
  routines: () => call<Routines>("GET", "/api/routines"),
  createRoutine: (r: RoutineDraft) => call<Routine>("POST", "/api/routines", r),
  updateRoutine: (id: string, r: RoutineDraft) => call<Routine>("PUT", `/api/routines/${id}`, r),
  deleteRoutine: (id: string) => call<void>("DELETE", `/api/routines/${id}`),
  /** Starts a run now; returns its thread. */
  runRoutine: (id: string) => call<{ thread: string }>("POST", `/api/routines/${id}/run`),
  recipes: () => call<Recipes>("GET", "/api/recipes"),
  createRecipe: (r: RecipeDraft) => call<Recipe>("POST", "/api/recipes", r),
  updateRecipe: (id: string, r: RecipeDraft) => call<Recipe>("PUT", `/api/recipes/${id}`, r),
  /** Deletes a recipe, or dismisses an agent's suggestion. */
  deleteRecipe: (id: string) => call<void>("DELETE", `/api/recipes/${id}`),
  keepRecipe: (id: string) => call<Recipe>("POST", `/api/recipes/${id}/keep`),
  /** Starts a run in `folder` (a recipe tied to a folder runs there); returns its thread. */
  runRecipe: (id: string, folder: string | null, note?: string) => call<{ thread: string }>("POST", `/api/recipes/${id}/run`, { folder, note }),
  /** One day's written devblog entry, null when there is none yet. Instant: nothing is worked out. */
  devlogDay: (date: string) => call<DevlogDay | null>("GET", `/api/devlog/day?date=${date}`),
  devlog: (days: number) => call<DevlogDay[]>("GET", `/api/devlog?days=${days}`),
  projectDevlog: (path: string, days: number, tz: number) =>
    call<ProjectDay[]>("GET", `/api/devlog/project?path=${encodeURIComponent(path)}&days=${days}&tz=${tz}`),
  /** What happened since `since` (ms) besides the threads. */
  away: (since: number, tz: number) => call<AwayReport>("GET", `/api/away?since=${since}&tz=${tz}`),
  history: (q: string, tz: number) => call<HistoryHit[]>("GET", `/api/history?q=${encodeURIComponent(q)}&tz=${tz}`),
  /** All AI use per day, in Hub and outside it (feature `usage.spend`); of every machine added
   *  together with `everywhere` (feature `usage.spend.machines`). With `all` (feature
   *  `usage.spend.all`) from the first day anything was counted, and `days` is what an older hubd
   *  gives instead (compat-until: 2026-12-03). With `hours` (feature `usage.spend.hours`) also hour by hour over the last 31
   *  days, and who asked per day. With `tokens` (feature `usage.spend.tokens`) every amount counts
   *  tokens in place of dollars. */
  spend: (days: number, tz: number, everywhere = false, all = false, hours = false, tokens = false) =>
    call<Spend>(
      "GET",
      `/api/usage/spend?days=${days}&tz=${tz}${everywhere ? "&everywhere=true" : ""}${all ? "&all=true" : ""}${hours ? "&hours=true" : ""}${tokens ? "&unit=tokens" : ""}`,
    ),
  /** What Hub sent to an AI per day and purpose, from the packing slips (feature `package.ledger`). */
  packageSummary: (days: number, tz: number) => call<PackageSummary>("GET", `/api/package/summary?days=${days}&tz=${tz}`),
  stats: (days: number, tz: number, project?: string) =>
    call<Stats>("GET", `/api/stats?days=${days}&tz=${tz}${project ? `&project=${encodeURIComponent(project)}` : ""}`),
  diff: (cwd: string, host?: string | null) =>
    call<{ text: string }>("GET", `/api/diff?cwd=${encodeURIComponent(cwd)}${at(host)}`),
  /** `refresh` asks the engine when the limits are old; `"force"` asks it now. */
  models: (refresh = false) => call<ModelTable>("GET", `/api/models${refresh ? "?refresh=true" : ""}`),
  connections: (refresh = false) => call<Connections>("GET", `/api/connections${refresh ? "?refresh=true" : ""}`),
  connectionPrefs: () => call<ConnectionPrefs>("GET", "/api/connections/prefs"),
  setConnectionPrefs: (prefs: ConnectionPrefs) => call<ConnectionPrefs>("PUT", "/api/connections/prefs", prefs),
  agentPrefs: () => call<AgentPrefs>("GET", "/api/agent-prefs"),
  setAgentPrefs: (prefs: AgentPrefs) => call<AgentPrefs>("PUT", "/api/agent-prefs", prefs),
  handoffPrefs: () => call<HandoffPrefs>("GET", "/api/handoff-prefs"),
  setHandoffPrefs: (prefs: HandoffPrefs) => call<HandoffPrefs>("PUT", "/api/handoff-prefs", prefs),
  localModels: (host?: string | null) => call<LocalModels>("GET", `/api/local-models${on(host)}`),
  engineDoctor: (engine: string) => call<DoctorReport>("POST", "/api/connections/doctor", { engine }),
  /** What a thread's tokens went to, from its engine's session log. */
  contextDetail: (id: string) => call<ContextDetail>("GET", `/api/threads/${id}/context`),
  threadConnections: (id: string, refresh = false) =>
    call<ThreadConnections>("GET", `/api/threads/${id}/connections${refresh ? "?refresh=true" : ""}`),
  usage: (refresh: boolean | "force" = false) =>
    call<UsageReport>("GET", `/api/usage${refresh === "force" ? "?force=true" : refresh ? "?refresh=true" : ""}`),
  setSetting: (id: string, configId: string, value: SettingValue) =>
    call<void>("POST", `/api/threads/${id}/settings`, { config_id: configId, value }),
  files: (id: string, q: string) => call<FileMatch[]>("GET", `/api/threads/${id}/files?q=${encodeURIComponent(q)}`),
  filesIn: (cwd: string, q: string, host?: string | null) =>
    call<FileMatch[]>("GET", `/api/files?cwd=${encodeURIComponent(cwd)}&q=${encodeURIComponent(q)}${at(host)}`),
  reach: (id: string, targets: string[]) => call<Reach[]>("POST", `/api/threads/${id}/reach`, { targets }),
  saveRule: (id: string, text: string, seq: number) => call<void>("POST", `/api/threads/${id}/rules`, { text, seq }),
  handoff: (id: string) => call<Tidied>("POST", `/api/threads/${id}/handoff`),
  undoPreview: (id: string, to: string) => call<UndoPreview>("POST", `/api/threads/${id}/undo/preview`, { to }),
  review: (id: string, ask: ReviewRequest = {}) => call<ReviewStarted>("POST", `/api/threads/${id}/review`, ask),
  /** Design mode on or off: the agent writes a design document under docs/design/ and no code. */
  design: (id: string, on: boolean) => call<void>("POST", `/api/threads/${id}/design`, { on }),
  /** Fresh sessions read the thread's design document and look for what it leaves out. */
  reviewDesign: (id: string, doc?: string) => call<DesignReviewStarted>("POST", `/api/threads/${id}/design/review`, doc ? { doc } : {}),
  attempts: (id: string) => call<AttemptGroup>("GET", `/api/threads/${id}/attempts`),
  pickAttempt: (id: string) => call<AttemptGroup>("POST", `/api/threads/${id}/attempts/pick`),
  undo: (id: string, to: string) => call<UndoPreview>("POST", `/api/threads/${id}/undo`, { to }),
  cancel: (id: string) => call<void>("POST", `/api/threads/${id}/cancel`),
  /** Extra folders and connections switched off for one thread (`thread.reach`). */
  threadReach: (id: string) => call<ThreadReach>("GET", `/api/threads/${id}/reach`),
  setThreadReach: (id: string, reach: ThreadReach) => call<ThreadReach>("PUT", `/api/threads/${id}/reach`, reach),
  /** The latest note of the judge for a thread (`chat.aside`), or null. */
  aside: (id: string) => call<Aside | null>("GET", `/api/threads/${id}/aside`),
  /** Carry a thread waiting for a plan limit on now, stop it waiting, or move it to the other subscription (`switch`, which an older hubd lacks; compat-until: 2026-12-01). */
  quotaWait: (id: string, action: "now" | "cancel" | "switch", engine?: string) => call<void>("POST", `/api/threads/${id}/quota-wait`, { action, engine }),
  answer: (id: string, requestId: string, optionId: string | null) =>
    call<void>("POST", `/api/threads/${id}/permissions/${requestId}`, { option_id: optionId }),
  /** Changes a message waiting behind the running turn, before the engine takes it. */
  editQueued: (id: string, seq: number, text: string) => call<void>("PATCH", `/api/threads/${id}/queue/${seq}`, { text }),
  removeQueued: (id: string, seq: number) => call<void>("DELETE", `/api/threads/${id}/queue/${seq}`),
  /** Stops the running turn so queued message `seq` goes next. */
  steer: (id: string, seq: number) => call<void>("POST", `/api/threads/${id}/queue/${seq}/steer`),
  git: (id: string) => call<GitInfo>("GET", `/api/threads/${id}/git`),
  /** Every file that differs from the thread's first snapshot; null without git. */
  changes: (id: string) => call<ThreadChange[] | null>("GET", `/api/threads/${id}/changes`),
  projects: () => call<Project[]>("GET", "/api/projects"),
  /** `host`: the machine to make it on, when not this one (the call is forwarded). */
  createProject: (p: NewProject, host?: string | null) => call<CreatedProject>("POST", `/api/projects${on(host)}`, p),
  /** The features a machine's hubd has (`compat::FEATURES`), for a call that only a newer one understands. */
  featuresOf: (host: string) => call<{ features?: string[] }>("GET", `/api/health${on(host)}`).then((h) => new Set(h.features ?? [])),
  /** The templates a machine's core writes and which of the tools they need it has (feature `project.templates`). */
  projectTemplates: (host?: string | null) => call<ProjectTemplates>("GET", `/api/projects/templates${on(host)}`),
  projectDefaults: (host?: string | null) => call<ProjectDefaults>("GET", `/api/projects/defaults${on(host)}`),
  setProjectRoot: (root: string) => call<void>("PUT", "/api/projects/defaults", { root }),
  removeProject: (path: string) => call<void>("DELETE", `/api/projects?path=${encodeURIComponent(path)}`),
};
