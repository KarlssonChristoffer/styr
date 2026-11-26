import Foundation
import SwiftUI

/// Machines, limits, projects, new threads and ideas: what the tabs and the composer need.
extension Store {
    var homeClient: HubClient? { home.flatMap(HubClient.init) }

    /// The URL of the machine a name stands for, for `has(_:on:)`.
    func url(of machine: String?) -> String? {
        guard let machine else { return home?.url }
        return machines.first { $0.name == machine }?.url ?? home?.url
    }

    /// Says once, for one entry, that a machine needs a newer Hub, and answers whether it may go on.
    func needs(_ feature: String, on machine: String? = nil) -> Bool {
        if has(feature, on: url(of: machine)) { return true }
        flash("\(machine ?? self.machine ?? "Hub") needs a newer Hub for this.")
        return false
    }

    /// The client for a machine by name: its own hubd when followed directly, else nil.
    func client(machine name: String?) -> HubClient? {
        guard let name else { return homeClient }
        if let m = machines.first(where: { $0.name == name }), m.home || live[m.url] == true { return HubClient(m) }
        return nil
    }

    // MARK: Machines tab

    /// How to ask a machine something: its own hubd when followed directly, else the paired one
    /// with `?host=` so it passes the call on.
    func target(_ name: String?) -> (client: HubClient, host: String?)? {
        guard let name, name != machine else { return homeClient.map { ($0, nil) } }
        if let direct = client(machine: name), !(machines.first { $0.name == name }?.home ?? true) { return (direct, nil) }
        return homeClient.map { ($0, name) }
    }

    /// Every machine's rings and checks, side by side.
    func loadVitals() async {
        guard has("phone.hosts") else { return }
        let names = hosts.isEmpty ? machines.map(\.name) : hosts.filter { $0.state == "online" }.map(\.name)
        await withTaskGroup(of: (String, MachineVitals).self) { group in
            for name in names {
                guard let t = target(name) else { continue }
                group.addTask {
                    async let snap = try? t.client.snapshot(host: t.host)
                    async let about = try? t.client.about(host: t.host)
                    async let ready = try? t.client.ready(host: t.host)
                    return (name, MachineVitals(snapshot: await snap, about: await about, ready: await ready))
                }
            }
            for await (name, v) in group { vitals[name] = v }
        }
    }

    func loadMachines() async {
        guard let client = homeClient else { return }
        halt = try? await client.halt()
        guard has("phone.hosts") else { tooOld.insert("hosts"); tooOld.insert("usage"); return }
        do { hosts = try await client.hosts() } catch let e as HubClient.Failure where e.tooOld { tooOld.insert("hosts") } catch {}
        do { usage = try await client.usage() } catch let e as HubClient.Failure where e.tooOld { tooOld.insert("usage") } catch {}
        halt = try? await client.halt()
    }

    func setPaused(_ on: Bool) async {
        guard let client = homeClient else { return }
        do {
            try await client.pause(on)
            Haptics.firm()
            flash(on ? "Every agent is paused" : "Agents carry on")
            halt = try? await client.halt()
        } catch {
            flash("Not \(on ? "paused" : "resumed"): \(error.localizedDescription)")
        }
    }

    // MARK: Projects

    /// Every project the phone knows: the folders threads run in, and the ones added in Hub.
    var projects: [ProjectRef] {
        // Read what it comes from first, so a page that asks is told when any of it changes, then answer
        // from the last result while nothing did: it takes every thread, and a list of projects asks
        // for it many times a redraw.
        let all = threads, stored = storedProjects, mine = machine
        var h = Hasher()
        h.combine(threadsVersion); h.combine(stored); h.combine(mine)
        let key = h.finalize()
        if let memo = projectsMemo, memo.key == key { return memo.value }
        var seen: [String: (ProjectRef, Double)] = [:]
        for t in all where !t.cwd.isEmpty && !Self.scratch(t.project) {
            // A thread on the paired machine has no host; it is the same project as a stored one.
            let ref = ProjectRef(path: Self.projectRoot(t.cwd), name: t.project, host: t.host == mine ? nil : t.host)
            let when = t.returned_at ?? t.sent_at ?? t.updated_at
            if let (_, w) = seen[ref.id], w >= when { continue }
            seen[ref.id] = (ref, when)
        }
        for p in stored where seen[p.id] == nil { seen[p.id] = (p, 0) }
        let value = seen.values.sorted { $0.1 > $1.1 }.map(\.0)
        projectsMemo = (key, value)
        return value
    }

    /// A scratch folder named by a date (`Dev/scratch/2026-11-01-130344`), not a project.
    static func scratch(_ name: String) -> Bool {
        name.range(of: #"^\d{4}-\d{2}-\d{2}"#, options: .regularExpression) != nil
    }

    /// A thread's own worktree (`<repo>/.hub-threads/<id>`) belongs to the repo's folder. Asked for every
    /// thread of every project on a redraw, so each answer is kept.
    private static let roots = NSCache<NSString, NSString>()

    static func projectRoot(_ cwd: String) -> String {
        if let hit = roots.object(forKey: cwd as NSString) { return hit as String }
        var root = cwd
        for marker in ["/.hub-threads/", "\\.hub-threads\\", "/.hub-worktrees/", "\\.hub-worktrees\\"] {
            if let r = cwd.range(of: marker) { root = String(cwd[..<r.lowerBound]); break }
        }
        roots.setObject(root as NSString, forKey: cwd as NSString)
        return root
    }

    /// The threads of every project in one pass, for a list of projects: asking `threads(of:)` once per
    /// row would go through every thread once per project (and again for each thing a row shows).
    func threadIndex() -> ThreadIndex { ThreadIndex(threads) }

    func threads(of project: ProjectRef) -> [HubThread] {
        threads.filter { Self.projectRoot($0.cwd) == project.path && $0.host == project.host && !$0.archived }
            .sorted { ($0.returned_at ?? $0.sent_at ?? $0.updated_at) > ($1.returned_at ?? $1.sent_at ?? $1.updated_at) }
    }

    func loadProjects() async {
        guard let client = homeClient, has("phone.projects") else { return }
        do {
            storedProjects = try await client.projects().map { ProjectRef(path: $0.path, name: $0.name.prefix(1).uppercased() + $0.name.dropFirst(), host: nil) }
        } catch let e as HubClient.Failure where e.tooOld { tooOld.insert("projects") } catch {}
    }

    /// Engines seen on threads, most used first: what a new thread can pick from.
    var engines: [String] {
        var count: [String: Int] = [:]
        for t in threads { count[t.engine, default: 0] += 1 }
        let list = count.sorted { $0.value > $1.value }.map(\.key)
        return list.isEmpty ? ["claude"] : list
    }

    /// Where a new thread goes when you do not choose: the project you worked in last, its machine,
    /// the engine used there last.
    var lastPlace: (project: ProjectRef, engine: String)? {
        // Only a real project counts: not a chat, a scratch or dated folder, or one of Hub's own.
        let real = threads.filter { t in
            !t.archived && !t.cwd.isEmpty && !isChat(t) && !t.isQuick && !t.inChatsFolder && !Self.scratch(t.project)
                && !isInternal(ProjectRef(path: Self.projectRoot(t.cwd), name: t.project, host: t.host))
        }
        guard let t = real.max(by: { ($0.sent_at ?? $0.updated_at) < ($1.sent_at ?? $1.updated_at) }) else { return nil }
        return (ProjectRef(path: Self.projectRoot(t.cwd), name: t.project, host: t.host), t.engine)
    }

    func loadProject(_ p: ProjectRef) async {
        guard let t = target(p.host) else { return }
        guard has("phone.projects", on: url(of: p.host)) else { tooOld.insert("project"); return }
        do { projectInfo[p.id] = try await t.client.project(path: p.path, host: t.host) }
        catch let e as HubClient.Failure where e.tooOld { tooOld.insert("project") } catch {}
    }

    /// Each project's stage and what is left to test, by machine.
    func loadFocusSummaries() async {
        guard has("phone.projects") else { return }
        let byHost = Dictionary(grouping: projects, by: { $0.host })
        for (host, list) in byHost {
            guard let t = target(host) else { continue }
            if let map = try? await t.client.focusSummaries(paths: list.map(\.path), host: t.host) {
                for p in list { if let s = map[p.path] { summaries[p.id] = s } }
            }
        }
    }

    /// Ticks a box in FOCUS.md and reads the file again.
    func tick(_ p: ProjectRef, line: Int, done: Bool) async {
        guard let t = target(p.host), needs("phone.focus", on: p.host) else { return }
        do {
            try await t.client.tick(path: p.path, line: line, done: done, host: t.host)
            if done { Haptics.done() } else { Haptics.tick() }
            await loadProject(p)
        } catch let e as HubClient.Failure where e.tooOld {
            flash("\(p.host ?? machine ?? "Hub") needs a newer hubd to tick FOCUS.md from the phone.")
        } catch {
            flash("Not ticked: \(error.localizedDescription)")
        }
    }

    func lastEngine(in project: ProjectRef) -> String? {
        threads(of: project).first?.engine
    }

    // MARK: New threads and ideas

    /// Starts a thread and returns its id, or nil when it could not.
    func newThread(in project: ProjectRef, engine: String?, message: String, attachments: [Attachment] = [], settings: [String: String] = [:], warm: String? = nil, chat: Bool = false, title: String? = nil, cloud: Bool = false) async -> String? {
        if isDemo {
            let id = "demo-\(UUID().uuidString.prefix(6))"
            var t = Demo.thread(id, String(message.prefix(40)), project.name, project.host ?? "PC", .running, 0)
            t.cwd = project.path
            threads.append(t)
            var f = Folded(); f.items = [.user(message)]
            folded[id] = f
            return id
        }
        guard needs(project.isScratch ? "phone.scratch" : "phone.threads", on: project.host) else { return nil }
        // Never on the machine when the cloud was asked for: a hubd without it refuses.
        if cloud, !has("phone.cloud", on: url(of: project.host)) { flash("\(project.host ?? machine ?? "Hub") needs a newer hubd to start cloud threads."); return nil }
        // Straight to the machine when it is followed directly, else through the paired one.
        let direct = project.host.flatMap { client(machine: $0) }
        guard let client = direct ?? homeClient else { return nil }
        do {
            let t = try await client.newThread(cwd: project.path, message: message, engine: engine,
                                               host: direct == nil ? project.host : nil, scratch: project.isScratch, attachments: attachments,
                                               settings: has("phone.engines", on: url(of: project.host)) ? settings : [:], warm: warm,
                                               chat: chat && has("phone.chat", on: url(of: project.host)), title: title, cloud: cloud)
            Haptics.sent()
            var copy = t
            if copy.host == nil { copy.host = project.host }
            if !threads.contains(where: { $0.id == t.id }) { threads.append(copy) }
            return t.id
        } catch let e as HubClient.Failure where e.tooOld {
            flash("\(project.host ?? machine ?? "Hub") needs a newer hubd to start threads from the phone.")
        } catch {
            flash("Not started: \(error.localizedDescription)")
        }
        return nil
    }

    func idea(_ text: String, in project: ProjectRef?) async {
        if isDemo { flash("Idea kept in \(project?.name ?? "Hub")"); return }
        let target = project?.host.flatMap { self.client(machine: $0) } ?? homeClient
        do {
            try await target?.idea(text, here: project?.path)
            Haptics.done()
            flash("Idea kept in \(project?.name ?? "the right project")")
        } catch {
            flash("Idea not kept: \(error.localizedDescription)")
        }
    }
}

/// Threads grouped by project, newest first, built once per redraw of a list of projects.
@MainActor
struct ThreadIndex {
    private var by: [String: [HubThread]] = [:]

    private static func key(_ root: String, _ host: String?) -> String { (host ?? "\u{0}") + "\u{1}" + root }

    init(_ all: [HubThread]) {
        for t in all where !t.archived { by[Self.key(Store.projectRoot(t.cwd), t.host), default: []].append(t) }
        for k in Array(by.keys) { by[k]?.sort { ($0.returned_at ?? $0.sent_at ?? $0.updated_at) > ($1.returned_at ?? $1.sent_at ?? $1.updated_at) } }
    }

    func of(_ p: ProjectRef) -> [HubThread] { by[Self.key(p.path, p.host)] ?? [] }

    func lastActive(_ p: ProjectRef) -> Double? { of(p).first.map { $0.returned_at ?? $0.sent_at ?? $0.updated_at } }
}
