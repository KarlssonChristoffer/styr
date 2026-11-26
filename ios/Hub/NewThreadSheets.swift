import SwiftUI

// Sheet "Ny tråd, som i T3 Code" (screens 3 and 4): the project as a word in the title opens a short
// list, one line per project with the machines it lives on at the right; the machine is a quiet
// line under the title with a state dot and opens "Run it on". Both are compact: one-line rows.

/// The card the short sheets sit in, above the reply box.
struct SheetCard<C: View>: View {
    let title: String
    var trailing: String?
    var trailingAction: () -> Void = {}
    let close: () -> Void
    @ViewBuilder var content: () -> C
    @State private var atTop = true

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(title).font(.system(size: 14, weight: .semibold))
                Spacer()
                if let trailing { Button(trailing, action: trailingAction).font(.system(size: 14, weight: .semibold)).foregroundStyle(Hub.accent) }
            }
            .padding(.horizontal, 14).padding(.top, 9).padding(.bottom, 6)
            ScrollView { content().padding(.horizontal, 8).padding(.bottom, 8).background(NoBounce()) }
                .trackTop($atTop)
                .frame(maxHeight: 290)
                .fixedSize(horizontal: false, vertical: true)
        }
        .background(RoundedRectangle(cornerRadius: 18).fill(Hub.side))
        .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(Hub.lineStrong))
        // A deliberate pull down closes it, like a tap on the dimmed page outside (Bow.swift).
        .bowDismiss(atTop: atTop, close: close)
    }
}

/// The machines a new thread could run on, with how each one stands.
struct MachineChoice: Identifiable {
    var name: String
    var online: Bool
    var note: String
    /// The chosen project is on it (or there is none, so any machine will do).
    var has: Bool
    var id: String { name }
}

extension Store {
    func machineChoices(for place: ProjectRef?) -> [MachineChoice] {
        let names = hosts.isEmpty ? machines.map(\.name) : hosts.map(\.name)
        return names.map { name in
            let up = hosts.isEmpty ? (machines.first { $0.name == name }.map { live[$0.url] == true } ?? true) : hosts.first { $0.name == name }?.state == "online"
            let has = place.map { p in p.isScratch || projects.contains { $0.name == p.name && ($0.host ?? machine) == name } } ?? true
            let note = !up ? "offline" : !has ? "no \(place?.name ?? "project") there" : ""
            return MachineChoice(name: name, online: up, note: note, has: has)
        }
    }

    /// The same project on another machine, or no project on it.
    func place(_ p: ProjectRef?, on machineName: String) -> ProjectRef? {
        guard let p else { return nil }
        if p.isScratch { return .noProject(on: machineName == machine ? nil : machineName) }
        return projects.first { $0.name == p.name && ($0.host ?? machine) == machineName }
    }
}

/// What `POST /api/phone/bring` made: the project's folder on the machine it was brought to.
struct BroughtProject: Decodable { var folder: String; var files: Int?; var dirty: Bool?; var via: String? }

extension HubClient {
    /// Brings a project (its repo) from one machine to another (`phone.bring`); `from` and `to` are
    /// machine names, nil for the machine this phone talks to.
    func bring(from: String?, path: String, to: String?) async throws -> BroughtProject {
        let body: [String: Any] = ["from": from ?? NSNull(), "path": path, "to": to ?? NSNull()]
        return try JSONDecoder().decode(BroughtProject.self, from: try await call("POST", "api/phone/bring", body: body))
    }
}

/// "Run it on": four plain lines, the state a dot and a word at the right, the reason for a grey one
/// in the same place. A machine that lacks the project is not grey: "Bring <project> here" copies it
/// there, and the thread starts there as usual.
struct RunOnSheet: View {
    @Environment(Store.self) private var store
    @Binding var place: ProjectRef?
    /// The cloud entry: on for one thread, ended by picking a machine. `cloudName` is whose cloud the
    /// model has; nil hides the entry (no cloud for this model, no project, or a hubd without it).
    @Binding var cloud: Bool
    var cloudName: String?
    let close: () -> Void
    @State private var busy: String?
    @State private var failed: [String: String] = [:]

    private var canBring: Bool { !store.isDemo && store.has("phone.bring") }

    var body: some View {
        let here = cloud ? nil : (place?.host ?? store.machine)
        SheetCard(title: "Run it on", close: close) {
            VStack(spacing: 0) {
                ForEach(store.machineChoices(for: place)) { m in
                    let bring = m.online && !m.has && place?.isScratch == false
                    let enabled = m.online && (m.has || canBring)
                    Button { if TouchGuard.ok { tap(m, bring: bring) } } label: {
                        HStack(spacing: 10) {
                            Image(systemName: MachineIcon.name(m.name, store: store)).font(.system(size: 14)).foregroundStyle(Hub.muted).frame(width: 22)
                            Text(m.name).font(.system(size: 14.5)).lineLimit(1)
                            Spacer(minLength: 6)
                            if busy == m.name { Working(size: 14) }
                            else {
                                Circle().fill(m.online ? (m.has ? Hub.green : Hub.amber) : Hub.red).frame(width: 7, height: 7)
                                Text(note(m, bring: bring)).font(.system(size: 12)).foregroundStyle(noteColor(m, bring: bring)).lineLimit(2).multilineTextAlignment(.trailing)
                            }
                            if m.name == here && busy != m.name { Image(systemName: "checkmark").font(.system(size: 12, weight: .bold)).foregroundStyle(Hub.accent) }
                        }
                        .padding(.horizontal, 10).frame(minHeight: 38)
                        .background(m.name == here ? Hub.accentBg : .clear).clipShape(RoundedRectangle(cornerRadius: 8))
                        .opacity(enabled || m.name == here ? 1 : 0.5)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.pressFlat).disabled((!enabled && m.name != here) || busy != nil)
                }
                if let cloudName {
                    Button { if TouchGuard.ok { cloud = true; Haptics.tap(); close() } } label: {
                        HStack(spacing: 10) {
                            Image(systemName: "icloud.and.arrow.up").font(.system(size: 14)).foregroundStyle(Hub.muted).frame(width: 22)
                            Text("Cloud").font(.system(size: 14.5))
                            Spacer(minLength: 6)
                            Text("\(cloudName) on its own servers").font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(2).multilineTextAlignment(.trailing)
                            if cloud { Image(systemName: "checkmark").font(.system(size: 12, weight: .bold)).foregroundStyle(Hub.accent) }
                        }
                        .padding(.horizontal, 10).frame(minHeight: 38)
                        .background(cloud ? Hub.accentBg : .clear).clipShape(RoundedRectangle(cornerRadius: 8))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.pressFlat).disabled(busy != nil)
                }
                Text("Machines with this project. Bring it to another and the thread starts there; grey ones say why. Swipe sideways on the machine line to cycle the online ones.")
                    .font(.system(size: 11.5)).foregroundStyle(Hub.muted).padding(.horizontal, 10).padding(.top, 8)
            }
        }
        .task { if store.hosts.isEmpty { await store.loadMachines() } }
    }

    private func note(_ m: MachineChoice, bring: Bool) -> String {
        if let e = failed[m.name] { return e }
        if bring { return canBring ? "Bring \(place?.name ?? "it") here" : "needs the next version" }
        return m.note.isEmpty ? "online" : m.note
    }

    private func noteColor(_ m: MachineChoice, bring: Bool) -> Color {
        if failed[m.name] != nil { return Hub.red }
        if bring && canBring { return Hub.accent }
        return m.note.isEmpty ? Hub.muted : Hub.amber
    }

    private func tap(_ m: MachineChoice, bring: Bool) {
        failed[m.name] = nil
        guard bring, canBring, let p = place else {
            if let p = store.place(place, on: m.name) { place = p; cloud = false; Haptics.tap(); close() }
            return
        }
        guard let home = store.target(nil) else { return }
        let target: String? = m.name == store.machine ? nil : m.name
        busy = m.name
        Task {
            do {
                // Send the project's folder as its machine lists it: the listed project whose folder holds this
                // one (a thread's folder may be a subfolder), else the folder as it is.
                let listed = (try? await home.client.projects(host: p.host)) ?? []
                let path = listed.map(\.path).filter { p.path == $0 || p.path.hasPrefix($0 + "/") || p.path.hasPrefix($0 + "\\") }.max { $0.count < $1.count } ?? p.path
                let done = try await home.client.bring(from: p.host, path: path, to: target)
                place = ProjectRef(path: done.folder, name: p.name, host: target)
                cloud = false
                await store.loadProjects()
                Haptics.tap()
                busy = nil
                close()
            } catch {
                failed[m.name] = error.localizedDescription
                busy = nil
            }
        }
    }
}

/// The project sheet in the layout of the full picker (More…), smaller: a search, Recent first with
/// what waits on each, then the rest grouped by machine (Hub's own folders and hidden ones left out),
/// and No project and Add at the foot. A hold on a row pins or hides it, as there.
struct ProjectSheet: View {
    @Environment(Store.self) private var store
    @Binding var place: ProjectRef?
    var picked: (ProjectRef) -> Void
    let close: () -> Void
    @State private var query = ""
    @State private var adding = false
    @State private var bump = 0
    @State private var atTop = true

    private var visible: [ProjectRef] {
        let hidden = store.hiddenProjects
        return store.projects.filter { !hidden.contains($0.id) && !store.isInternal($0) }
    }

    var body: some View {
        let _ = bump
        // The threads grouped by project once: a row asks for its own, not every thread.
        let index = store.threadIndex()
        VStack(spacing: 0) {
            Capsule().fill(Hub.lineStrong).frame(width: 36, height: 4).padding(.top, 7)
            Text("Project").font(.system(size: 14, weight: .semibold)).padding(.top, 6).padding(.bottom, 8)
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").foregroundStyle(Hub.muted)
                TextField("", text: $query, prompt: Text("Find a project").foregroundStyle(Hub.muted)).autocorrectionDisabled().textInputAutocapitalization(.never)
                if !query.isEmpty { Button { query = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(Hub.muted) }.buttonStyle(.plain) }
            }
            .font(.system(size: 14.5)).padding(.horizontal, 12).frame(height: 36)
            .background(RoundedRectangle(cornerRadius: 10).fill(Hub.surface)).padding(.horizontal, 12).padding(.bottom, 4)
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if query.trimmingCharacters(in: .whitespaces).isEmpty { browse(index) } else { results(index) }
                }
                .padding(.bottom, 6)
                .background(NoBounce())
            }
            .trackTop($atTop)
            .frame(maxHeight: UIScreen.main.bounds.height * 0.42)
            .fixedSize(horizontal: false, vertical: true)
            Hub.line.frame(height: 1)
            HStack {
                Button { place = .noProject(on: place?.host); Haptics.tap(); close() } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "tray").font(.system(size: 15)).foregroundStyle(Hub.muted).frame(width: 22)
                        Text("No project").font(.system(size: 14.5))
                        if place?.isScratch == true { Image(systemName: "checkmark").font(.system(size: 12, weight: .bold)).foregroundStyle(Hub.accent) }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                Spacer()
                Button { adding = true } label: {
                    HStack(spacing: 5) { Image(systemName: "plus").font(.system(size: 12, weight: .semibold)); Text("Add").font(.system(size: 14, weight: .medium)) }
                        .foregroundStyle(Hub.accent).contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 16).frame(height: 46)
        }
        .background(RoundedRectangle(cornerRadius: 18).fill(Hub.side))
        .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(Hub.lineStrong))
        .bowDismiss(atTop: atTop, close: close)
        .sheet(isPresented: $adding) { AddProjectSheet().presentationDetents([.medium, .large]).presentationDragIndicator(.visible) }
        .task { await store.loadProjects(); await store.loadFocusSummaries() }
    }

    // MARK: Browsing: Recent first, then by machine

    @ViewBuilder private func browse(_ index: ThreadIndex) -> some View {
        let all = visible
        let recent = store.recentProjects(index).filter { r in all.contains(r) }.prefix(4)
        let rest = all.filter { p in !recent.contains(p) }
        if !recent.isEmpty {
            label("Recent")
            ForEach(Array(recent)) { row($0, index) }
        }
        let byMachine = Dictionary(grouping: rest, by: { $0.host ?? store.machine ?? "This machine" })
        ForEach(byMachine.keys.sorted { a, b in a == (store.machine ?? "") ? true : b == (store.machine ?? "") ? false : a < b }, id: \.self) { machine in
            label("On \(machine)")
            ForEach(byMachine[machine] ?? []) { row($0, index) }
        }
    }

    @ViewBuilder private func results(_ index: ThreadIndex) -> some View {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        let hits = visible.filter { $0.name.lowercased().contains(q) || $0.path.lowercased().contains(q) }
        if hits.isEmpty { Text("No project called that.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(14) }
        ForEach(hits) { row($0, index) }
    }

    private func label(_ text: String) -> some View {
        Text(text.uppercased()).font(.system(size: 11, weight: .semibold)).foregroundStyle(Hub.muted)
            .padding(.horizontal, 16).padding(.top, 10).padding(.bottom, 3).frame(maxWidth: .infinity, alignment: .leading)
    }

    private func row(_ p: ProjectRef, _ index: ThreadIndex) -> some View {
        let threads = index.of(p)
        let s = store.summaries[p.id]
        let sub: String = {
            if let s, let st = s.stage { return "Stage \(s.n) · \(st)" }
            if let last = index.lastActive(p) { return "\(p.host ?? store.machine ?? "") · \(threads.count) thread\(threads.count == 1 ? "" : "s") · \(relative(last))" }
            return p.host ?? store.machine ?? ""
        }()
        let on = place?.path == p.path && place?.host == p.host && place?.isScratch == false
        let w = threads.filter { $0.status == .waiting_on_you }.count
        let r = threads.filter { $0.status == .running }.count
        return Button { place = p; picked(p); Haptics.tap(); close() } label: {
            HStack(spacing: 11) {
                Image(systemName: "folder").font(.system(size: 16)).foregroundStyle(Hub.projectColor(p.name)).frame(width: 22)
                VStack(alignment: .leading, spacing: 1) {
                    Text(p.name).font(.system(size: 14.5)).lineLimit(1)
                    Text(sub).font(.system(size: 11.5)).foregroundStyle(Hub.muted).lineLimit(1)
                }
                Spacer(minLength: 6)
                HStack(spacing: 6) {
                    if w > 0 { StatusDot(state: .waiting) }
                    if r > 0 { StatusDot(state: .running) }
                }
                if on { Image(systemName: "checkmark").font(.system(size: 12, weight: .bold)).foregroundStyle(Hub.accent) }
            }
            .padding(.horizontal, 16).frame(minHeight: 46)
            .background(on ? Hub.accentBg : .clear)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .contextMenu {
            let pinned = store.pinnedProjects.contains(p.id)
            Button { var set = store.pinnedProjects; if pinned { set.remove(p.id) } else { set.insert(p.id) }; store.pinnedProjects = set; bump += 1 } label: {
                Label(pinned ? "Unpin from Recent" : "Pin to Recent", systemImage: "pin")
            }
            Button { var set = store.hiddenProjects; set.insert(p.id); store.hiddenProjects = set; bump += 1 } label: {
                Label("Hide from this list", systemImage: "eye.slash")
            }
        }
    }
}
