import SwiftUI

// Focus on the phone, as on the desktop: one project Hub works in. The project button in the pill is
// marked and goes straight in, the inbox shows only that project and counts what the others hold back,
// and the Projects page keeps its card on top. Leave (or Unlock) lets go.

extension Nav {
    /// The project Hub is focused on, kept across launches.
    static func loadFocus() -> ProjectRef? {
        guard let a = UserDefaults.standard.stringArray(forKey: "focusProject"), a.count == 3 else { return nil }
        return ProjectRef(path: a[0], name: a[1], host: a[2].isEmpty ? nil : a[2])
    }

    func setFocus(_ p: ProjectRef?) {
        withAnimation(.snappy) { focus = p }
        if let p { UserDefaults.standard.set([p.path, p.name, p.host ?? ""], forKey: "focusProject") }
        else { UserDefaults.standard.removeObject(forKey: "focusProject") }
        Haptics.done()
    }

    /// The project button: straight into the focused project, from any tab. The tab and the stack change
    /// together without animation, so no page slides under another half way.
    func enterFocus() {
        guard let f = focus else { return }
        var t = Transaction(); t.disablesAnimations = true
        withTransaction(t) {
            if tab != 1 { tab = 1 }
            if projects != [Route.project(f)] { projects = [Route.project(f)] }
        }
        Haptics.tick()
    }
}

extension Store {
    /// Whether a thread belongs to a project (same folder, same machine).
    func belongs(_ t: HubThread, to p: ProjectRef) -> Bool {
        Self.projectRoot(t.cwd) == p.path && t.host == p.host
    }
}

/// The bar on top of the inbox while focused: which project, how many things the others hold, Leave.
struct FocusBar: View {
    @Environment(Nav.self) private var nav
    @Environment(Store.self) private var store
    let project: ProjectRef

    var body: some View {
        let held = store.threads(in: .needsYou).filter { !store.belongs($0, to: project) }.count
        HStack(spacing: 8) {
            Image(systemName: "lock.fill").font(.system(size: 11, weight: .semibold)).foregroundStyle(Hub.accent)
            Text(project.name).font(.system(size: 14, weight: .semibold)).lineLimit(1)
            if held > 0 { Text("\(held) held").font(.system(size: 12.5)).foregroundStyle(Hub.muted) }
            Spacer(minLength: 0)
            Button { nav.setFocus(nil) } label: {
                Text("Leave").font(.system(size: 13, weight: .semibold)).foregroundStyle(Hub.accent)
                    .padding(.horizontal, 12).frame(height: 28).background(Capsule().fill(Hub.accentBg))
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 14).frame(height: 42)
        .background(Capsule().fill(Hub.surface))
        .overlay(Capsule().strokeBorder(Hub.accent.opacity(0.35)))
        .padding(.horizontal, 14).padding(.bottom, 8)
        .accessibilityElement(children: .combine)
    }
}

/// The focused project on top of the Projects page: its stage and the numbers that matter.
struct FocusHero: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    let project: ProjectRef
    let threads: [HubThread]

    var body: some View {
        let color = Hub.projectColor(project.name)
        let summary = store.summaries[project.id]
        let running = threads.filter { $0.status == .running }.count
        let waiting = threads.filter { $0.status == .waiting_on_you }.count
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 6) {
                Image(systemName: "lock.fill").font(.system(size: 11, weight: .semibold))
                Text(["Working", project.host].compactMap { $0 }.joined(separator: " · ")).font(.system(size: 12.5))
                Spacer()
                Button { nav.setFocus(nil) } label: {
                    HStack(spacing: 4) { Image(systemName: "lock.open").font(.system(size: 11)); Text("Unlock") }
                        .font(.system(size: 12.5, weight: .medium)).foregroundStyle(Hub.text2)
                        .padding(.horizontal, 10).frame(height: 28).background(Capsule().fill(Color.black.opacity(0.25)))
                }
                .buttonStyle(.plain)
            }
            .foregroundStyle(Hub.accent)
            VStack(alignment: .leading, spacing: 3) {
                Text(project.name).font(.system(size: 24, weight: .bold))
                Text(summary.flatMap { s in s.stage.map { "Stage \(s.n) · \($0)" } } ?? "No stages yet")
                    .font(.system(size: 13)).foregroundStyle(Hub.text3).lineLimit(1)
            }
            ProgressBar(value: summary.map { $0.of > 0 ? Double($0.done) / Double($0.of) : 1 } ?? 0)
            HStack(spacing: 8) {
                stat(summary?.to_test ?? 0, "to test", Hub.amber)
                stat(running, "running", Hub.green)
                stat(waiting, "need you", Hub.text)
            }
        }
        .padding(14)
        .background(RoundedRectangle(cornerRadius: 16).fill(LinearGradient(colors: [color.opacity(0.22), Hub.surface], startPoint: .topLeading, endPoint: .bottomTrailing)))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(color.opacity(0.3)))
    }

    private func stat(_ n: Int, _ label: String, _ tint: Color) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text("\(n)").font(.system(size: 18, weight: .bold)).foregroundStyle(n > 0 ? tint : Hub.muted)
            Text(label).font(.system(size: 11)).foregroundStyle(Hub.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 10).padding(.vertical, 7)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color.black.opacity(0.22)))
    }
}

/// A project as a quiet row: its folder in its colour, what it is on, what is left to test, how far the stage is.
/// A tap opens it; a hold offers Set as working.
struct ProjectRow: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    let project: ProjectRef
    let threads: [HubThread]
    @State private var held = false
    @State private var frame: CGRect = .zero

    var body: some View {
        let color = Hub.projectColor(project.name)
        let summary = store.summaries[project.id]
        let waiting = threads.filter { $0.status == .waiting_on_you }.count
        let running = threads.filter { $0.status == .running }.count
        Button {
            guard TouchGuard.ok else { return }
            if held { held = false; return }
            nav.projects.append(.project(project))
        } label: {
            HStack(spacing: 12) {
                Image(systemName: "folder").font(.system(size: 16)).foregroundStyle(color)
                    .frame(width: 36, height: 36).background(RoundedRectangle(cornerRadius: 10).fill(color.opacity(0.15)))
                VStack(alignment: .leading, spacing: 2) {
                    Text(project.name).font(.system(size: 15.5, weight: .semibold)).lineLimit(1)
                    Text(sub(summary)).font(.system(size: 12.5)).foregroundStyle(Hub.muted).lineLimit(1)
                }
                Spacer(minLength: 6)
                VStack(alignment: .trailing, spacing: 5) {
                    if let s = summary, s.to_test > 0 {
                        Text("\(s.to_test) to test").font(.system(size: 11.5, weight: .semibold)).foregroundStyle(Hub.amber)
                            .padding(.horizontal, 7).padding(.vertical, 2).background(Capsule().fill(Hub.amberBg))
                    }
                    HStack(spacing: 8) {
                        if waiting > 0 { HStack(spacing: 3) { StatusDot(state: .waiting); Text("\(waiting)") } }
                        if running > 0 { HStack(spacing: 3) { StatusDot(state: .running); Text("\(running)") } }
                    }
                    .font(.system(size: 12)).foregroundStyle(Hub.muted)
                }
                if let s = summary, s.of > 0 { StageRing(value: Double(s.done) / Double(s.of)) }
            }
            .padding(.horizontal, 18).padding(.vertical, 10)
            .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 66) }
            .contentShape(Rectangle())
        }
        .buttonStyle(.press)
        .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { frame = $0 }
        .simultaneousGesture(
            LongPressGesture(minimumDuration: 0.4).onEnded { _ in
                held = true; Haptics.firm()
                let p = project
                withAnimation(.snappy(duration: 0.2)) {
                    nav.menu = HubMenuRequest(anchor: frame, content: AnyView(Group {
                        Button { nav.setFocus(p) } label: { Label("Set as working", systemImage: "lock") }
                        Button { nav.projects.append(.project(p)) } label: { Label("Open", systemImage: "arrow.up.right") }
                    }))
                }
            }
        )
    }

    private func sub(_ s: FocusSummary?) -> String {
        var parts: [String] = []
        if let s, let stage = s.stage { parts.append("Stage \(s.n) · \(stage)") }
        else if s != nil { parts.append("Every stage delivered") }
        else { parts.append(threads.isEmpty ? "No threads yet" : "\(threads.count) thread\(threads.count == 1 ? "" : "s")") }
        if let h = project.host { parts.append(h) }
        return parts.joined(separator: " · ")
    }
}

/// A small ring for how far a stage is.
struct StageRing: View {
    let value: Double
    var body: some View {
        ZStack {
            Circle().stroke(Hub.lineStrong, lineWidth: 3)
            Circle().trim(from: 0, to: min(1, max(0, value))).stroke(Hub.green, style: StrokeStyle(lineWidth: 3, lineCap: .round)).rotationEffect(.degrees(-90))
            Text("\(Int(value * 100))").font(.system(size: 10)).foregroundStyle(Hub.text3)
        }
        .frame(width: 30, height: 30)
    }
}

/// The bar on a peeked project (not the focused one): one tap back to the focused one.
struct PeekBar: View {
    @Environment(Nav.self) private var nav
    let focus: ProjectRef

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "eye").font(.system(size: 12)).foregroundStyle(Hub.accent)
            Text("Peeking").font(.system(size: 13)).foregroundStyle(Hub.accent)
            Spacer()
            Button { Haptics.tap(); nav.enterFocus() } label: {
                Text("Back to \(focus.name)").font(.system(size: 12.5, weight: .semibold)).foregroundStyle(Hub.bg)
                    .padding(.horizontal, 12).frame(height: 26).background(Capsule().fill(Hub.accent))
            }
            .buttonStyle(.plain)
        }
        .padding(.leading, 14).padding(.trailing, 6).frame(height: 36)
        .background(Capsule().fill(Hub.accentBg))
        .overlay(Capsule().strokeBorder(Hub.accent.opacity(0.5)))
        .padding(.horizontal, 14).padding(.bottom, 4)
    }
}

/// One part of a project on its home grid: what it is and one live fact.
struct PartTile: View {
    let icon: String
    let title: String
    let fact: String
    var factTint: Color = Hub.muted

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Image(systemName: icon).font(.system(size: 19)).foregroundStyle(Hub.text3)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 14, weight: .semibold)).foregroundStyle(Hub.text)
                Text(fact).font(.system(size: 11.5)).foregroundStyle(factTint).lineLimit(2).multilineTextAlignment(.leading)
            }
        }
        .frame(maxWidth: .infinity, minHeight: 88, alignment: .topLeading)
        .padding(11)
        .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
    }
}
