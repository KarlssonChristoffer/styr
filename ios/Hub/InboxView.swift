import SwiftUI

/// The home screen (sketch 1, "Inkorgen"): what needs you can be handled right on its row, a
/// swipe settles or snoozes, a tap opens the row in place, a long press picks many. New thread and
/// the mic sit by the thumb.
/// Where the inbox's stack can go.
enum Route: Hashable {
    case thread(String)
    /// A new thread, in a project when given, listening at once when `talk`.
    case new(ProjectRef?, talk: Bool)
    case shelf
    case project(ProjectRef)
    case machine(String)
    case changes(String)
    case pull(String)
    case settings(SettingsPage)
    case file(ProjectRef, String)
    case problems
    case problem(Int)
    case routines
    case routine(String)
    case space(String)
    case newChat
}

struct InboxView: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @State private var search = ""
    @FocusState private var searchFocused: Bool
    @State private var expanded: String?
    @State private var picked: Set<String> = []
    @State private var machineFilter: String?
    @State private var projectFilter: String?
    @AppStorage("inbox.cards") private var cards = true
    @AppStorage("snooze.moon") private var moon = true
    @State private var folded: Set<String> = InboxFolds.load()
    @State private var showChips = false
    private var searching: Bool { nav.searching }
    private var picking: Bool { nav.picking }

    var body: some View {
        let _ = Perf.count("InboxView.body")
        NavigationStack(path: Bindable(nav).inbox) {
            ZStack(alignment: .bottom) {
                if cards { cardsList } else { classicList }

                if picking { pickActions }
                if let toast = store.toast { ToastView(toast: toast) }
            }
            // The keyboard does not resize the list: the list is laid out once, not at every frame of the
            // keyboard sliding in (the end of the list has room above the keyboard while you search).
            .ignoresSafeArea(.keyboard)
            .background(Hub.bg)
            .toolbar(.hidden, for: .navigationBar)
            .navigationDestination(for: Route.self) { RouteView(route: $0) }
            .onChange(of: nav.searching) {
                if nav.searching { searchFocused = true } else { search = ""; searchFocused = false }
            }
            .onAppear {
                // `-open <thread id>` at launch opens that thread, for screenshots and tests.
                if let id = UserDefaults.standard.string(forKey: "open"), nav.inbox.isEmpty { nav.inbox = [id == "new" ? .new(nil, talk: false) : .thread(id)] }
                // `-expand <thread id>` opens that row in place, `-pick 1` starts picking.
                if let id = UserDefaults.standard.string(forKey: "expand") { expanded = id }
                // `-search 1`, `-penmenu 1`, `-talk 1` open search, the pen's menu and Talk an idea.
                if UserDefaults.standard.bool(forKey: "search") { nav.searching = true }
                if UserDefaults.standard.bool(forKey: "penmenu") { nav.penMenu = true }
                if UserDefaults.standard.bool(forKey: "talk") { nav.talking = true }
                if UserDefaults.standard.bool(forKey: "pick") { nav.picking = true; picked = Set(store.threads(in: .done).map(\.id)) }
            }
        }
    }

    private var classicList: some View {
        List {
            Group {
                top
                if !picking && (showChips || nav.quick || machineFilter != nil || projectFilter != nil) { chips }
                if store.isDemo { note("Demo data. Pair with Hub under Settings to see your threads.") }
                else if let error = store.error { note(error, color: Hub.red) }
                else if !store.anyLive { note("Connecting to \(store.machine ?? "Hub")…") }
                ForEach(InboxSection.allCases, id: \.self) { section in
                    let list = filtered(store.threads(in: section))
                    if !list.isEmpty {
                        sectionTitle(section, section.rawValue, list.count)
                        ForEach(list) { thread in row(thread, in: section) }
                    }
                }
                if queryActive || machineFilter != nil || projectFilter != nil {
                    let settled = settledMatches
                    if !settled.isEmpty {
                        sectionTitle(nil, "Settled", settled.count)
                        ForEach(settled) { thread in row(thread, in: .done) }
                    }
                }
                if !picking, !drafts.isEmpty {
                    sectionTitle(nil, "Drafts", drafts.count)
                    ForEach(drafts) { draft in draftRow(draft) }
                }
                if !store.isDemo && store.anyLive && store.threads.allSatisfy({ $0.section == nil }) && drafts.isEmpty {
                    note("Nothing needs you. Threads that work or finish show up here.")
                }
                Color.clear.frame(height: searching ? 360 : 110)
            }
            .listRowInsets(EdgeInsets())
            .listRowSeparator(.hidden)
            .listRowBackground(Hub.bg)
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .environment(\.defaultMinListRowHeight, 1)
        .scrollsToTop(0)
        // Pull the list down and search opens at the top (sketch "Inkorgen", screen 2); it
        // also asks the machines for the latest, which the pull used to do alone.
        .onScrollPhaseChange { old, new, context in
            guard old == .interacting, new != .interacting, !nav.searching, !nav.picking else { return }
            if context.geometry.contentOffset.y + context.geometry.contentInsets.top < -70 {
                Haptics.tap()
                withAnimation(.snappy) { nav.searching = true }
                // The machines are asked once the keyboard is up, not while it slides in.
                Task { try? await Task.sleep(for: .seconds(1)); await store.refresh() }
            }
        }
    }

    // MARK: The cards (sketches 23 and 24)

    /// The groups as the sketch has them: what needs you, what is done, then what works, Hub's own and
    /// the snoozed ones, folded. What needs you always stays in Needs you, wherever it came from.
    private var groups: [InboxGroupData] { Perf.time("inboxGroups", over: 4) { store.inboxGroups(match: matches) } }

    private var cardsList: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                // Worked out once for the page, not for each place that reads it.
                let groups = self.groups
                top
                if !picking && (showChips || nav.quick || machineFilter != nil || projectFilter != nil) { chips }
                if store.isDemo { note("Demo data. Pair with Hub under Settings to see your threads.") }
                else if let error = store.error { note(error, color: Hub.red) }
                else if !store.anyLive { note("Connecting to \(store.machine ?? "Hub")…") }
                ForEach(groups) { g in
                    let isFolded = folded.contains(g.id) && !queryActive
                    GroupHeading(title: g.title, count: g.threads.count, tint: g.tint, folded: isFolded,
                                 settleAll: g.settleAll ? { store.tidy(g.threads, .settle) } : nil) {
                        withAnimation(.snappy) { if folded.contains(g.id) { folded.remove(g.id) } else { folded.insert(g.id) } }
                        InboxFolds.save(folded)
                    }
                    if !isFolded {
                        GroupCard {
                            ForEach(Array(g.threads.enumerated()), id: \.element.id) { i, t in
                                cardRow(t).overlay(alignment: .bottom) { if i < g.threads.count - 1 { Hub.line.frame(height: 1) } }
                            }
                        }
                        .transition(.opacity)
                    }
                }
                if queryActive || machineFilter != nil || projectFilter != nil {
                    let settled = settledMatches
                    if !settled.isEmpty {
                        GroupHeading(title: "Settled", count: settled.count, folded: false) {}
                        GroupCard { ForEach(settled) { t in cardRow(t) } }
                    }
                }
                if !picking, !drafts.isEmpty {
                    GroupHeading(title: "Drafts", count: drafts.count, folded: false) {}
                    GroupCard { ForEach(drafts) { d in draftCard(d) } }
                }
                if !store.isDemo && store.anyLive && groups.isEmpty && drafts.isEmpty {
                    note("Nothing needs you. Threads that work or finish show up here.")
                }
                Color.clear.frame(height: searching ? 360 : 110)
            }
        }
        .scrollDismissesKeyboard(.interactively)
        .scrollsToTop(0)
        .onScrollPhaseChange { old, new, context in
            guard old == .interacting, new != .interacting, !nav.searching, !nav.picking else { return }
            if context.geometry.contentOffset.y + context.geometry.contentInsets.top < -70 {
                Haptics.tap()
                withAnimation(.snappy) { nav.searching = true }
                // The machines are asked once the keyboard is up, not while it slides in.
                Task { try? await Task.sleep(for: .seconds(1)); await store.refresh() }
            }
        }
    }

    private func cardRow(_ t: HubThread) -> some View {
        NightSwipeRow(moon: moon, enabled: !picking && store.has("phone.tidy", on: store.url(of: t.host)),
                      onSettle: { store.tidy([t], .settle) },
                      onSnooze: { store.tidy([t], .snooze(until: $0)) },
                      onTap: { if picking { toggle(t.id) } else { Task { await store.loadEvents(t.id) }; nav.inbox.append(.thread(t.id)) } },
                      onHold: {
                          guard !picking else { return }
                          Haptics.firm()
                          withAnimation(.snappy) { nav.picking = true; picked = [t.id]; expanded = nil }
                      }) {
            ThreadCardRow(thread: t, picking: picking, picked: picked.contains(t.id), mark: search.trimmingCharacters(in: .whitespaces))
        }
    }

    private func draftCard(_ d: IdeaDraft) -> some View {
        HStack(spacing: 0) {
            Rectangle().fill(Hub.projectColor(d.project ?? "")).frame(width: 3)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Image(systemName: "folder").font(.system(size: 12)).foregroundStyle(Hub.projectColor(d.project ?? ""))
                    Text(d.project ?? "No project").font(.system(size: 13.5, weight: .semibold)).foregroundStyle(Hub.projectColor(d.project ?? ""))
                    Spacer()
                    Text("Draft · \(relative(d.at))").font(.system(size: 12)).foregroundStyle(Hub.muted)
                }
                HStack(spacing: 8) { StatusDot(state: .idle); Text(d.text).font(.system(size: 15)).foregroundStyle(Hub.text2).lineLimit(1); Spacer(minLength: 0) }
            }
            .padding(.horizontal, 12).padding(.vertical, 9)
        }
        .contentShape(Rectangle())
        .tapRow {
            Drafts.set("new", d.text)
            store.removeIdeaDraft(d.id)
            nav.inbox.append(.new(d.ref, talk: false))
        }
        .contextMenu { Button(role: .destructive) { store.removeIdeaDraft(d.id) } label: { Label("Delete", systemImage: "trash") } }
    }

    // MARK: Rows

    @ViewBuilder private func row(_ thread: HubThread, in section: InboxSection) -> some View {
        HStack(spacing: 0) {
            if picking {
                Image(systemName: picked.contains(thread.id) ? "checkmark.circle.fill" : "circle")
                    .font(.system(size: 21)).foregroundStyle(picked.contains(thread.id) ? Hub.accent : Hub.lineStrong)
                    .padding(.leading, 16)
            }
            Group {
                if section == .working && expanded != thread.id {
                    WorkingRow(thread: thread, line: store.lastLine(thread))
                } else {
                    InboxRow(thread: thread, expanded: expanded == thread.id, open: { nav.inbox.append(.thread(thread.id)) })
                }
            }
        }
        .contentShape(Rectangle())
        .tapRow {
            // A tap anywhere on a row opens the thread; replying in place is in the long-press menu.
            if picking { toggle(thread.id); return }
            if expanded == thread.id { withAnimation(.snappy) { expanded = nil }; return }
            nav.inbox.append(.thread(thread.id))
        }
        .onLongPressGesture(minimumDuration: 0.4) {
            // Hold a row to pick many (sketch "Inkorgen", screen 5).
            guard !picking else { return }
            Haptics.firm()
            withAnimation(.snappy) { nav.picking = true; picked = [thread.id]; expanded = nil }
        }
        .swipeActions(edge: .leading, allowsFullSwipe: true) {
            if store.has("phone.tidy", on: store.url(of: thread.host)) {
            Button { store.tidy([thread], .settle) } label: { Label("Settle", systemImage: "checkmark") }.tint(Color(hex: 0x5f9f77))
            }
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
            if store.has("phone.tidy", on: store.url(of: thread.host)) {
            ForEach(store.snoozeChoices, id: \.label) { time in
                Button { store.tidy([thread], .snooze(until: time.until)) } label: { Label("\(time.label) \(time.sub)", systemImage: "moon") }
                    .tint(time.label == "1 h" ? Color(hex: 0x3b3f8f) : time.label == "Reset" ? Color(hex: 0x1f2257) : Color(hex: 0x2e3170))
            }
            }
        }
    }

    private func toggle(_ id: String) {
        if picked.contains(id) { picked.remove(id) } else { picked.insert(id) }
    }

    // MARK: Picking many (sketch 1, screen 5)

    private var pickBar: some View {
        HStack {
            Text(picked.isEmpty ? "Select" : "\(picked.count) selected").font(.system(size: 29, weight: .bold)).tracking(-0.4)
            Spacer()
            Button("Done") { withAnimation(.snappy) { nav.picking = false; picked = [] } }
                .buttonStyle(.borderless).foregroundStyle(Hub.accent).font(.system(size: 16, weight: .semibold))
        }
        .padding(.horizontal, 18).padding(.top, 8).padding(.bottom, 4)
    }

    /// Takes the pill's place while you pick (sketch "Inkorgen", screen 5).
    private var pickActions: some View {
        let chosen = store.threads.filter { picked.contains($0.id) }
        let allPinned = !chosen.isEmpty && chosen.allSatisfy { $0.pinned_at != nil }
        return HStack(spacing: 0) {
            pickButton("Settle", "checkmark") { store.tidy(chosen, .settle) }
            HubMenu {
                ForEach(store.snoozeChoices, id: \.label) { c in
                    Button(c.label == "1 h" ? "For an hour" : c.label == "Reset" ? "Until the limits reset, \(c.sub)" : "Until tomorrow 08:00") {
                        store.tidy(chosen, .snooze(until: c.until)); finishPicking()
                    }
                }
            } label: { pickFace("Snooze", "moon") }
            pickButton(allPinned ? "Unpin" : "Pin", "pin") { store.tidy(chosen, allPinned ? .unpin : .pin) }
            pickButton("Archive", "archivebox") { store.tidy(chosen, .archive) }
        }
        .disabled(chosen.isEmpty)
        .opacity(chosen.isEmpty ? 0.5 : 1)
        .padding(.horizontal, 10).padding(.vertical, 8)
        .background(Capsule().fill(Hub.surface))
        .overlay(Capsule().strokeBorder(Hub.lineStrong))
        .shadow(color: .black.opacity(0.45), radius: 12, y: 6)
        .padding(.horizontal, 16).padding(.top, 24).padding(.bottom, 8)
        .background(LinearGradient(colors: [Hub.bg.opacity(0), Hub.bg.opacity(0.94)], startPoint: .top, endPoint: .center).ignoresSafeArea(edges: .bottom))
        .transition(.move(edge: .bottom).combined(with: .opacity))
    }

    private func finishPicking() { withAnimation(.snappy) { nav.picking = false; picked = [] } }

    private func pickFace(_ title: String, _ icon: String) -> some View {
        VStack(spacing: 4) { Image(systemName: icon).font(.system(size: 18)).frame(height: 24); Text(title).font(.system(size: 11)) }
            .foregroundStyle(Hub.text2).frame(maxWidth: .infinity).frame(height: 48).contentShape(Rectangle())
    }

    private func pickButton(_ title: String, _ icon: String, _ action: @escaping () -> Void) -> some View {
        Button { action(); finishPicking() } label: { pickFace(title, icon) }.buttonStyle(.plain)
    }

    // MARK: Pieces

    private var header: some View {
        PageHead(title: "Inbox", sub: { if !searching { statusLine } else { Color.clear } }, trailing: {
            HStack(spacing: 8) {
                if store.halt != nil {
                    Text("Paused").font(.system(size: 11.5, weight: .semibold)).foregroundStyle(Hub.red)
                        .padding(.horizontal, 8).padding(.vertical, 2).background(Capsule().fill(Hub.redBg))
                }
                filterMenu
            }
        })
    }

    private var filterMenu: some View {
        HubMenu {
            Button { withAnimation { nav.picking = true } } label: { Label("Select threads", systemImage: "checkmark.circle") }
            Button { withAnimation(.snappy) { showChips.toggle() } } label: { Label(showChips ? "Hide the filters" : "Filters: Quick, machine, project", systemImage: "line.3.horizontal.decrease") }
            Button { nav.inbox.append(.shelf) } label: { Label("Settled, snoozed and archived", systemImage: "archivebox") }
        } label: { Image(systemName: "line.3.horizontal.decrease").font(.system(size: 17)).foregroundStyle(Hub.text2) }
    }

    /// While you search the page's title is the search box, in the title's own place and as tall as the
    /// title and its line, so nothing under it moves when search opens (sketch 28, A). The filters stay
    /// behind the menu at its right.
    private var searchRow: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 12) {
                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass").foregroundStyle(Hub.muted)
                    TextField("", text: $search, prompt: Text("Search threads").foregroundStyle(Hub.muted))
                        .focused($searchFocused).autocorrectionDisabled()
                    if !search.isEmpty {
                        Button { search = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(Hub.muted) }.buttonStyle(.plain)
                    }
                }
                .font(.system(size: 15))
                .padding(.horizontal, 16).frame(height: Metrics.titleRow)
                .background(Capsule().fill(Hub.surface))
                .overlay(Capsule().strokeBorder(Hub.lineStrong))
                filterMenu
            }
            .frame(height: Metrics.titleRow)
            Color.clear.frame(height: Metrics.subRow)
        }
        .padding(.horizontal, Metrics.side).padding(.top, Metrics.titleTop).padding(.bottom, Metrics.titleBottom)
    }

    /// The top of the page: the title, the search box in its place, or the selection bar.
    @ViewBuilder private var top: some View {
        if picking { pickBar } else if searching { searchRow } else {
            header
            if let f = nav.focus { FocusBar(project: f) }
        }
    }

    /// Something is typed: groups open at their hits and settled threads count as hits.
    private var queryActive: Bool { !search.trimmingCharacters(in: .whitespaces).isEmpty }

    /// "2 need you · 1 working · 3 up", with the dots the rows use; "All clear" when nothing waits.
    private var statusLine: some View {
        let need = store.threads(in: .needsYou).filter { t in (!nav.quick || t.isQuick) && (nav.focus.map { f in store.belongs(t, to: f) } ?? true) }.count
        let work = store.threads(in: .working).filter { t in (!nav.quick || t.isQuick) && (nav.focus.map { f in store.belongs(t, to: f) } ?? true) }.count
        let up = machinesUp
        return HStack(spacing: 14) {
            if need > 0 {
                HStack(spacing: 6) { StatusDot(state: .waiting); Text("\(need) need you") }.foregroundStyle(Hub.amber)
            } else {
                Text("All clear").foregroundStyle(Hub.muted)
            }
            if work > 0 {
                HStack(spacing: 6) { StatusDot(state: .running); Text("\(work) working") }.foregroundStyle(Hub.green)
            }
            if up > 0 { Text("\(up) up").foregroundStyle(Hub.muted) }
        }
        .font(.system(size: 13))
        .accessibilityElement(children: .combine)
    }

    private var machinesUp: Int {
        if !store.hosts.isEmpty { return store.hosts.filter { $0.state == "online" }.count }
        if store.isDemo { return Set(store.threads.compactMap(\.host)).count }
        return store.machines.filter { store.live[$0.url] == true }.count
    }

    /// The machine and project filters under the title (and under the search box).
    private var chips: some View {
        let machines = store.hosts.isEmpty ? Array(Set(store.threads.compactMap(\.host) + [store.machine].compactMap { $0 })).sorted() : store.hosts.map(\.name)
        let projects = Array(Set(store.threads.filter { $0.section != nil || $0.settled_at != nil }.map(\.project).filter { !Store.scratch($0) })).sorted()
        return ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                if nav.quick || store.threads.contains(where: { $0.isQuick && !store.isChat($0) }) {
                    Button { withAnimation(.snappy) { nav.quick.toggle() }; Haptics.tap() } label: {
                        HStack(spacing: 6) {
                            Image(systemName: "bolt.fill").font(.system(size: 12))
                            Text("Quick").lineLimit(1)
                        }
                        .font(.system(size: 13.5)).foregroundStyle(nav.quick ? Hub.accent : Hub.text2)
                        .padding(.horizontal, 12).frame(height: 34)
                        .background(Capsule().fill(nav.quick ? Hub.accentBg : Hub.surface))
                        .overlay(Capsule().strokeBorder(nav.quick ? Hub.accent.opacity(0.5) : Hub.lineStrong))
                    }
                    .buttonStyle(.plain)
                }
                chip("desktopcomputer", machineFilter ?? "All machines", active: machineFilter != nil, items: machines, pick: { machineFilter = $0 }, all: "All machines")
                chip("folder", projectFilter ?? "All projects", active: projectFilter != nil, items: projects, pick: { projectFilter = $0 }, all: "All projects")
            }
            .padding(.horizontal, 18)
        }
        .padding(.vertical, Metrics.chipsVertical)
    }

    private func chip(_ icon: String, _ title: String, active: Bool, items: [String], pick: @escaping (String?) -> Void, all: String) -> some View {
        HubMenu {
            Button(all) { pick(nil) }
            ForEach(items, id: \.self) { name in Button(name) { pick(name) } }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: icon).font(.system(size: 12))
                Text(title).lineLimit(1)
                Image(systemName: "chevron.down").font(.system(size: 9, weight: .semibold))
            }
            .font(.system(size: 13.5)).foregroundStyle(active ? Hub.accent : Hub.text2)
            .padding(.horizontal, 12).frame(height: Metrics.chipHeight)
            .background(Capsule().fill(active ? Hub.accentBg : Hub.surface))
            .overlay(Capsule().strokeBorder(active ? Hub.accent.opacity(0.5) : Hub.lineStrong))
        }
    }

    /// A draft said into the pen: tap opens a new thread with it written in the box.
    private func draftRow(_ draft: IdeaDraft) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 8) {
                StatusDot(state: .idle)
                HStack(spacing: 4) {
                    Image(systemName: "folder").foregroundStyle(Hub.projectColor(draft.project ?? ""))
                    Text(draft.project ?? "No project")
                }
                .font(.system(size: 12)).foregroundStyle(Hub.muted)
                Text("Draft").font(.system(size: 12)).foregroundStyle(Hub.muted)
                Spacer()
                Text(relative(draft.at)).font(.system(size: 12)).foregroundStyle(Hub.muted)
            }
            Text(draft.text).font(.system(size: 15)).foregroundStyle(Hub.text2).lineLimit(3).padding(.leading, 16)
        }
        .padding(.horizontal, 16).padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
        .contentShape(Rectangle())
        .tapRow {
            Drafts.set("new", draft.text)
            store.removeIdeaDraft(draft.id)
            nav.inbox.append(.new(draft.ref, talk: false))
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
            Button(role: .destructive) { store.removeIdeaDraft(draft.id) } label: { Label("Delete", systemImage: "trash") }
        }
    }

    private func matches(_ t: HubThread) -> Bool {
        if nav.quick && !t.isQuick { return false }
        if let f = nav.focus, !store.belongs(t, to: f) { return false }
        if let m = machineFilter, (t.host ?? store.machine) != m { return false }
        if let p = projectFilter, t.project != p { return false }
        let q = search.trimmingCharacters(in: .whitespaces).lowercased()
        guard !q.isEmpty else { return true }
        return t.title.lowercased().contains(q) || t.project.lowercased().contains(q) || (hostName(t)).lowercased().contains(q)
    }

    private func hostName(_ t: HubThread) -> String { t.host ?? "" }

    private func filtered(_ list: [HubThread]) -> [HubThread] { list.filter(matches) }

    /// Settled threads count as hits while you search or filter (sketch "Inkorgen", screen 2).
    private var settledMatches: [HubThread] {
        store.threads.filter { $0.settled_at != nil && !$0.archived && !store.isChat($0) && matches($0) }
            .sorted { ($0.settled_at ?? 0) > ($1.settled_at ?? 0) }
    }

    private var drafts: [IdeaDraft] {
        let q = search.trimmingCharacters(in: .whitespaces).lowercased()
        return store.ideaDrafts.filter { d in
            if let m = machineFilter, (d.host ?? store.machine) != m { return false }
            if let p = projectFilter, d.project != p { return false }
            return q.isEmpty || d.text.lowercased().contains(q) || (d.project ?? "").lowercased().contains(q)
        }
    }

    private func note(_ text: String, color: Color = Hub.muted) -> some View {
        Text(text).font(.system(size: 12)).foregroundStyle(color)
            .padding(.horizontal, 18).padding(.bottom, 2)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func sectionTitle(_ section: InboxSection?, _ name: String, _ count: Int) -> some View {
        HStack {
            Text("\(name) · \(count)")
                .foregroundStyle(section == .needsYou ? Hub.amber : Hub.muted)
            Spacer()
            if section == .done && !picking && store.has("phone.tidy") {
                Button("Settle all") { store.tidy(store.threads(in: .done), .settle) }
                    .buttonStyle(.borderless)
                    .foregroundStyle(Hub.accent).fontWeight(.medium)
            }
        }
        .font(.system(size: 12, weight: .semibold))
        .padding(.horizontal, Metrics.side).padding(.top, Metrics.sectionTop).padding(.bottom, Metrics.sectionBottom)
    }
}

/// "Settled …  Undo", above the thumb bar.
struct ToastView: View {
    let toast: Store.Toast

    var body: some View {
        HStack {
            Text(toast.text).font(.system(size: 13.5)).lineLimit(2)
            Spacer()
            if let undo = toast.undo {
                Button("Undo") { undo() }.foregroundStyle(Hub.accent).fontWeight(.semibold)
            }
        }
        .padding(.horizontal, 14).frame(minHeight: 44)
        .background(RoundedRectangle(cornerRadius: 13).fill(Color(hex: 0x2e2c2a)))
        .overlay(RoundedRectangle(cornerRadius: 13).strokeBorder(Hub.lineStrong))
        .shadow(color: .black.opacity(0.5), radius: 15, y: 10)
        .padding(.horizontal, 14).padding(.bottom, 84)
        .transition(.move(edge: .bottom).combined(with: .opacity))
    }
}

/// A full-width row: dot, project and machine, title, the last thing said, and what you can do.
/// Opened in place (sketch 1, screen 3) it shows the whole last reply and a reply box; a tap on
/// the title goes into the thread.
struct InboxRow: View {
    @Environment(Store.self) private var store
    let thread: HubThread
    var expanded = false
    var open: () -> Void = {}
    @State private var reply = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                StatusDot(state: thread.dot)
                ThreadMeta(thread: thread)
                if thread.pinned_at != nil { Image(systemName: "pin.fill").font(.system(size: 10)).foregroundStyle(Hub.muted) }
                Spacer()
                Text(relative(thread.returned_at ?? thread.updated_at)).font(.system(size: 12)).foregroundStyle(Hub.muted)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(thread.title.isEmpty ? "New thread" : thread.title)
                    .font(.system(size: 16, weight: thread.unread ? .semibold : .medium))
                    .lineLimit(2)
                    .safeTap { open() }
                if expanded {
                    expandedBody
                } else if let line = store.lastLine(thread) {
                    Text(line).font(.system(size: 13.5)).foregroundStyle(Hub.text3).lineLimit(3).lineSpacing(1.5)
                        .padding(.top, 1)
                } else if store.loading.contains(thread.id) {
                    Text("…").font(.system(size: 13.5)).foregroundStyle(Hub.muted)
                }
                actions.padding(.top, 7)
                if expanded { replyBox.padding(.top, 8) }
            }
            .padding(.leading, 16).padding(.top, 4)
        }
        .padding(.horizontal, 16).padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(expanded ? Hub.surface : Hub.bg)
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
    }

    @ViewBuilder private var expandedBody: some View {
        let text = store.folded[thread.id]?.lastReply ?? store.peeks[thread.id]?.last_reply ?? store.lastLine(thread)
        if let text, !text.isEmpty {
            ScrollView {
                Prose(text: text).frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxHeight: 260)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.top, 4)
        } else if store.loading.contains(thread.id) {
            Text("Loading…").font(.system(size: 13)).foregroundStyle(Hub.muted)
        }
        Button { open() } label: { Label("Open the thread", systemImage: "arrow.up.right").font(.system(size: 12.5)) }
            .buttonStyle(.borderless).foregroundStyle(Hub.accent).padding(.top, 4)
    }

    private var replyBox: some View {
        HStack(spacing: 8) {
            TextField("", text: $reply, prompt: Text("Reply").foregroundStyle(Hub.muted), axis: .vertical)
                .font(.system(size: 14.5)).lineLimit(1...4)
            Button {
                let text = reply.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !text.isEmpty else { return }
                reply = ""
                Task { await store.send(text, to: thread) }
            } label: {
                Image(systemName: "arrow.up").font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(reply.isEmpty ? Hub.muted : Hub.bg)
                    .frame(width: 28, height: 28)
                    .background(Circle().fill(reply.isEmpty ? Hub.lineStrong : Hub.accent))
            }
            .buttonStyle(.borderless)
        }
        .padding(.leading, 12).padding(.trailing, 5).padding(.vertical, 5)
        .background(RoundedRectangle(cornerRadius: 18).fill(Hub.surface2))
        .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(Hub.lineStrong))
    }

    @ViewBuilder private var actions: some View {
        if let ask = store.openAsk(thread) {
            AskButtons(thread: thread, ask: ask)
        } else if let replies = store.replies(thread) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 7) {
                    ForEach(replies.options, id: \.self) { reply in
                        Button(reply.label) { Task { await store.send(reply.message, to: thread) } }
                            .buttonStyle(PillButtonStyle())
                    }
                }
            }
        } else if thread.status == .error {
            HStack(spacing: 7) {
                Button { Task { await store.send("Try again", to: thread) } } label: {
                    Label("Try again", systemImage: "arrow.clockwise")
                }.buttonStyle(PillButtonStyle())
                Button("Open") { open() }.buttonStyle(PillButtonStyle())
            }
        }
    }
}

/// The agent's own options for a permission, at most one filled. A risky command takes a second tap.
struct AskButtons: View {
    @Environment(Store.self) private var store
    let thread: HubThread
    let ask: Ask
    @State private var armed: String?

    var body: some View {
        HStack(spacing: 7) {
            ForEach(Array(ask.options.prefix(3).enumerated()), id: \.element) { i, o in
                let allow = o.kind.hasPrefix("allow")
                Button(armed == o.id ? "Sure? \(o.name)" : o.name) {
                    if allow && ask.risky && armed != o.id { armed = o.id; return }
                    Task { await store.answer(thread, ask: ask, option: o.id) }
                }
                .buttonStyle(PillButtonStyle(kind: allow && i == 0 && !ask.risky ? .primary : (allow ? .plain : .danger)))
            }
            if !ask.hasReject {
                Button("Reject") { Task { await store.answer(thread, ask: ask, option: nil) } }
                    .buttonStyle(PillButtonStyle(kind: .danger))
            }
        }
    }
}

/// One line per working thread.
struct WorkingRow: View {
    let thread: HubThread
    let line: String?

    var body: some View {
        HStack(spacing: 8) {
            StatusDot(state: thread.dot)
            Text(thread.title.isEmpty ? "New thread" : thread.title).font(.system(size: 13.5)).foregroundStyle(Hub.text2).lineLimit(1)
            Spacer(minLength: 8)
            Text(line ?? thread.project).font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(1)
        }
        .padding(.horizontal, 18).frame(height: 36)
        .frame(maxWidth: .infinity)
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
    }
}
