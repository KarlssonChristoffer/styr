import SwiftUI

extension Store {
    /// Folders that are Hub's own or a thread's leftovers, not projects to start in: Hub's data
    /// folder, fix worktrees, widgets, dated scratch folders, folders named by a hash, and a folder
    /// that only holds other projects.
    func isInternal(_ p: ProjectRef) -> Bool {
        // Asked for every thread of the inbox on a redraw: the answer holds while the projects are the same.
        let all = projects
        let key = "\(projectsMemo?.key ?? 0)|\(p.id)"
        if let hit = internalMemo[key] { return hit }
        if internalMemo.count > 4000 { internalMemo = [:] }
        let answer = computeInternal(p, among: all)
        internalMemo[key] = answer
        return answer
    }

    private func computeInternal(_ p: ProjectRef, among projects: [ProjectRef]) -> Bool {
        let path = p.path.lowercased().replacingOccurrences(of: "\\", with: "/")
        let name = p.name.lowercased()
        if path.contains("/appdata/local/hub") || path.contains("/.local/share/hub") || path.contains("/application support/hub") { return true }
        // Hub's desk: a folder to look across projects from, never one to write code in. Matched by its
        // own name, wherever the machine keeps Hub's data (a machine run with another data folder, such as
        // tincan, has it outside .../hub/).
        let leaf = path.split(separator: "/").last.map(String.init) ?? ""
        if leaf == "desk" || leaf == "platschef" { return true }
        if path.contains("/.hub-") || path.contains("/fix/") || path.contains("/widgets/") { return true }
        if Store.scratch(p.name) || name.range(of: #"^[a-z]?[0-9a-f]{7,}$"#, options: .regularExpression) != nil { return true }
        let others = projects.filter { $0.id != p.id && $0.host == p.host }
        return others.contains { $0.path.lowercased().replacingOccurrences(of: "\\", with: "/").hasPrefix(path + "/") }
    }

    var pinnedProjects: Set<String> {
        get { Set(UserDefaults.standard.stringArray(forKey: "projects.pinned") ?? []) }
        set { UserDefaults.standard.set(Array(newValue), forKey: "projects.pinned") }
    }

    var hiddenProjects: Set<String> {
        get { Set(UserDefaults.standard.stringArray(forKey: "projects.hidden") ?? []) }
        set { UserDefaults.standard.set(Array(newValue), forKey: "projects.hidden") }
    }

    /// The projects worth starting in, recent first: pinned, then those with threads lately.
    func recentProjects(_ index: ThreadIndex) -> [ProjectRef] {
        let pinned = pinnedProjects, hidden = hiddenProjects
        let usable = projects.filter { !hidden.contains($0.id) && !isInternal($0) }
        let withThreads = usable.filter { !index.of($0).isEmpty }
        return (usable.filter { pinned.contains($0.id) } + withThreads.filter { !pinned.contains($0.id) }).prefix(4).map { $0 }
    }

}

/// Choose a project (sketch "Projektval" 1, 3, 4): recent ones as cards, the rest by machine,
/// Hub's own folders hidden, search that marks what matched, and a long press to pin or hide.
struct ProjectPicker: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    var picked: (ProjectRef) -> Void
    @State private var search = ""
    @State private var adding = false
    @State private var showHidden = false
    @State private var bump = 0
    @FocusState private var searching: Bool

    var body: some View {
        let _ = bump
        // The threads grouped by project once: a row asks for its own, not every thread.
        let index = store.threadIndex()
        VStack(spacing: 0) {
            HubHeader(parent: "New thread", title: "Choose project", back: { dismiss() }) {
                HeaderButton("plus", label: "Add", accessibility: "Add a project") { adding = true }
            }
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").foregroundStyle(Hub.muted)
                TextField("", text: $search, prompt: Text("Search projects").foregroundStyle(Hub.muted))
                    .autocorrectionDisabled().textInputAutocapitalization(.never).focused($searching)
                if !search.isEmpty { Button { search = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(Hub.muted) } }
            }
            .font(.system(size: 15)).padding(.horizontal, 12).frame(height: 38)
            .background(RoundedRectangle(cornerRadius: 10).fill(Hub.surface2)).padding(.horizontal, 14).padding(.bottom, 6)
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if search.isEmpty { browse(index) } else { results(index) }
                }
                .padding(.bottom, 30)
            }
            .scrollDismissesKeyboard(.interactively)
        }
        .background(Hub.bg)
        .swipeBack { dismiss() }
        .sheet(isPresented: $adding) { AddProjectSheet().presentationDetents([.medium, .large]).presentationDragIndicator(.visible) }
        .task { await store.loadProjects(); await store.loadFocusSummaries() }
    }

    // MARK: Browsing: recent cards, then by machine

    /// Start without a project (T3 Code's "No project"): a fresh scratch folder on the machine.
    @ViewBuilder private var noProject: some View {
        let names = store.hosts.isEmpty ? store.machines.map(\.name) : store.hosts.filter { $0.state == "online" }.map(\.name)
        GroupLabel(text: "No project")
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(names.isEmpty ? [store.machine ?? "This machine"] : names, id: \.self) { name in
                    Button { if TouchGuard.ok { choose(.noProject(on: name == store.machine ? nil : name)) } } label: {
                        HStack(spacing: 7) {
                            Image(systemName: "sparkles").font(.system(size: 13))
                            Text("On \(name)").font(.system(size: 13.5, weight: .medium))
                        }
                        .foregroundStyle(Hub.text2).padding(.horizontal, 12).frame(height: 36)
                        .background(RoundedRectangle(cornerRadius: 10).fill(Hub.surface))
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 14)
        }
        .swipeZone()
        Text("A fresh scratch folder on that machine, for trying something out.")
            .font(.system(size: 11.5)).foregroundStyle(Hub.muted).padding(.horizontal, 18).padding(.top, 4)
    }

    @ViewBuilder private func browse(_ index: ThreadIndex) -> some View {
        noProject
        let recent = store.recentProjects(index)
        let hidden = store.hiddenProjects
        let rest = store.projects.filter { p in !recent.contains(p) && (showHidden || (!hidden.contains(p.id) && !store.isInternal(p))) }
        let hiddenCount = store.projects.filter { hidden.contains($0.id) || store.isInternal($0) }.count
        if !recent.isEmpty {
            GroupLabel(text: "Recent")
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 9) { ForEach(recent) { card($0, index) } }.padding(.horizontal, 14)
            }
            .swipeZone()
        }
        let byMachine = Dictionary(grouping: rest, by: { $0.host ?? store.machine ?? "This machine" })
        ForEach(byMachine.keys.sorted { a, b in a == (store.machine ?? "") ? true : b == (store.machine ?? "") ? false : a < b }, id: \.self) { machine in
            GroupLabel(text: "On \(machine)")
            ForEach(byMachine[machine] ?? []) { row($0, highlight: nil, index) }
        }
        if hiddenCount > 0 {
            Button { withAnimation { showHidden.toggle() } } label: {
                (Text(showHidden ? "Showing \(hiddenCount) hidden folders. " : "\(hiddenCount) folders hidden: Hub's own, fix worktrees, widgets, scratch. ")
                    .foregroundColor(Hub.muted) + Text(showHidden ? "Hide" : "Show").foregroundColor(Hub.accent).bold())
                    .font(.system(size: 12)).multilineTextAlignment(.leading)
            }
            .buttonStyle(.plain).padding(.horizontal, 18).padding(.top, 14)
        }
    }

    // MARK: Searching

    @ViewBuilder private func results(_ index: ThreadIndex) -> some View {
        let q = search.lowercased()
        let list = store.projects.filter { $0.name.lowercased().contains(q) || $0.path.lowercased().contains(q) }
        GroupLabel(text: list.isEmpty ? "No matching projects" : "\(list.count) match\(list.count == 1 ? "" : "es")")
        ForEach(list) { row($0, highlight: q, index) }
        if list.isEmpty {
            Button { adding = true } label: { Label("Add a folder as a project", systemImage: "plus") }
                .font(.system(size: 14)).foregroundStyle(Hub.accent).padding(.horizontal, 18).padding(.top, 6)
        }
    }

    // MARK: Pieces

    private func choose(_ p: ProjectRef) { picked(p); Haptics.tap(); dismiss() }

    private func card(_ p: ProjectRef, _ index: ThreadIndex) -> some View {
        let s = store.summaries[p.id]
        let threads = index.of(p)
        return Button { if TouchGuard.ok { choose(p) } } label: {
            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 6) {
                    Image(systemName: "folder").foregroundStyle(Hub.projectColor(p.name))
                    Text(p.name).font(.system(size: 15, weight: .semibold)).lineLimit(1)
                    if store.pinnedProjects.contains(p.id) { Image(systemName: "pin.fill").font(.system(size: 9)).foregroundStyle(Hub.muted) }
                }
                Text([p.host ?? store.machine, index.lastActive(p).map(relative)].compactMap { $0 }.joined(separator: " · "))
                    .font(.system(size: 11.5)).foregroundStyle(Hub.muted)
                Text(s.flatMap { $0.stage.map { st in "Stage \(s!.n) · \(st)" } } ?? "\(threads.count) thread\(threads.count == 1 ? "" : "s")")
                    .font(.system(size: 11.5)).foregroundStyle(Hub.text3).lineLimit(2).frame(height: 30, alignment: .topLeading)
                ProgressBar(value: s.map { $0.of > 0 ? Double($0.done) / Double($0.of) : 0 } ?? 0)
                dots(threads)
            }
            .padding(11).frame(width: 150, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
        }
        .buttonStyle(.press)
        .contextMenu { menu(p) }
    }

    private func row(_ p: ProjectRef, highlight: String?, _ index: ThreadIndex) -> some View {
        let s = store.summaries[p.id]
        let threads = index.of(p)
        let sub: String = {
            if let s, let st = s.stage { return "Stage \(s.n) · \(st) · \(s.done) of \(s.of)" }
            if let last = index.lastActive(p) { return "\(threads.count) thread\(threads.count == 1 ? "" : "s") · \(relative(last))" }
            return highlight == nil ? "No threads yet" : (p.host ?? store.machine ?? "") + " · " + p.path
        }()
        return Button { if TouchGuard.ok { choose(p) } } label: {
            HStack(spacing: 11) {
                Image(systemName: "folder").font(.system(size: 17)).foregroundStyle(Hub.projectColor(p.name))
                VStack(alignment: .leading, spacing: 2) {
                    Text(marked(p.name, highlight)).font(.system(size: 15))
                    Text(sub).font(.system(size: 11.5)).foregroundStyle(Hub.muted).lineLimit(1).truncationMode(.middle)
                }
                Spacer(minLength: 6)
                dots(threads)
            }
            .padding(.horizontal, 16).padding(.vertical, 9).contentShape(Rectangle())
            .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 44) }
        }
        .buttonStyle(.pressFlat)
        .contextMenu { menu(p) }
    }

    @ViewBuilder private func menu(_ p: ProjectRef) -> some View {
        let pinned = store.pinnedProjects.contains(p.id)
        Button { var s = store.pinnedProjects; if pinned { s.remove(p.id) } else { s.insert(p.id) }; store.pinnedProjects = s; bump += 1 } label: {
            Label(pinned ? "Unpin from Recent" : "Pin to Recent", systemImage: "pin")
        }
        Button { dismiss(); DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { NotificationCenter.default.post(name: .openProject, object: p) } } label: {
            Label("Open the project page", systemImage: "arrow.up.right.square")
        }
        let hidden = store.hiddenProjects.contains(p.id)
        Button { var s = store.hiddenProjects; if hidden { s.remove(p.id) } else { s.insert(p.id) }; store.hiddenProjects = s; bump += 1 } label: {
            Label(hidden ? "Show in this list" : "Hide from this list", systemImage: hidden ? "eye" : "eye.slash")
        }
    }

    private func dots(_ threads: [HubThread]) -> some View {
        let w = threads.filter { $0.status == .waiting_on_you }.count
        let r = threads.filter { $0.status == .running }.count
        let e = threads.filter { $0.status == .error && $0.section != nil }.count
        return HStack(spacing: 6) {
            if w > 0 { HStack(spacing: 3) { StatusDot(state: .waiting); Text("\(w)") } }
            if r > 0 { HStack(spacing: 3) { StatusDot(state: .running); Text("\(r)") } }
            if e > 0 { HStack(spacing: 3) { StatusDot(state: .error); Text("\(e)") } }
        }
        .font(.system(size: 11.5)).foregroundStyle(Hub.muted)
    }

    private func marked(_ name: String, _ q: String?) -> AttributedString {
        var a = AttributedString(name)
        guard let q, !q.isEmpty, let r = name.lowercased().range(of: q) else { return a }
        let lower = name.distance(from: name.startIndex, to: r.lowerBound), len = q.count
        let start = a.index(a.startIndex, offsetByCharacters: lower), end = a.index(start, offsetByCharacters: len)
        a[start..<end].backgroundColor = Hub.accentBg
        a[start..<end].foregroundColor = Hub.text
        return a
    }
}

extension Notification.Name {
    static let openProject = Notification.Name("hub.openProject")
}
