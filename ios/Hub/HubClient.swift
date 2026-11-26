import Foundation
import UIKit

/// Talks to one hubd over the tailnet with a phone token, the same few calls the `/phone` page
/// may make (`crate::phone::may`).
struct HubClient {
    var base: URL
    var token: String

    init(base: URL, token: String) { self.base = base; self.token = token }
    init?(_ m: Machine) {
        guard let url = URL(string: m.url) else { return nil }
        self.init(base: url, token: m.token)
    }

    enum Failure: LocalizedError {
        case http(Int, String)
        var errorDescription: String? {
            switch self {
            case let .http(code, body):
                // The server answers a failure as {"error": "..."}: show that sentence, never the JSON.
                if let data = body.data(using: .utf8), let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                    if let e = obj["error"] as? String, !e.isEmpty { return e }
                    if let m = obj["message"] as? String, !m.isEmpty { return m }
                }
                let t = body.trimmingCharacters(in: .whitespacesAndNewlines)
                return t.isEmpty ? "HTTP \(code)" : t
            }
        }
        var forbidden: Bool { if case let .http(c, _) = self { return c == 403 }; return false }
        /// An older hubd: it has no such call, or does not let a phone make it.
        var tooOld: Bool { if case let .http(c, _) = self { return c == 403 || c == 404 || c == 405 }; return false }
        var unauthorized: Bool { if case let .http(c, _) = self { return c == 401 }; return false }
    }

    /// Reads a pairing link from Hub > Settings > Access > Your phone, e.g.
    /// `https://tincan.tail1.ts.net/phone#pair=ABCD-1234`, into the machine's address and the code.
    static func parse(link: String) -> (URL, String)? {
        let trimmed = link.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let comps = URLComponents(string: trimmed), let scheme = comps.scheme, let host = comps.host,
              let fragment = comps.fragment, let range = fragment.range(of: "pair=") else { return nil }
        let code = String(fragment[range.upperBound...]).split(separator: "&").first.map(String.init) ?? ""
        var root = URLComponents()
        root.scheme = scheme
        root.host = host
        root.port = comps.port
        guard !code.isEmpty, let url = root.url else { return nil }
        return (url, code)
    }

    static var deviceName: String { "\(UIDevice.current.name) (app)" }

    static func redeem(base: URL, code: String) async throws -> Paired {
        var req = URLRequest(url: base.appendingPathComponent("api/pair/redeem"))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "content-type")
        req.httpBody = try JSONSerialization.data(withJSONObject: ["code": code, "name": deviceName])
        let (data, resp) = try await URLSession.shared.data(for: req)
        try check(resp, data)
        return try JSONDecoder().decode(Paired.self, from: data)
    }

    var streamURL: URL? {
        var c = URLComponents(url: base.appendingPathComponent("api/stream"), resolvingAgainstBaseURL: false)
        c?.scheme = base.scheme == "https" ? "wss" : "ws"
        c?.queryItems = [URLQueryItem(name: "token", value: token)]
        return c?.url
    }

    func threads() async throws -> [HubThread] {
        try JSONDecoder().decode(Lossy<HubThread>.self, from: try await call("GET", "api/threads")).items
    }

    func events(thread: String, after: Int) async throws -> [HubEvent] {
        try JSONDecoder().decode(Lossy<HubEvent>.self, from: try await call("GET", "api/threads/\(thread)/events", query: ["after": "\(after)", "limit": "50000"])).items
    }

    /// What this hubd can do beyond the core, from `GET /api/health` (open). A hubd from before the
    /// list answers without it: the core and nothing more is assumed. nil when it could not be
    /// asked, which callers treat as "present" so a blip never hides anything.
    func features() async -> Set<String>? {
        guard let data = try? await call("GET", "api/health"),
              let health = try? JSONDecoder().decode(Health.self, from: data) else { return nil }
        return Set(health.features ?? [])
    }

    private struct Health: Decodable { var features: [String]? }

    func peek(thread: String) async throws -> Peek {
        try JSONDecoder().decode(Peek.self, from: try await call("GET", "api/threads/\(thread)/peek"))
    }

    /// `steer`: stop the running turn and take this message next, instead of after it. An older
    /// hubd ignores the field and queues it.
    func send(thread: String, text: String, steer: Bool = false, attachments: [Attachment] = []) async throws {
        var body: [String: Any] = ["text": text, "client_id": UUID().uuidString]
        if steer { body["steer"] = true }
        if !attachments.isEmpty { body["attachments"] = attachments.map(\.json) }
        _ = try await call("POST", "api/threads/\(thread)/messages", body: body)
    }

    /// An emoji on a message (a user message, or the turn end of an agent reply), or taken away.
    func react(thread: String, seq: Int, emoji: String, on: Bool) async throws {
        _ = try await call("POST", "api/threads/\(thread)/react", body: ["seq": seq, "emoji": emoji, "on": on])
    }

    struct EmojiState: Decodable {
        struct Custom: Decodable { var name: String; var mime: String; var data: String }
        var custom: [Custom] = []
        var quick: [String] = []
    }

    /// Your own emojis with their pictures, and the ones you use most.
    func emoji() async throws -> EmojiState {
        try JSONDecoder().decode(EmojiState.self, from: try await call("GET", "api/emoji"))
    }

    func putEmoji(name: String, mime: String, data: String) async throws {
        _ = try await call("PUT", "api/emoji/\(name)", body: ["mime": mime, "data": data])
    }

    func deleteEmoji(name: String) async throws {
        _ = try await call("DELETE", "api/emoji/\(name)")
    }

    func cancel(thread: String) async throws {
        _ = try await call("POST", "api/threads/\(thread)/cancel")
    }

    func answer(thread: String, request: String, option: String?) async throws {
        _ = try await call("POST", "api/threads/\(thread)/permissions/\(request)", body: ["option_id": option.map { $0 as Any } ?? NSNull()])
    }

    /// Settle, snooze, pin, archive or mark seen (`POST /api/threads/{id}/tidy`).
    func tidy(thread: String, _ patch: [String: Any]) async throws {
        _ = try await call("POST", "api/threads/\(thread)/tidy", body: patch)
    }

    func hosts() async throws -> [HostInfo] {
        try JSONDecoder().decode([HostInfo].self, from: try await call("GET", "api/hosts"))
    }

    func usage() async throws -> UsageReport {
        try JSONDecoder().decode(UsageReport.self, from: try await call("GET", "api/usage"))
    }

    struct StoredProject: Decodable { var path: String; var name: String }
    /// The projects a machine lists; `host` names another machine this one relays to.
    func projects(host: String? = nil) async throws -> [StoredProject] {
        try JSONDecoder().decode([StoredProject].self, from: try await call("GET", "api/projects", query: host.map { ["host": $0] } ?? [:]))
    }

    func halt() async throws -> Halt? {
        let data = try await call("GET", "api/halt")
        return try? JSONDecoder().decode(Halt.self, from: data)
    }

    func pause(_ on: Bool) async throws {
        _ = try await call("POST", on ? "api/halt/pause" : "api/halt/resume", body: [:])
    }

    /// An idea, kept in the project's focus file (`POST /api/ideas`).
    func idea(_ text: String, here: String?) async throws {
        var body: [String: Any] = ["text": text]
        if let here { body["here"] = here }
        _ = try await call("POST", "api/ideas", body: body)
    }

    /// A new ordinary thread (`POST /api/phone/threads`), on `host` when it is another machine
    /// that this one relays to.
    func newThread(cwd: String, message: String, engine: String?, host: String?, scratch: Bool = false, attachments: [Attachment] = [], settings: [String: String] = [:], warm: String? = nil, chat: Bool = false, title: String? = nil, cloud: Bool = false) async throws -> HubThread {
        var body: [String: Any] = ["cwd": scratch || chat ? "" : cwd, "message": message, "client_id": UUID().uuidString]
        if !attachments.isEmpty { body["attachments"] = attachments.map(\.json) }
        if chat { body["chat"] = true } else if scratch { body["scratch"] = true }
        if let engine { body["engine"] = engine }
        if !settings.isEmpty { body["settings"] = settings }
        if let warm { body["warm"] = warm }
        if let title { body["title"] = title }
        if cloud { body["cloud"] = true }
        let data = try await call("POST", "api/phone/threads", query: host.map { ["host": $0] } ?? [:], body: body)
        return try JSONDecoder().decode(HubThread.self, from: data)
    }

    /// `host` names another machine this one relays to; nil asks this machine itself.
    func snapshot(host: String?) async throws -> HostSnapshot {
        try JSONDecoder().decode(HostSnapshot.self, from: try await call("GET", "api/host", query: host.map { ["host": $0] } ?? [:]))
    }

    func about(host: String?) async throws -> MachineAbout {
        try JSONDecoder().decode(MachineAbout.self, from: try await call("GET", "api/host/about", query: host.map { ["host": $0] } ?? [:]))
    }

    func ready(host: String?) async throws -> MachineReady {
        try JSONDecoder().decode(MachineReady.self, from: try await call("GET", "api/machine/ready", query: host.map { ["host": $0] } ?? [:]))
    }

    func project(path: String, host: String?) async throws -> ProjectInfo {
        var q = ["path": path]
        if let host { q["host"] = host }
        return try JSONDecoder().decode(ProjectInfo.self, from: try await call("GET", "api/project", query: q))
    }

    func focusSummaries(paths: [String], host: String?) async throws -> [String: FocusSummary] {
        try JSONDecoder().decode([String: FocusSummary].self, from: try await call("POST", "api/project/focus/summaries", query: host.map { ["host": $0] } ?? [:], body: ["paths": paths]))
    }

    func tick(path: String, line: Int, done: Bool, host: String?) async throws {
        _ = try await call("POST", "api/phone/focus", query: host.map { ["host": $0] } ?? [:], body: ["path": path, "line": line, "done": done])
    }

    func changes(thread: String) async throws -> [ThreadChange] {
        try JSONDecoder().decode([ThreadChange].self, from: try await call("GET", "api/threads/\(thread)/changes"))
    }

    // MARK: Git for a thread (feature `phone.git`)

    func gitState(thread: String) async throws -> GitState {
        try JSONDecoder().decode(GitState.self, from: try await call("GET", "api/threads/\(thread)/gitstate"))
    }

    func gitDraft(thread: String) async throws -> GitDraft {
        try JSONDecoder().decode(GitDraft.self, from: try await call("POST", "api/threads/\(thread)/gitdraft", body: [:]))
    }

    /// Commit, push and open a pull request, in that order, whichever are asked for.
    func ship(thread: String, commit: Bool = false, push: Bool = false, pr: Bool = false, message: String? = nil, title: String? = nil, body: String? = nil) async throws -> GitShipped {
        var b: [String: Any] = ["cwd": "", "commit": commit, "push": push, "pr": pr, "pull": false, "draft": false]
        if let message { b["message"] = message }
        if let title { b["title"] = title }
        if let body { b["body"] = body }
        return try JSONDecoder().decode(GitShipped.self, from: try await call("POST", "api/threads/\(thread)/ship", body: b))
    }

    /// A pull request's whole diff as text.
    func githubDiff(path: String, number: Int, host: String?) async throws -> String {
        var q = ["path": path, "number": "\(number)"]
        if let host { q["host"] = host }
        struct T: Decodable { var text: String }
        return try JSONDecoder().decode(T.self, from: try await call("GET", "api/github/diff", query: q)).text
    }

    // MARK: Thread menu, settings, pull requests

    func patch(thread: String, _ body: [String: Any]) async throws {
        _ = try await call("PATCH", "api/threads/\(thread)", body: body)
    }

    /// One of the engine's own settings for this thread (`POST /api/threads/{id}/settings`).
    func setOption(thread: String, id: String, value: Any) async throws {
        _ = try await call("POST", "api/threads/\(thread)/settings", body: ["config_id": id, "value": value])
    }

    func move(thread: String, to machine: String, stop: Bool) async throws {
        _ = try await call("POST", "api/threads/move", body: ["thread": thread, "to": machine, "stop": stop])
    }

    func notify(host: String?) async throws -> NotifyPrefs {
        try JSONDecoder().decode(NotifyPrefs.self, from: try await call("GET", "api/phone/notify", query: host.map { ["host": $0] } ?? [:]))
    }

    func setNotify(_ n: NotifyPrefs, host: String?) async throws {
        let body = try JSONSerialization.jsonObject(with: JSONEncoder().encode(n)) as? [String: Any] ?? [:]
        _ = try await call("PUT", "api/phone/notify", query: host.map { ["host": $0] } ?? [:], body: body)
    }

    /// What the phone has on screen, so the machine holds back notices about it (`phone.looking`).
    func looking(active: Bool, thread: String?, host: String?) async throws {
        var body: [String: Any] = ["active": active]
        if let thread { body["thread"] = thread }
        _ = try await call("POST", "api/phone/looking", query: host.map { ["host": $0] } ?? [:], body: body)
    }

    func testNotify(host: String?) async throws {
        _ = try await call("POST", "api/phone/notify/test", query: host.map { ["host": $0] } ?? [:], body: [:])
    }

    func permissions() async throws -> PermissionPrefs {
        try JSONDecoder().decode(PermissionPrefs.self, from: try await call("GET", "api/prefs/permissions"))
    }

    func setPermissions(_ p: PermissionPrefs) async throws {
        let body = try JSONSerialization.jsonObject(with: JSONEncoder().encode(p)) as? [String: Any] ?? [:]
        _ = try await call("PUT", "api/prefs/permissions", body: body)
    }

    func threadPrefs() async throws -> ThreadPrefs {
        try JSONDecoder().decode(ThreadPrefs.self, from: try await call("GET", "api/prefs/threads"))
    }

    func setThreadPrefs(_ p: ThreadPrefs) async throws {
        _ = try await call("PUT", "api/prefs/threads", body: ["settle_after_days": p.settle_after_days, "settle_on_merge": p.settle_on_merge])
    }

    func chatDetail() async throws -> ChatDetailPrefs {
        try JSONDecoder().decode(ChatDetailPrefs.self, from: try await call("GET", "api/prefs/chatdetail"))
    }

    func setChatDetail(_ p: ChatDetailPrefs) async throws {
        _ = try await call("PUT", "api/prefs/chatdetail", body: ["default": p.default, "threads": p.threads])
    }

    /// The same fleet snapshot and shipping target as desktop Settings > Updates.
    func updates() async throws -> UpdateFleet {
        try JSONDecoder().decode(UpdateFleet.self, from: try await call("GET", "api/updates", query: ["fresh": "true"]))
    }

    func updateHubd(machine: String) async throws {
        _ = try await call("POST", "api/hosts/\(machine)/update", body: [:])
    }

    func reconnect(machine: String) async throws {
        _ = try await call("POST", "api/hosts/\(machine)/reconnect", body: [:])
    }

    /// The pull request each thread watches, by thread id.
    func watches(host: String?) async throws -> [String: Int] {
        try JSONDecoder().decode([String: Int].self, from: try await call("GET", "api/github/watches", query: host.map { ["host": $0] } ?? [:]))
    }

    func pull(path: String, number: Int, host: String?) async throws -> PullDetail {
        var q = ["path": path, "number": "\(number)"]
        if let host { q["host"] = host }
        return try JSONDecoder().decode(PullDetail.self, from: try await call("GET", "api/github/pull", query: q))
    }

    func pullAction(path: String, number: Int, action: String, method: String? = nil, body: String? = nil, host: String?) async throws {
        var b: [String: Any] = ["path": path, "number": number, "action": action, "delete_branch": action == "merge"]
        b["method"] = method ?? NSNull()
        b["body"] = body ?? NSNull()
        _ = try await call("POST", "api/github/action", query: host.map { ["host": $0] } ?? [:], body: b)
    }

    func addProject(path: String, create: Bool, host: String?) async throws {
        _ = try await call("POST", "api/projects", query: host.map { ["host": $0] } ?? [:], body: ["path": path, "create": create])
    }

    func library(project: String, host: String?) async throws -> [LibraryItem] {
        var q = ["path": project]
        if let host { q["host"] = host }
        return try JSONDecoder().decode(Lossy<LibraryItem>.self, from: try await call("GET", "api/project/outputs", query: q)).items
    }

    /// One picture from the library, as bytes.
    func picture(project: String, id: String, host: String?) async throws -> Data {
        var q = ["path": project, "rel": id]
        if let host { q["host"] = host }
        return try await call("GET", "api/project/outputs/raw", query: q)
    }

    func otherMachines(have: [String]) async throws -> [PhoneMachine] {
        try JSONDecoder().decode([PhoneMachine].self, from: try await call("POST", "api/phone/machines", body: ["have": have]))
    }

    func call(_ method: String, _ path: String, query: [String: String] = [:], body: [String: Any]? = nil) async throws -> Data {
        var c = URLComponents(url: base.appendingPathComponent(path), resolvingAgainstBaseURL: false)!
        if !query.isEmpty { c.queryItems = query.map { URLQueryItem(name: $0.key, value: $0.value) } }
        var req = URLRequest(url: c.url!)
        req.httpMethod = method
        req.timeoutInterval = 20
        if path == "api/updates" { req.cachePolicy = .reloadIgnoringLocalCacheData }
        req.setValue("Bearer \(token)", forHTTPHeaderField: "authorization")
        if let body {
            req.setValue("application/json", forHTTPHeaderField: "content-type")
            req.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        let (data, resp) = try await URLSession.shared.data(for: req)
        try Self.check(resp, data)
        return data
    }

    private static func check(_ resp: URLResponse, _ data: Data) throws {
        guard let http = resp as? HTTPURLResponse else { return }
        if !(200..<300).contains(http.statusCode) {
            throw Failure.http(http.statusCode, String(data: data, encoding: .utf8) ?? "")
        }
    }
}

/// One machine's `/api/stream`: threads and events as they happen. Reconnects by itself.
final class StreamLink: NSObject, URLSessionWebSocketDelegate, @unchecked Sendable {
    enum Message {
        case thread(HubThread)
        case threadDeleted(String)
        case event(HubEvent)
        case resync
    }

    private let url: URL
    private let onMessage: @MainActor (Message) -> Void
    private let onLive: @MainActor (Bool) -> Void
    private var task: URLSessionWebSocketTask?
    private var session: URLSession?
    private var retry: Double = 1
    private var stopped = false

    init(url: URL, onMessage: @escaping @MainActor (Message) -> Void, onLive: @escaping @MainActor (Bool) -> Void) {
        self.url = url
        self.onMessage = onMessage
        self.onLive = onLive
    }

    func start() {
        stopped = false
        guard task == nil else { return }
        let session = URLSession(configuration: .default, delegate: self, delegateQueue: nil)
        self.session = session
        let task = session.webSocketTask(with: url)
        self.task = task
        task.resume()
        receive(task)
    }

    func stop() {
        stopped = true
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        session?.invalidateAndCancel()
        session = nil
    }

    func urlSession(_ session: URLSession, webSocketTask: URLSessionWebSocketTask, didOpenWithProtocol protocol: String?) {
        retry = 1
        Task { @MainActor in onLive(true); onMessage(.resync) }
    }

    private func receive(_ task: URLSessionWebSocketTask) {
        task.receive { [weak self] result in
            guard let self, task === self.task else { return }
            switch result {
            case let .success(message):
                if case let .string(text) = message, let msg = Self.decode(text) {
                    Task { @MainActor in self.onMessage(msg) }
                }
                self.receive(task)
            case .failure:
                self.task = nil
                self.session?.invalidateAndCancel()
                self.session = nil
                Task { @MainActor in self.onLive(false) }
                guard !self.stopped else { return }
                let wait = self.retry
                self.retry = min(self.retry * 2, 15)
                DispatchQueue.global().asyncAfter(deadline: .now() + wait) { [weak self] in
                    guard let self, !self.stopped else { return }
                    self.start()
                }
            }
        }
    }

    private static func decode(_ text: String) -> Message? {
        guard let data = text.data(using: .utf8),
              let head = try? JSONDecoder().decode(Head.self, from: data) else { return nil }
        switch head.type {
        case "thread": return (try? JSONDecoder().decode(ThreadMsg.self, from: data)).map { .thread($0.thread) }
        case "thread_deleted": return (try? JSONDecoder().decode(DeletedMsg.self, from: data)).map { .threadDeleted($0.id) }
        case "event": return (try? JSONDecoder().decode(EventMsg.self, from: data)).map { .event($0.event) }
        case "resync": return .resync
        default: return nil
        }
    }

    private struct Head: Decodable { var type: String }
    private struct ThreadMsg: Decodable { var thread: HubThread }
    private struct DeletedMsg: Decodable { var id: String }
    private struct EventMsg: Decodable { var event: HubEvent }
}
