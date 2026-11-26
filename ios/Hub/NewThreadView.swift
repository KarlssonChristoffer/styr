import SwiftUI

/// A new thread (sketch 2, screens 1 and 2; T3 Code's new task): "What should we build in Hub?"
/// with the project a tap away, a line above the box saying where it runs (project · machine ·
/// engine · safety), and one sheet with the four choices. It is only sent on a tap of the arrow,
/// and the draft stays when you leave.
struct NewThreadView: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @Environment(\.dismiss) private var dismiss
    var project: ProjectRef?
    var talk = false
    var opened: (String) -> Void

    @State private var text = Drafts.get("new")
    @State private var place: ProjectRef?
    @State private var engine: String?
    /// Run it on the harness's own cloud instead of a machine (Claude or Codex, by the model picked).
    @State private var cloud = false
    @State private var choosing = false
    @State private var picking = false
    @State private var sending = false
    @State private var failed: String?
    @State private var level: String?
    @State private var adding = false
    @State private var attachments: [Attachment] = []
    @FocusState private var focused: Bool
    @State private var settings: [String: String] = [:]
    @State private var modelOpen = false
    @State private var projectOpen = false
    @State private var machineOpen = false
    @State private var warmId: String?
    @State private var warmOptions: [EngineOption] = []
    @State private var warming = false
    @State private var warmedFor = ""

    var body: some View {
        VStack(spacing: 0) {
            HubHeader(parent: nav.tabName, title: "New thread", back: { dismiss() })

            Spacer()
            hero
            Spacer()

            if !store.isDemo, !store.has(place?.isScratch == true ? "phone.scratch" : "phone.threads", on: store.url(of: place?.host)) {
                Label("\(place?.host ?? store.machine ?? "The PC") needs a newer Hub to start threads from the phone. It arrives with Hub's next update (Settings › Updates shows the step).", systemImage: "arrow.down.circle")
                    .font(.system(size: 12.5)).foregroundStyle(Hub.amber)
                    .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                    .background(RoundedRectangle(cornerRadius: 12).fill(Hub.amberBg)).padding(.horizontal, 12).padding(.bottom, 6)
            }

            if let failed {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Could not start the thread").font(.system(size: 14, weight: .semibold)).foregroundStyle(Hub.red)
                    Text(failed).font(.system(size: 12.5)).foregroundStyle(Hub.text3)
                    Text("Your words are kept below.").font(.system(size: 12)).foregroundStyle(Hub.muted)
                }
                .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 12).fill(Hub.redBg)).padding(.horizontal, 12).padding(.bottom, 6)
            }

            Composer(text: $text, placeholder: "Describe the task", sending: sending, startDictating: talk,
                     focus: $focused, onSend: send,
                     onPlus: { focused = false; adding = true },
                     engineLabel: boxLabel,
                     engineLogo: AnyView(EngineLogo(engine: currentEngine, size: 13)),
                     onEngine: { focused = false; withAnimation(.snappy) { projectOpen = false; machineOpen = false; modelOpen.toggle() } },
                     attachments: $attachments)
        }
        .background(Hub.bg)
        .overlay {
            // The sheets stand over a dimmed page: a tap outside them, or a drag down on them, closes.
            if modelOpen || projectOpen || machineOpen {
                ZStack(alignment: .bottom) {
                    Color.black.opacity(0.4).ignoresSafeArea().onTapGesture { closeSheets() }
                    VStack(spacing: 0) {
                if modelOpen {
                    ModelSheet(engine: $engine, settings: $settings, warmOptions: warmOptions, loading: warming, level: $level,
                               onEngine: { settings = [:]; Task { await warm(force: true) } },
                               onSetting: { Task { await warm(force: true) } },
                               setLevel: { l in level = l; Haptics.tap(); saveLevel(l) },
                               done: { withAnimation(.snappy) { modelOpen = false } })
                        .padding(.horizontal, 10).padding(.bottom, 6)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
                if projectOpen {
                    ProjectSheet(place: $place, picked: { p in engine = store.lastEngine(in: p) ?? engine; settings = [:] },
                                 close: { withAnimation(.snappy) { projectOpen = false } })
                        .padding(.horizontal, 10).padding(.bottom, 6)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
                if machineOpen {
                    RunOnSheet(place: $place, cloud: $cloud, cloudName: cloudName, close: { withAnimation(.snappy) { machineOpen = false } })
                        .padding(.horizontal, 10).padding(.bottom, 6)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }

                    }
                    .padding(.bottom, 74)
                }
                .transition(.opacity)
            }
        }
        .toolbar(.hidden, for: .navigationBar)
        .fullScreenCover(isPresented: $picking) {
            ProjectPicker { p in place = p; engine = store.lastEngine(in: p) ?? engine }
        }
        .swipeBack(enabled: !(modelOpen || projectOpen || machineOpen)) { dismiss() }
        .hubPanel("Add context", isPresented: $adding) {
            ContextPanel(attachments: $attachments, text: $text) { withAnimation(.snappy) { adding = false } }
        }
        .onChange(of: text) { Drafts.set("new", text) }
        .onAppear {
            if place == nil { place = project ?? store.lastPlace?.project ?? .noProject(on: nil) }
            if engine == nil { engine = place.flatMap(store.lastEngine) }
            // The keyboard opens only on a tap in the box, never by itself.
            if nav.pickProjectFirst { nav.pickProjectFirst = false; picking = true }
        }
        .onChange(of: place) { Task { await warm() } }
        .task {
            await store.loadPhoneEngines()
            await warm()
            await store.loadProjects()
            if level == nil, !store.isDemo, let c = store.homeClient { level = (try? await c.permissions())?.level }
        }
    }

    private var currentEngine: String { engine ?? store.phoneEngines.first(where: { $0.default == true })?.name ?? store.allEngines.first ?? "claude" }

    /// What goes to the server: hints from the built-in lists stay on the phone.
    private var sendable: [String: String] { settings.filter { !$0.key.hasPrefix("hint:") } }

    private var warmKey: String { "\(place?.id ?? "")|\(currentEngine)|\(sendable.sorted { $0.key < $1.key }.map { "\($0.key)=\($0.value)" }.joined(separator: ","))" }

    /// The engine's company and the model picked, for the box: "Opus 4.8", else "Claude".
    private var boxLabel: String {
        let d = PickerData.make(engine: currentEngine, agent: warmOptions.isEmpty ? store.knownOptions(currentEngine) : warmOptions, phone: store.phoneEngines.first { $0.name == currentEngine }, values: settings)
        let model = d.groups.flatMap(\.choices).first { $0.value == d.modelValue }?.name
        return model ?? store.engineLabel(currentEngine)
    }

    /// Starts the engine while the page is open, so it can say what it offers; what it says shows in the model sheet.
    private func warm(force: Bool = false) async {
        guard !store.isDemo, store.has("phone.engines", on: store.url(of: place?.host)), let place, !place.isScratch else { return }
        let key = warmKey
        guard force || warmedFor != key else { return }
        guard let t = store.target(place.host) else { return }
        warming = true; warmedFor = key
        defer { warming = false }
        guard let id = try? await t.client.warm(cwd: place.path, engine: engine, settings: sendable, host: t.host) else { warmId = nil; return }
        warmId = id
        // The agent reports its options a moment later, as events of the warmed thread.
        for _ in 0..<10 {
            if let events = try? await t.client.warmEvents(thread: id, host: t.host) {
                let fresh = Folded(events).options
                if !fresh.isEmpty { warmOptions = fresh; return }
            }
            try? await Task.sleep(for: .milliseconds(700))
            if warmKey != key { return }
        }
    }

    private func saveLevel(_ l: String) {
        guard !store.isDemo, let c = store.homeClient else { return }
        Task {
            guard var p = try? await c.permissions() else { store.flash("Safety needs a newer hubd to change from the phone."); return }
            p.level = l
            try? await c.setPermissions(p)
        }
    }

    /// The hero: the question with the project as an underlined word, and the machine on its own
    /// quiet line with a state dot. A tap on either opens its short sheet; the machine line also
    /// cycles the online machines with a swipe sideways.
    private var hero: some View {
        VStack(spacing: 10) {
            if let place, place.isScratch {
                (Text("What should we try on ") + Text(place.host ?? store.machine ?? "this machine").underline().foregroundColor(Hub.accent) + Text("?"))
                    .font(.system(size: 22, weight: .semibold)).multilineTextAlignment(.center)
                    .safeTap { openProject() }
            } else if let place {
                (Text("What should we build in ") + Text(place.name).underline().foregroundColor(Hub.accent) + Text("?"))
                    .font(.system(size: 22, weight: .semibold)).multilineTextAlignment(.center)
                    .safeTap { openProject() }
                    .onLongPressGesture { openProject() }
            } else {
                Text("What should we work on?").font(.system(size: 22, weight: .semibold))
                Button("Pick a project") { picking = true }.buttonStyle(PillButtonStyle(kind: .primary))
            }
            if let place {
                let name = place.host ?? store.machine ?? "this machine"
                let up = cloudOn || (store.machineChoices(for: place).first { $0.name == name }?.online ?? true)
                Button { focused = false; withAnimation(.snappy) { modelOpen = false; projectOpen = false; machineOpen.toggle() } } label: {
                    HStack(spacing: 6) {
                        Image(systemName: cloudOn ? "icloud.and.arrow.up" : "desktopcomputer").font(.system(size: 12))
                        Text(cloudOn ? "in the \(cloudName ?? "") cloud" : place.isScratch ? "no project · \(name)" : "on \(name)")
                        Image(systemName: machineOpen ? "chevron.down" : "chevron.up").font(.system(size: 9, weight: .semibold))
                        Circle().fill(up ? Hub.green : Hub.red).frame(width: 6, height: 6)
                    }
                    .font(.system(size: 13)).foregroundStyle(Hub.muted).contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(cloudOn ? "Runs in the \(cloudName ?? "") cloud. Tap to change" : "Run it on \(name). Swipe sideways to change machine")
                // This line changes machine on a sideways swipe, so the page's own back swipe leaves it alone.
                .swipeZone()
                .simultaneousGesture(DragGesture(minimumDistance: 24).onEnded { v in
                    guard abs(v.translation.width) > abs(v.translation.height) else { return }
                    cycleMachine(v.translation.width < 0 ? 1 : -1)
                })
            }
        }
        .padding(.horizontal, 30)
    }

    /// Whose cloud the model picked has, or nil: only the plain Claude and Codex harnesses, in a project (the
    /// cloud works on a copy from GitHub), and a hubd that can start one.
    private var cloudName: String? {
        guard let place, !place.isScratch, store.has("phone.cloud", on: store.url(of: place.host)) else { return nil }
        switch engine ?? "claude" { case "claude": return "Claude"; case "codex": return "Codex"; default: return nil }
    }

    private var cloudOn: Bool { cloud && cloudName != nil }

    private func closeSheets() {
        withAnimation(.snappy) { modelOpen = false; projectOpen = false; machineOpen = false }
    }

    private func openProject() {
        focused = false
        withAnimation(.snappy) { modelOpen = false; machineOpen = false; projectOpen.toggle() }
    }

    /// The next (or previous) online machine that has this project.
    private func cycleMachine(_ step: Int) {
        let list = store.machineChoices(for: place).filter { $0.online && $0.has }
        let here = place?.host ?? store.machine
        guard list.count > 1, let i = list.firstIndex(where: { $0.name == here }) else { return }
        let next = list[(i + step + list.count) % list.count]
        if let p = store.place(place, on: next.name) { withAnimation(.snappy) { place = p; cloud = false }; Haptics.tap() }
    }

    private var levelName: String {
        switch level { case "read": "Read"; case "edit": "Edits"; case "everything": "Full"; default: "Ask first" }
    }

    private func send(_ message: String) {
        guard !sending else { return }
        guard let place else {
            failed = "Pick a project or explicitly choose No project before starting a coding thread."
            picking = true
            return
        }
        sending = true
        failed = nil
        focused = false
        Task {
            if let id = await store.newThread(in: place, engine: engine, message: message, attachments: attachments, settings: PickerData.sendable(settings, names: store.has("phone.pickbyname", on: store.url(of: place.host))), warm: warmedFor == warmKey ? warmId : nil, cloud: cloudOn) {
                attachments = []
                text = ""
                Drafts.set("new", "")
                opened(id)
            } else {
                failed = store.toast?.text ?? "The machine did not answer."
            }
            sending = false
        }
    }
}

/// Where it runs (sketch 2, screen 2): one sheet with the four choices, one tap each, Done goes
/// Lays chips out in rows, wrapping as needed.
struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? 320
        var x: CGFloat = 0, y: CGFloat = 0, row: CGFloat = 0
        for v in subviews {
            let s = v.sizeThatFits(.unspecified)
            if x + s.width > width, x > 0 { x = 0; y += row + spacing; row = 0 }
            x += s.width + spacing
            row = max(row, s.height)
        }
        return CGSize(width: width, height: y + row)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, row: CGFloat = 0
        for v in subviews {
            let s = v.sizeThatFits(.unspecified)
            if x + s.width > bounds.maxX, x > bounds.minX { x = bounds.minX; y += row + spacing; row = 0 }
            v.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(s))
            x += s.width + spacing
            row = max(row, s.height)
        }
    }
}
