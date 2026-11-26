import SwiftUI

/// An idea said out loud and kept for later (sketch "Inkorgen", screen 4): it waits in the inbox
/// under Drafts, on the phone, with the project Hub guessed. Nothing runs until you open it.
struct IdeaDraft: Codable, Identifiable, Hashable {
    var id = UUID().uuidString
    var text: String
    var project: String?
    var path: String?
    var host: String?
    var at: Double = Date().timeIntervalSince1970 * 1000

    var ref: ProjectRef? { path.map { ProjectRef(path: $0, name: project ?? "", host: host) } }

    private static let key = "ideadrafts"
    static func load() -> [IdeaDraft] {
        guard let data = UserDefaults.standard.data(forKey: key) else { return [] }
        return (try? JSONDecoder().decode([IdeaDraft].self, from: data)) ?? []
    }
    static func save(_ list: [IdeaDraft]) {
        UserDefaults.standard.set(try? JSONEncoder().encode(list), forKey: key)
    }
}

extension Store {
    func saveIdeaDraft(_ text: String, in project: ProjectRef?) {
        let p = project.flatMap { $0.isScratch ? nil : $0 }
        ideaDrafts.insert(IdeaDraft(text: text, project: p?.name, path: p?.path, host: p?.host), at: 0)
        IdeaDraft.save(ideaDrafts)
    }

    func removeIdeaDraft(_ id: String) {
        ideaDrafts.removeAll { $0.id == id }
        IdeaDraft.save(ideaDrafts)
    }
}

/// What a hold on the pen offers (sketch "Inkorgen", screen 3). The hold goes on as one move: slide to a
/// row (it lights up, with a tick) and let go on it to choose; let go outside and it closes; lift the
/// finger without moving and it stays open to tap. The menu hangs off the pen, so a drag down or toward
/// the corner folds it back into the pen.
struct PenMenu: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @State private var frames: [Int: CGRect] = [:]
    @State private var hover: Int?
    @State private var start: CGPoint?
    @State private var moved = false
    @State private var away: CGSize = .zero

    private struct Entry { let icon: String; let title: String; let hint: String; let run: () -> Void }

    private var entries: [Entry] {
        let last = store.lastPlace?.project
        return [
            Entry(icon: "pencil", title: last.map { "New thread in \($0.name)" } ?? "New thread", hint: last.map { $0.host ?? store.machine ?? "" } ?? "") {
                nav.push(.new(last, talk: false))
            },
            Entry(icon: "mic", title: "Talk an idea", hint: "hold") { nav.talking = true },
            Entry(icon: "folder", title: "New thread in…", hint: "") { nav.pickProjectFirst = true; nav.push(.new(nil, talk: false)) },
            Entry(icon: "cloud", title: "Thread without a project", hint: "") { nav.push(.new(.noProject(on: last?.host), talk: false)) },
        ]
    }

    /// How far the menu has been dragged toward the pen (down or to the right).
    private var pulled: CGFloat { max(0, max(away.width, away.height)) }

    var body: some View {
        let list = entries
        let dim: Double = 0.5 * (1 - Double(min(1, pulled / 220)))
        let shrink: CGFloat = 1 - (min(pulled, 200) / 200) * 0.18
        let fade: Double = 1 - Double(min(pulled, 260) / 260) * 0.6
        ZStack(alignment: .bottomTrailing) {
            Color.black.opacity(dim).ignoresSafeArea().onTapGesture(perform: close)
            VStack(spacing: 0) {
                ForEach(Array(list.enumerated()), id: \.offset) { i, e in row(i, e, last: i == list.count - 1) }
            }
            .frame(width: 300)
            .background(RoundedRectangle(cornerRadius: 20).fill(Hub.surface2))
            .overlay(RoundedRectangle(cornerRadius: 20).strokeBorder(Hub.lineStrong))
            .shadow(color: .black.opacity(0.5), radius: 18, y: 10)
            .scaleEffect(shrink, anchor: .bottomTrailing)
            .offset(x: max(0, away.width) * 0.6, y: max(0, away.height) * 0.6)
            .opacity(fade)
            .simultaneousGesture(fold)
            .padding(.trailing, 16).padding(.bottom, 84)
            .transition(.scale(scale: 0.9, anchor: .bottomTrailing).combined(with: .opacity))
        }
        .onChange(of: nav.penFinger) { _, now in follow(now, list) }
        .onChange(of: nav.penLetGo) { letGo(list) }
    }

    private func close() { withAnimation(.snappy) { nav.penMenu = false } }

    /// The finger of the hold moves: light the row under it.
    private func follow(_ at: CGPoint?, _ list: [Entry]) {
        guard let at else { return }
        if start == nil { start = at }
        if let s = start, hypot(at.x - s.x, at.y - s.y) > 24 { moved = true }
        // A little slack to the sides and above, as a finger is wider than a line.
        let now = frames.first { $0.value.insetBy(dx: -24, dy: 0).contains(at) }?.key
        if now != hover {
            hover = now
            if now != nil { Haptics.tick() }
        }
    }

    private func letGo(_ list: [Entry]) {
        defer { start = nil; moved = false }
        if let i = hover, list.indices.contains(i) {
            hover = nil
            Haptics.tap()
            withAnimation(.snappy) { nav.penMenu = false }
            list[i].run()
        } else if moved {
            close()
        }
    }

    /// Dragging the menu toward the pen folds it back in; a short drag springs back.
    private var fold: some Gesture {
        DragGesture(minimumDistance: 14, coordinateSpace: .global)
            .onChanged { v in
                let dx = v.translation.width, dy = v.translation.height
                guard dy > 0 || dx > 0, abs(dy) + abs(dx) > 0 else { return }
                away = CGSize(width: dx, height: dy)
            }
            .onEnded { v in
                TouchGuard.swiped()
                let far = pulled > 70 || max(v.predictedEndTranslation.height, v.predictedEndTranslation.width) > 200
                if far { Haptics.tap(); withAnimation(.snappy) { nav.penMenu = false } }
                else { withAnimation(.snappy) { away = .zero } }
            }
    }

    private func row(_ i: Int, _ e: Entry, last: Bool) -> some View {
        Button {
            guard TouchGuard.ok else { return }
            Haptics.tap()
            withAnimation(.snappy) { nav.penMenu = false }
            e.run()
        } label: {
            HStack(spacing: 12) {
                Image(systemName: e.icon).font(.system(size: 16)).foregroundStyle(Hub.accent).frame(width: 22)
                Text(e.title).font(.system(size: 15.5)).lineLimit(1)
                Spacer(minLength: 6)
                if !e.hint.isEmpty { Text(e.hint).font(.system(size: 12.5)).foregroundStyle(Hub.muted) }
            }
            .padding(.horizontal, 16).frame(height: 52)
            .background(hover == i ? Hub.accent.opacity(0.16) : .clear)
            .overlay(alignment: .bottom) { if !last { Hub.line.frame(height: 1).padding(.leading, 16) } }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { frames[i] = $0 }
    }
}

/// Talk an idea: hold the mic to speak, let go to stop (a tap starts and stops too); the words
/// show as you say them. Save draft puts them in the inbox under Drafts, nothing runs.
struct TalkIdea: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @State private var dictation = Dictation()
    @State private var pressed: Date?
    @State private var wasListening = false
    /// How far the page has been pulled down; past a line, letting go closes it.
    @State private var pull: CGFloat = 0
    @State private var pulling: Bool?

    var body: some View {
        VStack(spacing: 0) {
            Spacer()
            VStack(alignment: .leading, spacing: 14) {
                if dictation.listening {
                    TimelineView(.periodic(from: .now, by: 1)) { _ in
                        let s = Int(Date().timeIntervalSince(dictation.started))
                        HStack(spacing: 7) { StatusDot(state: .running); Text("Listening · \(s / 60):\(String(format: "%02d", s % 60))") }
                    }
                    .font(.system(size: 13)).foregroundStyle(Hub.text3)
                } else if let error = dictation.error {
                    Text(error).font(.system(size: 13)).foregroundStyle(Hub.red)
                } else {
                    Text(dictation.text.isEmpty ? "Hold the mic and say it" : "Stopped").font(.system(size: 13)).foregroundStyle(Hub.muted)
                }
                Text(dictation.text).font(.system(size: 30, weight: .regular)).lineSpacing(3)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let p = store.lastPlace?.project {
                    Text("Goes to Drafts in the inbox, for \(p.name)").font(.system(size: 12.5)).foregroundStyle(Hub.muted)
                }
            }
            .padding(.horizontal, 24)
            Spacer()
            HStack(spacing: 14) {
                Button("Cancel") { dictation.cancel(); close() }
                    .buttonStyle(PillButtonStyle()).frame(maxWidth: .infinity)
                mic
                Button("Save draft") { save() }
                    .buttonStyle(PillButtonStyle(kind: .primary)).frame(maxWidth: .infinity)
                    .disabled(dictation.text.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .padding(.horizontal, 20).padding(.bottom, 26)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Hub.bg.ignoresSafeArea())
        .clipShape(RoundedRectangle(cornerRadius: pull > 0 ? 28 : 0, style: .continuous))
        .shadow(color: .black.opacity(pull > 0 ? 0.6 : 0), radius: 24, y: -8)
        .offset(y: pull)
        .simultaneousGesture(pullDown)
        .task { await dictation.start() }
        .onDisappear { dictation.cancel() }
    }

    /// A swipe down closes the page. Words already said are kept as a draft, since a swipe should not
    /// lose them; with none, it only closes. The bottom of the page (the mic and its buttons) is left alone.
    private var pullDown: some Gesture {
        DragGesture(minimumDistance: 18, coordinateSpace: .global)
            .onChanged { v in
                if pulling == nil {
                    pulling = v.translation.height > 0 && abs(v.translation.height) > abs(v.translation.width) * 1.5
                        && v.startLocation.y < UIScreen.main.bounds.height - 200
                    if pulling == true { Haptics.ready() }
                }
                guard pulling == true else { return }
                pull = max(0, v.translation.height) * 0.9
            }
            .onEnded { v in
                let mine = pulling == true
                pulling = nil
                guard mine else { return }
                TouchGuard.swiped()
                if pull > 120 || v.predictedEndTranslation.height > 320 {
                    if dictation.text.trimmingCharacters(in: .whitespaces).isEmpty { dictation.cancel(); close() } else { save() }
                } else {
                    withAnimation(.snappy) { pull = 0 }
                }
            }
    }

    private var mic: some View {
        Image(systemName: "mic.fill").font(.system(size: 26)).foregroundStyle(Hub.bg)
            .frame(width: 84, height: 84)
            .background(Circle().fill(Hub.accent))
            .overlay(Circle().strokeBorder(Hub.accent.opacity(dictation.listening ? 0.5 : 0), lineWidth: 6 + CGFloat(dictation.level) * 14).padding(-6))
            .animation(.easeOut(duration: 0.15), value: dictation.level)
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in
                        guard pressed == nil else { return }
                        pressed = Date(); wasListening = dictation.listening
                        if !dictation.listening { Task { await dictation.start() } }
                    }
                    .onEnded { _ in
                        let held = Date().timeIntervalSince(pressed ?? Date())
                        pressed = nil
                        if (held > 0.5 || wasListening) && dictation.listening { dictation.stop() }
                    }
            )
            .accessibilityLabel("Hold to speak")
            .accessibilityAddTraits(.isButton)
    }

    private func save() {
        let text = dictation.stop()
        guard !text.isEmpty else { return }
        store.saveIdeaDraft(text, in: store.lastPlace?.project)
        store.flash("Draft saved in the inbox")
        close()
    }

    private func close() { withAnimation(.snappy) { nav.talking = false } }
}
