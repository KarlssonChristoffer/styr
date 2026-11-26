import SwiftUI

/// A thread (sketch 2, screens 3 and 4, with T3 Code's feed as the model): your messages as
/// bubbles, the agent's words in plain text, its work as a grey group you can open, earlier turns
/// folded to one line, a status that ticks while it works, and an ask docked above the box.
struct ThreadView: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var phase
    let threadId: String
    /// Holds the feed at its end while the log comes in and the lazy rows find their heights, so a
    /// thread always opens at its latest message; ends at the first touch or once the log has settled.
    @State private var landing = true
    @State private var landed = false
    /// When the app went to the background.
    @State private var hiddenAt: Date?
    @State private var menu = false
    @State private var draft = ""
    @State private var sending = false
    @State private var atEnd = true
    /// Bumped to bring the feed to its end (after you send).
    @State private var toEnd = 0
    @State private var openTurns: Set<Int> = []
    @State private var moving = false
    /// The card about the thread from its title, and whether it opened ready to rename.
    @State private var about = false
    @State private var aboutRename = false
    /// Off while the page slides in: the turns already there are drawn as they are, and only a
    /// message sent after that springs up from the box.
    @State private var settled = false
    @State private var engine = false
    @State private var adding = false
    @State private var attachments: [Attachment] = []
    @State private var changed: [ThreadChange] = []
    @State private var prNumber: Int?
    @State private var gitState: GitState?
    @State private var gitOpen = false
    /// The machine's iPhone simulator, when one is running, and whether it is open full screen.
    @State private var simulator: SimState?
    @State private var simulatorOpen = false
    @State private var didTurn: Int?
    /// Pulling up past the end of the feed (the next thread): how far, whether the pull began at the end, whether it is past the line.
    @State private var pull: CGFloat = 0
    @State private var pullFromEnd = false
    @State private var pullArmed = false
    @State private var interacting = false
    /// Only the latest turns are drawn; pulling down at the top (or tapping the line there) opens the
    /// next twelve above, in place.
    @State private var window = 12
    @State private var topPull: CGFloat = 0
    @State private var pullFromTop = false
    @State private var topArmed = false
    @FocusState private var typing: Bool

    private var thread: HubThread? { store.thread(threadId) }
    private var isChat: Bool { thread.map { store.isChat($0) } ?? false }

    var body: some View {
        let _ = Perf.count("ThreadView.body")
        VStack(spacing: 0) {
            ThreadTop(thread: thread, back: { dismiss() }, menu: { typing = false; menu = true },
                      about: { edit in typing = false; aboutRename = edit; withAnimation(.snappy) { about = true } },
                      simulator: simulator, openSimulator: { typing = false; Haptics.tap(); simulatorOpen = true })
            if let thread {
                gitLine(thread)
                feed(thread)
                status(thread)
                plan(thread)
                dock(thread)
                Composer(text: $draft, placeholder: placeholder(thread), running: thread.status == .running,
                         sending: sending, focus: $typing, onSend: { send($0, thread) },
                         onStop: { Task { await store.stop(thread) } },
                         onSteer: { send($0, thread, steer: true) },
                         onPlus: { typing = false; adding = true },
                         engineLabel: engineLabel(thread), engineLogo: AnyView(EngineLogo(engine: thread.engine, size: 13)), onEngine: { typing = false; engine = true },
                         mentions: mentions(thread), commands: commands(thread), attachments: $attachments)
            } else {
                Spacer()
                Text(store.anyLive || store.isDemo ? "This thread is gone." : "Connecting…").foregroundStyle(Hub.muted)
                Spacer()
            }
        }
        // Solid to the screen's edges, behind the status bar too: while the page slides in, nothing of
        // the page under it shows through (sketch 33, A).
        .background(Hub.bg.ignoresSafeArea())
        .containerBackground(Hub.bg, for: .navigation)
        .environment(\.quoteReply) { quote($0) }
        .environment(\.swipeToQuote, isChat)
        // A dev thread settles and snoozes on a swipe; a chat is not in the inbox, so there a swipe to the
        // right is back and a swipe to the left on a message quotes it.
        .modifier(ThreadTriage(thread: isChat ? nil : thread, dismiss: { dismiss() }))
        .modifier(ChatBack(on: isChat) { dismiss() })
        .overlay(alignment: .bottom) { if let toast = store.toast { ToastView(toast: toast).padding(.bottom, -10) } }
        .toolbar(.hidden, for: .navigationBar)
        .task { draft = Drafts.get(threadId); await store.open(threadId) }
        .onChange(of: draft) { Drafts.set(threadId, draft) }
        .onDisappear { store.close(threadId) }
        .onChange(of: phase) { _, now in
            if now == .background { hiddenAt = Date(); store.stepAway(threadId) }
            else if now == .active, let at = hiddenAt {
                hiddenAt = nil
                if Date().timeIntervalSince(at) >= 10 { store.cameBack(threadId) }
            }
        }
        .onAppear {
            Perf.note("thread shown, \(store.folded[threadId]?.items.count ?? 0) items")
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.7) { settled = true }
            if UserDefaults.standard.bool(forKey: "menu") { menu = true }
        }
        // A Mac with a simulator running gets a phone button in the top (Simulator.swift).
        .task(id: threadId) {
            while !Task.isCancelled {
                if let t = thread { simulator = await SimAsk.state(store, t) }
                try? await Task.sleep(nanoseconds: 6_000_000_000)
            }
        }
        .fullScreenCover(isPresented: $simulatorOpen) {
            if let thread, let simulator { SimulatorView(thread: thread, sim: simulator, close: { simulatorOpen = false }) }
        }
        .hubPanel(thread.map { $0.title.isEmpty ? "Thread" : $0.title } ?? "Thread", isPresented: $menu) {
            if let thread {
                ThreadMenuPanel(thread: thread, close: { withAnimation(.snappy) { menu = false } }, leave: { dismiss() })
            }
        }
        .hubPanel("What it did", isPresented: Binding(get: { didTurn != nil }, set: { if !$0 { didTurn = nil } })) {
            WhatItDid(turn: thread.flatMap { store.folded[$0.id] }.map(Self.split)?.first { $0.number == didTurn })
        }
        .hubPanel("Git", isPresented: $gitOpen) {
            if let thread {
                GitPanel(thread: thread, state: gitState, files: changed.count, added: changed.reduce(0) { $0 + $1.added }, removed: changed.reduce(0) { $0 + $1.removed },
                         pull: prNumber,
                         review: { gitOpen = false; nav.push(.changes(thread.id)) },
                         openPull: { gitOpen = false; nav.push(.pull(thread.id)) },
                         ask: { gitOpen = false; send($0, thread) },
                         changed: { await loadGit(thread) })
            }
        }
        .overlay {
            ZStack {
                if about, let thread {
                    ThreadAbout(thread: thread, editing: aboutRename,
                                close: { withAnimation(.snappy) { about = false } },
                                engine: { withAnimation(.snappy) { about = false; engine = true } },
                                move: { withAnimation(.snappy) { about = false; moving = true } })
                        .zIndex(10)
                }
            }
            .animation(.snappy, value: about)
        }
        .hubPanel("Run this thread on", isPresented: $moving) {
            if let thread { MovePanel(thread: thread) { moving = false } }
        }
        .hubPanel("Engine", isPresented: $engine) {
            if let thread { EnginePanel(thread: thread) { withAnimation(.snappy) { engine = false } } }
        }
        .hubPanel("Add context", isPresented: $adding) {
            ContextPanel(attachments: $attachments, text: $draft) { withAnimation(.snappy) { adding = false } }
        }
        .task(id: threadId) {
            if let t = thread, store.has("phone.changes", on: store.url(of: t.host)) {
                changed = (try? await store.client(for: t.id)?.changes(thread: t.id)) ?? []
            }
            if let t = thread { prNumber = await store.pullNumber(for: t); await loadGit(t) }
        }
    }

    /// Twelve more turns above the ones shown; the first one shown stays where it is on the screen.
    private func openEarlier(_ proxy: ScrollViewProxy, first: Int?) {
        Haptics.tap()
        landing = false
        window += 12
        guard let first else { return }
        DispatchQueue.main.async { proxy.scrollTo(first, anchor: .top) }
    }

    /// The folder's branch and push state, from a hubd that has `phone.git`; refreshed after a commit or push.
    private func loadGit(_ t: HubThread) async {
        guard !store.isDemo, store.has("phone.git", on: store.url(of: t.host)) else { return }
        gitState = try? await store.client(for: t.id)?.gitState(thread: t.id)
        changed = (try? await store.client(for: t.id)?.changes(thread: t.id)) ?? changed
    }

    /// Where the work stands, under the header, only when there is something to say.
    @ViewBuilder private func gitLine(_ t: HubThread) -> some View {
        if !changed.isEmpty || prNumber != nil || gitState?.pushWord != nil {
            GitLine(branch: gitState?.branch, pushWord: gitState?.pushWord, files: changed.count, added: changed.reduce(0) { $0 + $1.added }, removed: changed.reduce(0) { $0 + $1.removed },
                    pull: prNumber) { typing = false; gitOpen = true }
        }
    }

    /// The engine's model and effort as one short label in the box, a tap from its settings.
    private func engineLabel(_ t: HubThread) -> String {
        let opts = store.folded[t.id]?.options ?? []
        let pick = { (cat: String) in opts.first { $0.category == cat }.flatMap { o in o.choices.first { $0.value == o.value }?.name } }
        return pick("model") ?? EnginePanel.label(t.engine)
    }

    /// What @ offers: the files this thread changed, then recent threads, then projects.
    private func mentions(_ t: HubThread) -> [Mention] {
        let root = Store.projectRoot(t.cwd)
        let files = changed.map { c -> Mention in
            var rel = c.path.replacingOccurrences(of: "\\", with: "/")
            let r = root.replacingOccurrences(of: "\\", with: "/")
            if rel.hasPrefix(r) { rel = String(rel.dropFirst(r.count)).trimmingCharacters(in: CharacterSet(charactersIn: "/")) }
            return Mention(kind: .file, label: rel, detail: "changed", insert: "@" + rel)
        }
        let threads = store.threads.filter { $0.id != t.id }.sorted { ($0.returned_at ?? $0.updated_at) > ($1.returned_at ?? $1.updated_at) }.prefix(8)
            .map { Mention(kind: .thread, label: $0.title.isEmpty ? "New thread" : $0.title, detail: "thread", insert: "@thread:\"\($0.title)\"") }
        let projects = store.projects.filter { !store.isInternal($0) }.prefix(8)
            .map { Mention(kind: .project, label: $0.name, detail: "project", insert: "@" + $0.name) }
        return files + threads + projects
    }

    /// The agent's plan for this turn, while there is one: to approve when it planned first and
    /// nothing has begun, else ticked off as it works.
    @ViewBuilder private func plan(_ t: HubThread) -> some View {
        if let steps = store.planToApprove(t) {
            PlanApprovalCard(steps: steps, approve: { approve(t) }, change: {
                draft = "Change the plan:\n" + steps.map { "> " + $0.text }.joined(separator: "\n") + "\n\n"
                typing = true
            })
        } else if let f = store.folded[t.id], Self.hasPlan(f) {
            let start = Self.split(f).last?.start ?? (t.sent_at ?? 0)
            PlanDock(steps: f.plan, running: t.status == .running, start: f.planAt, seed: "\(t.id):\(Int(start))")
                .animation(.snappy, value: t.status == .running)
        }
    }

    /// The latest turn has a plan: the card or pill shows above the box, and while it works the working
    /// line goes without its cube, since the step at work has it (one moving cube per thread).
    static func hasPlan(_ f: Folded) -> Bool { !f.plan.isEmpty && f.planTurn == (f.turns.last ?? 0) }

    /// A go-ahead: the agent's own "yes" when it asked for one, else a message saying so.
    private func approve(_ t: HubThread) {
        Haptics.tap()
        if let ask = store.openAsk(t), let o = ask.options.first(where: { $0.kind.hasPrefix("allow") }) {
            Task { await store.answer(t, ask: ask, option: o.id) }
        } else {
            send("Looks good. Go ahead and run the plan.", t)
        }
    }

    /// Puts a message, quoted, in the box and the keyboard up.
    private func quote(_ message: String) {
        draft = (draft.isEmpty ? "" : draft + "\n") + Quote.text(message)
        typing = true
    }

    /// What `/` offers on the phone: Review the changes opens them, the others write in the box.
    private func commands(_ t: HubThread) -> [SlashCommand] {
        var list: [SlashCommand] = []
        if store.isDemo || store.has("phone.changes", on: store.url(of: t.host)) {
            list.append(SlashCommand(name: "diff", detail: "Review the changes", run: { typing = false; nav.push(.changes(t.id)) }))
        }
        list.append(SlashCommand(name: "plan", detail: "Plan first, no edits", fill: "Plan this first and change nothing until I say go: "))
        list.append(SlashCommand(name: "compact", detail: "Shrink the context", fill: "/compact"))
        return list
    }

    // MARK: Header: back, title with project · machine · engine, the menu

    private var header: some View {
        HubHeader(parent: nav.tabName, title: thread.map { $0.title.isEmpty ? "New thread" : $0.title } ?? "Thread",
                  sub: thread.map { t in [t.project, t.host ?? store.machine, t.engine.isEmpty ? nil : t.engine.capitalized].compactMap { $0 }.joined(separator: " · ") },
                  back: { dismiss() }) {
            HeaderButton("ellipsis", accessibility: "Thread menu") { menu = true }
        }
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
    }

    private func placeholder(_ t: HubThread) -> String {
        switch t.status {
        case .running: return "Queue a message for after this turn"
        case .waiting_on_you: return store.openAsk(t) == nil ? "Reply" : "Or write what it should do instead"
        default: return "Message"
        }
    }

    private func send(_ said: String, _ t: HubThread, steer: Bool = false) {
        // A chat may always use the internet, so there is no web search switch to add an instruction for.
        let text = said
        sending = true
        typing = false
        atEnd = true
        Task {
            let sent = attachments
            attachments = []
            toEnd += 1
            await store.send(text, to: t, steer: steer, attachments: sent)
            draft = ""
            sending = false
            atEnd = true
            toEnd += 1
        }
    }

    // MARK: The feed

    private func feed(_ t: HubThread) -> some View {
        let f = store.folded[t.id] ?? Folded()
        let every = Self.split(f)
        let hidden = max(0, every.count - window)
        let turns = Array(every.suffix(window))
        let detail = store.detail(of: t)
        let last = turns.last?.number
        return ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 16) {
                    if hidden > 0 {
                        Button { openEarlier(proxy, first: turns.first?.number) } label: {
                            Text("\(hidden) earlier turn\(hidden == 1 ? "" : "s") · pull down to open")
                                .font(.system(size: 12)).foregroundStyle(Hub.muted)
                                .frame(maxWidth: .infinity).padding(.vertical, 6)
                        }
                        .buttonStyle(.plain)
                    }
                    if turns.isEmpty {
                        Text(store.loading.contains(t.id) ? "Loading…" : "No conversation yet.")
                            .font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.top, 30).frame(maxWidth: .infinity)
                    }
                    ForEach(turns, id: \.number) { turn in
                        TurnView(turn: turn, latest: turn.number == last, running: t.status == .running && turn.number == last,
                                 open: openTurns.contains(turn.number), threadId: t.id, chat: store.isChat(t), detail: detail, made: f.made[turn.number] ?? [], did: { didTurn = $0.number }) {
                            if openTurns.contains(turn.number) { openTurns.remove(turn.number) } else { openTurns.insert(turn.number) }
                        }
                        // A message you send springs up from the box (sketch "Känsla", 4).
                        .transition(settled ? .asymmetric(insertion: .scale(scale: 0.86, anchor: .bottomTrailing).combined(with: .offset(y: 30)).combined(with: .opacity),
                                                          removal: .identity) : .identity)
                    }
                    ThreadPictures(thread: t)
                    if t.status == .running {
                        let start = turns.last?.start ?? (t.sent_at ?? 0)
                        let asks = store.openAsk(t) != nil
                        // A web search says so, with the sites it finds, until the answer starts.
                        let webTurn = turns.last.map { TurnView.web($0.items) } ?? false
                        if !asks, f.activity == "Searching the web…" || (webTurn && f.activity != "Writing…") {
                            WebSearching(start: start, pages: Web.pages(turns.last?.items ?? []))
                        } else {
                            // Minimal says only that it works: one playful word, a new one with every step.
                            let word = detail == .minimal ? PlayfulWord.pick(seed: t.id, (turns.last?.items ?? []).reduce(0) { n, i in if case let .work(s) = i { return n + s.count }; return n }) + "…" : nil
                            WorkingLine(words: asks ? "Waiting for you" : (word ?? f.activity ?? "Working…"), start: start, seed: "\(t.id):\(Int(start))", cube: !Self.hasPlan(f))
                        }
                    }
                    Color.clear.frame(height: 4).id("end")
                }
                .padding(.horizontal, 16).padding(.top, 14)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollDismissesKeyboard(.interactively)
            // It opens at the end from the first frame, so the page never shows its top first and jumps.
            .defaultScrollAnchor(.bottom, for: .initialOffset)
            .onScrollGeometryChange(for: Bool.self) { g in
                g.contentOffset.y + g.containerSize.height >= g.contentSize.height - 60
            } action: { _, end in atEnd = end }
            // How far the feed is pulled past its end: the next thread peeks up with that.
            .onScrollGeometryChange(for: CGFloat.self) { g in
                g.contentOffset.y + g.containerSize.height - g.contentSize.height - g.contentInsets.bottom
            } action: { _, over in
                let up = max(0, over)
                pull = (interacting && pullFromEnd) ? up : 0
                guard interacting, pullFromEnd, store.nextThread(after: t.id) != nil else { return }
                // The rubber band gives about a third of the finger's travel, so a short pull arms it.
                let reached = up > (pullArmed ? 30 : 42)
                if reached != pullArmed { pullArmed = reached; Haptics.tick() }
            }
            // How far the feed is pulled down past its top: the line of earlier turns opens with that.
            .onScrollGeometryChange(for: CGFloat.self) { g in
                max(0, -(g.contentOffset.y + g.contentInsets.top))
            } action: { _, over in
                topPull = (interacting && pullFromTop) ? over : 0
                guard interacting, pullFromTop, hidden > 0 else { return }
                let reached = over > (topArmed ? 56 : 72)
                if reached != topArmed { topArmed = reached; Haptics.tick() }
            }
            .onScrollPhaseChange { old, new, context in
                if new == .interacting {
                    // Only a pull that starts with the feed already at its end counts; a scroll that gets there stays a scroll.
                    let g = context.geometry
                    // A little slack: the lazy feed's height still settles by a few points after it lands at the end.
                    pullFromEnd = g.contentOffset.y + g.containerSize.height >= g.contentSize.height + g.contentInsets.bottom - 24
                    pullFromTop = g.contentOffset.y <= -g.contentInsets.top + 2
                    interacting = true
                    landing = false
                    if pullFromEnd || pullFromTop { Haptics.ready() }
                } else if old == .interacting {
                    interacting = false
                    let go = pullFromEnd && pullArmed
                    let earlier = pullFromTop && topArmed && hidden > 0
                    pullFromEnd = false; pullArmed = false; pull = 0
                    pullFromTop = false; topArmed = false; topPull = 0
                    if go, let next = store.nextThread(after: t.id) { Haptics.tap(); nav.replaceTop(.thread(next.id)) }
                    if earlier { openEarlier(proxy, first: turns.first?.number) }
                }
            }
            .overlay(alignment: .top) {
                if topPull > 4, hidden > 0 {
                    HStack(spacing: 7) {
                        Image(systemName: "arrow.down").font(.system(size: 12, weight: .semibold)).rotationEffect(.degrees(topArmed ? 180 : 0))
                        Text(topArmed ? "Let go for earlier turns" : "\(min(hidden, 12)) earlier turns")
                    }
                    .font(.system(size: 12.5)).foregroundStyle(topArmed ? Hub.accent : Hub.muted)
                    .frame(height: min(40, topPull * 0.8), alignment: .center).clipped().padding(.top, 4)
                    .allowsHitTesting(false)
                }
            }
            .overlay(alignment: .bottom) {
                if pull > 4, let next = store.nextThread(after: t.id) {
                    NextPeek(thread: next, armed: pullArmed)
                        .frame(height: min(64, pull * 0.9), alignment: .top).clipped()
                        .padding(.bottom, 6).allowsHitTesting(false)
                }
            }
            // A thread always opens at its latest message (the "New" pill goes up to what is new). The
            // log arrives after the page and the lazy rows settle their heights for a moment, so every
            // change of the feed's height puts it back at the end until the first touch or about a
            // second after the conversation came in.
            .onAppear { proxy.scrollTo("end", anchor: .bottom) }
            .onScrollGeometryChange(for: CGFloat.self) { $0.contentSize.height } action: { _, _ in
                guard landing, !interacting else { return }
                proxy.scrollTo("end", anchor: .bottom)
            }
            .onChange(of: f.items.count, initial: true) { _, count in
                guard count > 0, !landed else { return }
                landed = true
                proxy.scrollTo("end", anchor: .bottom)
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { landing = false }
            }
            .overlay(alignment: .top) {
                if store.hasNew(t.id), !store.newInView, let turn = Self.newTurn(turns) {
                    Button { Haptics.tap(); withAnimation(.snappy) { proxy.scrollTo(turn, anchor: .top) } } label: {
                        HStack(spacing: 6) {
                            Circle().fill(Color(hex: 0xec9a64)).frame(width: 6, height: 6)
                            Image(systemName: "arrow.up").font(.system(size: 11, weight: .semibold))
                            Text("New").font(.system(size: 12.5, weight: .medium))
                        }
                        .foregroundStyle(Hub.text3)
                        .padding(.horizontal, 12).padding(.vertical, 7)
                        .background(Capsule().fill(Hub.surface2)).overlay(Capsule().strokeBorder(Hub.lineStrong))
                        .shadow(color: .black.opacity(0.4), radius: 8, y: 4)
                    }
                    .padding(.top, 8)
                    .transition(.opacity)
                    .accessibilityLabel("Back to what is new")
                }
            }
            .onChange(of: f.items.count) { if atEnd { withAnimation { proxy.scrollTo("end", anchor: .bottom) } } }
            .onChange(of: f.lastReply) { if atEnd { proxy.scrollTo("end", anchor: .bottom) } }
            .onChange(of: typing) { if typing { withAnimation { proxy.scrollTo("end", anchor: .bottom) } } }
            .onChange(of: toEnd) { Self.jump(proxy) }
            .overlay(alignment: .bottom) {
                // In the middle, clear of the suggested replies at the right.
                if !atEnd && !turns.isEmpty {
                    Button { Self.jump(proxy); Haptics.tap() } label: {
                        Image(systemName: "chevron.down").font(.system(size: 15, weight: .semibold)).foregroundStyle(Hub.text)
                            .frame(width: 40, height: 40).background(Circle().fill(Hub.surface2)).overlay(Circle().strokeBorder(Hub.lineStrong))
                            .shadow(color: .black.opacity(0.4), radius: 8, y: 4)
                    }
                    .padding(10)
                    .accessibilityLabel("Scroll to the end")
                }
            }
        }
    }

    /// To the end of the feed, also while it still glides from a flick: a jump without animation stops
    /// the glide (an animated scroll lost against it), and a second one after the lazy rows below have
    /// been laid out lands on the real end.
    static func jump(_ proxy: ScrollViewProxy) {
        proxy.scrollTo("end", anchor: .bottom)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.12) { withAnimation(.snappy(duration: 0.2)) { proxy.scrollTo("end", anchor: .bottom) } }
    }

    /// The turn that holds the "New" break, if there is one.
    static func newTurn(_ turns: [Turn]) -> Int? {
        turns.first { $0.items.contains { if case let .brk(b) = $0 { return b.kind == "new" }; return false } }?.number
    }

    /// One turn: what you wrote and everything up to your next message.
    struct Turn {
        var number: Int
        var start: Double
        var end: Double
        var items: [Item]
        /// The seq of your message and of the turn end that closes the reply: what a reaction points at.
        var userSeq: Int?
        var endSeq: Int?
        var reactions: [Int: [ReactionChip]] = [:]
    }

    static func split(_ f: Folded) -> [Turn] {
        var turns: [Int: Turn] = [:]
        for (i, item) in f.items.enumerated() {
            let n = i < f.turns.count ? f.turns[i] : 0
            let ts = i < f.times.count ? f.times[i] : 0
            if turns[n] == nil { turns[n] = Turn(number: n, start: ts, end: f.turnEnd[n] ?? ts, items: [], userSeq: f.userSeq[n], endSeq: f.endSeq[n], reactions: f.reactions) }
            turns[n]?.items.append(item)
        }
        return turns.values.sorted { $0.number < $1.number }
    }

    // MARK: Status: one line that ticks (sketch 2, screen 4)

    @ViewBuilder private func status(_ t: HubThread) -> some View {
        let turns = store.folded[t.id].map(Self.split) ?? []
        let start = turns.last?.start ?? (t.sent_at ?? 0)
        // A running thread shows its one line at the end of the feed (`WorkingLine`), not a bar here.
        if t.status == .running {
            EmptyView()
        } else if t.status == .waiting_on_you, let ret = t.returned_at {
            let worked = turns.last.map { $0.end - $0.start } ?? 0
            HStack(spacing: 6) {
                StatusDot(state: .waiting)
                Text("Waiting for you · \(relative(ret))\(worked > 60_000 ? " · worked \(Self.duration(worked))" : "")")
                    .font(.system(size: 12)).foregroundStyle(Hub.muted)
                Spacer()
            }
            .padding(.horizontal, 18).padding(.bottom, 4)
        } else if t.status == .in_terminal {
            Text("Taken over in a terminal. Messages wait until it is handed back.")
                .font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.horizontal, 18).padding(.bottom, 4)
        }
    }

    static func duration(_ ms: Double) -> String {
        let s = max(0, Int(ms / 1000))
        if s < 60 { return "\(s)s" }
        if s < 3600 { return String(format: "%dm %02ds", s / 60, s % 60) }
        return String(format: "%dh %02dm", s / 3600, (s % 3600) / 60)
    }

    // MARK: Docked: the ask, or the agent's suggested replies

    @ViewBuilder private func dock(_ t: HubThread) -> some View {
        if let ask = store.openAsk(t) {
            AskCard(thread: t, ask: ask)
                .padding(.horizontal, 12).padding(.bottom, 4)
                .transition(.move(edge: .bottom).combined(with: .opacity))
        } else if let replies = store.replies(t) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 7) {
                    ForEach(replies.options, id: \.self) { r in
                        Button(r.label) { send(r.message, t) }.buttonStyle(PillButtonStyle())
                    }
                }
                .padding(.horizontal, 16)
            }
            .swipeZone()
            .padding(.bottom, 2)
        }
    }
}
