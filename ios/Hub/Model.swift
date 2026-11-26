import Foundation
import UIKit

/// The parts of hubd's `Thread` (crates/hub-core/src/model.rs) the app reads.
struct HubThread: Codable, Identifiable, Hashable {
    var id: String
    var title: String
    var cwd: String
    var engine: String
    var status: Status
    var archived: Bool = false
    var host: String?
    var updated_at: Double
    var returned_at: Double?
    var seen_at: Double?
    var settled_at: Double?
    var snoozed_until: Double?
    var pinned_at: Double?
    var sent_at: Double?
    /// The thread that started this one as a side job; a side job is not listed on its own.
    var parent: String?
    /// Set when the thread runs on Anthropic's or OpenAI's servers (`cloud_threads.rs`) and not on a machine.
    var cloud: CloudRun?
    /// Set by the store, never sent: since when a side job of this thread has been running, nil when none is.
    /// A thread whose own turn is over but whose side job still works is working, not done.
    var sideBusy: Double?

    enum Status: String, Codable {
        case idle, running, waiting_on_you, error, in_terminal

        /// A status a newer hubd has and this build lacks reads as idle, so the thread is still
        /// listed (docs/ARCHITECTURE.md, "Versions that talk to each other").
        init(from decoder: Decoder) throws {
            let raw = try decoder.singleValueContainer().decode(String.self)
            self = Status(rawValue: raw) ?? .idle
        }
    }

    var project: String {
        // Windows paths too, and a thread's own worktree (`<repo>/.hub-threads/<id>`) is its repo.
        var parts = cwd.split(whereSeparator: { $0 == "/" || $0 == "\\" }).map(String.init)
        if let i = parts.firstIndex(where: { $0.hasPrefix(".hub-") }) { parts = Array(parts[..<i]) }
        let last = parts.last ?? cwd
        return last.isEmpty ? "—" : last.prefix(1).uppercased() + last.dropFirst()
    }

    var unread: Bool {
        guard let r = returned_at else { return false }
        return r > (seen_at ?? 0)
    }

    var dot: DotState {
        switch status {
        case .waiting_on_you: return .waiting
        case .running, .in_terminal: return .running
        case .error: return .error
        case .idle: return sideBusy != nil ? .running : unread ? .unread : .idle
        }
    }

    /// A quick thread runs in a scratch folder (a dated folder under scratch), on any machine.
    var isQuick: Bool { !inChatsFolder && project.range(of: #"^\d{4}-\d{2}-\d{2}"#, options: .regularExpression) != nil }

    /// A chat lives in a folder named for its time under `chats` (Hub's own, `phone.chat`).
    var inChatsFolder: Bool {
        let c = cwd.replacingOccurrences(of: "\\", with: "/")
        return c.contains("/chats/")
    }

    /// The words the inbox row's group uses, for the status line of a thread's page.
    var statusWord: String {
        if archived { return "Archived" }
        if settled_at != nil { return "Settled" }
        if let s = section { return s.rawValue }
        if let s = snoozed_until, s > Date().timeIntervalSince1970 * 1000 { return "Snoozed" }
        return "Done"
    }

    var section: InboxSection? {
        if archived || settled_at != nil { return nil }
        if let s = snoozed_until, s > Date().timeIntervalSince1970 * 1000, status != .waiting_on_you, status != .error {
            return nil
        }
        switch status {
        case .waiting_on_you, .error: return .needsYou
        case .running, .in_terminal: return .working
        case .idle: return sideBusy != nil ? .working : returned_at == nil ? nil : .done
        }
    }
}

/// Reading what a hubd of any version sends: only the id and the keys every hubd has sent from
/// the start are needed, anything else may be missing, and fields this build does not know are
/// ignored. A hubd newer or older than the app must never empty the inbox.
extension HubThread {
    private enum Keys: String, CodingKey {
        case id, title, cwd, engine, status, archived, host, updated_at, returned_at, seen_at
        case settled_at, snoozed_until, pinned_at, sent_at, parent
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = try c.decode(String.self, forKey: .id)
        title = (try? c.decodeIfPresent(String.self, forKey: .title)) ?? ""
        cwd = (try? c.decodeIfPresent(String.self, forKey: .cwd)) ?? ""
        engine = (try? c.decodeIfPresent(String.self, forKey: .engine)) ?? ""
        status = (try? c.decodeIfPresent(Status.self, forKey: .status)) ?? .idle
        archived = (try? c.decodeIfPresent(Bool.self, forKey: .archived)) ?? false
        host = try? c.decodeIfPresent(String.self, forKey: .host)
        updated_at = (try? c.decodeIfPresent(Double.self, forKey: .updated_at)) ?? 0
        returned_at = try? c.decodeIfPresent(Double.self, forKey: .returned_at)
        seen_at = try? c.decodeIfPresent(Double.self, forKey: .seen_at)
        settled_at = try? c.decodeIfPresent(Double.self, forKey: .settled_at)
        snoozed_until = try? c.decodeIfPresent(Double.self, forKey: .snoozed_until)
        pinned_at = try? c.decodeIfPresent(Double.self, forKey: .pinned_at)
        sent_at = try? c.decodeIfPresent(Double.self, forKey: .sent_at)
        parent = try? c.decodeIfPresent(String.self, forKey: .parent)
    }
}

/// A list read one element at a time: one this build cannot read is skipped, so a single odd
/// thread or event never fails the whole list.
struct Lossy<T: Decodable>: Decodable {
    var items: [T]

    init(from decoder: Decoder) throws {
        var c = try decoder.unkeyedContainer()
        var out: [T] = []
        while !c.isAtEnd {
            if let item = try? c.decode(T.self) { out.append(item) } else { _ = try? c.decode(JSON.self) }
        }
        items = out
    }
}

enum DotState { case waiting, running, error, unread, idle }

enum InboxSection: String, CaseIterable {
    case needsYou = "Needs you"
    case working = "Working"
    case done = "Done"
}

/// One entry in hubd's append-only event log.
struct HubEvent: Decodable, Hashable {
    var thread_id: String
    var seq: Int
    var ts: Double
    var kind: String
    var data: JSON

    private enum Keys: String, CodingKey { case thread_id, seq, ts, kind, data }

    /// `data` may be missing; a kind this build does not know is just a string the thread view skips.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        thread_id = try c.decode(String.self, forKey: .thread_id)
        seq = try c.decode(Int.self, forKey: .seq)
        ts = (try? c.decodeIfPresent(Double.self, forKey: .ts)) ?? 0
        kind = (try? c.decodeIfPresent(String.self, forKey: .kind)) ?? ""
        data = (try? c.decodeIfPresent(JSON.self, forKey: .data)) ?? .null
    }
}

/// What `GET /api/threads/{id}/peek` says about a thread: its last reply, the quick replies it
/// offers, what it waits for. Only the fields the inbox shows.
struct Peek: Decodable, Hashable {
    struct Replies: Decodable, Hashable { var question: String; var options: [Reply] }
    struct Reply: Decodable, Hashable { var label: String; var message: String }
    var last_reply: String = ""
    var replies: Replies?
    var permission: String?
    var error: String?
}

/// A tool call, shown as one grey row.
struct Step: Hashable {
    var title: String
    var status: String
    var command: String?
    /// What the tool gave back, cut to the last part a phone can show.
    var output: String?
    /// ACP's kind of the call ("read", "edit", "fetch"…), when the engine said.
    var kind: String?
}

/// A permission question the agent asks, with the agent's own options.
struct Ask: Hashable {
    struct Option: Hashable { var id: String; var name: String; var kind: String }
    var id: String
    var title: String
    var command: String?
    var options: [Option]
    var resolved = false
    var answer: String?

    var allowOptions: [Option] { options.filter { $0.kind.hasPrefix("allow") } }
    var hasReject: Bool { options.contains { $0.kind.hasPrefix("reject") } }

    /// Deletes or rewrites history: takes a second tap, as in the desktop app.
    var risky: Bool {
        guard let command else { return false }
        let pattern = #"\brm\s+-[a-z]*[rf]|\bgit\s+(push\s+.*--force|reset\s+--hard|clean\s+-[a-z]*f)|\bdrop\s+(table|database)\b|\bmkfs\b|\bdd\s+if=|Remove-Item.*-Recurse"#
        return command.range(of: pattern, options: [.regularExpression, .caseInsensitive]) != nil
    }
}

/// The thread as the phone shows it: what was said, the work between as grey rows, asks, failures.
enum Item: Hashable {
    case user(String)
    case hub(String)
    case reply(String)
    case work([Step])
    case ask(Ask)
    case error(String)
    case files([String])
    /// A wavy line where the engine, the machine or a wait changed (`Rich.swift`).
    case brk(Brk)
    /// A live page the agent showed.
    case page(PageItem)
    /// A picture the agent made or showed, such as Codex's image generation.
    case picture(Picture)
    /// A side job the agent handed to another thread: a bracket in the margin (`Rich.swift`).
    case side(SideJob)
    /// What you sent along with your message (photos, files, links): small, above your words.
    case sent([SentFile])
}

/// One thing sent with a message: a picture to show small, or a file or link shown by its name.
struct SentFile: Hashable {
    var name: String
    var picture: Picture?
    var link: String?
    var folder = false

    static func found(in attachments: [JSON], seq: Int) -> [SentFile] {
        attachments.enumerated().map { i, a in
            let mime = a["mime"].string ?? ""
            let name = a["name"].string ?? "File"
            if mime.hasPrefix("image/"), let data = a["data"].string, !data.isEmpty {
                return SentFile(name: name, picture: Picture(id: "\(seq):sent\(i)", mime: mime, base64: data, prompt: nil))
            }
            return SentFile(name: name, link: a["url"].string, folder: mime == "inode/directory")
        }
    }
}

/// One side job in the parent's log. The first `child` event starts it; later ones for the same
/// job change it in place (done with its answer or pictures, or failed).
struct SideJob: Hashable {
    var id: String
    var title: String
    /// "image", "research", "review", "quick", "subtask" or "other".
    var job: String
    /// The engine doing the job, as Hub names it ("codex").
    var engine: String
    /// "working", "done" or "failed".
    var state: String
    var started: Double
    var finished: Double?
    var pictures: Int
    var error: String?
}

/// Folds the event log into items, the way the phone page does (`phone/index.html`, `fold`).
/// The desktop's transcript (app/src/lib/transcript.ts) is the full version.
struct Folded {
    var items: [Item] = []
    /// For each item: when it began (Unix ms) and which turn it belongs to (a turn starts with a
    /// message from you). Empty for made-up threads.
    var times: [Double] = []
    var turns: [Int] = []
    /// When each turn's last event came, by turn.
    var turnEnd: [Int: Double] = [:]
    /// The emojis on each message, by the seq of the message: your own (a `user_message`) or the
    /// `turn_end` that closes an agent reply. `userSeq` and `endSeq` say which seq a turn's two
    /// messages have (`Reactions.swift`).
    var reactions: [Int: [ReactionChip]] = [:]
    var userSeq: [Int: Int] = [:]
    var endSeq: [Int: Int] = [:]
    /// The engine's own settings as it last reported them, and the agent's latest plan.
    var options: [EngineOption] = []
    var plan: [PlanStep] = []
    var planTurn = 0
    /// Per turn, the tables and drawings (CSV, TSV, SVG) the reply wrote whole, with the text it wrote
    /// (`MadeFile`, as `fromEdits` on the desktop): at most two, shown as cards after the turn.
    var made: [Int: [MadeFile]] = [:]
    /// When the latest turn's plan first came (ms).
    var planAt: Double = 0
    /// What the agent is doing now in plain words, from the latest live event ("Thinking…", "Reading…").
    var activity: String?
    /// The ask still waiting for an answer, if any.
    var openAsk: Ask?
    /// Quick replies offered since the person last wrote.
    var replies: Peek.Replies?

    /// The words for what a live event shows the agent doing, nil for an event that says nothing about it.
    /// What a message Hub wrote to the agent itself was, as one quiet line; empty for one that needs
    /// no line (a side job's result), nil for a message of yours.
    static func hubNote(_ d: JSON) -> String? {
        let said = UserMessagePayload(d)
        if !said.mark("child").isNull { return "" }
        if !said.mark("leak").isNull { return "Hub told the agent about a secret in its work" }
        if !said.mark("customs").isNull { return "Hub told the agent what Customs found" }
        if !said.mark("proof").isNull { return "Hub asked the agent to show that it works" }
        if !said.mark("quota").isNull { return "Hub asked the agent to carry on after the limit" }
        if !said.mark("restart").isNull { return "Hub restarted the agent and told it where it was" }
        if !said.mark("watch").isNull { return said.mark("watch")["number"].number.map { "Hub told the agent about pull request #\(Int($0))" } ?? "Hub told the agent about its pull request" }
        if !said.mark("review").isNull { return "Hub passed the review on to the agent" }
        return nil
    }

    static func words(for d: JSON) -> String? {
        switch UpdatePayload(d).kind {
        case "agent_thought_chunk": return "Thinking…"
        case "agent_message_chunk": return "Writing…"
        case "tool_call", "tool_call_update":
            let title = (d["title"].string ?? "").lowercased()
            if title.contains("show_page") { return "Building the page…" }
            if title.contains("image") { return "Generating image…" }
            if title.contains("websearch") || title.contains("web search") || title.contains("web_search") { return "Searching the web…" }
            switch d["kind"].string {
            case "read": return "Reading…"
            case "edit", "delete", "move": return "Editing…"
            case "search": return "Searching…"
            case "fetch": return "Searching the web…"
            case "execute": return "Running code…"
            case "think": return "Thinking…"
            default: return "Working…"
            }
        default: return nil
        }
    }

    /// A message you just sent, shown at once as a new turn until the machine's own event for it
    /// comes back (`Store.sentLocal`).
    mutating func addOwn(_ text: String, at ts: Double) {
        let turn = (turns.last ?? 0) + 1
        items.append(.user(text)); times.append(ts); turns.append(turn); turnEnd[turn] = ts
        replies = nil
        activity = nil
    }

    /// The text a tool call gave back, from ACP's `content` blocks or `rawOutput`.
    static func output(_ d: JSON) -> String? {
        var parts: [String] = []
        for block in d["content"].array {
            if let t = block["content"]["text"].string ?? block["text"].string { parts.append(t) }
        }
        if parts.isEmpty, let raw = d["rawOutput"]["output"].string ?? d["rawOutput"].string { parts.append(raw) }
        guard !parts.isEmpty else { return nil }
        let all = parts.joined(separator: "\n")
        return all.count > 4000 ? "…" + all.suffix(4000) : all
    }

    var lastReply: String? {
        for item in items.reversed() { if case let .reply(t) = item { return t } }
        return nil
    }

    /// `since`: when you last looked at the thread. A "New" break goes before the first thing the
    /// agent did after it (the same as `folded(events, since)` on the desktop); your own message
    /// after you came back means you were looking, so it places none, and it stands in for the
    /// break after a long pause.
    init(_ events: [HubEvent] = [], since: Double? = nil) {
        var newMarked = since == nil
        var newAt: Int?
        var calls: [String: (work: Int, index: Int)] = [:]
        var asks: [String: Int] = [:]
        var workIndex: Int?
        // A plan-limit wait is one line that later events of the same wait change in place.
        var waits: [Int: (index: Int, info: WaitInfo)] = [:]
        var lastWait: Int?
        var pageAt: [String: Int] = [:]
        var pageFamily: [String: String] = [:]
        var lastActive: Double = 0
        var pictured: Set<String> = []
        var sideAt: [String: Int] = [:]
        // The turn's file edits by tool call, in order, and the calls that failed (their edits did not happen).
        var edits: [(call: String, path: String, old: String, new: String)] = []
        var failed: Set<String> = []

        var turn = 0
        var now: Double = 0
        // The latest reaction event per message, emoji and who is the state; chips keep the order
        // the emojis were first put on.
        var reacted: [Int: [String]] = [:]
        var reactedBy: [String: (you: Bool, agent: Bool)] = [:]
        func add(_ item: Item) {
            items.append(item)
            times.append(now)
            turns.append(turn)
        }

        func setWork(_ i: Int, _ f: (inout [Step]) -> Void) {
            if case var .work(steps) = items[i] { f(&steps); items[i] = .work(steps) }
        }

        for e in events {
            let d = e.data
            now = e.ts
            if !newMarked, let since, e.ts > since, e.kind == "update",
               let u = UpdatePayload(d).kind, u == "agent_message_chunk" || u == "tool_call" || u == "plan" {
                newMarked = true
                newAt = items.count
                add(.brk(Brk(kind: "new", text: "New", note: nil)))
                workIndex = nil
            }
            if e.kind == "user_message" {
                turn += 1
                let fromHub = ["leak", "customs", "proof", "quota", "restart", "watch", "child", "review", "terminal", "routine", "recipe", "vakten", "widget", "docs"].contains { !UserMessagePayload(d).mark($0).isNull }
                if !fromHub { userSeq[turn] = e.seq }
                if !newMarked, let since, e.ts > since, !fromHub { newMarked = true }
                if !fromHub, lastActive > 0, e.ts - lastActive >= 3_600_000, newAt == nil {
                    add(.brk(Brk(kind: "time", text: "New", note: Self.later(e.ts - lastActive))))
                }
            }
            if e.kind == "user_message" || e.kind == "update" { lastActive = e.ts }
            // A reaction is a note on a message: it does not move when the turn ended.
            if e.kind != "reaction" { turnEnd[turn] = e.ts }
            if e.kind == "turn_end" {
                endSeq[turn] = e.seq
                let files = MadeFile.found(edits.filter { !failed.contains($0.call) }.map { (path: $0.path, old: $0.old, new: $0.new) })
                if !files.isEmpty { made[turn] = files }
                edits = []; failed = []
            }
            if e.kind == "user_message" { edits = []; failed = [] }
            if e.kind == "user_message" { activity = nil }
            if e.kind == "update" { if let w = Self.words(for: d) { activity = w } }
            switch e.kind {
            case "user_message":
                workIndex = nil
                replies = nil
                // A note Hub wrote to the agent itself is one quiet line that says what it was. A side
                // job's result has none: the side job's line above already says it came back.
                if let note = Self.hubNote(d) {
                    if !note.isEmpty { add(.hub(note)) }
                } else {
                    let sent = SentFile.found(in: UserMessagePayload(d).attachments, seq: e.seq)
                    if !sent.isEmpty { add(.sent(sent)) }
                    add(.user(UserMessagePayload(d).text ?? ""))
                }
            case "update":
                let kind = UpdatePayload(d).kind
                if kind == "agent_message_chunk" {
                    guard d["content"]["type"].string == "text", let text = d["content"]["text"].string, !text.isEmpty else { break }
                    if case let .reply(prev)? = items.last { items[items.count - 1] = .reply(prev + text) }
                    else { add(.reply(text)) }
                    workIndex = nil
                } else if kind == "plan" {
                    // Each step keeps when it started and finished, matched by its words, so the sheet can
                    // say how long each took. A step starts when it is marked in progress, or else when
                    // the one before it finished, or when the plan came.
                    let before = planTurn == turn ? plan : []
                    if before.isEmpty { planAt = now }
                    var fresh = d["entries"].array.map { PlanStep(text: $0["content"].string ?? "", status: $0["status"].string ?? "pending") }
                    for i in fresh.indices {
                        let old = before.first { $0.text == fresh[i].text }
                        fresh[i].started = old?.started
                        fresh[i].finished = old?.finished
                        if fresh[i].status == "in_progress", fresh[i].started == nil { fresh[i].started = now }
                        if fresh[i].status == "completed", fresh[i].finished == nil {
                            fresh[i].finished = now
                            if fresh[i].started == nil { fresh[i].started = i > 0 ? (fresh[i - 1].finished ?? planAt) : planAt }
                        }
                    }
                    plan = fresh
                    planTurn = turn
                } else if kind == "config_option_update" {
                    let fresh = EngineOption.parse(d["configOptions"].array)
                    if !fresh.isEmpty { options = fresh }
                } else if kind == "tool_call" || kind == "tool_call_update" {
                    let callId = d["toolCallId"].string ?? UUID().uuidString
                    if calls[callId] == nil {
                        if workIndex == nil { add(.work([])); workIndex = items.count - 1 }
                        var n = 0
                        setWork(workIndex!) { $0.append(Step(title: d["title"].string ?? "Tool", status: d["status"].string ?? "pending")); n = $0.count - 1 }
                        calls[callId] = (workIndex!, n)
                    }
                    // A picture in the tool's content (Codex's image generation) is part of the
                    // answer, not of the work, so it stands on its own below the group. page_check's
                    // screenshot is the agent looking at its own page before it shows it: the page is
                    // in the chat itself, so the picture would only be a second copy of it (as on
                    // the desktop, transcript.ts).
                    let (cw, ci) = calls[callId]!
                    var callTitle = d["title"].string ?? ""
                    if callTitle.isEmpty, case let .work(steps) = items[cw] { callTitle = steps[ci].title }
                    if !pictured.contains(callId), callTitle.range(of: "page_check", options: .caseInsensitive) == nil {
                        let found = Picture.found(in: d["content"].array, seq: e.seq)
                        if !found.isEmpty {
                            pictured.insert(callId)
                            for p in found { add(.picture(p)) }
                            workIndex = nil
                        }
                    }
                    // A diff in the content is the edit itself (a whole file when it wrote one); a call's
                    // later update says it again, so it replaces what the call had.
                    let diffs = d["content"].array.filter { $0["type"].string == "diff" && $0["path"].string != nil }
                    if !diffs.isEmpty {
                        edits.removeAll { $0.call == callId }
                        for c in diffs { edits.append((callId, c["path"].string ?? "", c["oldText"].string ?? "", c["newText"].string ?? "")) }
                    }
                    if d["status"].string == "failed" { failed.insert(callId) }
                    let (w, i) = calls[callId]!
                    setWork(w) { steps in
                        if let t = d["title"].string { steps[i].title = t }
                        if let s = d["status"].string { steps[i].status = s }
                        if let c = d["rawInput"]["command"].string { steps[i].command = c }
                        if let k = d["kind"].string { steps[i].kind = k }
                        // A web step's query or page, so a search shows what it looked for.
                        if steps[i].command == nil, let q = d["rawInput"]["query"].string ?? d["rawInput"]["url"].string { steps[i].command = q }
                        if let out = Self.output(d) { steps[i].output = out }
                    }
                }
            case "session_state":
                let fresh = EngineOption.parse(SessionStatePayload(d).configOptions)
                if !fresh.isEmpty { options = fresh }
            case "setting":
                // Your own pick. A stopped engine only takes it when it next starts, and says nothing
                // till then, so the pick is shown from here; otherwise it fell back to the old value.
                let p = SettingPayload(d)
                if let id = p.configId, let i = options.firstIndex(where: { $0.id == id }) {
                    if let s = p.value.string { options[i].value = s }
                    else if let b = p.value.bool { options[i].value = b ? "true" : "false" }
                }
            case "engine_switch":
                options = []
                let to = Self.provider(EngineSwitchPayload(d).to ?? "")
                let from = EngineSwitchPayload(d).from.map(Self.provider)
                add(.brk(Brk(kind: "engine", text: from.map { "\($0) → switched to \(to)" } ?? "Switched to \(to)", note: Store.clock(e.ts), hint: "Hub handed it the conversation so far.")))
                // The switch belongs to the message it was made for, not the turn before it: there it
                // would land above that turn's pages and "Worked" chip, which are drawn after its items.
                turns[turns.count - 1] = turn + 1
                workIndex = nil
            case "moved":
                let p = MovedPayload(d)
                let from = p.from ?? "another machine"
                let dirty = Int(p.dirty ?? 0)
                let note = (dirty > 0 ? "with \(dirty) uncommitted \(dirty == 1 ? "file" : "files") · " : "") + Store.clock(e.ts)
                let hint = (p.stopped ?? false) ? "The turn that was running carries on." : "Your next message hands the conversation to a fresh session here."
                add(.brk(Brk(kind: "machine", text: "Moved here from \(from)", note: note, hint: hint, glyph: "laptopcomputer.and.iphone")))
                workIndex = nil
            case "moved_away":
                let to = MovedAwayPayload(d).to ?? "another machine"
                add(.brk(Brk(kind: "machine", text: "Moved to \(to)", note: Store.clock(e.ts), hint: "It carries on there; open it there.", glyph: "laptopcomputer.and.iphone")))
                workIndex = nil
            case "quota_switch":
                let p = QuotaSwitchPayload(d)
                add(.hub("Switched to \(p.to ?? "the other engine"): \((p.limit ?? "the limit").lowercased()) at \(Int(p.percent ?? 0))%"))
            case "quota_wait":
                let p = QuotaWaitPayload(d)
                let key = p.wait.map { Int($0) }
                if let key, var w = waits[key] {
                    w.info.apply(p, at: e.ts)
                    waits[key] = w
                    items[w.index] = .brk(w.info.brk)
                } else {
                    var info = WaitInfo(seq: e.seq)
                    info.apply(p, at: e.ts)
                    if let before = lastWait, let prev = waits[before], prev.info.state == "switched" { info.outToo = prev.info.to }
                    // The engine's own error says the same; the wait line takes its place.
                    if case .error? = items.last { items.removeLast(); times.removeLast(); turns.removeLast() }
                    add(.brk(info.brk))
                    waits[e.seq] = (items.count - 1, info)
                    lastWait = e.seq
                }
                workIndex = nil
            case "child":
                let p = ChildPayload(d)
                guard let id = p.id else { break }
                let state = p.state ?? "working"
                var job = SideJob(id: id, title: p.title ?? "Side job", job: p.kind ?? "other",
                                  engine: p.engine ?? "", state: state, started: e.ts,
                                  finished: state == "working" ? nil : e.ts, pictures: p.images.count,
                                  error: p.error)
                if let at = sideAt[id], case let .side(first) = items[at] {
                    job.started = first.started
                    items[at] = .side(job)
                } else {
                    add(.side(job))
                    sideAt[id] = items.count - 1
                }
                workIndex = nil
            case "page":
                let p = PagePayload(d)
                guard let id = p.id else { break }
                var page = PageItem(id: id, title: p.title ?? "Page", height: p.height)
                let family = p.replaces.flatMap { pageFamily[$0] } ?? id
                pageFamily[id] = family
                let kin = pageFamily.filter { $0.value == family }.map { $0.key }
                page.version = kin.count
                page.of = kin.count
                for other in kin where other != id {
                    if let i = pageAt[other], case var .page(p) = items[i] { p.of = kin.count; items[i] = .page(p) }
                }
                add(.page(page))
                pageAt[id] = items.count - 1
                workIndex = nil
            case "permission_request":
                let p = PermissionRequestPayload(d)
                let tool = p.toolCall
                var command = tool["rawInput"]["command"].string
                var title = tool["title"].string
                if let id = tool["toolCallId"].string, let (w, i) = calls[id], case let .work(steps) = items[w] {
                    command = command ?? steps[i].command
                    title = title ?? steps[i].title
                }
                let options = p.options.map {
                    Ask.Option(id: $0["optionId"].string ?? "", name: $0["name"].string ?? "OK", kind: $0["kind"].string ?? "")
                }
                let ask = Ask(id: p.id ?? "", title: title ?? "The agent wants to continue", command: command, options: options)
                asks[ask.id] = items.count
                add(.ask(ask))
                workIndex = nil
            case "permission_resolved":
                let p = PermissionResolvedPayload(d)
                if let id = p.id, let i = asks[id], case var .ask(ask) = items[i] {
                    ask.resolved = true
                    ask.answer = p.optionId
                    items[i] = .ask(ask)
                }
            case "replies":
                let p = RepliesPayload(d)
                let options = p.options.compactMap { o -> Peek.Reply? in
                    guard let label = o["label"].string?.trimmingCharacters(in: .whitespaces), !label.isEmpty else { return nil }
                    let message = o["message"].string.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 } ?? label
                    return Peek.Reply(label: label, message: message)
                }
                replies = options.count >= 2 ? Peek.Replies(question: p.question ?? "", options: options) : nil
            case "error":
                add(.error(ErrorPayload(d).message ?? "The engine failed"))
                workIndex = nil
            case "restart":
                add(.hub("Hub restarted in the middle of this turn and asked the agent to carry on"))
            case "reaction":
                let p = ReactionPayload(d)
                guard let target = p.seq.map({ Int($0) }), let emoji = p.emoji, !emoji.isEmpty else { break }
                let key = "\(target)|\(emoji)"
                if reactedBy[key] == nil { reactedBy[key] = (false, false); reacted[target, default: []].append(emoji) }
                let on = p.on ?? true
                if p.by == "agent" { reactedBy[key]?.agent = on } else if p.by == "you" { reactedBy[key]?.you = on }
            default:
                break
            }
        }
        for (target, list) in reacted {
            let chips = list.compactMap { emoji -> ReactionChip? in
                guard let who = reactedBy["\(target)|\(emoji)"], who.you || who.agent else { return nil }
                return ReactionChip(emoji: emoji, you: who.you, agent: who.agent)
            }
            if !chips.isEmpty { reactions[target] = chips }
        }
        for item in items.reversed() {
            if case let .ask(a) = item, !a.resolved { openAsk = a; break }
        }
        // "New · 3 replies · 12 min ago": how much came, and since when you looked.
        if let at = newAt, let since {
            let replies = items[(at + 1)...].filter { if case .reply = $0 { return true }; return false }.count
            var parts: [String] = []
            if replies > 0 { parts.append("\(replies) \(replies == 1 ? "reply" : "replies")") }
            parts.append(Self.ago(since))
            items[at] = .brk(Brk(kind: "new", text: "New", note: parts.joined(separator: " · ")))
        }
    }
}
