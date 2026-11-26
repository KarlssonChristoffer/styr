// The machines screens (sketch 3): the list, a machine card and one machine.
import SwiftUI

struct MachinesView: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @State private var confirmPause = false

    var body: some View {
        NavigationStack(path: Bindable(nav).machines) {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    let list = rows
                    PageTitle(title: "Machines", sub: "\(list.filter(\.up).count) of \(list.count) online · \(store.threads.filter { $0.status == .running }.count) threads running") { EmptyView() }
                    let needs = list.filter { $0.needs }
                    if !needs.isEmpty {
                        GroupLabel(text: "Needs you", color: Hub.amber)
                        VStack(spacing: 10) { ForEach(needs) { MachineCard(row: $0) } }.padding(.horizontal, 14)
                    }
                    let good = list.filter { !$0.needs }
                    if !good.isEmpty {
                        GroupLabel(text: "All good")
                        VStack(spacing: 10) { ForEach(good) { MachineCard(row: $0) } }.padding(.horizontal, 14)
                    }
                    limits
                    panic
                }
                .padding(.bottom, 30)
            }
            .contentMargins(.bottom, 96, for: .scrollContent)
            .scrollsToTop(2)
            .refreshable { await load() }
            .background(Hub.bg)
            .toolbar(.hidden, for: .navigationBar)
            .navigationDestination(for: Route.self) { RouteView(route: $0) }
            .task { await load() }
        }
    }

    private func load() async {
        await store.loadMachines()
        await store.loadVitals()
    }

    struct Row: Identifiable {
        var id: String { name }
        var name: String
        var up: Bool
        var os: String
        var version: String?
        var error: String?
        var running: [HubThread]
        var waiting: Int
        var vitals: MachineVitals?
        /// Behind the others, down, or with a check that is not fine.
        var behind: Bool
        var needs: Bool { !up || waiting > 0 || behind || (vitals?.ready?.checks.contains { $0.state != "ok" } ?? false) }
    }

    private var rows: [Row] {
        let newest = store.hosts.compactMap(\.version).max()
        func mine(_ name: String) -> [HubThread] { store.threads.filter { ($0.host ?? store.machine) == name } }
        if !store.hosts.isEmpty {
            return store.hosts.map { h in
                let t = mine(h.name)
                return Row(name: h.name, up: h.state == "online", os: store.vitals[h.name]?.snapshot?.os ?? store.vitals[h.name]?.about?.os ?? "",
                           version: h.version, error: h.error, running: t.filter { $0.status == .running }, waiting: t.filter { $0.status == .waiting_on_you }.count,
                           vitals: store.vitals[h.name], behind: h.version != nil && newest != nil && h.version != newest)
            }
        }
        let names = store.isDemo ? Array(Set(store.threads.compactMap(\.host))).sorted() : store.machines.map(\.name)
        return names.map { name in
            let t = mine(name)
            let up = store.isDemo || store.machines.first { $0.name == name }.map { store.live[$0.url] == true } ?? false
            return Row(name: name, up: up, os: "", version: nil, error: nil, running: t.filter { $0.status == .running },
                       waiting: t.filter { $0.status == .waiting_on_you }.count, vitals: store.vitals[name], behind: false)
        }
    }

    @ViewBuilder private var limits: some View {
        let plans: [(String, UsageReport.Plan?)] = (store.usage.map { u in [("Claude", u.plan)] + u.others.map { ($0.label, $0.plan) } }) ?? []
        if plans.contains(where: { !($0.1?.limits.isEmpty ?? true) }) {
            GroupLabel(text: "Plans")
            VStack(spacing: 0) {
                ForEach(Array(plans.enumerated()), id: \.offset) { _, entry in
                    ForEach(entry.1?.limits ?? [], id: \.self) { limit in LimitRow(name: entry.0, limit: limit) }
                }
            }
            .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
            .padding(.horizontal, 14)
        }
    }

    private var panic: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                if store.halt != nil { Task { await store.setPaused(false) } } else { confirmPause = true }
            } label: {
                Label(store.halt != nil ? "Let the agents carry on" : "Pause every agent, everywhere", systemImage: store.halt != nil ? "play.fill" : "pause.fill")
                    .font(.system(size: 15, weight: .semibold))
                    .frame(maxWidth: .infinity).frame(height: 48)
                    .foregroundStyle(store.halt != nil ? Hub.bg : Hub.red)
                    .background(RoundedRectangle(cornerRadius: 13).fill(store.halt != nil ? Hub.green : Hub.redBg))
            }
            if let h = store.halt {
                Text("Paused since \(Store.clock(h.since)), from \(h.by).").font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.horizontal, 16)
            }
        }
        .padding(.horizontal, 14).padding(.top, 26)
        .hubConfirm("Pause every agent?", message: "Every agent on every machine stops where it is. Nothing is lost, and you start them again from here.", confirm: "Pause", hold: true, destructive: true, isPresented: $confirmPause) { Task { await store.setPaused(true) } }
    }
}

/// A machine's card (sketch 3, screen 1): name and system, what runs, the rings, at most three
/// buttons with the most important one filled.
struct MachineCard: View {
    @Environment(Nav.self) private var nav
    let row: MachinesView.Row

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Image(systemName: icon).foregroundStyle(Hub.text3)
                Text(row.name).font(.system(size: 16, weight: .semibold))
                if !row.os.isEmpty { Text(row.os).font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(1) }
                Spacer()
                if !row.up {
                    pill("Offline", Hub.red, Hub.redBg)
                } else if row.behind {
                    pill("Update", Hub.amber, Hub.amberBg)
                } else if !row.running.isEmpty {
                    pill("\(row.running.count) running", Hub.green, Hub.greenBg)
                }
            }
            Text(line).font(.system(size: 13)).foregroundStyle(row.up ? Hub.text3 : Hub.red).lineLimit(2)
            if row.up, let v = row.vitals {
                HStack(spacing: 14) {
                    if let x = v.cpu { RingStat(label: "CPU", value: x) }
                    if let x = v.ram { RingStat(label: "RAM", value: x) }
                    if let x = v.disk { RingStat(label: "Disk", value: x) }
                    if let x = v.temp { RingStat(label: "Temp", value: x, unit: "°", warnAt: 75, badAt: 90) }
                }
            }
            HStack(spacing: 8) {
                if row.up {
                    Button("New thread") { nav.machines.append(.new(nil, talk: false)) }.buttonStyle(PillButtonStyle(kind: row.waiting > 0 ? .plain : .primary))
                }
                Button("Open") { nav.machines.append(.machine(row.name)) }.buttonStyle(PillButtonStyle())
            }
        }
        .padding(14)
        .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
        .contentShape(Rectangle())
        .tapRow { nav.machines.append(.machine(row.name)) }
    }

    private var icon: String {
        let os = row.os.lowercased()
        if os.contains("mac") { return "laptopcomputer" }
        if os.contains("windows") { return "desktopcomputer" }
        return "server.rack"
    }

    private var line: String {
        if !row.up { return row.error ?? "Not reached" }
        if row.waiting > 0 { return row.waiting == 1 ? "1 thread waits for you" : "\(row.waiting) threads wait for you" }
        if !row.running.isEmpty { return row.running.prefix(2).map(\.title).joined(separator: " · ") }
        return row.version.map { "hubd \($0.prefix(7)) · idle" } ?? "Idle"
    }

    private func pill(_ text: String, _ fg: Color, _ bg: Color) -> some View {
        Text(text).font(.system(size: 11.5, weight: .semibold)).foregroundStyle(fg)
            .padding(.horizontal, 8).padding(.vertical, 2).background(Capsule().fill(bg))
    }
}

/// A ring with its number inside and a label under it, coloured by the same limits as the PC.
struct RingStat: View {
    let label: String
    let value: Double
    var unit = "%"
    var warnAt: Double = 70
    var badAt: Double = 90

    var body: some View {
        VStack(spacing: 4) {
            ZStack {
                Circle().stroke(Hub.lineStrong, lineWidth: 3.5)
                Circle().trim(from: 0, to: min(1, value / 100)).stroke(color, style: StrokeStyle(lineWidth: 3.5, lineCap: .round)).rotationEffect(.degrees(-90))
                Text("\(Int(value.rounded()))\(unit)").font(.system(size: 11, weight: .semibold))
            }
            .frame(width: 42, height: 42)
            Text(label).font(.system(size: 10.5)).foregroundStyle(Hub.muted)
        }
    }

    private var color: Color { value >= badAt ? Hub.red : value >= warnAt ? Hub.amber : Hub.green }
}

/// The colour of a plan limit: calm, then amber, then red.
enum Ring {
    static func color(_ p: Double) -> Color { p >= 90 ? Hub.red : p >= 70 ? Hub.amber : Hub.green }
}

/// One machine (sketch 3, screens 2 and 3): its rings, Ready for work, its projects, what runs there.
struct MachineView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    let name: String
    @State private var tab = 0

    var body: some View {
        let v = store.vitals[name]
        let host = store.hosts.first { $0.name == name }
        let threads = store.threads.filter { ($0.host ?? store.machine) == name }
        let running = threads.filter { $0.status == .running || $0.status == .waiting_on_you }
        VStack(spacing: 0) {
            HubHeader(parent: "Machines", title: name,
                      sub: [host?.state == "offline" ? "Offline" : "Online",
                            v?.about.map { "up \(Self.days($0.uptime_secs))" }].compactMap { $0 }.joined(separator: " · "),
                      back: { dismiss() }) {
                HubMenu {
                    Button { Task { await store.loadVitals() } } label: { Label("Refresh", systemImage: "arrow.clockwise") }
                } label: { HeaderFace(icon: "ellipsis") }
                .accessibilityLabel("More")
            }
            let ports = (v?.snapshot?.listeners ?? []).filter { !$0.system }
            Tabs(selection: $tab, items: ["Overview", "Threads \(running.count)", "Ports \(ports.count)", "Run"]).padding(.horizontal, 14).padding(.vertical, 12)
            ModePager(selection: $tab, count: 4) { i in
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        if i == 2 {
                            VStack(spacing: 0) {
                                if ports.isEmpty { Text("Nothing listens here.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(13) }
                                ForEach(ports.sorted { $0.port < $1.port }, id: \.self) { l in
                                    HStack(spacing: 11) {
                                        Text(":\(l.port)").font(.system(size: 14, design: .monospaced))
                                        Text(l.is_hub ? "hubd" : (l.process ?? "?")).font(.system(size: 14.5))
                                        Spacer()
                                        if let id = l.thread_id, let t = store.threads.first(where: { $0.id == id }) {
                                            NavigationLink(value: Route.thread(id)) { Text(t.title).font(.system(size: 12)).foregroundStyle(Hub.accent).lineLimit(1) }
                                        }
                                    }
                                    .padding(.horizontal, 13).frame(minHeight: 44)
                                    .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
                                }
                            }
                            .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface)).padding(.horizontal, 14)
                            Text("Open a port in the browser: http://\(name):port on your tailnet.").font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.horizontal, 30).padding(.top, 6)
                        } else if i == 3 {
                            GroupLabel(text: "Quick commands")
                            VStack(spacing: 0) {
                                runRow("Update hubd", sub: host?.version.map { "now \($0.prefix(7))" } ?? "", icon: "arrow.down.circle") {
                                    try await store.homeClient?.updateHubd(machine: name)
                                }
                                runRow("Reconnect", sub: "if the machine shows as offline", icon: "arrow.clockwise") {
                                    try await store.homeClient?.reconnect(machine: name)
                                }
                            }
                            .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface)).padding(.horizontal, 14)
                            Text("Commands of your own stay on the PC: the phone may not run a shell.").font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.horizontal, 30).padding(.top, 6)
                        } else if i == 0 {
                            if let v {
                                HStack(spacing: 16) {
                                    if let x = v.cpu { RingStat(label: "CPU", value: x) }
                                    if let x = v.ram { RingStat(label: "RAM", value: x) }
                                    if let x = v.disk { RingStat(label: "Disk", value: x) }
                                    if let x = v.temp { RingStat(label: "Temp", value: x, unit: "°", warnAt: 75, badAt: 90) }
                                }
                                .padding(.horizontal, 18).padding(.bottom, 6)
                            }
                            if let checks = v?.ready?.checks, !checks.isEmpty {
                                GroupLabel(text: "Ready for work")
                                VStack(spacing: 0) {
                                    ForEach(checks, id: \.key) { c in
                                        HStack(spacing: 11) {
                                            Circle().fill(c.state == "ok" ? Hub.green : c.state == "warn" ? Hub.amber : Hub.red).frame(width: 8, height: 8)
                                            VStack(alignment: .leading, spacing: 1) {
                                                Text(c.label).font(.system(size: 14.5))
                                                if !c.detail.isEmpty { Text(c.detail).font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(2) }
                                            }
                                            Spacer()
                                            if c.state != "ok" { Text("Fix on the PC").font(.system(size: 12)).foregroundStyle(Hub.muted) }
                                        }
                                        .padding(.horizontal, 13).padding(.vertical, 10)
                                        .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 32) }
                                    }
                                }
                                .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
                                .padding(.horizontal, 14)
                            } else if store.tooOld.contains("hosts") || v == nil {
                                Text("Its rings and checks need a newer hubd.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.horizontal, 18)
                            }
                            let projects = store.projects.filter { ($0.host ?? store.machine) == name }
                            if !projects.isEmpty {
                                GroupLabel(text: "Projects here")
                                VStack(spacing: 0) {
                                    ForEach(projects) { p in
                                        NavigationLink(value: Route.project(p)) {
                                            HStack {
                                                Image(systemName: "folder").foregroundStyle(Hub.projectColor(p.name))
                                                Text(p.name).font(.system(size: 14.5))
                                                Spacer()
                                                Text("\(store.threads(of: p).count) threads").font(.system(size: 12)).foregroundStyle(Hub.muted)
                                                Image(systemName: "chevron.right").font(.system(size: 11)).foregroundStyle(Hub.muted)
                                            }
                                            .padding(.horizontal, 13).frame(minHeight: 46)
                                            .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 36) }
                                            .contentShape(Rectangle())
                                        }
                                        .buttonStyle(.pressFlat)
                                    }
                                }
                                .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
                                .padding(.horizontal, 14)
                            }
                        } else {
                            if running.isEmpty { Text("Nothing runs here now.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.horizontal, 18) }
                            VStack(spacing: 10) {
                                ForEach(running) { t in
                                    VStack(alignment: .leading, spacing: 6) {
                                        HStack(spacing: 8) {
                                            StatusDot(state: t.dot)
                                            Text(t.title).font(.system(size: 15, weight: .medium)).lineLimit(1)
                                            Spacer()
                                            Text(relative(t.sent_at ?? t.updated_at)).font(.system(size: 12)).foregroundStyle(Hub.muted)
                                        }
                                        Text("\(t.project) · \(t.engine.capitalized)").font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.leading, 16)
                                        HStack(spacing: 8) {
                                            if t.status == .running { Button { Task { await store.stop(t) } } label: { Label("Stop", systemImage: "stop") }.buttonStyle(PillButtonStyle()) }
                                            NavigationLink(value: Route.thread(t.id)) { Text("Open") }.buttonStyle(PillButtonStyle())
                                        }
                                        .padding(.leading, 16)
                                    }
                                    .padding(13)
                                    .background(RoundedRectangle(cornerRadius: 13).fill(Hub.surface))
                                }
                            }
                            .padding(.horizontal, 14)
                        }
                    }
                    .padding(.bottom, 30)
                }
            }
        }
        .background(Hub.bg)
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack(enabled: tab == 0) { dismiss() }
        .task { await store.loadVitals() }
        .refreshable { await store.loadVitals() }
    }

    private func runRow(_ title: String, sub: String, icon: String, _ run: @escaping () async throws -> Void) -> some View {
        Button {
            Task {
                do { try await run(); Haptics.tap(); store.flash("\(title): started on \(name)") }
                catch { store.flash("\(title) failed: \(error.localizedDescription)") }
                await store.loadMachines()
            }
        } label: {
            HStack(spacing: 12) {
                Image(systemName: icon).foregroundStyle(Hub.muted).frame(width: 20)
                VStack(alignment: .leading, spacing: 2) { Text(title).font(.system(size: 15)); if !sub.isEmpty { Text(sub).font(.system(size: 12)).foregroundStyle(Hub.muted) } }
                Spacer()
            }
            .padding(.horizontal, 13).frame(minHeight: 50).contentShape(Rectangle())
            .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 45) }
        }
        .buttonStyle(.plain)
    }

    static func days(_ secs: Double) -> String {
        let d = Int(secs / 86400)
        if d >= 1 { return "\(d) day\(d == 1 ? "" : "s")" }
        return "\(Int(secs / 3600)) h"
    }
}
