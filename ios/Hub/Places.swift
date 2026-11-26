import SwiftUI

// Where you have been, kept for a hold on the pill: each tab remembers the last five places opened in
// it, and a hold on its icon lists them. Like the pen, the hold goes on as one move: slide to a row and
// let go on it to jump there, let go outside to close, lift the finger without moving to leave it open
// to tap. The same drag across the pill (before the hold has time to come) changes tab icon by icon.

extension Nav {
    private func key(_ tab: Int) -> String { "\(world.rawValue)\(tab)" }

    /// A place was opened in `tab`: it goes first in the list, once.
    func remember(_ r: Route, tab: Int) {
        switch r {
        case .new, .newChat, .problem: return
        default: break
        }
        var list = recent[key(tab)] ?? []
        list.removeAll { $0 == r }
        list.insert(r, at: 0)
        recent[key(tab)] = Array(list.prefix(5))
    }

    func recents(_ tab: Int) -> [Route] { recent[key(tab)] ?? [] }

    /// Jumps straight to a place: the tab, with the place as its only page above the root.
    func open(_ r: Route, tab t: Int) {
        self.tab = t
        switch t { case 0: inbox = [r]; case 1: projects = [r]; case 2: machines = [r]; default: settings = [r] }
    }
}

/// Watches the four stacks and remembers what was pushed on them.
struct PlaceWatcher: ViewModifier {
    @Environment(Nav.self) private var nav

    func body(content: Content) -> some View {
        content
            .onChange(of: nav.inbox) { old, now in note(now, old, 0) }
            .onChange(of: nav.projects) { old, now in note(now, old, 1) }
            .onChange(of: nav.machines) { old, now in note(now, old, 2) }
            .onChange(of: nav.settings) { old, now in note(now, old, 3) }
    }

    private func note(_ now: [Route], _ old: [Route], _ tab: Int) {
        guard now.count > old.count, let r = now.last else { return }
        nav.remember(r, tab: tab)
    }
}

extension View {
    func watchesPlaces() -> some View { modifier(PlaceWatcher()) }
}

/// The list a hold on a pill icon opens.
struct RecentPlaces: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    let tab: Int
    @State private var frames: [Int: CGRect] = [:]
    @State private var hover: Int?
    @State private var start: CGPoint?
    @State private var moved = false

    var body: some View {
        let list = nav.recents(tab)
        ZStack(alignment: .bottomLeading) {
            Color.black.opacity(0.5).ignoresSafeArea().onTapGesture(perform: close)
            VStack(alignment: .leading, spacing: 0) {
                Text("Recent places").font(.system(size: 11.5, weight: .semibold)).foregroundStyle(Hub.muted)
                    .padding(.horizontal, 16).padding(.top, 12).padding(.bottom, 6)
                ForEach(Array(list.enumerated()), id: \.offset) { i, r in row(i, r, last: i == list.count - 1) }
            }
            .frame(width: 280)
            .background(RoundedRectangle(cornerRadius: 20).fill(Hub.surface2))
            .overlay(RoundedRectangle(cornerRadius: 20).strokeBorder(Hub.lineStrong))
            .shadow(color: .black.opacity(0.5), radius: 18, y: 10)
            .padding(.leading, 12).padding(.bottom, 84)
            .transition(.scale(scale: 0.9, anchor: .bottomLeading).combined(with: .opacity))
        }
        .onChange(of: nav.placesFinger) { _, now in follow(now) }
        .onChange(of: nav.placesLetGo) { letGo(list) }
    }

    private func close() { withAnimation(.snappy) { nav.placesTab = nil } }

    private func follow(_ at: CGPoint?) {
        guard let at else { return }
        if start == nil { start = at }
        if let s = start, hypot(at.x - s.x, at.y - s.y) > 24 { moved = true }
        let now = frames.first { $0.value.insetBy(dx: -24, dy: 0).contains(at) }?.key
        if now != hover {
            hover = now
            if now != nil { Haptics.tick() }
        }
    }

    private func letGo(_ list: [Route]) {
        defer { start = nil; moved = false }
        if let i = hover, list.indices.contains(i) {
            hover = nil
            go(list[i])
        } else if moved {
            close()
        }
    }

    private func go(_ r: Route) {
        Haptics.tap()
        withAnimation(.snappy) { nav.placesTab = nil }
        nav.open(r, tab: tab)
    }

    private func row(_ i: Int, _ r: Route, last: Bool) -> some View {
        let p = describe(r)
        return Button {
            guard TouchGuard.ok else { return }
            go(r)
        } label: {
            HStack(spacing: 11) {
                Image(systemName: p.icon).font(.system(size: 15)).foregroundStyle(Hub.accent).frame(width: 22)
                Text(p.title).font(.system(size: 14.5)).lineLimit(1)
                Spacer(minLength: 6)
                Text(p.sub).font(.system(size: 11.5)).foregroundStyle(Hub.muted).lineLimit(1)
            }
            .padding(.horizontal, 16).frame(height: 48)
            .background(hover == i ? Hub.accent.opacity(0.16) : .clear)
            .overlay(alignment: .bottom) { if !last { Hub.line.frame(height: 1).padding(.leading, 16) } }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { frames[i] = $0 }
    }

    /// What to call a place: an icon, a name and a short word for what it is.
    private func describe(_ r: Route) -> (icon: String, title: String, sub: String) {
        switch r {
        case let .thread(id):
            let t = store.thread(id)
            return ("text.bubble", (t?.title).flatMap { $0.isEmpty ? nil : $0 } ?? "Thread", t?.project ?? "Thread")
        case let .project(p): return ("folder", p.name, "Project")
        case let .machine(m): return ("server.rack", m, "Machine")
        case let .changes(id): return ("plus.forwardslash.minus", store.thread(id)?.title ?? "Changes", "Changes")
        case let .pull(id): return ("arrow.triangle.pull", "Pull request", id)
        case let .settings(page): return ("gearshape", String(describing: page).capitalized, "Settings")
        case let .file(p, path): return ("doc.text", (path as NSString).lastPathComponent, p.name)
        case let .space(id): return ("square.grid.2x2", store.spaces.first { $0.id == id }?.name ?? "Space", "Space")
        case .shelf: return ("archivebox", "Out of the inbox", "Inbox")
        case .routines, .routine: return ("clock.arrow.circlepath", "Routines", "Routines")
        case .problems, .problem: return ("exclamationmark.triangle", "Problems", "Problems")
        case .new, .newChat: return ("pencil", "New thread", "")
        }
    }
}
