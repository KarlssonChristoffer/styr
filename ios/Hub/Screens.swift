import SwiftUI

/// Where any tab's stack can go, drawn the same from every tab.
struct RouteView: View {
    @Environment(Nav.self) private var nav
    @Environment(Store.self) private var store
    let route: Route

    var body: some View {
        switch route {
        // One view per thread: the next-thread pull and the triage swipe swap the top of the stack in
        // place, and without its own identity SwiftUI kept the old page with a new id, so the new
        // thread's log was never opened and the page stayed empty.
        case let .thread(id): ThreadView(threadId: id).id(id)
        case let .new(project, talk): NewThreadView(project: project, talk: talk) { id in if nav.world == .chat { store.markChat(id) }; nav.replaceTop(.thread(id)) }
        case .shelf: ShelfView()
        case let .project(p): ProjectView(project: p)
        case let .machine(name): MachineView(name: name)
        case let .changes(id): ChangesRoute(id: id)
        case let .pull(id): PullRoute(id: id)
        case let .settings(page): SettingsPageView(page: page)
        case let .file(p, rel): FileView(project: p, rel: rel)
        case .problems: ProblemsView()
        case let .problem(id): ProblemPage(id: id)
        case .routines: RoutinesView()
        case let .routine(id): RoutinePage(id: id)
        case let .space(id): SpacePage(id: id)
        case .newChat: NewChatView { id in store.markChat(id); nav.replaceTop(.thread(id)) }
        }
    }

}

/// The changes page for a thread by id, once the thread is known.
struct ChangesRoute: View {
    @Environment(Store.self) private var store
    let id: String
    var body: some View {
        if let t = store.thread(id) { ChangesView(thread: t) } else { ThreadGone(parent: "Thread", title: "Changes") }
    }
}

/// What a pushed page of a thread shows when the thread is not there (any more): Hub's own top with a way
/// back, never the system's bar.
struct ThreadGone: View {
    @Environment(\.dismiss) private var dismiss
    let parent: String
    let title: String

    var body: some View {
        VStack(spacing: 0) {
            HubHeader(parent: parent, title: title, back: { dismiss() })
            Spacer()
            Text("This thread is gone.").font(.system(size: 13)).foregroundStyle(Hub.muted)
            Spacer()
        }
        .background(Hub.bg)
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack { dismiss() }
    }
}

/// A big page title with an optional button on its right, as each tab's start has (`PageHead`).
struct PageTitle<Trailing: View>: View {
    let title: String
    var sub: String?
    @ViewBuilder var trailing: () -> Trailing

    var body: some View {
        PageHead(title: title, reserveSub: sub != nil, sub: {
            Text(sub ?? "").font(.system(size: 12.5)).foregroundStyle(Hub.muted).lineLimit(1)
        }, trailing: trailing)
    }
}

struct GroupLabel: View {
    let text: String
    var color: Color = Hub.muted
    var body: some View {
        Text(text).font(.system(size: 12, weight: .semibold)).foregroundStyle(color)
            .padding(.horizontal, 30).padding(.top, 16).padding(.bottom, 6)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// MARK: Projects (sketch 5, screens 1 and 2)

struct ProjectsView: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @State private var adding = false

    var body: some View {
        NavigationStack(path: Bindable(nav).projects) {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    PageTitle(title: "Projects", sub: "\(store.projects.count) project\(store.projects.count == 1 ? "" : "s")") {
                        Button { adding = true } label: { Image(systemName: "plus") }
                    }
                    let index = store.threadIndex()
                    let focus = nav.focus.flatMap { f in store.projects.first { $0.id == f.id } ?? f }
                    if let f = focus {
                        FocusHero(project: f, threads: index.of(f)).safeTap { nav.projects.append(.project(f)) }
                            .padding(.horizontal, 14).padding(.top, 4)
                    }
                    let groups = grouped(index).map { ($0.0, $0.1.filter { $0.id != focus?.id }) }.filter { !$0.1.isEmpty }
                    ForEach(groups, id: \.0) { title, list in
                        GroupLabel(text: focus != nil && groups.count == 1 ? "Others" : title)
                        VStack(spacing: 0) {
                            ForEach(list) { p in ProjectRow(project: p, threads: index.of(p)) }
                        }
                    }
                    if store.projects.isEmpty {
                        Text("Projects show up here once Hub has threads.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18)
                    }
                }
                .padding(.bottom, 30)
            }
            .contentMargins(.bottom, 96 + (quickThreads.isEmpty ? 0 : 52), for: .scrollContent)
            .scrollsToTop(1)
            .overlay(alignment: .bottom) { if !quickThreads.isEmpty { quickRow.padding(.bottom, 84) } }
            .refreshable { await store.loadProjects(); await store.loadFocusSummaries() }
            .background(Hub.bg)
            .toolbar(.hidden, for: .navigationBar)
            .navigationDestination(for: Route.self) { RouteView(route: $0) }
            .task { await store.loadProjects(); await store.loadFocusSummaries() }
            .sheet(isPresented: $adding) { AddProjectSheet().presentationDetents([.medium, .large]).presentationDragIndicator(.visible) }
        }
    }

    /// Threads in a scratch folder, on any machine: kept out of the project list, here as one row.
    private var quickThreads: [HubThread] { store.threads.filter { $0.isQuick && !$0.archived && !store.isChat($0) } }

    /// "Quick threads · N" with the states as dots, fixed above the pill; a tap opens the inbox with Quick on.
    private var quickRow: some View {
        let list = quickThreads
        let waiting = list.filter { $0.status == .waiting_on_you }.count
        let running = list.filter { $0.status == .running }.count
        let failed = list.filter { $0.status == .error && $0.section != nil }.count
        return Button {
            nav.quick = true; nav.inbox = []; nav.tab = 0; Haptics.tap()
        } label: {
            HStack(spacing: 10) {
                Image(systemName: "bolt.fill").font(.system(size: 13)).foregroundStyle(Hub.accent)
                Text("Quick threads · \(list.count)").font(.system(size: 14.5, weight: .medium))
                Spacer()
                HStack(spacing: 8) {
                    if waiting > 0 { HStack(spacing: 3) { StatusDot(state: .waiting); Text("\(waiting)") } }
                    if running > 0 { HStack(spacing: 3) { StatusDot(state: .running); Text("\(running)") } }
                    if failed > 0 { HStack(spacing: 3) { StatusDot(state: .error); Text("\(failed)") } }
                }
                .font(.system(size: 12)).foregroundStyle(Hub.muted)
                Image(systemName: "chevron.right").font(.system(size: 11)).foregroundStyle(Hub.muted)
            }
            .padding(.horizontal, 16).frame(height: 46)
            .background(Capsule().fill(Hub.surface))
            .overlay(Capsule().strokeBorder(Hub.lineStrong))
            .shadow(color: .black.opacity(0.4), radius: 10, y: 4)
            .padding(.horizontal, 16)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Quick threads, \(list.count). Show them in the inbox")
    }

    /// Projects with something going on, then this week's, then the quiet ones.
    private func grouped(_ index: ThreadIndex) -> [(String, [ProjectRef])] {
        let week = Date().timeIntervalSince1970 * 1000 - 7 * 86_400_000
        var active: [ProjectRef] = [], recent: [ProjectRef] = [], quiet: [ProjectRef] = []
        for p in store.projects {
            let threads = index.of(p)
            if threads.contains(where: { $0.section == .needsYou || $0.section == .working }) { active.append(p) }
            else if threads.contains(where: { ($0.returned_at ?? $0.sent_at ?? $0.updated_at) > week }) { recent.append(p) }
            else { quiet.append(p) }
        }
        return [("Going on", active), ("This week", recent), ("Quiet", quiet)].filter { !$0.1.isEmpty }
    }
}

/// A project as a card: name, its threads as dots, what is left to test, and its stage.
struct ProjectCard: View {
    @Environment(Store.self) private var store
    let project: ProjectRef
    /// The project's threads, from the list's one pass over them.
    let threads: [HubThread]

    var body: some View {
        let waiting = threads.filter { $0.status == .waiting_on_you }.count
        let running = threads.filter { $0.status == .running }.count
        let failed = threads.filter { $0.status == .error && $0.section != nil }.count
        let summary = store.summaries[project.id]
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: "folder").foregroundStyle(Hub.projectColor(project.name))
                Text(project.name).font(.system(size: 16, weight: .semibold))
                if let h = project.host { Text(h).font(.system(size: 12)).foregroundStyle(Hub.muted) }
                Spacer()
                HStack(spacing: 6) {
                    if waiting > 0 { HStack(spacing: 3) { StatusDot(state: .waiting); Text("\(waiting)") } }
                    if running > 0 { HStack(spacing: 3) { StatusDot(state: .running); Text("\(running)") } }
                    if failed > 0 { HStack(spacing: 3) { StatusDot(state: .error); Text("\(failed)") } }
                }
                .font(.system(size: 12)).foregroundStyle(Hub.muted)
                if let s = summary, s.to_test > 0 {
                    Text("\(s.to_test) to test").font(.system(size: 11.5, weight: .semibold)).foregroundStyle(Hub.amber)
                        .padding(.horizontal, 7).padding(.vertical, 2).background(Capsule().fill(Hub.amberBg))
                }
            }
            Group {
                if let s = summary, let stage = s.stage {
                    Text("Stage \(s.n) · \(stage) · \(s.done) of \(s.of)")
                } else if summary != nil {
                    Text("Every stage delivered")
                } else {
                    Text(threads.isEmpty ? "No threads yet" : "No stages yet · \(threads.count) thread\(threads.count == 1 ? "" : "s")")
                }
            }
            .font(.system(size: 13)).foregroundStyle(Hub.text3).lineLimit(1).padding(.leading, 26)
            ProgressBar(value: summary.map { $0.of > 0 ? Double($0.done) / Double($0.of) : 1 } ?? 0)
                .padding(.leading, 26)
        }
        .padding(14)
        .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
    }
}

struct ProgressBar: View {
    let value: Double
    var color: Color = Hub.green
    /// Where an even pace would be (0 to 1): a white line. Past it the bar turns amber.
    var pace: Double?
    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(Hub.lineStrong)
                Capsule().fill(color).frame(width: g.size.width * min(1, max(0, value)))
                if let pace {
                    Rectangle().fill(Hub.text).frame(width: 2, height: 10)
                        .offset(x: g.size.width * min(1, max(0, pace)) - 1)
                }
            }
        }
        .frame(height: pace == nil ? 4 : 10)
    }
}

/// A plan limit with a pace marker (sheet "Maskiner", screen 3): the bar is how much you have used,
/// the white line how much of the window has passed. Ahead of the line the bar turns amber.
struct LimitRow: View {
    let name: String
    let limit: UsageReport.Limit

    var body: some View {
        let pace = Self.pace(limit)
        let ahead = pace.map { limit.percent / 100 > $0 + 0.05 } ?? false
        VStack(alignment: .leading, spacing: 7) {
            HStack {
                Text("\(name) · \(limit.label)").font(.system(size: 14.5))
                Spacer()
                Text("\(Int(limit.percent)) %\(limit.resets_at.map { " · resets \(Store.clock($0))" } ?? "")")
                    .font(.system(size: 12)).foregroundStyle(Hub.muted)
            }
            ProgressBar(value: limit.percent / 100, color: ahead ? Hub.amber : Ring.color(limit.percent), pace: pace)
            if let pace {
                Text("\(Int(limit.percent)) % used, \(Int(pace * 100)) % of the time gone. \(ahead ? "Slow down a little" : "Room to spare").")
                    .font(.system(size: 12)).foregroundStyle(ahead ? Hub.amber : Hub.muted)
            }
        }
        .padding(.horizontal, 13).padding(.vertical, 11)
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
    }

    /// How much of the limit's window has passed, from its label (5 hours, a week) and its reset time.
    static func pace(_ l: UsageReport.Limit) -> Double? {
        guard let resets = l.resets_at else { return nil }
        let label = l.label.lowercased()
        let window: Double
        if label.contains("week") || label.contains("7") || label.contains("day") { window = 7 * 86_400_000 }
        else if label.contains("5") || label.contains("hour") || label.contains("session") { window = 5 * 3_600_000 }
        else { return nil }
        let left = resets - Date().timeIntervalSince1970 * 1000
        guard left > 0, left <= window else { return left <= 0 ? 1 : nil }
        return 1 - left / window
    }
}

/// A project's page: FOCUS.md as a page you tick from the sofa, its threads, its ideas.
struct ProjectView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @Environment(Nav.self) private var nav
    let project: ProjectRef
    @State private var tab = 0
    @State private var allTests = false

    var body: some View {
        let info = store.projectInfo[project.id]
        let threads = store.threads(of: project)
        VStack(spacing: 0) {
            HubHeader(parent: nav.tabName, title: project.name,
                      sub: [threads.count == 1 ? "1 thread" : "\(threads.count) threads",
                            store.summaries[project.id].flatMap { $0.to_test > 0 ? "\($0.to_test) to test" : nil }].compactMap { $0 }.joined(separator: " · "),
                      back: { dismiss() }) {
                HubMenu {
                    Button { nav.push(.new(project, talk: false)) } label: { Label("New thread here", systemImage: "square.and.pencil") }
                    Button { Task { await store.loadProject(project) } } label: { Label("Refresh", systemImage: "arrow.clockwise") }
                    if nav.focus?.id == project.id {
                        Button { nav.setFocus(nil) } label: { Label("Unlock", systemImage: "lock.open") }
                    } else {
                        Button { nav.setFocus(project) } label: { Label("Set as working", systemImage: "lock") }
                    }
                } label: { HeaderFace(icon: "ellipsis") }
                .accessibilityLabel("More")
            }
            if let f = nav.focus, f.id != project.id { PeekBar(focus: f) }
            Tabs(selection: $tab, items: ["Home", "Focus", "Threads \(threads.count)", "Ideas \(ideas(info).count)", "Files", "Library"])
                .padding(.horizontal, 14).padding(.vertical, 12)
            ModePager(selection: $tab, count: 6) { i in
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        switch i {
                        case 0: home(info, threads)
                        case 1: focus(info)
                        case 2:
                            NavigationLink(value: Route.new(project, talk: false)) {
                                Label("New thread here", systemImage: "plus").font(.system(size: 14.5, weight: .medium))
                                    .foregroundStyle(Hub.text).frame(maxWidth: .infinity).frame(height: 44)
                                    .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface2))
                            }
                            .buttonStyle(.plain).padding(.horizontal, 14).padding(.bottom, 8)
                            ThreadList(threads: Array(threads.prefix(150)))
                        case 3: ideasList(info)
                        case 4: FilesTab(project: project)
                        default: LibraryTab(project: project)
                        }
                    }
                    .padding(.bottom, 30)
                }
            }
        }
        .background(Hub.bg)
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack(enabled: tab == 0) { dismiss() }
        .task { await store.loadProject(project) }
        .refreshable { await store.loadProject(project) }
    }

    /// The project's home: its parts as tiles with one live fact each, then what is left to test.
    @ViewBuilder private func home(_ info: ProjectInfo?, _ threads: [HubThread]) -> some View {
        let running = threads.filter { $0.status == .running }.count
        let waiting = threads.filter { $0.status == .waiting_on_you }.count
        let f = info?.focus
        let stage = f.flatMap { fo in fo.active.flatMap { fo.stages.indices.contains($0) ? fo.stages[$0] : nil } }
        let n = ideas(info).count
        let columns = Array(repeating: GridItem(.flexible(), spacing: 8), count: 3)
        LazyVGrid(columns: columns, spacing: 8) {
            tile("bubble.left", "Threads", threads.isEmpty ? "None yet" : waiting > 0 ? "\(waiting) need you" : running > 0 ? "\(running) running" : "\(threads.count) in all", to: 2, tint: waiting > 0 ? Hub.amber : running > 0 ? Hub.green : Hub.muted)
            tile("checklist", "Focus", stage.map { "\($0.done.filter(\.done).count) of \($0.done.count) delivered" } ?? "No stages", to: 1)
            tile("lightbulb", "Ideas", n == 0 ? "None" : "\(n) kept", to: 3)
            tile("doc", "Files", "On \(project.host ?? store.machine ?? "Hub")", to: 4)
            tile("photo.on.rectangle", "Library", "Made in threads", to: 5)
        }
        .padding(.horizontal, 14)
        if let stage {
            let open = stage.test.filter { !$0.done }
            HStack {
                Text("To test by hand · \(open.count)").font(.system(size: 13, weight: .semibold)).foregroundStyle(Hub.amber)
                Spacer()
                if open.count > 3 { Button("All") { withAnimation(.snappy) { tab = 1 } }.font(.system(size: 13)).foregroundStyle(Hub.accent) }
            }
            .padding(.horizontal, 18).padding(.top, 20).padding(.bottom, 6)
            checklist(Array(open.prefix(3)))
        }
        Text("Git, Docs, Backlog and Board come as parts with the next version.").font(.system(size: 12)).foregroundStyle(Hub.muted)
            .padding(.horizontal, 18).padding(.top, 18)
    }

    private func tile(_ icon: String, _ title: String, _ fact: String, to mode: Int, tint: Color = Hub.muted) -> some View {
        Button { Haptics.tap(); withAnimation(.snappy) { tab = mode } } label: { PartTile(icon: icon, title: title, fact: fact, factTint: tint) }
            .buttonStyle(.press)
    }

    private func ideas(_ info: ProjectInfo?) -> [ProjectFocus.Idea] {
        guard let f = info?.focus else { return [] }
        let stage = f.active.flatMap { f.stages.indices.contains($0) ? f.stages[$0].ideas : nil } ?? []
        return stage + f.parked
    }

    @ViewBuilder private func focus(_ info: ProjectInfo?) -> some View {
        if let f = info?.focus, f.exists, let i = f.active, f.stages.indices.contains(i) {
            let stage = f.stages[i]
            HStack {
                Text("Stage \(i + 1) · \(stage.title)").font(.system(size: 13, weight: .semibold))
                Spacer()
                Text("\(stage.done.filter(\.done).count) of \(stage.done.count)").font(.system(size: 12)).foregroundStyle(Hub.muted)
            }
            .padding(.horizontal, 18).padding(.bottom, 6)
            checklist(stage.done)
            let open = stage.test.filter { !$0.done }
            HStack {
                Text("To test by hand · \(open.count)").font(.system(size: 13, weight: .semibold)).foregroundStyle(Hub.amber)
                Spacer()
                if open.count > 4 { Button(allTests ? "Show fewer" : "Show all") { allTests.toggle() }.font(.system(size: 13)).foregroundStyle(Hub.accent) }
            }
            .padding(.horizontal, 18).padding(.top, 18).padding(.bottom, 6)
            checklist(allTests ? open : Array(open.prefix(4)))
            Text("Ticking a box here changes docs/FOCUS.md.").font(.system(size: 12)).foregroundStyle(Hub.muted)
                .padding(.horizontal, 30).padding(.top, 10)
        } else if info == nil {
            Text(store.tooOld.contains("project") ? "\(project.host ?? store.machine ?? "Hub") needs a newer hubd to show FOCUS.md." : "Loading…")
                .font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.horizontal, 18)
        } else {
            Text("No stages yet. Write a docs/FOCUS.md, or ask a thread to.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.horizontal, 18)
        }
    }

    private func checklist(_ items: [ProjectFocus.Check]) -> some View {
        VStack(spacing: 0) {
            ForEach(items, id: \.line) { c in
                Button { Task { await store.tick(project, line: c.line, done: !c.done) } } label: {
                    HStack(alignment: .top, spacing: 11) {
                        Image(systemName: c.done ? "checkmark.square.fill" : "square")
                            .font(.system(size: 19)).foregroundStyle(c.done ? Hub.green : Hub.lineStrong)
                        Text(c.text).font(.system(size: 14.5)).foregroundStyle(Hub.text).multilineTextAlignment(.leading)
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 13).padding(.vertical, 11)
                    .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 43) }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
        .padding(.horizontal, 14)
    }

    private func ideasList(_ info: ProjectInfo?) -> some View {
        let list = ideas(info)
        return VStack(alignment: .leading, spacing: 0) {
            if list.isEmpty { Text("No ideas yet. Hold the mic in the inbox and say one.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.horizontal, 18) }
            ForEach(list, id: \.line) { idea in
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: "lightbulb").foregroundStyle(Hub.muted)
                    Text(idea.text).font(.system(size: 14.5))
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 18).padding(.vertical, 10)
                .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
            }
        }
    }
}

/// Text tabs under a page title, the chosen one on a grey pill (sketches 3 and 5).
struct Tabs: View {
    @Binding var selection: Int
    let items: [String]
    @Namespace private var pill

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 4) {
                    ForEach(Array(items.enumerated()), id: \.offset) { i, item in
                        Button { if selection != i { Haptics.tick() }; withAnimation(.snappy) { selection = i } } label: {
                            Text(item)
                                .font(.system(size: 14, weight: selection == i ? .semibold : .regular))
                                .foregroundStyle(selection == i ? Hub.text : Hub.muted)
                                .padding(.horizontal, 11).frame(height: 30)
                                .background {
                                    if selection == i { Capsule().fill(Hub.surface2).matchedGeometryEffect(id: "pill", in: pill) }
                                }
                                .contentShape(Capsule())
                        }
                        .buttonStyle(.plain)
                        .id(i)
                    }
                }
                // The pill glides to the mode whether a tap or a swipe on the page chose it.
                .animation(.snappy, value: selection)
            }
            .onChange(of: selection) { _, now in withAnimation(.snappy) { proxy.scrollTo(now, anchor: .center) } }
        }
        .swipeZone()
    }
}

// MARK: Machines (sketch 3)
