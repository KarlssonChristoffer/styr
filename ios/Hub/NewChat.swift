import SwiftUI

/// A new chat (the pen in Chat): the box, a model picker, Search and Image, the mic and attach.
/// No project, no checkout, no machine row: the machine is chosen for you (the PC, else tincan,
/// else the VPS, else any that is up) and said in one quiet line. Claude on Sonnet is the default,
/// GPT (Codex) on Terra the other; the last pick is remembered. A chat asks before it acts.
struct NewChatView: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @Environment(\.dismiss) private var dismiss
    var opened: (String) -> Void

    @State private var text = ""
    @State private var image = false
    @State private var sending = false
    @State private var adding = false
    @State private var modelOpen = false
    @State private var attachments: [Attachment] = []
    @State private var engine = UserDefaults.standard.string(forKey: "chat.engine") ?? "claude"
    @State private var settings: [String: String] = (UserDefaults.standard.dictionary(forKey: "chat.settings") as? [String: String]) ?? [:]
    @State private var level: String? = "ask"
    @State private var warmId: String?
    @State private var warmOptions: [EngineOption] = []
    @State private var warming = false
    @State private var warmedFor = ""
    @FocusState private var focused: Bool

    var body: some View {
        let host = chatHost()
        VStack(spacing: 0) {
            HubHeader(parent: "Chats", title: "New chat", back: { dismiss() })
            Spacer()
            VStack(spacing: 10) {
                Text("What do you want to know?").font(.system(size: 26, weight: .medium, design: .serif)).multilineTextAlignment(.center)
                if let host {
                    Text("on \(host.name)").font(.system(size: 12.5)).foregroundStyle(Hub.muted)
                } else {
                    Text("No machine is up right now, so a chat cannot start. Wake the PC or tincan.").font(.system(size: 12.5)).foregroundStyle(Hub.amber).multilineTextAlignment(.center)
                }
            }
            .padding(.horizontal, 30)
            Spacer()
            HStack(spacing: 8) {
                chip("photo", "Image", on: $image)
                Spacer()
            }
            .padding(.horizontal, 16).padding(.bottom, 4)
            Composer(text: $text, placeholder: "Ask anything", sending: sending, focus: $focused,
                     onSend: { send($0, host: host) },
                     onPlus: { focused = false; adding = true },
                     engineLabel: boxLabel,
                     engineLogo: AnyView(EngineLogo(engine: engine, size: 13)),
                     onEngine: { focused = false; withAnimation(.snappy) { modelOpen.toggle() } },
                     attachments: $attachments)
        }
        .background(Hub.bg)
        .overlay {
            // The model sheet stands over a dimmed page: a tap outside it, or a drag down on it, closes.
            if modelOpen {
                ZStack(alignment: .bottom) {
                    Color.black.opacity(0.4).ignoresSafeArea().onTapGesture { withAnimation(.snappy) { modelOpen = false } }
                    ModelSheet(engine: Binding(get: { engine }, set: { engine = $0 ?? "claude" }), settings: $settings, warmOptions: warmOptions, loading: warming, level: $level,
                               onEngine: { settings = defaults(for: engine); remember(); Task { await warm(force: true) } },
                               onSetting: { remember(); Task { await warm(force: true) } }, setLevel: { _ in },
                               done: { withAnimation(.snappy) { modelOpen = false } }, allowSafety: false)
                        .padding(.horizontal, 10).padding(.bottom, 74)
                }
                .transition(.opacity)
            }
        }
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack { dismiss() }
        .hubPanel("Add context", isPresented: $adding) {
            ContextPanel(attachments: $attachments, text: $text) { withAnimation(.snappy) { adding = false } }
        }
        .task {
            await store.loadPhoneEngines()
            engine = store.allEngines.first { $0.lowercased().contains(engine.lowercased()) } ?? engine
            if settings.isEmpty { settings = defaults(for: engine) }
            await warm()
        }
    }

    private func chip(_ icon: String, _ title: String, on: Binding<Bool>) -> some View {
        Button {
            on.wrappedValue.toggle()
            Haptics.tap()
        } label: {
            HStack(spacing: 5) { Image(systemName: icon).font(.system(size: 12)); Text(title).font(.system(size: 13)) }
                .foregroundStyle(on.wrappedValue ? Hub.accent : Hub.text2)
                .padding(.horizontal, 12).frame(height: 32)
                .background(Capsule().fill(on.wrappedValue ? Hub.accentBg : Hub.surface2))
        }
        .buttonStyle(.plain)
    }

    /// "Sonnet", "Terra": the model name in the box.
    private var boxLabel: String {
        let d = PickerData.make(engine: engine, agent: store.knownOptions(engine), phone: store.phoneEngines.first { $0.name == engine }, values: settings)
        let model = d.groups.flatMap(\.choices).first { $0.value == d.modelValue }?.name
        return model ?? store.engineLabel(engine)
    }

    /// Sonnet for Claude, Terra for GPT: the real option when the engine's options are known, else a hint.
    private func defaults(for engine: String) -> [String: String] {
        let l = engine.lowercased()
        let wanted = l.contains("claude") ? "sonnet" : (l.contains("codex") || l.contains("openai") || l.contains("gpt")) ? "terra" : nil
        guard let wanted else { return [:] }
        let opts = EngineOption.parse(store.phoneEngines.first { $0.name == engine }?.options ?? []) + store.knownOptions(engine)
        if let m = opts.first(where: { $0.category == "model" }), let c = m.choices.first(where: { $0.name.lowercased().contains(wanted) || $0.value.lowercased().contains(wanted) }) {
            return [m.id: c.value]
        }
        return ["hint:model": wanted]
    }

    /// What goes to the server: hints from the built-in lists stay on the phone.
    private var sendable: [String: String] { settings.filter { !$0.key.hasPrefix("hint:") } }

    private var warmKey: String { "\(engine)|\(sendable.sorted { $0.key < $1.key }.map { "\($0.key)=\($0.value)" }.joined(separator: ","))" }

    /// Starts the engine while the page is open, so the first answer does not wait for it to start;
    /// what it reports shows in the model sheet. The id goes with the thread when it is made.
    private func warm(force: Bool = false) async {
        guard !store.isDemo, store.has("phone.engines"), let host = chatHost() else { return }
        let key = warmKey
        guard force || warmedFor != key else { return }
        guard let t = store.target(host.host) else { return }
        warming = true; warmedFor = key
        defer { warming = false }
        guard let id = try? await t.client.warm(cwd: "", engine: engine, settings: sendable, host: t.host, chat: true) else { warmId = nil; return }
        warmId = id
        for _ in 0..<10 {
            if let events = try? await t.client.warmEvents(thread: id, host: t.host) {
                let fresh = Folded(events).options
                if !fresh.isEmpty { warmOptions = fresh; return }
            }
            try? await Task.sleep(for: .milliseconds(700))
            if warmKey != key { return }
        }
    }

    private func remember() {
        UserDefaults.standard.set(engine, forKey: "chat.engine")
        UserDefaults.standard.set(settings, forKey: "chat.settings")
    }

    /// The machine a chat runs on: the PC, then tincan, then the VPS, then any other that is up; the
    /// MacBook only when nothing else is.
    private func chatHost() -> (host: String?, name: String)? {
        let up = store.machineChoices(for: nil).filter(\.online).map(\.name)
        func find(_ test: (String) -> Bool) -> String? { up.first(where: test) }
        let pick = find { $0.lowercased() == "pc" } ?? find { $0.lowercased() == "tincan" } ?? find { $0.lowercased().contains("vps") }
            ?? find { !$0.lowercased().contains("macbook") && !$0.lowercased().contains("mac") } ?? find { _ in true }
        guard let pick else { return nil }
        return (pick == store.machine ? nil : pick, pick)
    }

    private func send(_ message: String, host: (host: String?, name: String)?) {
        guard let host, !sending else { return }
        sending = true
        focused = false
        var asked = message
        if image { asked = "Make a picture of this and show it: \(message)" }
        let sent = attachments
        let real = PickerData.sendable(settings, names: store.has("phone.pickbyname", on: store.url(of: host.host)))
        let warmUse = warmedFor == warmKey ? warmId : nil
        Task {
            if let id = await store.newThread(in: .noProject(on: host.host), engine: engine, message: asked, attachments: sent, settings: real, warm: warmUse, chat: true) {
                text = ""; attachments = []
                remember()
                opened(id)
            }
            sending = false
        }
    }
}
