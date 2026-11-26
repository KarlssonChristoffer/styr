import Foundation
import Security
import SwiftUI

/// The app's state: demo data until the phone is paired, then every machine's threads, live.
/// It follows the paired machine and each machine that one introduces, directly, so it keeps
/// working while the PC sleeps (as the `/phone` page does).
@MainActor
@Observable
final class Store {
    var machines: [Machine] = []
    var threads: [HubThread] = []
    /// Side jobs, by id: threads a thread started for itself, like a subagent. They belong to the
    /// thread that started them (its side job line opens them) and are in no list or the inbox.
    var sideThreads: [String: HubThread] = [:]
    /// Each loaded thread's log folded for the phone. Loaded: the open thread and those that need you.
    var folded: [String: Folded] = [:]
    var peeks: [String: Peek] = [:]
    var live: [String: Bool] = [:]
    var loading: Set<String> = []
    var drafts: [String: String] = [:]
    /// Problems and routines from a machine that has `phone.watch` (`Watch.swift`).
    var watchProblems: [WatchProblem] = []
    /// Every engine the machine can start, with its models (`phone.engines`, `Engines.swift`).
    var phoneEngines: [PhoneEngine] = []
    /// Chat spaces, kept on the phone (`Chat.swift`).
    var spaces: [ChatSpace] = ChatSpace.load()
    /// Threads started in the Chat world. They belong to Chat alone: Dev's inbox, quick threads and
    /// counts leave them out, and Chat lists only them.
    var chatIds: Set<String> = Set(UserDefaults.standard.stringArray(forKey: "chat.ids") ?? [])
    var watchRoutines: [WatchRoutine] = []
    /// Ideas said into the pen and kept on the phone until you open them (`Capture.swift`).
    var ideaDrafts: [IdeaDraft] = IdeaDraft.load()
    var error: String?
    /// The Machines tab: each machine as hubd sees it, plan limits, the panic button.
    var hosts: [HostInfo] = []
    /// When each machine last got the phone's notice settings (`syncNotices`).
    @ObservationIgnored var noticesSent: [String: Date] = [:]
    var usage: UsageReport?
    var halt: Halt?
    var storedProjects: [ProjectRef] = []
    var vitals: [String: MachineVitals] = [:]
    var projectInfo: [String: ProjectInfo] = [:]
    var summaries: [String: FocusSummary] = [:]
    var libraryCache: [String: (at: Date, items: [LibraryItem])] = [:]
    @ObservationIgnored let pictures = NSCache<NSString, UIImage>()
    /// A call the paired hubd is too old for, by name, so the app can say so once.
    var tooOld: Set<String> = []
    /// What each machine's hubd lists in `/api/health`, by the machine's address. Absent until it
    /// has answered, which counts as "has it" so nothing is hidden on a blip.
    var features: [String: Set<String>] = [:]
    /// How much of the agent's work threads show (`phone.chatdetail`). nil on a hubd without it: threads
    /// then draw as they always did.
    var chatDetailPrefs: ChatDetailPrefs?
    var toast: Toast?
    /// Your own emojis and the quick row for reactions (`Reactions.swift`).
    var emoji = EmojiBook()

    struct Toast: Equatable {
        let id = UUID()
        var text: String
        var undo: (@MainActor () -> Void)?
        static func == (a: Toast, b: Toast) -> Bool { a.id == b.id }
    }

    private var links: [String: StreamLink] = [:]
    private var homeThreads: [String: HubThread] = [:]
    private var machineThreads: [String: [String: HubThread]] = [:]
    private var owner: [String: String] = [:]
    /// The threads as the last merge made them: a merge that finds the same ones changes nothing, so no
    /// page is rebuilt for it (every machine's stream and every refresh ends in one).
    @ObservationIgnored private var merged: [String: HubThread] = [:]
    /// Counts the times `threads` was set, for what is worked out from it and kept (`projects`).
    @ObservationIgnored var threadsVersion = 0
    @ObservationIgnored var projectsMemo: (key: Int, value: [ProjectRef])?
    @ObservationIgnored var internalMemo: [String: Bool] = [:]
    private var logs: [String: [HubEvent]] = [:]
    /// Messages you sent that the machine has not echoed back yet: shown at once, by thread.
    private var sentLocal: [String: (text: String, at: Double)] = [:]

    /// The thread's log folded, with a message you just sent at its end until its own event is in.
    private func fold(_ id: String, _ log: [HubEvent]) -> Folded {
        var f = Folded(log, since: since[id])
        guard let s = sentLocal[id] else { return f }
        let want = s.text.trimmingCharacters(in: .whitespacesAndNewlines)
        // The phone's clock and the machine's can differ a little, so allow some minutes.
        let echoed = log.contains { $0.kind == "user_message" && $0.ts >= s.at - 600_000
            && (UserMessagePayload($0.data).text ?? "").trimmingCharacters(in: .whitespacesAndNewlines) == want }
        if echoed { sentLocal[id] = nil } else { f.addOwn(s.text, at: s.at) }
        return f
    }
    /// Where "New" starts in the thread you have open: when you last looked at it (see `open`).
    @ObservationIgnored private var since: [String: Double] = [:]
    /// When you left each thread on this phone, kept so it survives a restart.
    @ObservationIgnored private var leftAt: [String: Double] = (UserDefaults.standard.dictionary(forKey: "thread.left") as? [String: Double]) ?? [:]
    /// The "New" break is on the screen (the jump button shows when it is not).
    var newInView = true
    private var peekAllowed = true
    private var discovered = Date.distantPast
    private var pendingSummaries: Set<String> = []

    /// Made-up threads from the sketches, to look around before pairing (or `-demo 1` at launch).
    private(set) var demo = false

    init() {
        machines = Keychain.load()
        if machines.isEmpty && UserDefaults.standard.bool(forKey: "demo") { useDemo() }
    }

    var isDemo: Bool { demo }
    /// Paired with at least one machine, or looking at the demo: the app proper, not first start.
    var ready: Bool { demo || !machines.isEmpty }

    func useDemo() {
        demo = true
        loadDemo()
    }
    var home: Machine? { machines.first { $0.home } }
    var machine: String? { home?.name }
    var anyLive: Bool { machines.contains { live[$0.url] == true } }

    /// The level a thread shows its work at; nil when this hubd has no chat detail, which keeps today's
    /// drawing. A thread's own level wins, then the default; with neither, Chat starts at Minimal and
    /// the work world at Quiet, like the desktop's.
    func detail(of t: HubThread) -> ChatDetail? {
        guard let p = chatDetailPrefs, has("phone.chatdetail") else { return nil }
        if let own = p.threads[t.id].flatMap(ChatDetail.init(rawValue:)) { return own }
        if let all = ChatDetail(rawValue: p.default) { return all }
        return isChat(t) ? .minimal : .quiet
    }

    func loadChatDetail(_ client: HubClient? = nil) async {
        guard has("phone.chatdetail"), let client = client ?? homeClient else { return }
        if isDemo { chatDetailPrefs = chatDetailPrefs ?? ChatDetailPrefs(); return }
        if let p = try? await client.chatDetail() { chatDetailPrefs = p }
    }

    /// Gives a thread a level of its own; nil has it follow the default again.
    func setDetail(_ level: ChatDetail?, for t: HubThread) {
        var p = chatDetailPrefs ?? ChatDetailPrefs()
        p.threads[t.id] = level?.rawValue
        saveChatDetail(p)
    }

    func setDefaultDetail(_ level: ChatDetail?) {
        var p = chatDetailPrefs ?? ChatDetailPrefs()
        p.default = level?.rawValue ?? ""
        saveChatDetail(p)
    }

    private func saveChatDetail(_ p: ChatDetailPrefs) {
        let before = chatDetailPrefs
        chatDetailPrefs = p
        guard !isDemo, let client = homeClient else { return }
        Task {
            do { try await client.setChatDetail(p) } catch { chatDetailPrefs = before; flash("Not saved") }
        }
    }

    func isChat(_ t: HubThread) -> Bool { t.inChatsFolder || chatIds.contains(t.id) }

    func markChat(_ id: String) {
        guard !chatIds.contains(id) else { return }
        chatIds.insert(id)
        UserDefaults.standard.set(Array(chatIds), forKey: "chat.ids")
    }

    func threads(in section: InboxSection) -> [HubThread] {
        threads.filter { $0.section == section && !isChat($0) }.sorted {
            if ($0.pinned_at != nil) != ($1.pinned_at != nil) { return $0.pinned_at != nil }
            return ($0.returned_at ?? $0.sent_at ?? $0.updated_at) > ($1.returned_at ?? $1.sent_at ?? $1.updated_at)
        }
    }

    // MARK: What a row shows

    func openAsk(_ t: HubThread) -> Ask? {
        t.status == .waiting_on_you ? folded[t.id]?.openAsk : nil
    }

    func replies(_ t: HubThread) -> Peek.Replies? {
        guard t.status != .running else { return nil }
        if let f = folded[t.id] { return f.replies }
        return peeks[t.id]?.replies
    }

    func lastLine(_ t: HubThread) -> String? {
        if let ask = openAsk(t) { return ask.command.map { "Wants to run \($0)" } ?? ask.title }
        if t.status == .error {
            if let f = folded[t.id], case let .error(text)? = f.items.last(where: { if case .error = $0 { return true }; return false }) { return text }
            if let e = peeks[t.id]?.error { return e }
        }
        if let r = replies(t), !r.question.isEmpty { return r.question }
        let reply = folded[t.id]?.lastReply ?? peeks[t.id]?.last_reply
        guard let reply, !reply.isEmpty else { return nil }
        return Self.tail(reply)
    }

    /// The end of a reply, as one or two lines: its last paragraph, with Markdown marks dropped.
    static func tail(_ text: String) -> String {
        let paragraphs = text.components(separatedBy: "\n\n").map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty && !$0.hasPrefix("```") }
        let last = paragraphs.last ?? text
        // Links read as their words, pictures as nothing: a row is for reading, not for paths.
        let plain = last
            .replacingOccurrences(of: #"!\[[^\]]*\]\([^)]*\)"#, with: "", options: .regularExpression)
            .replacingOccurrences(of: #"\[([^\]]*)\]\([^)]*\)"#, with: "$1", options: .regularExpression)
        return plain.replacingOccurrences(of: "**", with: "").replacingOccurrences(of: "`", with: "").replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespaces)
    }

    // MARK: Pairing

    func pair(link: String) async {
        guard let (base, code) = HubClient.parse(link: link) else {
            error = "That is not a pairing link. Scan the QR code under Hub › Settings › Access › Your phone."
            return
        }
        do {
            let paired = try await HubClient.redeem(base: base, code: code)
            stopAll()
            demo = false
            machines = [Machine(id: nil, name: paired.machine, url: base.absoluteString, token: paired.token, home: true)]
            Keychain.save(machines)
            threads = []; sideThreads = [:]; merged = [:]; threadsVersion &+= 1; folded = [:]; peeks = [:]; homeThreads = [:]; machineThreads = [:]; logs = [:]
            error = nil
            discovered = .distantPast
            start()
        } catch {
            self.error = "That code did not work: \(error.localizedDescription). Make a new one in Hub and scan it again."
        }
    }

    func forget() {
        stopAll()
        Keychain.clear()
        machines = []
        demo = false
        threads = []; sideThreads = [:]; merged = [:]; threadsVersion &+= 1; folded = [:]; peeks = [:]; hosts = []; usage = nil
    }

    // MARK: Following the machines

    /// Opens a stream to every machine and loads what they have. Safe to call again.
    func start() {
        guard !isDemo else { return }
        for m in machines { connect(m) }
        Task { await refresh() }
    }

    func stopAll() {
        links.values.forEach { $0.stop() }
        links = [:]
        live = [:]
    }

    private func connect(_ m: Machine) {
        guard links[m.url] == nil, let url = HubClient(m)?.streamURL else { return }
        let key = m.url
        let link = StreamLink(url: url, onMessage: { [weak self] msg in self?.take(msg, from: key) }, onLive: { [weak self] up in
            self?.live[key] = up
            self?.merge()
        })
        links[key] = link
        link.start()
    }

    func refresh() async {
        guard !isDemo else { return }
        await withTaskGroup(of: Void.self) { group in
            for m in machines { group.addTask { await self.load(m) } }
        }
        merge()
        halt = try? await home.flatMap(HubClient.init)?.halt()
        await discover()
        await loadSummaries()
        if let open = openThread { await loadEvents(open) }
        await syncNotices()
    }

    /// Whether a machine's hubd has a feature (`compat::FEATURES` in the core, e.g. "phone.tidy").
    /// Show or enable an entry point only when this is true; where it is false say once, beside
    /// that one entry, that the machine needs a newer Hub. The core (threads, live, send, answer)
    /// is never gated. An old hubd lists no features, so nothing beyond the core is assumed.
    func has(_ feature: String, on machineURL: String? = nil) -> Bool {
        guard let set = features[machineURL ?? home?.url ?? ""] else { return true }
        return set.contains(feature)
    }

    private func load(_ m: Machine) async {
        guard let client = HubClient(m) else { return }
        if let listed = await client.features() { features[m.url] = listed }
        if m.home { await loadChatDetail(client) }
        do {
            let list = try await client.threads()
            if m.home {
                homeThreads = Dictionary(list.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
                error = nil
            } else {
                machineThreads[m.url] = Dictionary(list.filter { $0.host == nil }.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            }
        } catch let e as HubClient.Failure where e.unauthorized && m.home {
            error = "This phone is no longer let in. Scan a new QR code in Hub."
        } catch {
            // A sleeping machine: the paired machine relays its threads meanwhile.
            if m.home && !anyLive { self.error = "\(m.name) does not answer: \(error.localizedDescription)" }
        }
    }

    /// One list from every machine: the paired machine's threads, then each other machine's own,
    /// which replace the copies the paired machine relays while that machine is reachable.
    private func merge() {
        var all: [String: HubThread] = [:]
        var owners: [String: String] = [:]
        let direct = machines.filter { !$0.home && live[$0.url] == true }
        let directNames = Set(direct.map(\.name))
        for t in homeThreads.values where !(t.host.map(directNames.contains) ?? false) { all[t.id] = t }
        for m in direct {
            for t in (machineThreads[m.url] ?? [:]).values {
                var copy = t
                copy.host = m.name
                all[t.id] = copy
                owners[t.id] = m.url
            }
        }
        if owner != owners { owner = owners }
        guard all != merged else { Perf.count("merge.same"); return }
        merged = all
        threadsVersion &+= 1
        sideThreads = all.filter { $0.value.parent != nil }
        // A side job that still runs keeps the thread that started it working (`HubThread.sideBusy`).
        var busy: [String: Double] = [:]
        for side in all.values where !side.archived && (side.status == .running || side.status == .in_terminal) {
            if let p = side.parent { busy[p] = min(busy[p] ?? .infinity, side.sent_at ?? side.updated_at) }
        }
        threads = Perf.time("merge", detail: "\(all.count) threads") {
            all.values.filter { $0.parent == nil }.map { t in
                var t = t
                t.sideBusy = busy[t.id]
                return t
            }
        }
    }

    private func take(_ msg: StreamLink.Message, from key: String) {
        let isHome = machines.first { $0.url == key }?.home ?? false
        switch msg {
        case let .thread(t):
            if isHome { homeThreads[t.id] = t }
            else if t.host == nil { machineThreads[key, default: [:]][t.id] = t }
            merge()
            if t.status == .waiting_on_you || t.status == .error { summarize(t.id) }
        case let .threadDeleted(id):
            homeThreads[id] = nil
            machineThreads[key]?[id] = nil
            merge()
        case let .event(e):
            // The same event can come from the machine itself and from the paired machine's relay.
            if var log = logs[e.thread_id] {
                // Two streams (the machine itself and the paired one's relay) can bring events in another
                // order, and a dropped link can lose some. An event is put in its place, and a hole in the
                // seq numbers is fetched again, so a message written on the PC never goes missing here.
                if !log.contains(where: { $0.seq == e.seq }) {
                    let hole = e.seq > (log.last?.seq ?? 0) + 1
                    log.append(e)
                    log.sort { $0.seq < $1.seq }
                    logs[e.thread_id] = log
                    folded[e.thread_id] = Perf.time("fold", detail: "\(log.count) events") { fold(e.thread_id, log) }
                    if hole { Task { await loadEvents(e.thread_id) } }
                }
            } else if ["turn_end", "permission_request", "replies", "error"].contains(e.kind) {
                summarize(e.thread_id)
            }
            if e.kind == "turn_end" { Haptics.arrived() } else if e.kind == "permission_request" { Haptics.asks() }
        case .resync:
            Task { await refresh() }
        }
    }

    /// Asks the paired machine for the others and trades each new one's code for a token there.
    private func discover() async {
        guard let home, let client = HubClient(home), Date().timeIntervalSince(discovered) > 300 else { return }
        discovered = Date()
        let have = machines.compactMap { $0.home ? nil : $0.id }
        guard let found = try? await client.otherMachines(have: have) else { discovered = .distantPast; return }
        var changed = false
        for f in found {
            if let i = machines.firstIndex(where: { $0.id == f.id }) {
                if machines[i].url != f.url || machines[i].name != f.name {
                    machines[i].url = f.url; machines[i].name = f.name; changed = true
                }
                connect(machines[i])
            } else if let code = f.code, let url = URL(string: f.url),
                      let paired = try? await HubClient.redeem(base: url, code: code) {
                let m = Machine(id: f.id, name: f.name, url: f.url, token: paired.token, home: false)
                machines.append(m)
                changed = true
                connect(m)
                await load(m)
            }
        }
        if changed { Keychain.save(machines); merge() }
    }

    // MARK: Threads

    private(set) var openThread: String?

    /// A thread by id, side jobs too (they are not in `threads`).
    func thread(_ id: String) -> HubThread? { threads.first { $0.id == id } ?? sideThreads[id] }

    func open(_ id: String) async {
        let here = openThread == id
        openThread = id
        let t = thread(id)
        // Before `seen_at` moves up to now: the later of when you left it here and when any device saw it.
        if !here {
            let at = max(leftAt[id] ?? 0, t?.seen_at ?? 0)
            since[id] = at > 0 ? at : nil
            newInView = true
            if let log = logs[id] { folded[id] = fold(id, log) }
        }
        if let t, t.unread { tidy([t], .seen) }
        guard !isDemo else { return }
        await loadEvents(id)
    }

    func close(_ id: String) {
        if openThread == id { openThread = nil }
        stepAway(id)
        since[id] = nil
    }

    /// You stopped looking at this thread now: the next time "New" starts from here.
    func stepAway(_ id: String) {
        leftAt[id] = Date().timeIntervalSince1970 * 1000
        if leftAt.count > 300, let oldest = leftAt.min(by: { $0.value < $1.value })?.key { leftAt[oldest] = nil }
        UserDefaults.standard.set(leftAt, forKey: "thread.left")
    }

    /// The app came back after a while away: "New" moves to where you stopped looking.
    func cameBack(_ id: String) {
        guard openThread == id else { return }
        let at = max(leftAt[id] ?? 0, thread(id)?.seen_at ?? 0)
        since[id] = at > 0 ? at : nil
        folded[id] = fold(id, logs[id] ?? [])
    }

    /// Whether the thread has a "New" break to jump to.
    func hasNew(_ id: String) -> Bool {
        folded[id]?.items.contains { if case let .brk(b) = $0 { return b.kind == "new" }; return false } ?? false
    }

    /// The client for a thread: the machine it runs on when that is reachable, else the paired one,
    /// which passes the call on.
    func client(for id: String) -> HubClient? {
        if let key = owner[id], live[key] == true, let m = machines.first(where: { $0.url == key }) { return HubClient(m) }
        return home.flatMap(HubClient.init)
    }

    func loadEvents(_ id: String) async {
        guard let client = client(for: id), !loading.contains(id) else { return }
        loading.insert(id)
        defer { loading.remove(id) }
        // From the first hole in the log (seq numbers are one after the other), not from its end.
        var after = 0
        for e in logs[id] ?? [] { if e.seq == after + 1 { after = e.seq } else if e.seq > after + 1 { break } }
        do {
            let more = try await client.events(thread: id, after: after)
            var log = logs[id] ?? []
            let have = Set(log.map(\.seq))
            log.append(contentsOf: more.filter { !have.contains($0.seq) })
            log.sort { $0.seq < $1.seq }
            logs[id] = log
            folded[id] = fold(id, log)
        } catch {
            if openThread == id { self.error = error.localizedDescription }
        }
    }

    private func summarize(_ id: String) {
        guard !pendingSummaries.contains(id) else { return }
        pendingSummaries.insert(id)
        Task {
            try? await Task.sleep(for: .milliseconds(400))
            pendingSummaries.remove(id)
            await summary(id)
        }
    }

    /// What a row needs: the log for a thread that needs you (its ask and options), a peek otherwise.
    private func summary(_ id: String) async {
        guard let t = threads.first(where: { $0.id == id }) else { return }
        let canPeek = peekAllowed && has("phone.peek", on: url(of: t.host))
        if logs[id] != nil || t.status == .waiting_on_you || t.status == .error || !canPeek {
            // Without the card, the log itself gives the row its last words (an older hubd).
            await loadEvents(id)
            return
        }
        guard let client = client(for: id), has("phone.peek", on: url(of: t.host)) else { return }
        do {
            peeks[id] = try await client.peek(thread: id)
        } catch let e as HubClient.Failure where e.forbidden {
            // An older hubd that does not let a phone peek: the rows show less.
            peekAllowed = false
        } catch {}
    }

    private func loadSummaries() async {
        let wanted = threads(in: .needsYou) + threads(in: .done).prefix(12)
        await withTaskGroup(of: Void.self) { group in
            for t in wanted { group.addTask { await self.summary(t.id) } }
        }
    }

    // MARK: Doing things

    func send(_ text: String, to thread: HubThread, steer: Bool = false, attachments: [Attachment] = []) async {
        if isDemo { demoSend(text, thread); return }
        guard let client = client(for: thread.id) else { return }
        // Shown at once, not when the machine's event for it comes back.
        let id = thread.id
        Haptics.sent()
        if !steer, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            sentLocal[id] = (text, Date().timeIntervalSince1970 * 1000)
            withAnimation(.spring(response: 0.42, dampingFraction: 0.72)) { folded[id] = fold(id, logs[id] ?? []) }
        }
        do {
            try await client.send(thread: thread.id, text: text, steer: steer, attachments: attachments)
            drafts[thread.id] = nil
        } catch {
            Haptics.fail()
            sentLocal[id] = nil
            folded[id] = fold(id, logs[id] ?? [])
            self.error = error.localizedDescription
            flash("Not sent: \(error.localizedDescription)")
        }
    }

    func answer(_ thread: HubThread, ask: Ask, option: String?) async {
        if isDemo { demoAnswer(thread, option: option); return }
        guard let client = client(for: thread.id) else { return }
        do {
            try await client.answer(thread: thread.id, request: ask.id, option: option)
            Haptics.done()
            flash(option == nil ? "Rejected" : (ask.options.first { $0.id == option }?.name ?? "Answered"))
        } catch {
            Haptics.fail()
            flash("Not answered: \(error.localizedDescription)")
        }
    }

    func stop(_ thread: HubThread) async {
        if isDemo { return }
        guard let client = client(for: thread.id) else { return }
        do { try await client.cancel(thread: thread.id); flash("Stopped") }
        catch { flash("Not stopped: \(error.localizedDescription)") }
    }

    // MARK: Tidying the inbox

    enum Tidy {
        case settle, snooze(until: Double), pin, unpin, archive, seen
        /// Out of the inbox again: a settled thread back, a snoozed one woken (`was` is when it was to return), an archived one restored.
        case reopen, wake(was: Double), unarchive

        var patch: [String: Any] {
            switch self {
            case .settle: return ["settled": true]
            case let .snooze(until): return ["snooze_until": Int64(until)]
            case .pin: return ["pinned": true]
            case .unpin: return ["pinned": false]
            case .archive: return ["archived": true]
            case .seen: return ["seen": true]
            case .reopen: return ["settled": false]
            case .wake: return ["snooze_until": 0]
            case .unarchive: return ["archived": false]
            }
        }

        var undo: [String: Any] {
            switch self {
            case .settle: return ["settled": false]
            case .snooze: return ["snooze_until": 0]
            case .pin: return ["pinned": false]
            case .unpin: return ["pinned": true]
            case .archive: return ["archived": false]
            case .seen: return [:]
            case .reopen: return ["settled": true]
            case let .wake(was): return ["snooze_until": Int64(was)]
            case .unarchive: return ["archived": true]
            }
        }

        func apply(_ t: inout HubThread) {
            let now = Date().timeIntervalSince1970 * 1000
            switch self {
            case .settle: t.settled_at = now
            case let .snooze(until): t.snoozed_until = until
            case .pin: t.pinned_at = now
            case .unpin: t.pinned_at = nil
            case .archive: t.archived = true
            case .seen: t.seen_at = now
            case .reopen: t.settled_at = nil
            case .wake: t.snoozed_until = nil
            case .unarchive: t.archived = false
            }
        }

        func word(_ n: Int, _ title: String) -> String {
            let what = n == 1 ? "\u{201C}\(title)\u{201D}" : "\(n) threads"
            switch self {
            case .settle: return "Settled \(what)"
            case let .snooze(until): return "Snoozed \(what) until \(Store.clock(until))"
            case .pin: return "Pinned \(what)"
            case .unpin: return "Unpinned \(what)"
            case .archive: return "Archived \(what)"
            case .seen: return ""
            case .reopen: return "Back in the inbox: \(what)"
            case .wake: return "Woke \(what)"
            case .unarchive: return "Restored \(what)"
            }
        }
    }

    /// When a snooze ends, as the swipe offers it: in an hour, tomorrow at 08:00.
    nonisolated static func snoozeTimes() -> [(label: String, until: Double)] {
        let cal = Calendar.current
        let hour = Date().addingTimeInterval(3600)
        let tomorrow = cal.date(bySettingHour: 8, minute: 0, second: 0, of: cal.date(byAdding: .day, value: 1, to: Date())!)!
        return [("1 h", hour.timeIntervalSince1970 * 1000), ("08:00", tomorrow.timeIntervalSince1970 * 1000)]
    }

    nonisolated static func clock(_ ms: Double) -> String {
        let d = Date(timeIntervalSince1970: ms / 1000)
        let f = DateFormatter()
        f.dateFormat = Calendar.current.isDateInToday(d) ? "HH:mm" : "EEE HH:mm"
        return f.string(from: d)
    }

    /// Changes threads at once, shows it in the list right away, and offers to undo for 5 s.
    func tidy(_ list: [HubThread], _ what: Tidy) {
        guard !list.isEmpty else { return }
        if !isDemo, !has("phone.tidy", on: url(of: list[0].host)) {
            if case .seen = what { return }
            flash("\(list[0].host ?? machine ?? "Hub") needs a newer Hub to tidy from the phone.")
            return
        }
        let before = list
        for t in list { mutate(t.id) { what.apply(&$0) } }
        switch what {
        case .settle: Haptics.done()
        case .snooze: Haptics.snooze()
        case .archive: Haptics.firm()
        case .pin, .unpin: Haptics.tick()
        case .seen: break
        case .reopen, .wake, .unarchive: Haptics.tap()
        }
        let text = what.word(list.count, list[0].title)
        if !text.isEmpty {
            flash(text, undo: { [weak self] in self?.restore(before, undoing: what) })
        }
        guard !isDemo else { return }
        Task {
            for t in list {
                guard let client = client(for: t.id) else { continue }
                do { try await client.tidy(thread: t.id, what.patch) }
                catch let e as HubClient.Failure where e.tooOld {
                    if case .seen = what { return }
                    restore(before, undoing: nil)
                    flash("\(t.host ?? machine ?? "Hub") needs a newer hubd for this. Update Hub there.")
                    return
                } catch {
                    if case .seen = what { return }
                    restore(before, undoing: nil)
                    flash("Not saved: \(error.localizedDescription)")
                    return
                }
            }
        }
    }

    /// Changes a thread everywhere it is kept, so the next merge does not undo it.
    private func mutate(_ id: String, _ f: (inout HubThread) -> Void) {
        if var t = homeThreads[id] { f(&t); homeThreads[id] = t }
        for key in machineThreads.keys { if var t = machineThreads[key]?[id] { let host = t.host; f(&t); t.host = host; machineThreads[key]?[id] = t } }
        if let i = threads.firstIndex(where: { $0.id == id }) { let host = threads[i].host; f(&threads[i]); threads[i].host = host }
    }

    private func restore(_ before: [HubThread], undoing what: Tidy?) {
        for t in before { mutate(t.id) { $0 = t } }
        toast = nil
        Haptics.tick()
        guard let what, !isDemo else { return }
        Task {
            for t in before { try? await client(for: t.id)?.tidy(thread: t.id, what.undo) }
        }
    }

    func flash(_ text: String, undo: (@MainActor () -> Void)? = nil) {
        let t = Toast(text: text, undo: undo)
        withAnimation { toast = t }
        Task {
            try? await Task.sleep(for: .seconds(undo == nil ? 3 : 5))
            withAnimation { if toast == t { toast = nil } }
        }
    }

    // MARK: Demo

    private func loadDemo() {
        threads = Demo.threads
        folded = Demo.folded
        peeks = [:]
    }

    private func demoSend(_ text: String, _ thread: HubThread) {
        folded[thread.id, default: Folded()].items.append(.user(text))
        folded[thread.id]?.replies = nil
        folded[thread.id]?.openAsk = nil
        if let i = threads.firstIndex(of: thread) { threads[i].status = .running; threads[i].returned_at = nil }
    }

    private func demoAnswer(_ thread: HubThread, option: String?) {
        folded[thread.id]?.openAsk = nil
        if let i = threads.firstIndex(of: thread) { threads[i].status = option == nil ? .idle : .running }
        flash(option == nil ? "Denied" : "Allowed")
    }
}
