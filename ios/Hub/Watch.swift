import SwiftUI

// Sheet "Rutiner och problem": a Watch group in Settings with Problems and Routines, as on the
// PC under System. The phone reads them and can start a fix thread, archive a problem, switch a
// routine on or off and run one now (feature `phone.watch`); it does not edit them.

struct WatchProblem: Decodable, Hashable, Identifiable {
    var id: Int
    var project: String
    var title: String
    var culprit: String?
    var folder: String?
    var machine: String?
    var first_seen: Double?
    var last_seen: Double?
    var count: Int?
    var status: String?
    var thread: String?
    var days: [Int]?
}

struct WatchHit: Decodable, Hashable {
    struct Frame: Decodable, Hashable { var file: String?; var function: String?; var line: Int? }
    var message: String
    var trace: String?
    var frames: [Frame]?
    var ts: Double?
}

struct WatchProblemDetail: Decodable {
    var problem: WatchProblem
    var hits: [WatchHit]?
}

struct WatchRoutine: Decodable, Hashable, Identifiable {
    struct Schedule: Codable, Hashable { var every: String; var at: Int?; var weekday: Int?; var hours: Int? }
    struct Run: Decodable, Hashable {
        var thread: String?
        var at: Double
        var by: String?
        var state: String
        var report: String?
        var ended_at: Double?
    }
    var id: String
    var name: String
    var folder: String
    var engine: String?
    var prompt: String
    var schedule: Schedule
    var enabled: Bool
    var template: String?
    var runs: [Run]?
    var next_at: Double?

    /// "Weekdays 07:00", "Every 6 h".
    var when: String {
        let time = String(format: "%02d:%02d", (schedule.at ?? 0) / 60, (schedule.at ?? 0) % 60)
        switch schedule.every {
        case "weekdays": return "Weekdays \(time)"
        case "week": return "\(["Mondays", "Tuesdays", "Wednesdays", "Thursdays", "Fridays", "Saturdays", "Sundays"][min(max(schedule.weekday ?? 0, 0), 6)]) \(time)"
        case "hours": return "Every \(schedule.hours ?? 1) h"
        default: return "Daily \(time)"
        }
    }
}

private struct RoutineList: Decodable { var routines: [WatchRoutine] }
private struct Started: Decodable { var thread: String? }
private struct ProblemStarted: Decodable { struct T: Decodable { var id: String }; var thread: T? }

extension HubClient {
    func problems(host: String?) async throws -> [WatchProblem] {
        try JSONDecoder().decode(Lossy<WatchProblem>.self, from: try await call("GET", "api/problems", query: host.map { ["host": $0] } ?? [:])).items
    }

    func problem(id: Int, host: String?) async throws -> WatchProblemDetail {
        try JSONDecoder().decode(WatchProblemDetail.self, from: try await call("GET", "api/problems/\(id)", query: host.map { ["host": $0] } ?? [:]))
    }

    func archiveProblem(id: Int, host: String?) async throws {
        _ = try await call("PATCH", "api/problems/\(id)", query: host.map { ["host": $0] } ?? [:], body: ["status": "archived"])
    }

    /// Starts a thread on a problem; answers its id, none when no folder is known for it.
    func fixProblem(id: Int, host: String?) async throws -> String? {
        let data = try await call("POST", "api/problems/\(id)/thread", query: host.map { ["host": $0] } ?? [:], body: [:])
        return (try? JSONDecoder().decode(ProblemStarted.self, from: data))?.thread?.id
    }

    func routines(host: String?) async throws -> [WatchRoutine] {
        try JSONDecoder().decode(RoutineList.self, from: try await call("GET", "api/routines", query: host.map { ["host": $0] } ?? [:])).routines
    }

    /// Switches a routine on or off (`POST /api/routines/{id}/enabled`).
    func setRoutine(_ r: WatchRoutine, enabled: Bool, host: String?) async throws {
        _ = try await call("POST", "api/routines/\(r.id)/enabled", query: host.map { ["host": $0] } ?? [:], body: ["enabled": enabled])
    }

    func runRoutine(id: String, host: String?) async throws -> String? {
        (try? JSONDecoder().decode(Started.self, from: try await call("POST", "api/routines/\(id)/run", query: host.map { ["host": $0] } ?? [:], body: [:])))?.thread
    }
}

extension Store {
    var watchAvailable: Bool { isDemo ? false : has("phone.watch") }

    func loadWatch() async {
        guard watchAvailable, let t = target(nil) else { return }
        async let p = try? t.client.problems(host: t.host)
        async let r = try? t.client.routines(host: t.host)
        if let p = await p { watchProblems = p }
        if let r = await r { watchRoutines = r }
    }

    var openProblems: [WatchProblem] { watchProblems.filter { ($0.status ?? "open") == "open" }.sorted { ($0.last_seen ?? 0) > ($1.last_seen ?? 0) } }
}

// MARK: Problems

struct ProblemsView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let list = store.openProblems
        let archived = store.watchProblems.filter { $0.status == "archived" }.count
        VStack(spacing: 0) {
            HubHeader(parent: "Settings", title: "Problems", sub: "\(list.count) open", back: { dismiss() })
            ScrollView {
                LazyVStack(spacing: 0) {
                    if list.isEmpty { Text("Nothing is wrong right now.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(24) }
                    ForEach(list) { p in
                        NavigationLink(value: Route.problem(p.id)) { row(p) }.buttonStyle(.press)
                    }
                    if archived > 0 {
                        HStack { Text("Archived").foregroundStyle(Hub.muted); Spacer(); Text("\(archived)").foregroundStyle(Hub.accent) }
                            .font(.system(size: 12.5, weight: .semibold)).padding(.horizontal, 18).padding(.top, 18)
                    }
                }
            }
            .refreshable { await store.loadWatch() }
        }
        .background(Hub.bg)
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack { dismiss() }
        .task { await store.loadWatch() }
    }

    private func row(_ p: WatchProblem) -> some View {
        HStack(spacing: 12) {
            Group {
                if p.thread != nil { StatusDot(state: .running) } else { StatusDot(state: .waiting) }
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(p.title).font(.system(size: 15)).lineLimit(1)
                Text("\(p.project) · \(relative(p.last_seen))").font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(1)
            }
            Spacer(minLength: 8)
            Spark(days: p.days ?? [])
            Text("\(p.count ?? 0)").font(.system(size: 15)).frame(minWidth: 28, alignment: .trailing)
        }
        .padding(.horizontal, 18).frame(minHeight: 60).contentShape(Rectangle())
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
    }
}

/// Seven days of hits as small bars, red when today is the worst day.
struct Spark: View {
    let days: [Int]
    var body: some View {
        let top = max(1, days.max() ?? 1)
        HStack(alignment: .bottom, spacing: 2) {
            ForEach(Array(days.enumerated()), id: \.offset) { i, n in
                RoundedRectangle(cornerRadius: 1).fill(i == days.count - 1 && n == days.max() ? Hub.red : Hub.lineStrong)
                    .frame(width: 4, height: max(3, 22 * CGFloat(n) / CGFloat(top)))
            }
        }
        .frame(height: 22)
    }
}

struct ProblemPage: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @Environment(\.dismiss) private var dismiss
    let id: Int
    @State private var detail: WatchProblemDetail?
    @State private var busy = false

    var body: some View {
        let p = detail?.problem ?? store.watchProblems.first { $0.id == id }
        VStack(spacing: 0) {
            HubHeader(parent: "Problems", title: p?.title ?? "Problem", sub: p.map { "\($0.count ?? 0) times · \($0.machine ?? "")" }, back: { dismiss() })
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    if let p {
                        HStack(alignment: .bottom, spacing: 10) {
                            Spark(days: p.days ?? [])
                            Text("First seen \(relative(p.first_seen)) ago · last \(relative(p.last_seen)) ago").font(.system(size: 12.5)).foregroundStyle(Hub.muted)
                        }
                        if let c = p.culprit { Text(c).font(.system(size: 12, design: .monospaced)).foregroundStyle(Hub.text3) }
                    }
                    if let hit = detail?.hits?.first {
                        VStack(alignment: .leading, spacing: 6) {
                            Text(hit.message).font(.system(size: 12.5, design: .monospaced)).foregroundStyle(Hub.red)
                            ForEach(Array((hit.frames ?? []).prefix(6).enumerated()), id: \.offset) { _, f in
                                Text("\(f.file ?? "?")\(f.line.map { ":\($0)" } ?? "") \(f.function ?? "")").font(.system(size: 11.5, design: .monospaced)).foregroundStyle(Hub.text3)
                            }
                            if let t = hit.trace, (hit.frames ?? []).isEmpty { Text(String(t.prefix(800))).font(.system(size: 11.5, design: .monospaced)).foregroundStyle(Hub.text3) }
                        }
                        .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                        .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
                    }
                    if let t = p?.thread {
                        Button { nav.push(.thread(t)) } label: { Label("Open the fix thread", systemImage: "arrow.up.right").frame(maxWidth: .infinity) }.buttonStyle(PillButtonStyle())
                    }
                }
                .padding(.horizontal, 14).padding(.top, 6)
            }
            HStack(spacing: 10) {
                Button { archive() } label: { Text("Archive").frame(maxWidth: .infinity) }.buttonStyle(PillButtonStyle())
                Button { fix() } label: {
                    Group { if busy { Working(tint: Hub.bg, size: 17) } else { Label("Fix in a thread", systemImage: "sparkles") } }.frame(maxWidth: .infinity)
                }
                .buttonStyle(PillButtonStyle(kind: .primary))
            }
            .padding(.horizontal, 14).padding(.top, 6).padding(.bottom, 10)
        }
        .background(Hub.bg)
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack { dismiss() }
        .task {
            guard let t = store.target(nil) else { return }
            detail = try? await t.client.problem(id: id, host: t.host)
        }
    }

    private func archive() {
        guard let t = store.target(nil) else { return }
        Task {
            do { try await t.client.archiveProblem(id: id, host: t.host); store.flash("Archived"); await store.loadWatch(); dismiss() }
            catch { store.flash("Not archived: \(error.localizedDescription)") }
        }
    }

    private func fix() {
        guard !busy, let t = store.target(nil) else { return }
        busy = true
        Task {
            do {
                if let thread = try await t.client.fixProblem(id: id, host: t.host) { nav.push(.thread(thread)) }
                else { store.flash("No folder is known for this problem, so no thread was started.") }
            } catch { store.flash("Not started: \(error.localizedDescription)") }
            busy = false
        }
    }
}

// MARK: Routines

struct RoutinesView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let list = store.watchRoutines
        VStack(spacing: 0) {
            HubHeader(parent: "Settings", title: "Routines", sub: Self.summary(list), back: { dismiss() })
            ScrollView {
                VStack(spacing: 0) {
                    if list.isEmpty { Text("No routines on this machine.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(24) }
                    ForEach(list) { r in
                        HStack(spacing: 12) {
                            NavigationLink(value: Route.routine(r.id)) {
                                HStack(spacing: 10) {
                                    Circle().fill(Self.dot(r)).frame(width: 8, height: 8)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(r.name).font(.system(size: 15)).lineLimit(1)
                                        Text("\(r.when)\(r.runs?.first.map { " · ran \(Store.clock($0.at)), \($0.state)" } ?? "")").font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(1)
                                    }
                                    Spacer(minLength: 4)
                                }
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.press)
                            Toggle("", isOn: Binding(get: { r.enabled }, set: { set(r, $0) })).labelsHidden().tint(Hub.green)
                        }
                        .padding(.horizontal, 18).frame(minHeight: 60)
                        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
                    }
                    Text("Routines only read and report. Edit one on the PC.").font(.system(size: 12)).foregroundStyle(Hub.muted).padding(18)
                }
            }
            .refreshable { await store.loadWatch() }
        }
        .background(Hub.bg)
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack { dismiss() }
        .task { await store.loadWatch() }
    }

    static func dot(_ r: WatchRoutine) -> Color {
        guard r.enabled else { return Hub.lineStrong }
        switch r.runs?.first?.state { case "failed": return Hub.red; case "working": return Hub.green; default: return Hub.green }
    }

    static func summary(_ list: [WatchRoutine]) -> String {
        let on = list.filter(\.enabled)
        let next = on.compactMap(\.next_at).min()
        return "\(on.count) on" + (next.map { " · next \(Store.clock($0))" } ?? "")
    }

    private func set(_ r: WatchRoutine, _ on: Bool) {
        guard let t = store.target(nil) else { return }
        if let i = store.watchRoutines.firstIndex(where: { $0.id == r.id }) { store.watchRoutines[i].enabled = on }
        Task {
            do { try await t.client.setRoutine(r, enabled: on, host: t.host) }
            catch { store.flash("Not changed: \(error.localizedDescription)"); await store.loadWatch() }
        }
    }
}

struct RoutinePage: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @Environment(\.dismiss) private var dismiss
    let id: String
    @State private var running = false

    var body: some View {
        let r = store.watchRoutines.first { $0.id == id }
        VStack(spacing: 0) {
            HubHeader(parent: "Routines", title: r?.name ?? "Routine", sub: r?.when, back: { dismiss() })
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    GroupLabel(text: "Last runs")
                    VStack(spacing: 0) {
                        if (r?.runs ?? []).isEmpty { Text("No runs yet.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(14) }
                        ForEach(Array((r?.runs ?? []).enumerated()), id: \.offset) { _, run in
                            Button { if let t = run.thread { nav.push(.thread(t)) } } label: {
                                HStack(spacing: 11) {
                                    Circle().fill(run.state == "failed" ? Hub.red : run.state == "working" ? Hub.amber : Hub.green).frame(width: 8, height: 8)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(Self.when(run.at)).font(.system(size: 15))
                                        Text(run.state == "failed" ? "failed: \(String((run.report ?? "").prefix(60)))" : run.state == "working" ? "working" : (run.by == "you" ? "run by you" : "done"))
                                            .font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(1)
                                    }
                                    Spacer()
                                    if run.thread != nil { Image(systemName: "chevron.right").font(.system(size: 11)).foregroundStyle(Hub.lineStrong) }
                                }
                                .padding(.horizontal, 14).frame(minHeight: 56).contentShape(Rectangle())
                                .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 33) }
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface)).padding(.horizontal, 14)
                }
            }
            Button { run() } label: {
                Group { if running { Working(tint: Hub.bg, size: 18) } else { Label("Run now", systemImage: "play") } }
                    .font(.system(size: 16, weight: .semibold)).foregroundStyle(Hub.bg).frame(maxWidth: .infinity).frame(height: 52)
                    .background(Capsule().fill(Hub.accent))
            }
            .padding(.horizontal, 14).padding(.top, 6).padding(.bottom, 10)
        }
        .background(Hub.bg)
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack { dismiss() }
    }

    static func when(_ ms: Double) -> String {
        let f = DateFormatter(); f.dateFormat = "EEE d MMM, HH:mm"
        return f.string(from: Date(timeIntervalSince1970: ms / 1000))
    }

    private func run() {
        guard !running, let t = store.target(nil) else { return }
        running = true
        Task {
            do {
                if let thread = try await t.client.runRoutine(id: id, host: t.host) { nav.push(.thread(thread)) }
                await store.loadWatch()
            } catch { store.flash("Not run: \(error.localizedDescription)") }
            running = false
        }
    }
}
