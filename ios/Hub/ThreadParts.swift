import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// The thread's top (sketch "Ramen", screen 6): a round back button, the title in a capsule with
/// where the thread stands under it (the inbox row's words, then project and machine), and a round
/// ⋯ for the thread menu, which also moves it to another machine.
struct ThreadTop: View {
    @Environment(Store.self) private var store
    let thread: HubThread?
    let back: () -> Void
    let menu: () -> Void
    /// The card about the thread (`ThreadAbout`); true opens it ready to rename.
    var about: (Bool) -> Void = { _ in }
    /// The machine has an iPhone simulator running: a round phone button opens it (`Simulator.swift`).
    var simulator: SimState?
    var openSimulator: () -> Void = {}

    var body: some View {
        HStack(spacing: 8) {
            Button(action: back) {
                Image(systemName: "chevron.left").font(.system(size: 16, weight: .semibold)).foregroundStyle(Hub.text)
                    .frame(width: 42, height: 42).background(Circle().fill(Hub.surface2))
            }
            .buttonStyle(.plain).accessibilityLabel("Back")
            VStack(spacing: 1) {
                Text(thread.map { $0.title.isEmpty ? "New thread" : $0.title } ?? "Thread").font(.system(size: 14, weight: .semibold)).lineLimit(1)
                Text(summary).font(.system(size: 11)).foregroundStyle(Hub.muted).lineLimit(1)
            }
            .padding(.horizontal, 14).frame(maxWidth: .infinity, minHeight: 42)
            .background(Capsule().fill(Hub.surface2))
            .contentShape(Capsule())
            // A tap or a pull down opens what the thread is; a hold opens it ready to rename.
            .safeTap { about(false) }
            .onLongPressGesture(minimumDuration: 0.4) { Haptics.ready(); about(true) }
            .simultaneousGesture(DragGesture(minimumDistance: 14).onEnded { v in
                if v.translation.height > 30, abs(v.translation.width) < v.translation.height { Haptics.tap(); about(false) }
            })
            .accessibilityAddTraits(.isButton)
            .accessibilityHint("About the thread: name, engine, machine")
            if let simulator {
                Button(action: openSimulator) {
                    Image(systemName: "iphone").font(.system(size: 16, weight: .semibold)).foregroundStyle(simulator.live ? Hub.green : Hub.text)
                        .frame(width: 42, height: 42).background(Circle().fill(Hub.surface2)).contentShape(Circle())
                }
                .buttonStyle(.plain).accessibilityLabel("iPhone simulator")
            }
            Button(action: menu) {
                Image(systemName: "ellipsis").font(.system(size: 16, weight: .semibold)).foregroundStyle(Hub.text)
                    .frame(width: 42, height: 42).background(Circle().fill(Hub.surface2)).contentShape(Circle())
            }
            .buttonStyle(.plain).accessibilityLabel("Thread menu")
        }
        .padding(.horizontal, 12).padding(.top, 4).padding(.bottom, 8)
    }

    /// The same words as the inbox row: where it stands, its project and its machine.
    private var summary: String {
        guard let t = thread else { return "" }
        if store.planToApprove(t) != nil { return "Needs you · plan" }
        return [t.statusWord, store.isChat(t) ? "Chat" : t.isQuick ? "Quick" : t.project, t.host ?? store.machine].compactMap { $0 }.joined(separator: " · ")
    }
}

/// Run this thread on another machine (inspiration round 2, screen 2).
struct MovePanel: View {
    @Environment(Store.self) private var store
    let thread: HubThread
    let done: () -> Void

    var body: some View {
        let here = thread.host ?? store.machine
        let names = store.hosts.isEmpty ? store.machines.map(\.name) : store.hosts.map(\.name)
        PanelGroup {
            ForEach(Array(names.enumerated()), id: \.element) { i, name in
                let up = MachineIcon.up(name, store: store)
                let running = store.threads.filter { ($0.host ?? store.machine) == name && $0.status == .running }.count
                let has = store.projects.contains { $0.name == thread.project && ($0.host ?? store.machine) == name }
                PanelRow(title: name,
                         sub: !up ? "Down" : name == here ? "Here now\(running > 1 ? " · \(running - 1) other running" : "")"
                            : "\(running == 0 ? "Nothing running" : "\(running) running") · \(has ? "\(thread.project) is there" : "gets \(thread.project) copied over")",
                         dim: !up, last: i == names.count - 1) {
                    Image(systemName: MachineIcon.name(name, store: store)).font(.system(size: 14)).foregroundStyle(Hub.text)
                        .frame(width: 28, height: 28).background(RoundedRectangle(cornerRadius: 9).fill(Hub.surface2))
                } trailing: {
                    if name == here { Text("Here").font(.system(size: 12, weight: .semibold)).foregroundStyle(Hub.accent) }
                    else { Image(systemName: "chevron.right").font(.system(size: 12)).foregroundStyle(Hub.muted) }
                } action: {
                    guard name != here else { return }
                    Task { await store.move(thread, to: name) }
                    done()
                }
            }
        }
        Text("The thread carries on there with its branch and .env. A running turn stops and picks up over there.")
            .font(.system(size: 11.5)).foregroundStyle(Hub.muted)
            .task { if store.hosts.isEmpty { await store.loadMachines() } }
    }
}

/// Engine and its settings (inspiration round 2, screens 3 to 5): providers first, then the
/// chosen one's own settings as it reports them, effort as rows with a filling ring.
struct EnginePanel: View {
    @Environment(Store.self) private var store
    let thread: HubThread
    /// Closes the panel: a model picked is the choice made, so the panel goes once the check has shown.
    var done: () -> Void = {}
    @State private var sel: String?
    /// What you picked, drawn at once and kept until the machine reports the same, so an old value
    /// arriving first never flashes back.
    @State private var pending: [String: String] = [:]

    private var shown: String { sel ?? thread.engine }

    /// Every engine of the machine when it says so, else the ones seen; always the thread's own.
    private var engines: [String] {
        var list = store.allEngines
        if !thread.engine.isEmpty, !list.contains(thread.engine) { list.insert(thread.engine, at: 0) }
        return list
    }

    var body: some View {
        let own = shown == thread.engine
        let agent = own ? (store.folded[thread.id]?.options ?? []) : store.knownOptions(shown)
        let data = pickerData(for: shown)
        VStack(alignment: .leading, spacing: 8) {
            ModelPickerBody(engines: engines, selected: Binding(get: { shown }, set: { sel = $0 }), blocked: blocked, data: data,
                            onModel: { v in pick(data.modelId, v, on: shown); closeSoon() }, onEffort: { v in pick(data.effortId, v, on: shown) },
                            engineData: { e in pickerData(for: e) },
                            onPickFavorite: { e, v in pick(pickerData(for: e).modelId, v, on: e); closeSoon() })
            if !own {
                Button { Task { await store.setEngine(thread, shown) } } label: {
                    Text("Carry on with \(store.engineLabel(shown))").font(.system(size: 14.5, weight: .semibold)).foregroundStyle(Hub.bg)
                        .frame(maxWidth: .infinity).frame(height: 42).background(Capsule().fill(Hub.accent))
                }
                .buttonStyle(.plain)
            } else if !agent.isEmpty {
                Text("Changes this thread from its next turn.").font(.system(size: 11.5)).foregroundStyle(Hub.muted)
            }
        }
        .task { if store.vitals.isEmpty { await store.loadVitals() }; await store.loadPhoneEngines() }
        .onChange(of: store.folded[thread.id]?.options) { settle() }
    }

    /// What an engine offers here: this thread's own options for its engine, else what an old thread showed.
    private func pickerData(for e: String) -> PickerData {
        let own = e == thread.engine
        let agent = own ? (store.folded[thread.id]?.options ?? []) : store.knownOptions(e)
        return PickerData.make(engine: e, agent: agent, phone: store.phoneEngines.first { $0.name == e }, values: own ? pending : [:], builtIn: false)
    }

    /// Long enough to see the check land on the row picked.
    private func closeSoon() {
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(260))
            done()
        }
    }

    /// A model or effort picked on `engine`; a favourite can sit on another engine than the one shown.
    private func pick(_ id: String?, _ value: String, on engine: String) {
        if engine != thread.engine {
            // A model on another engine: carry on with that engine, started on this model.
            Haptics.tap()
            if let id { pending[id] = value }
            Task {
                await store.setEngine(thread, engine)
                if let id { _ = await store.setOption(thread, id: id, value: value) }
                sel = nil
            }
            return
        }
        guard let id else { return }
        Haptics.tap()
        pending[id] = value
        Task {
            let ok = await store.setOption(thread, id: id, value: value)
            if !ok { pending[id] = nil }
            // If the machine never says the same, let go of the pick after a while.
            try? await Task.sleep(for: .seconds(6))
            if pending[id] == value { pending[id] = nil }
        }
    }

    /// A pick is let go of once the machine reports the same value.
    private func settle() {
        let options = store.folded[thread.id]?.options ?? []
        for (id, v) in pending where options.first(where: { $0.id == id })?.value == v { pending[id] = nil }
    }

    /// Why an engine cannot be used on the thread's machine, from that machine's readiness checks.
    /// It stays in the list, with the reason in amber (sheet "Tråden", screen 4).
    private func blocked(_ e: String) -> String? {
        guard e != thread.engine, let host = thread.host ?? store.machine,
              let c = store.vitals[host]?.ready?.checks.first(where: { $0.key == "engine:\(e)" }), c.state != "ok", c.state != "warn" else { return nil }
        let d = c.detail.lowercased()
        if d.contains("not signed in") || d.contains("signed out") { return "Signed out here" }
        if d.contains("not found") || d.contains("not installed") { return "Not installed here" }
        return String(c.detail.prefix(40))
    }

    private func set(_ o: EngineOption, _ value: String, engine: String) {
        guard engine == thread.engine else { return }
        Haptics.tap()
        Task { await store.setOption(thread, id: o.id, value: value) }
    }

    static func label(_ e: String) -> String {
        switch e.lowercased() {
        case "claude": return "Claude Code"
        case "codex": return "Codex"
        default: return e.split(separator: "-").map { $0.prefix(1).uppercased() + $0.dropFirst() }.joined(separator: " · ")
        }
    }

    static func color(_ e: String) -> Color {
        let l = e.lowercased()
        if l.contains("claude") { return Color(hex: 0xd97757) }
        if l.contains("codex") { return Color(hex: 0xececec) }
        if l.contains("deepseek") { return Color(hex: 0x6e8cff) }
        return Hub.projectColor(e)
    }
}

/// + for context (inspiration sketch 2): the camera, the latest photos, files on the phone, and
/// another thread. What is picked goes with the next message.
struct ContextPanel: View {
    @Environment(Store.self) private var store
    @Binding var attachments: [Attachment]
    @Binding var text: String
    let done: () -> Void
    @State private var photos: [PhotosPickerItem] = []
    @State private var camera = false
    @State private var files = false
    @State private var threads = false

    var body: some View {
        if threads { threadList } else { main }
    }

    private var main: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                tile("Camera", "camera") { camera = true }
                PhotosPicker(selection: $photos, maxSelectionCount: 6, matching: .images) {
                    tileLabel("Photos", "photo.on.rectangle")
                }
                .buttonStyle(.plain)
            }
            row("Files on the phone", "doc") { files = true }
            row("Another thread", "bubble.left.and.bubble.right") { withAnimation(.snappy) { threads = true } }
            Text("Pictures and files go with your next message.").font(.system(size: 11.5)).foregroundStyle(Hub.muted)
        }
        .onChange(of: photos) {
            Task {
                for item in photos {
                    if let data = try? await item.loadTransferable(type: Data.self) {
                        let img = UIImage(data: data)
                        let jpeg = img?.jpegData(compressionQuality: 0.8) ?? data
                        attachments.append(Attachment(name: "photo-\(attachments.count + 1).jpg", mime: "image/jpeg", data: jpeg, preview: img))
                    }
                }
                photos = []
                done()
            }
        }
        .fileImporter(isPresented: $files, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            for url in (try? result.get()) ?? [] {
                let ok = url.startAccessingSecurityScopedResource()
                defer { if ok { url.stopAccessingSecurityScopedResource() } }
                if let data = try? Data(contentsOf: url) {
                    let mime = UTType(filenameExtension: url.pathExtension)?.preferredMIMEType ?? "application/octet-stream"
                    attachments.append(Attachment(name: url.lastPathComponent, mime: mime, data: data, preview: UIImage(data: data)))
                }
            }
            done()
        }
        .fullScreenCover(isPresented: $camera) {
            CameraView { img in
                if let img, let data = img.jpegData(compressionQuality: 0.8) {
                    attachments.append(Attachment(name: "camera-\(attachments.count + 1).jpg", mime: "image/jpeg", data: data, preview: img))
                }
                camera = false
                done()
            }
            .ignoresSafeArea()
        }
    }

    private var threadList: some View {
        let list = store.threads.sorted { ($0.returned_at ?? $0.updated_at) > ($1.returned_at ?? $1.updated_at) }.prefix(20)
        return PanelGroup(label: "Point to another thread") {
            ForEach(Array(list.enumerated()), id: \.element.id) { i, t in
                PanelRow(title: t.title.isEmpty ? "New thread" : t.title, sub: [t.project, t.host ?? store.machine].compactMap { $0 }.joined(separator: " · "), last: i == list.count - 1) {
                    StatusDot(state: t.dot)
                } trailing: { EmptyView() } action: {
                    text += (text.isEmpty || text.hasSuffix(" ") ? "" : " ") + "@thread:\"\(t.title)\" "
                    done()
                }
            }
        }
    }

    private func tile(_ title: String, _ icon: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) { tileLabel(title, icon) }.buttonStyle(.plain)
    }

    private func tileLabel(_ title: String, _ icon: String) -> some View {
        VStack(spacing: 6) {
            Image(systemName: icon).font(.system(size: 22))
            Text(title).font(.system(size: 12.5))
        }
        .foregroundStyle(Hub.text).frame(maxWidth: .infinity).frame(height: 86)
        .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
    }

    private func row(_ title: String, _ icon: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 11) {
                Image(systemName: icon).font(.system(size: 16)).foregroundStyle(Hub.text2).frame(width: 22)
                Text(title).font(.system(size: 14.5))
                Spacer()
                Image(systemName: "chevron.right").font(.system(size: 12)).foregroundStyle(Hub.muted)
            }
            .padding(.horizontal, 14).frame(height: 48)
            .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
        }
        .buttonStyle(.plain)
    }
}

/// The camera, for a picture to send.
struct CameraView: UIViewControllerRepresentable {
    let picked: (UIImage?) -> Void

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let c = UIImagePickerController()
        c.sourceType = UIImagePickerController.isSourceTypeAvailable(.camera) ? .camera : .photoLibrary
        c.delegate = context.coordinator
        return c
    }

    func updateUIViewController(_ c: UIImagePickerController, context: Context) {}
    func makeCoordinator() -> Coordinator { Coordinator(picked) }

    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let picked: (UIImage?) -> Void
        init(_ picked: @escaping (UIImage?) -> Void) { self.picked = picked }
        func imagePickerController(_ p: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            picked(info[.originalImage] as? UIImage)
        }
        func imagePickerControllerDidCancel(_ p: UIImagePickerController) { picked(nil) }
    }
}

/// @ to mention (inspiration sketch 4): the files the thread changed first, then threads and
/// projects. The pick goes into the text where the @ was.
struct Mention: Hashable, Identifiable {
    enum Kind: String { case file, thread, project }
    var kind: Kind
    var label: String
    var detail: String
    var insert: String
    var id: String { kind.rawValue + label }
}

struct MentionPopover: View {
    let query: String
    let items: [Mention]
    let pick: (Mention) -> Void

    var body: some View {
        let q = query.lowercased()
        let list = items.filter { q.isEmpty || $0.label.lowercased().contains(q) }.prefix(8)
        VStack(alignment: .leading, spacing: 2) {
            Text(list.isEmpty ? "Nothing called \(query)" : "Mention a file, thread or project").font(.system(size: 11.5)).foregroundStyle(Hub.muted)
                .padding(.horizontal, 8).padding(.bottom, 4)
            ForEach(Array(list)) { m in
                Button { pick(m) } label: {
                    HStack(spacing: 10) {
                        Image(systemName: m.kind == .file ? "doc" : m.kind == .thread ? "bubble.left" : "folder")
                            .font(.system(size: 14)).foregroundStyle(m.kind == .project ? Hub.projectColor(m.label) : Hub.text2).frame(width: 20)
                        Text(m.label).font(.system(size: 14)).lineLimit(1)
                        Spacer()
                        Text(m.detail).font(.system(size: 11.5)).foregroundStyle(Hub.muted)
                    }
                    .padding(.horizontal, 8).frame(height: 36).contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 18).fill(Hub.surface))
        .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(Hub.lineStrong))
        .shadow(color: .black.opacity(0.5), radius: 18, y: 8)
        .padding(.horizontal, 10).padding(.bottom, 4)
    }
}


/// The thread menu (⋯) as Hub's own panel, what is used most first: settle and snooze, then the
/// rest, archive last. Short rows, so nothing needs scrolling.
struct ThreadMenuPanel: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    let thread: HubThread
    let close: () -> Void
    let leave: () -> Void
    /// The "Chat detail" submenu is open.
    @State private var detailOpen = false

    var body: some View {
        HStack(spacing: 8) {
            Button { store.tidy([thread], .settle); close(); leave() } label: {
                Label("Settle", systemImage: "checkmark").font(.system(size: 14, weight: .semibold)).foregroundStyle(Hub.bg)
                    .frame(maxWidth: .infinity).frame(height: 40).background(RoundedRectangle(cornerRadius: 11).fill(Hub.accent))
            }
            .buttonStyle(.plain)
            Button { store.tidy([thread], thread.pinned_at == nil ? .pin : .unpin); close() } label: {
                Label(thread.pinned_at == nil ? "Pin" : "Unpin", systemImage: "pin").font(.system(size: 14, weight: .medium)).foregroundStyle(Hub.text)
                    .frame(maxWidth: .infinity).frame(height: 40).background(RoundedRectangle(cornerRadius: 11).fill(Hub.surface))
            }
            .buttonStyle(.plain)
        }
        VStack(alignment: .leading, spacing: 6) {
            Text("Snooze until").font(.system(size: 12, weight: .semibold)).foregroundStyle(Hub.muted)
            HStack(spacing: 8) {
                ForEach(store.snoozeChoices, id: \.label) { c in
                    Button { store.tidy([thread], .snooze(until: c.until)); close(); leave() } label: {
                        Text(c.label == "Reset" ? "Limits reset \(c.sub)" : c.label == "1 h" ? "1 h" : "08:00")
                            .font(.system(size: 13)).foregroundStyle(Hub.text2).lineLimit(1)
                            .padding(.horizontal, 11).frame(height: 34).background(RoundedRectangle(cornerRadius: 9).fill(Hub.surface))
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        PanelGroup {
            item("Review changes", "plus.forwardslash.minus") { close(); nav.push(.changes(thread.id)) }
            item("Pull request", "arrow.triangle.pull") { close(); nav.push(.pull(thread.id)) }
            if let reply = store.folded[thread.id]?.lastReply {
                item("Copy the last reply", "doc.on.doc", last: store.chatDetailPrefs == nil || !store.has("phone.chatdetail")) { UIPasteboard.general.string = reply; store.flash("Copied"); close() }
            }
            if store.chatDetailPrefs != nil, store.has("phone.chatdetail") { chatDetail }
        }
        Button {
            if thread.status == .running { Task { await store.stop(thread) } }
            store.tidy([thread], .archive); close(); leave()
        } label: {
            Label(thread.status == .running ? "Stop and archive" : "Archive", systemImage: "archivebox").font(.system(size: 14.5, weight: .medium))
                .foregroundStyle(Hub.red).frame(maxWidth: .infinity).frame(height: 44).background(RoundedRectangle(cornerRadius: 12).fill(Hub.redBg))
        }
        .buttonStyle(.plain)
    }

    /// "Chat detail" with its levels as a submenu: Follow default, then the four levels.
    @ViewBuilder private var chatDetail: some View {
        let own = store.chatDetailPrefs?.threads[thread.id].flatMap(ChatDetail.init(rawValue:))
        let now = store.detail(of: thread)
        item("Chat detail", "text.alignleft", value: own == nil ? "Default" : own?.name, last: !detailOpen) { withAnimation(.snappy) { detailOpen.toggle() } }
        if detailOpen {
            choice("Follow default", sub: now.map { "Now \($0.name)" }, on: own == nil, last: false) { store.setDetail(nil, for: thread) }
            ForEach(ChatDetail.allCases) { level in
                choice(level.name, sub: level.blurb, on: own == level, last: level == .all) { store.setDetail(level, for: thread) }
            }
        }
    }

    private func choice(_ title: String, sub: String?, on: Bool, last: Bool, _ action: @escaping () -> Void) -> some View {
        PanelRow(title: title, sub: sub, last: last) {
            Image(systemName: on ? "checkmark" : "circle").font(.system(size: 13, weight: .semibold))
                .foregroundStyle(on ? Hub.accent : Hub.muted.opacity(0.35)).frame(width: 20)
        } trailing: {
            EmptyView()
        } action: {
            Haptics.tap(); action()
        }
    }

    private func item(_ title: String, _ icon: String, value: String? = nil, last: Bool = false, _ action: @escaping () -> Void) -> some View {
        PanelRow(title: title, sub: nil, last: last) {
            Image(systemName: icon).font(.system(size: 14)).foregroundStyle(Hub.muted).frame(width: 20)
        } trailing: {
            HStack(spacing: 6) {
                if let value { Text(value).font(.system(size: 13)).foregroundStyle(Hub.muted).lineLimit(1) }
                Image(systemName: "chevron.right").font(.system(size: 11)).foregroundStyle(Hub.muted)
            }
        } action: {
            action()
        }
    }
}

