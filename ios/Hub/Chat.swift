import SwiftUI

// The Chat world: the everyday sibling of Dev, in a warm theme. A chat is a normal Hub thread that
// starts without a project (a scratch folder, like a quick thread), so it needs no calls the phone
// does not already have. Spaces are a grouping kept on the phone. Subscriptions only.

/// A space: a named group of chats, kept on the phone.
struct ChatSpace: Codable, Identifiable, Hashable {
    var id = UUID().uuidString
    var name: String
    var icon: String = "folder"
    var note: String = ""
    var threadIds: [String] = []

    private static let key = "chat.spaces"
    static func load() -> [ChatSpace] {
        guard let data = UserDefaults.standard.data(forKey: key) else { return [] }
        return (try? JSONDecoder().decode([ChatSpace].self, from: data)) ?? []
    }
    static func save(_ list: [ChatSpace]) { UserDefaults.standard.set(try? JSONEncoder().encode(list), forKey: key) }
    static let icons = ["lightbulb", "airplane", "fork.knife", "book", "house", "heart", "cart", "figure.run"]
}

extension Store {
    /// Every chat: a thread started in the Chat world, on any machine, newest first.
    var chats: [HubThread] {
        threads.filter { isChat($0) && !$0.archived }
            .sorted { ($0.returned_at ?? $0.sent_at ?? $0.updated_at) > ($1.returned_at ?? $1.sent_at ?? $1.updated_at) }
    }

    func addSpace(_ name: String) {
        let n = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !n.isEmpty else { return }
        spaces.append(ChatSpace(name: n, icon: ChatSpace.icons[spaces.count % ChatSpace.icons.count]))
        ChatSpace.save(spaces)
    }

    func file(_ thread: String, in space: String?) {
        for i in spaces.indices { spaces[i].threadIds.removeAll { $0 == thread } }
        if let space, let i = spaces.firstIndex(where: { $0.id == space }) { spaces[i].threadIds.append(thread) }
        ChatSpace.save(spaces)
    }

    func space(of thread: String) -> ChatSpace? { spaces.first { $0.threadIds.contains(thread) } }
}

/// The title of a Chat page: a serif, large.
struct ChatTitle: View {
    let text: String
    var body: some View {
        Text(text).font(.system(size: 33, weight: .medium, design: .serif)).tracking(-0.3)
    }
}

/// A filter chip under a Chat page's title (the label of a menu): icon, name, a small chevron.
struct ChatChip: View {
    let icon: String
    let title: String
    let on: Bool
    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: icon).font(.system(size: 12))
            Text(title).lineLimit(1)
            Image(systemName: "chevron.down").font(.system(size: 9, weight: .semibold))
        }
        .font(.system(size: 13.5)).foregroundStyle(on ? Hub.accent : Hub.text2)
        .padding(.horizontal, 12).frame(height: 34)
        .background(Capsule().fill(on ? Hub.accentBg : Hub.surface))
        .overlay(Capsule().strokeBorder(on ? Hub.accent.opacity(0.5) : Hub.lineStrong))
    }
}

// MARK: Chats

struct ChatsView: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @State private var spaceFilter: String?
    @State private var oldestFirst = false
    /// The chat whose space is being chosen (a swipe to the left on its row).
    @State private var spaceFor: HubThread?

    var body: some View {
        let all = filtered
        let fresh = all.filter { $0.unread || $0.status == .waiting_on_you }
        let earlier = all.filter { t in !fresh.contains(where: { $0.id == t.id }) }
        NavigationStack(path: Bindable(nav).inbox) {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    PageHead(title: "Chat", sub: {
                        HStack(spacing: 14) {
                            if !fresh.isEmpty { HStack(spacing: 6) { Circle().fill(Hub.accent).frame(width: 7, height: 7); Text("\(fresh.count) new answer\(fresh.count == 1 ? "" : "s")") }.foregroundStyle(Hub.accent) }
                            else { Text("All read") }
                            Text("\(store.spaces.count) space\(store.spaces.count == 1 ? "" : "s")")
                        }
                        .font(.system(size: 13)).foregroundStyle(Hub.muted)
                    })
                    chips
                    if all.isEmpty { Text("Tap the pen to start a chat.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.horizontal, 18).padding(.top, 20) }
                    if !fresh.isEmpty { group("New answers", fresh) }
                    if !earlier.isEmpty { group("Earlier", earlier) }
                }
            }
            .contentMargins(.bottom, 96, for: .scrollContent)
            .scrollsToTop(0)
            .background(Hub.bg)
            .toolbar(.hidden, for: .navigationBar)
            .navigationDestination(for: Route.self) { RouteView(route: $0) }
        }
        .sheet(item: $spaceFor) { t in SpacePickSheet(thread: t).presentationDetents([.medium, .large]).presentationDragIndicator(.visible) }
    }

    private var filtered: [HubThread] {
        var list = store.chats
        if let id = spaceFilter, let s = store.spaces.first(where: { $0.id == id }) { list = list.filter { s.threadIds.contains($0.id) } }
        return oldestFirst ? list.reversed() : list
    }

    private var chips: some View {
        HStack(spacing: 8) {
            HubMenu {
                Button("All spaces") { spaceFilter = nil }
                ForEach(store.spaces) { s in Button(s.name) { spaceFilter = s.id } }
            } label: { ChatChip(icon: "square.grid.2x2", title: store.spaces.first { $0.id == spaceFilter }?.name ?? "All spaces", on: spaceFilter != nil) }
            HubMenu {
                Button("Recent first") { oldestFirst = false }
                Button("Oldest first") { oldestFirst = true }
            } label: { ChatChip(icon: "clock", title: oldestFirst ? "Oldest" : "Recent", on: false) }
            Spacer()
        }
        .padding(.horizontal, Metrics.side).padding(.vertical, Metrics.chipsVertical)
    }

    private func group(_ title: String, _ list: [HubThread]) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("\(title) · \(list.count)").font(.system(size: 12, weight: .semibold)).foregroundStyle(title == "New answers" ? Hub.accent : Hub.muted)
                .padding(.horizontal, Metrics.side).padding(.top, Metrics.sectionTop).padding(.bottom, Metrics.sectionBottom)
            ForEach(list) { t in
                StripSwipe(
                    right: SwipeAction(label: "Archive", icon: "archivebox", tint: Color(hex: 0x6a625a)) { store.tidy([t], .archive) },
                    left: SwipeAction(label: "Space", icon: "square.grid.2x2", tint: Color(hex: 0x4a5088)) { spaceFor = t }
                ) {
                    NavigationLink(value: Route.thread(t.id)) { row(t) }.buttonStyle(.press)
                }
                    .contextMenu {
                        Menu {
                            ForEach(store.spaces) { s in Button(s.name) { store.file(t.id, in: s.id) } }
                            if store.space(of: t.id) != nil { Button("Take out of its space", role: .destructive) { store.file(t.id, in: nil) } }
                        } label: { Label("Put in a space", systemImage: "square.grid.2x2") }
                        Button(role: .destructive) { store.tidy([t], .archive) } label: { Label("Archive", systemImage: "archivebox") }
                    }
            }
        }
    }

    private func row(_ t: HubThread) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 8) {
                if t.unread || t.status == .waiting_on_you { Circle().fill(Hub.accent).frame(width: 7, height: 7) }
                Text(t.title.isEmpty ? "New chat" : t.title).font(.system(size: 16, weight: t.unread ? .semibold : .medium)).lineLimit(1)
                Spacer(minLength: 6)
                if let s = store.space(of: t.id) { Text(s.name).font(.system(size: 11.5)).foregroundStyle(Hub.muted).lineLimit(1) }
                Text(relative(t.returned_at ?? t.updated_at)).font(.system(size: 12)).foregroundStyle(Hub.muted)
            }
            if let line = store.lastLine(t) { Text(line).font(.system(size: 13.5)).foregroundStyle(Hub.text3).lineLimit(1) }
        }
        .padding(.horizontal, 18).padding(.vertical, 10).frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
        .contentShape(Rectangle())
    }
}

// MARK: Spaces

struct SpacesView: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @State private var naming = false
    @State private var name = ""
    @State private var organizing = false

    var body: some View {
        NavigationStack(path: Bindable(nav).projects) {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    PageHead(title: "Spaces", sub: {
                        Text(store.spaces.isEmpty ? "No spaces yet" : "\(store.spaces.count) space\(store.spaces.count == 1 ? "" : "s")").font(.system(size: 13)).foregroundStyle(Hub.muted)
                    }, trailing: {
                        HStack(spacing: 8) {
                            Button { organizing = true } label: {
                                Label("Organize", systemImage: "wand.and.stars").font(.system(size: 13, weight: .medium)).foregroundStyle(Hub.accent)
                                    .padding(.horizontal, 12).frame(height: 32).background(Capsule().fill(Hub.accentBg))
                            }
                            .buttonStyle(.plain)
                            Button { name = ""; naming = true } label: {
                                Image(systemName: "plus").font(.system(size: 14, weight: .semibold)).foregroundStyle(Hub.text).frame(width: 32, height: 32).background(Circle().fill(Hub.surface2))
                            }
                            .buttonStyle(.plain).accessibilityLabel("New space")
                        }
                    })
                    VStack(spacing: 8) {
                        ForEach(store.spaces) { s in NavigationLink(value: Route.space(s.id)) { card(s) }.buttonStyle(.press) }
                    }
                    .padding(.horizontal, 14)
                    if store.spaces.isEmpty {
                        Text("A space gathers chats about one thing: a trip, the kitchen, a hobby. Make one with + and put chats in it from the Chats page (hold a chat).")
                            .font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18)
                    }
                }
            }
            .contentMargins(.bottom, 96, for: .scrollContent)
            .scrollsToTop(1)
            .background(Hub.bg)
            .toolbar(.hidden, for: .navigationBar)
            .navigationDestination(for: Route.self) { RouteView(route: $0) }
            .alert("New space", isPresented: $naming) {
                TextField("Name", text: $name)
                Button("Make") { store.addSpace(name) }
                Button("Cancel", role: .cancel) {}
            }
            .hubPanel("Organize", isPresented: $organizing) { OrganizePanel() }
        }
    }

    private func card(_ s: ChatSpace) -> some View {
        let chats = store.chats.filter { s.threadIds.contains($0.id) }
        return HStack(spacing: 12) {
            Image(systemName: s.icon).font(.system(size: 17)).foregroundStyle(Hub.accent).frame(width: 40, height: 40)
                .background(RoundedRectangle(cornerRadius: 12).fill(Hub.accentBg))
            VStack(alignment: .leading, spacing: 2) {
                Text(s.name).font(.system(size: 16, weight: .semibold))
                Text(chats.isEmpty ? "No chats yet" : "\(chats.count) chat\(chats.count == 1 ? "" : "s") · \(chats.first.map { store.lastLine($0) ?? $0.title } ?? "")")
                    .font(.system(size: 12.5)).foregroundStyle(Hub.muted).lineLimit(1)
            }
            Spacer()
            if chats.contains(where: { $0.unread }) { Circle().fill(Hub.accent).frame(width: 8, height: 8) }
            Image(systemName: "chevron.right").font(.system(size: 11)).foregroundStyle(Hub.muted)
        }
        .padding(12).background(RoundedRectangle(cornerRadius: 16).fill(Hub.surface))
    }
}

/// The chats that are in no space, each with a menu to file it; you tick what is right.
struct OrganizePanel: View {
    @Environment(Store.self) private var store

    var body: some View {
        let loose = store.chats.filter { store.space(of: $0.id) == nil }
        if loose.isEmpty { Text("Every chat is in a space.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.vertical, 8) }
        if store.spaces.isEmpty && !loose.isEmpty { Text("Make a space first with +.").font(.system(size: 13)).foregroundStyle(Hub.muted) }
        PanelGroup {
            ForEach(Array(loose.prefix(30).enumerated()), id: \.element.id) { i, t in
                HStack {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(t.title.isEmpty ? "New chat" : t.title).font(.system(size: 14)).lineLimit(1)
                        if let l = store.lastLine(t) { Text(l).font(.system(size: 11.5)).foregroundStyle(Hub.muted).lineLimit(1) }
                    }
                    Spacer()
                    HubMenu {
                        ForEach(store.spaces) { s in Button(s.name) { store.file(t.id, in: s.id) } }
                    } label: { Text("Put in…").font(.system(size: 13, weight: .medium)).foregroundStyle(Hub.accent) }
                }
                .padding(.horizontal, 12).frame(minHeight: 44)
                .overlay(alignment: .bottom) { if i < min(loose.count, 30) - 1 { Hub.line.frame(height: 1).padding(.leading, 12) } }
            }
        }
    }
}

/// One space: its chats.
struct SpacePage: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    let id: String

    var body: some View {
        let s = store.spaces.first { $0.id == id }
        let chats = store.chats.filter { s?.threadIds.contains($0.id) == true }
        VStack(spacing: 0) {
            HubHeader(parent: "Spaces", title: s?.name ?? "Space", sub: "\(chats.count) chat\(chats.count == 1 ? "" : "s")", back: { dismiss() }) {
                HubMenu {
                    Button(role: .destructive) { store.spaces.removeAll { $0.id == id }; ChatSpace.save(store.spaces); dismiss() } label: { Label("Delete the space", systemImage: "trash") }
                } label: { HeaderFace(icon: "ellipsis") }
            }
            ScrollView {
                LazyVStack(spacing: 0) {
                    if chats.isEmpty { Text("No chats here yet. Hold a chat on the Chats page and put it in this space.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(24) }
                    ForEach(chats) { t in
                        NavigationLink(value: Route.thread(t.id)) {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(t.title.isEmpty ? "New chat" : t.title).font(.system(size: 16, weight: .medium)).lineLimit(1)
                                if let l = store.lastLine(t) { Text(l).font(.system(size: 13.5)).foregroundStyle(Hub.text3).lineLimit(1) }
                            }
                            .padding(.horizontal, 18).padding(.vertical, 10).frame(maxWidth: .infinity, alignment: .leading)
                            .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
                        }
                        .buttonStyle(.press)
                    }
                }
            }
        }
        .background(Hub.bg)
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack { dismiss() }
    }
}

// MARK: Artifacts

/// The kinds of thing a chat makes, as the library names them (`outputs.rs`), in the order the
/// Artifacts page lists them. Servers a thread starts stay in Dev.
enum ArtifactKind: String, CaseIterable, Identifiable {
    case page, picture, doc, file

    var id: String { rawValue }
    var title: String {
        switch self {
        case .page: return "Pages"
        case .picture: return "Pictures"
        case .doc: return "Docs"
        case .file: return "Files"
        }
    }
    var icon: String {
        switch self {
        case .page: return "doc.richtext"
        case .picture: return "photo"
        case .doc: return "doc.text"
        case .file: return "doc"
        }
    }
}

/// One thing a chat made, and the chat's folder whose library it is read from.
struct Artifact: Identifiable {
    var project: ProjectRef
    var item: LibraryItem
    var kind: ArtifactKind
    var id: String { project.id + "#" + item.id }
}

/// What the chats made, by kind: pages the agent showed with `show_page` (built with Hub's kit),
/// pictures, docs and other files, read from the libraries of the chats' folders. Chips filter by
/// kind and by space; a page opens big, a picture full screen, a doc or file in the reader, and a
/// hold opens the chat that made it.
struct ArtifactsView: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @State private var made: [Artifact] = []
    @State private var loaded = false
    @State private var kindFilter: ArtifactKind?
    @State private var spaceFilter: String?
    @State private var bigPage: Artifact?

    private let columns = [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)]

    var body: some View {
        let shown = filtered
        NavigationStack(path: Bindable(nav).machines) {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    PageHead(title: "Artifacts", sub: {
                        Text(summary).font(.system(size: 13)).foregroundStyle(Hub.muted)
                    })
                    chips
                    if made.isEmpty {
                        if loaded {
                            Text("Pages, pictures, docs and files your chats make show up here, sorted by kind. Ask a chat for something to look at, like an illustration, a chart or a small tool, and it builds it as a page.")
                                .font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.horizontal, 18).padding(.top, 10)
                        }
                    } else if shown.isEmpty {
                        Text(kindFilter.map { "No \($0.title.lowercased()) here yet." } ?? "Nothing made in this space yet.")
                            .font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.horizontal, 18).padding(.top, 10)
                    }
                    ForEach(ArtifactKind.allCases) { kind in
                        let list = shown.filter { $0.kind == kind }
                        if !list.isEmpty { section(kind, list) }
                    }
                }
            }
            .contentMargins(.bottom, 96, for: .scrollContent)
            .scrollsToTop(2)
            .refreshable { await load(fresh: true) }
            .background(Hub.bg)
            .toolbar(.hidden, for: .navigationBar)
            .navigationDestination(for: Route.self) { RouteView(route: $0) }
            // The page lives as long as Chat does, so it reads again when a chat is added and each
            // time you come to it (the libraries are kept a minute, so that is cheap).
            .task(id: store.chats.count) { await load() }
            .onChange(of: nav.tab) { if nav.tab == 2 { Task { await load() } } }
            .sheet(item: $bigPage) { a in
                if let url = pageURL(a) { PageSheet(title: a.item.title, url: url) }
            }
        }
    }

    // MARK: Reading

    private var summary: String {
        if made.isEmpty { return loaded ? "Nothing made yet" : "Looking…" }
        let chats = Set(made.compactMap(\.item.thread)).count
        return "\(made.count) made in \(chats) chat\(chats == 1 ? "" : "s")"
    }

    private var filtered: [Artifact] {
        var list = made
        if let kindFilter { list = list.filter { $0.kind == kindFilter } }
        if let id = spaceFilter, let s = store.spaces.first(where: { $0.id == id }) {
            list = list.filter { a in a.item.thread.map { s.threadIds.contains($0) } ?? false }
        }
        return list
    }

    /// Every chat's library, one read per folder, at the same time. Only what that chat made
    /// counts, so an older scratch folder shared by several threads lists each thing once.
    private func load(fresh: Bool = false) async {
        var byFolder: [String: (ref: ProjectRef, chats: Set<String>)] = [:]
        for t in store.chats.prefix(40) {
            let ref = store.projectRef(of: t)
            byFolder[ref.id, default: (ref: ref, chats: [])].chats.insert(t.id)
        }
        let shared = store
        if fresh { for id in byFolder.keys { shared.libraryCache[id] = nil } }
        var out: [Artifact] = []
        await withTaskGroup(of: [Artifact].self) { group in
            for (ref, chats) in byFolder.values {
                group.addTask {
                    let items = await shared.library(ref)
                    return items.compactMap { (item: LibraryItem) -> Artifact? in
                        guard let thread = item.thread, chats.contains(thread), let kind = ArtifactKind(rawValue: item.kind) else { return nil }
                        return Artifact(project: ref, item: item, kind: kind)
                    }
                }
            }
            for await list in group { out += list }
        }
        made = out.sorted { $0.item.made_at > $1.item.made_at }
        loaded = true
    }

    // MARK: Pieces

    private var chips: some View {
        HStack(spacing: 8) {
            HubMenu {
                Button("All kinds") { kindFilter = nil }
                ForEach(ArtifactKind.allCases) { k in Button { kindFilter = k } label: { Label(k.title, systemImage: k.icon) } }
            } label: { ChatChip(icon: kindFilter?.icon ?? "square.stack.3d.up", title: kindFilter?.title ?? "All kinds", on: kindFilter != nil) }
            HubMenu {
                Button("All spaces") { spaceFilter = nil }
                ForEach(store.spaces) { s in Button(s.name) { spaceFilter = s.id } }
            } label: { ChatChip(icon: "square.grid.2x2", title: store.spaces.first { $0.id == spaceFilter }?.name ?? "All spaces", on: spaceFilter != nil) }
            Spacer()
        }
        .padding(.horizontal, 18).padding(.vertical, 10)
    }

    @ViewBuilder private func section(_ kind: ArtifactKind, _ list: [Artifact]) -> some View {
        Text("\(kind.title) · \(list.count)").font(.system(size: 12, weight: .semibold)).foregroundStyle(Hub.muted)
            .padding(.horizontal, 18).padding(.top, 16).padding(.bottom, 8)
        switch kind {
        case .page:
            LazyVGrid(columns: columns, spacing: 14) {
                ForEach(list) { a in pageCard(a) }
            }
            .padding(.horizontal, 14)
        case .picture:
            LazyVGrid(columns: columns, spacing: 10) {
                ForEach(list) { a in
                    LibraryPicture(project: a.project, item: a.item, height: 150).contextMenu { openChat(a) }
                }
            }
            .padding(.horizontal, 14)
        case .doc, .file:
            ForEach(list) { a in
                NavigationLink(value: route(a)) { row(a) }.buttonStyle(.press).contextMenu { openChat(a) }
            }
        }
    }

    /// A page: the page itself, small and asleep (it never takes a touch here), its name and its chat.
    private func pageCard(_ a: Artifact) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Group {
                if let url = pageURL(a) {
                    PageWeb(url: url, interactive: false, height: .constant(nil), zoom: 0.5)
                } else {
                    Image(systemName: "doc.richtext").font(.system(size: 22)).foregroundStyle(Hub.muted)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .frame(height: 132).frame(maxWidth: .infinity)
            .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
            .clipShape(RoundedRectangle(cornerRadius: 14))
            .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(Hub.lineStrong))
            .padding(.bottom, 4)
            Text(a.item.title).font(.system(size: 13.5, weight: .medium)).lineLimit(1)
            Text(source(a)).font(.system(size: 11.5)).foregroundStyle(Hub.muted).lineLimit(1)
        }
        // Over the web view, so the tap is ours and never the page's.
        .overlay { Color.black.opacity(0.001) }
        .contentShape(Rectangle())
        .tapRow { Haptics.tap(); bigPage = a }
        .contextMenu { openChat(a) }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
    }

    private func row(_ a: Artifact) -> some View {
        HStack(spacing: 11) {
            Image(systemName: a.kind.icon).font(.system(size: 15)).foregroundStyle(Hub.accent)
                .frame(width: 34, height: 34).background(RoundedRectangle(cornerRadius: 10).fill(Hub.accentBg))
            VStack(alignment: .leading, spacing: 2) {
                Text(a.item.title).font(.system(size: 15)).lineLimit(1)
                Text(source(a)).font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(1)
            }
            Spacer(minLength: 6)
            Text(relative(a.item.made_at)).font(.system(size: 12)).foregroundStyle(Hub.muted)
        }
        .padding(.horizontal, 18).padding(.vertical, 9)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 63) }
        .contentShape(Rectangle())
    }

    @ViewBuilder private func openChat(_ a: Artifact) -> some View {
        if let thread = a.item.thread {
            Button { nav.push(.thread(thread)) } label: { Label("Open the chat", systemImage: "bubble.left") }
        }
    }

    /// The chat it came from, and its space when it has one.
    private func source(_ a: Artifact) -> String {
        let chat = a.item.thread.flatMap { id in store.threads.first { $0.id == id }?.title }.flatMap { $0.isEmpty ? nil : $0 }
            ?? a.item.thread_title ?? "A chat"
        if let id = a.item.thread, let space = store.space(of: id) { return "\(chat) · \(space.name)" }
        return chat
    }

    private func pageURL(_ a: Artifact) -> URL? {
        guard let thread = a.item.open?.thread ?? a.item.thread, let page = a.item.open?.page else { return nil }
        return store.client(for: thread)?.base.appendingPathComponent("api/threads/\(thread)/pages/\(page)")
    }

    /// A doc or file opens in the reader when the machine has it (`phone.files`) and it lies in the
    /// chat's folder; otherwise the chat that made it opens.
    private func route(_ a: Artifact) -> Route {
        if let path = a.item.open?.path, store.has("phone.files", on: store.url(of: a.project.host)),
           let rel = Self.pathInside(path, root: a.project.path) {
            return .file(a.project, rel)
        }
        return a.item.thread.map(Route.thread) ?? .project(a.project)
    }

    /// `path` inside `root` as a path from it with forward slashes, or nil when it is elsewhere.
    static func pathInside(_ path: String, root: String) -> String? {
        let p = path.replacingOccurrences(of: "\\", with: "/")
        var r = root.replacingOccurrences(of: "\\", with: "/")
        while r.hasSuffix("/") { r.removeLast() }
        guard !r.isEmpty, p.count > r.count + 1, p.lowercased().hasPrefix(r.lowercased() + "/") else { return nil }
        return String(p.dropFirst(r.count + 1))
    }
}


/// Which space a chat goes in: Hub's own list, a tap picks (the swipe to the left on a chat row).
struct SpacePickSheet: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    let thread: HubThread

    var body: some View {
        let current = store.space(of: thread.id)?.id
        VStack(alignment: .leading, spacing: 0) {
            Text("Put in a space").font(.system(size: 17, weight: .semibold)).padding(.horizontal, 20).padding(.top, 22).padding(.bottom, 4)
            Text(thread.title.isEmpty ? "New chat" : thread.title).font(.system(size: 13)).foregroundStyle(Hub.muted).lineLimit(1)
                .padding(.horizontal, 20).padding(.bottom, 12)
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(store.spaces) { sp in
                        pick(sp.name, icon: sp.icon, on: sp.id == current) { store.file(thread.id, in: sp.id) }
                    }
                    if current != nil { pick("Take out of its space", icon: "arrow.uturn.backward", on: false) { store.file(thread.id, in: nil) } }
                    if store.spaces.isEmpty { Text("No spaces yet. Make one from the Spaces tab.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(20) }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Hub.surface.ignoresSafeArea())
    }

    private func pick(_ name: String, icon: String, on: Bool, _ action: @escaping () -> Void) -> some View {
        HStack(spacing: 12) {
            Image(systemName: icon).foregroundStyle(Hub.accent).frame(width: 22)
            Text(name).font(.system(size: 15.5))
            Spacer()
            if on { Image(systemName: "checkmark").foregroundStyle(Hub.accent) }
        }
        .padding(.horizontal, 20).frame(height: 52).contentShape(Rectangle())
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 20) }
        .tapRow(flat: true) { Haptics.tap(); action(); dismiss() }
    }
}
