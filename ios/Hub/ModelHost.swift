// Machines, projects, prefs and the other types the app reads from hubd besides threads and events.
import Foundation
import UIKit

/// A machine the phone follows: the one it paired with, and the others that one introduced.
struct Machine: Codable, Hashable {
    var id: String?
    var name: String
    var url: String
    var token: String
    var home: Bool
}

/// Answer to `POST /api/pair/redeem`.
struct Paired: Codable {
    var token: String
    var machine: String
}

/// An entry in `POST /api/phone/machines`.
struct PhoneMachine: Codable {
    var id: String
    var name: String
    var url: String
    var code: String?
}

/// A machine as `GET /api/hosts` describes it.
struct HostInfo: Decodable, Hashable, Identifiable {
    var name: String
    var local: Bool
    var address: String
    var state: String
    var version: String?
    var error: String?
    var sending: String?
    var id: String { name }
}

/// Plan limits (`GET /api/usage`): Claude's, and the other subscriptions the engines use.
struct UsageReport: Decodable {
    struct Limit: Decodable, Hashable { var label: String; var percent: Double; var resets: String?; var resets_at: Double? }
    struct Plan: Decodable { var plan: String?; var limits: [Limit]; var checked_at: Double }
    struct Account: Decodable { var engine: String; var label: String; var plan: Plan?; var note: String?; var balance: String? }
    var plan: Plan?
    var plan_error: String?
    var others: [Account] = []
}

/// The panic button's state (`GET /api/halt`): null while agents run.
struct Halt: Decodable { var since: Double; var by: String }

/// A project: its folder, its name, and the machine its threads run on.
struct ProjectRef: Hashable, Identifiable {
    var path: String
    var name: String
    var host: String?
    var id: String { (host ?? "") + "|" + path }

    /// No project: the machine makes a fresh scratch folder for the thread.
    static let scratchPath = "\u{0}scratch"
    static func noProject(on host: String?) -> ProjectRef { ProjectRef(path: scratchPath, name: "No project", host: host) }
    var isScratch: Bool { path == Self.scratchPath }
}

/// `GET /api/host`: how busy a machine is now.
struct HostSnapshot: Decodable {
    struct Listener: Decodable, Hashable { var port: Int; var process: String?; var is_hub: Bool; var system: Bool; var thread_id: String? }
    var hostname: String; var os: String; var cpu_percent: Double; var mem_used: Double; var mem_total: Double
    var listeners: [Listener] = []
}

/// `GET /api/host/about`: what a machine is.
struct MachineAbout: Decodable {
    struct Disk: Decodable { var total: Double; var free: Double }
    /// desktop, laptop, board, vps or server, when hubd can tell.
    var kind: String?
    var hostname: String
    var os: String
    var model: String?
    var cores: Int
    var uptime_secs: Double
    var disk: Disk?
    var temp_c: Double?
    var tailscale: String?
}

/// `GET /api/machine/ready`: whether a machine is ready for work, check by check.
struct MachineReady: Decodable {
    struct Check: Decodable, Hashable { var key: String; var label: String; var state: String; var detail: String }
    var checks: [Check]
}

/// A machine's rings and checks, as the Machines tab shows them.
struct MachineVitals {
    var snapshot: HostSnapshot?
    var about: MachineAbout?
    var ready: MachineReady?
    var cpu: Double? { snapshot?.cpu_percent }
    var ram: Double? { snapshot.flatMap { $0.mem_total > 0 ? $0.mem_used / $0.mem_total * 100 : nil } }
    var disk: Double? { about?.disk.flatMap { $0.total > 0 ? ($0.total - $0.free) / $0.total * 100 : nil } }
    var temp: Double? { about?.temp_c }
}

/// FOCUS.md as `GET /api/project` reads it.
struct ProjectFocus: Decodable {
    struct Check: Decodable, Hashable { var text: String; var done: Bool; var line: Int }
    struct Idea: Decodable, Hashable { var text: String; var line: Int }
    struct Stage: Decodable, Hashable { var title: String; var about: String; var line: Int; var done: [Check]; var test: [Check]; var ideas: [Idea] }
    var exists: Bool
    var direction: Stage?
    var stages: [Stage]
    var active: Int?
    var parked: [Idea]
}

struct ProjectInfo: Decodable {
    var name: String
    var path: String
    var summary: String?
    var branch: String?
    var focus: ProjectFocus
}

/// One line about a project's focus (`POST /api/project/focus/summaries`).
struct FocusSummary: Decodable, Hashable { var stage: String?; var n: Int; var stages: Int; var done: Int; var of: Int; var to_test: Int }

/// A file the thread changed (`GET /api/threads/{id}/changes`).
struct ThreadChange: Decodable, Hashable, Identifiable {
    var path: String
    var diff: String
    var added: Int
    var removed: Int
    var id: String { path }
    var name: String { path.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? path }
}

/// The phone's notices from one machine (`/api/phone/notify`, `ntfy.rs` in hubd).
struct NotifyPrefs: Codable, Equatable {
    var server = "https://ntfy.sh"
    var topic = ""
    var approvals = true
    var questions = true
    var failures = true
    var done = true
    var quiet_from: Int?
    var quiet_to: Int?
    var utc_offset_min = 0
    /// No notice about the thread open in Hub on the computer in use (`presence.desktop`). Missing from a hubd that is older: on.
    var quiet_at_pc: Bool?
}

/// Safety (`/api/prefs/permissions`): how much threads may do without asking.
struct PermissionPrefs: Codable, Equatable {
    var level: String
    var allow: [String]
    var deny: [String]
    var protected: [String]
}

/// When Hub settles threads by itself (`/api/prefs/threads`).
struct ThreadPrefs: Codable, Equatable { var settle_after_days: Int; var settle_on_merge: Bool }

/// How much of the agent's work a thread shows (`/api/prefs/chatdetail`, the desktop's Chat detail).
/// Names and blurbs match `DETAILS` in `app/src/lib/prefs.svelte.ts`.
enum ChatDetail: String, CaseIterable, Identifiable {
    case minimal, quiet, normal, all
    var id: String { rawValue }
    var name: String {
        switch self { case .minimal: "Minimal"; case .quiet: "Quiet"; case .normal: "Normal"; case .all: "Full" }
    }
    var blurb: String {
        switch self {
        case .minimal: "One playful word while it works. Just the answer when it is done."
        case .quiet: "What it is doing now, in one line. Files changed under the answer."
        case .normal: "Its latest note and the command output, the rest folded."
        case .all: "Every note, edit with its diff and command output, in place."
        }
    }
}

/// The level each thread follows when it has none of its own, and the levels threads were given. An empty
/// default reads as Quiet; a level this app does not know is left out, so a newer hubd never breaks it.
struct ChatDetailPrefs: Codable, Equatable {
    var `default`: String = ""
    var threads: [String: String] = [:]
}

/// The playful words of Minimal (`playfulWord` in `app/src/lib/activity.ts`): they say that something is
/// happening without saying what, and change when the agent does something new.
enum PlayfulWord {
    static let words = [
        "Tempering", "Simmering", "Noodling", "Untangling", "Kneading", "Whittling", "Polishing", "Mulling",
        "Stitching", "Sifting", "Brewing", "Tinkering", "Threading", "Weaving", "Distilling", "Sketching",
        "Winnowing", "Percolating", "Smithing", "Gardening", "Folding", "Burnishing", "Sanding", "Kindling",
    ]
    static func pick(seed: String, _ n: Int) -> String {
        var h: UInt32 = 0
        for c in seed.utf16 { h = h &* 31 &+ UInt32(c) }
        return words[(Int(h) + n * 7) % words.count]
    }
}

/// A pull request as `GET /api/github/pull` gives it.
struct PullDetail: Decodable {
    struct Row: Decodable { var number: Int; var title: String; var url: String; var state: String; var draft: Bool; var head: String; var base: String; var additions: Int; var deletions: Int; var checks: String; var review: String; var mergeable: String }
    struct Check: Decodable, Hashable { var name: String; var state: String }
    struct File: Decodable, Hashable { var path: String?; var filename: String?; var additions: Int?; var deletions: Int? }
    var pull: Row
    var body: String
    var checks: [Check]
    var merge_state: String
    var methods: [String]
    var auto_merge: Bool
    /// Not every hubd sends these; the page shows what it has.
    var created_at: Double?
    var comments: [Comment]?
    var commits: [Commit]?
    var files: [File]?

    struct Comment: Decodable, Hashable {
        var kind: String
        var author: String
        var body: String
        var at: Double?
        var verdict: String?
        var path: String?
        var line: Int?
    }
    struct Commit: Decodable, Hashable { var oid: String; var headline: String; var author: String?; var at: Double? }
}

/// Something a project's threads made (`GET /api/project/outputs`): a picture, a doc, a page.
struct LibraryItem: Decodable, Hashable, Identifiable {
    struct Open: Decodable, Hashable { var thread: String?; var page: String?; var path: String?; var url: String? }
    var id: String
    var kind: String
    var title: String
    var thread: String?
    var thread_title: String?
    var made_at: Double
    var open: Open?
}


/// One of an engine's own settings, as it reports them (`session_state.config_options`, or a
/// `config_option_update`): its model, its effort, its mode. Each engine has its own.
struct EngineOption: Hashable, Identifiable {
    struct Choice: Hashable { var value: String; var name: String; var description: String? }
    var id: String
    var name: String
    var category: String?
    var isToggle: Bool
    var value: String
    var choices: [Choice]

    static func parse(_ list: [JSON]) -> [EngineOption] {
        list.compactMap { o in
            guard let id = o["id"].string else { return nil }
            var choices: [Choice] = []
            for c in o["options"].array {
                let inner = c["options"].array
                for x in inner.isEmpty ? [c] : inner {
                    if let v = Self.text(x["value"]) { choices.append(Choice(value: v, name: x["name"].string ?? v, description: x["description"].string)) }
                }
            }
            return EngineOption(id: id, name: o["name"].string ?? id, category: o["category"].string,
                                isToggle: o["type"].string == "boolean", value: Self.text(o["currentValue"]) ?? "", choices: choices)
        }
    }

    static func text(_ j: JSON) -> String? {
        switch j { case let .string(s): return s; case let .bool(b): return b ? "true" : "false"; case let .number(n): return String(n); default: return nil }
    }
}

/// A table or drawing a reply wrote whole (`Folded.made`): the text it wrote, not the file as it is
/// now, so an old reply shows what it made then (as `Changes.svelte` on the desktop).
struct MadeFile: Hashable, Identifiable {
    enum Kind: String { case csv, tsv, svg }
    var path: String
    var kind: Kind
    var text: String
    var id: String { path }
    var name: String { path.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? path }

    static func kind(of path: String) -> Kind? {
        switch path.split(separator: ".").last.map({ $0.lowercased() }) {
        case "csv": return .csv
        case "tsv", "tab": return .tsv
        case "svg": return .svg
        default: return nil
        }
    }

    /// The files of a turn's edits that are tables or drawings written whole: the first edit wrote
    /// the file (no old text), and the later ones apply to it (`fromEdits` in `filePreview.ts`).
    static func found(_ edits: [(path: String, old: String, new: String)]) -> [MadeFile] {
        var order: [String] = []
        var byPath: [String: [(old: String, new: String)]] = [:]
        for e in edits where kind(of: e.path) != nil {
            if byPath[e.path] == nil { order.append(e.path) }
            byPath[e.path, default: []].append((e.old, e.new))
        }
        var out: [MadeFile] = []
        for path in order {
            guard let list = byPath[path], let first = list.first, first.old.isEmpty, let k = kind(of: path) else { continue }
            var text: String? = first.new
            for e in list.dropFirst() {
                guard let t = text else { break }
                if e.old.isEmpty { text = e.new }
                else if let r = t.range(of: e.old) { text = t.replacingCharacters(in: r, with: e.new) }
                else { text = nil }
            }
            if let text { out.append(MadeFile(path: path, kind: k, text: text)) }
        }
        return Array(out.prefix(2))
    }
}

/// A step of the agent's plan (ACP `plan` updates).
struct PlanStep: Hashable {
    var text: String
    var status: String
    /// When it was marked in progress and completed (ms), as far as the plan updates tell.
    var started: Double?
    var finished: Double?
    var took: Double? { if let started, let finished, finished >= started { return finished - started }; return nil }
}

/// A picture or file sent with a message (`Attachment` in hubd): bytes as base64.
struct Attachment: Hashable, Identifiable {
    let id = UUID()
    var name: String
    var mime: String
    var data: Data
    var preview: UIImage?
    var json: [String: Any] { ["name": name, "mime": mime, "data": data.base64EncodedString()] }
}


/// `GET /api/threads/{id}/gitstate` (feature `phone.git`): where the thread's folder stands.
struct GitState: Decodable, Hashable {
    var branch: String?
    var `default`: String
    var files: [String]
    var upstream: Bool
    var ahead: Int
    var behind: Int
    var github: Bool

    /// "not pushed", "2 ahead" or nil when there is nothing to say about the push.
    var pushWord: String? {
        if branch == nil || branch == `default` && ahead == 0 { return nil }
        if !upstream { return "not pushed" }
        return ahead > 0 ? "\(ahead) not pushed" : nil
    }
}

/// What `POST …/ship` did, and the words `POST …/gitdraft` wrote for it.
struct GitShipped: Decodable { var commit: String?; var pushed: Bool?; var pr_url: String? }
struct GitDraft: Decodable { var message: String; var title: String; var body: String }


extension JSON {
    var number: Double? { if case let .number(n) = self { return n }; return nil }
    var bool: Bool? { if case let .bool(b) = self { return b }; return nil }
}

/// One plan-limit wait as its events told it, and the break it is drawn as (the desktop's `wait`
/// item in Transcript.svelte).
struct WaitInfo {
    var seq: Int
    var state = "waiting"
    var at: Double?
    var known = false
    var limit: String?
    var tries = 0
    var by: String?
    var from: String?
    var to: String?
    var switchedAt: Double?
    /// The engine the wait before this one handed over from: it ran out too.
    var outToo: String?

    mutating func apply(_ p: QuotaWaitPayload, at ts: Double) {
        if let s = p.state { state = s }
        if let a = p.at { at = a }
        if let k = p.known { known = k }
        if let l = p.limit { limit = l }
        if let t = p.tries { tries = Int(t) }
        if let b = p.by { by = b }
        if let f = p.from { from = f }
        if let t = p.to { to = t }
        if state == "switched" { switchedAt = ts }
    }

    private func when() -> String {
        guard let at else { return "later" }
        return "at \(Store.clock(at))"
    }

    var brk: Brk {
        switch state {
        case "switched":
            let to = Folded.provider(to ?? "")
            let text = (from.map { "\(Folded.provider($0)) ran out" } ?? "A plan limit ran out") + " → continued on \(to)"
            return Brk(kind: "engine", text: text, note: switchedAt.map(Store.clock))
        case "waiting":
            if let outToo {
                return Brk(kind: "stuck", text: "\(Folded.provider(outToo)) ran out too", note: "both subscriptions are out",
                           hint: "Hub carries on \(when()), never with a paid API")
            }
            return Brk(kind: "waiting", text: "Stopped by a plan limit", note: limit, hint: "Hub carries on \(when())\(known ? "" : ", a guess since the engine did not say when it resets")", glyph: "clock")
        case "resumed":
            return Brk(kind: "waiting", text: by == "you" ? "You carried on" : "The limit reset and Hub carried on", glyph: "clock")
        case "cancelled":
            return Brk(kind: "time", text: by == "message" ? "You wrote before the limit reset, so Hub did not carry on" : "Not carrying on after the limit")
        default:
            return Brk(kind: "stuck", text: "Stopped by the limit again after \(tries) \(tries == 1 ? "try" : "tries")", note: "write to continue")
        }
    }
}

extension Folded {
    /// An engine by the name people use: Claude, Codex, Hermes.
    static func provider(_ engine: String) -> String {
        let l = engine.lowercased()
        if l.contains("codex") { return "Codex" }
        if l.contains("claude") { return "Claude" }
        return engine.split(separator: "-").map { $0.prefix(1).uppercased() + $0.dropFirst() }.joined(separator: " ")
    }

    /// How long a thread sat before a message: "2 h later", "3 days later".
    /// When you last looked, as it reads on the "New" break: "3 min ago".
    static func ago(_ since: Double) -> String {
        let m = Int(((Date().timeIntervalSince1970 * 1000 - since) / 60_000).rounded())
        if m < 1 { return "just now" }
        if m < 60 { return "\(m) min ago" }
        let h = Int((Double(m) / 60).rounded())
        return h < 36 ? "\(h) h ago" : "\(Int((Double(h) / 24).rounded())) days ago"
    }

    static func later(_ ms: Double) -> String {
        let h = Int((ms / 3_600_000).rounded())
        return h < 36 ? "\(h) h later" : "\(Int((Double(h) / 24).rounded())) days later"
    }
}


/// A picture an ACP `image` block carried (the data comes with the event), with the prompt the
/// image model actually used. Equal by its place in the log, so folding stays cheap.
struct Picture: Hashable {
    var id: String
    var mime: String
    var base64: String
    var prompt: String?

    static func == (a: Picture, b: Picture) -> Bool { a.id == b.id }
    func hash(into h: inout Hasher) { h.combine(id) }

    static func found(in content: [JSON], seq: Int) -> [Picture] {
        let note = content.compactMap { $0["content"] }.first { $0["type"].string == "text" && ($0["text"].string ?? "").hasPrefix("Revised prompt: ") }
        let prompt = note?["text"].string.map { String($0.dropFirst("Revised prompt: ".count)) }
        var out: [Picture] = []
        for block in content {
            let c = block["content"]
            guard block["type"].string == "content", c["type"].string == "image", let data = c["data"].string, !data.isEmpty else { continue }
            out.append(Picture(id: "\(seq):\(out.count)", mime: c["mimeType"].string ?? "image/png", base64: data, prompt: prompt))
        }
        return out
    }
}

/// Where a cloud thread stands, as the core last read it. Only the parts the phone shows; a field a
/// newer hubd adds is ignored, and a state this build lacks reads as working.
struct CloudRun: Codable, Hashable {
    var provider: String
    var state: String = "working"
    var url: String?
    var repo: String?
    var branch: String?
    var note: String?

    var providerName: String { provider == "codex" ? "Codex" : "Claude" }
    var stateWord: String {
        switch state {
        case "starting": "Starting"
        case "waiting": "Asks you"
        case "done": "Done"
        case "failed": "Failed"
        case "stopped": "Stopped"
        default: "Working"
        }
    }
}
