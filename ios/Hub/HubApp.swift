import SwiftUI

@main
struct HubApp: App {
    @State private var store = Store()
    @State private var nav = Nav()
    /// The "What's new" sheet after an update, and how high it is (`ReleaseNotesView.swift`).
    @State private var notes: ReleaseBuild?
    @State private var notesDetent: PresentationDetent = .medium
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            Group {
                if store.ready {
                    main
                } else {
                    Onboarding { nav.tab = 0 }
                }
            }
            .foregroundStyle(nav.world == .dev ? Theme.dev.text : Theme.warm.text)
            .tint(nav.world == .dev ? Theme.dev.accent : Theme.warm.accent)
            .preferredColorScheme(.dark)
            .environment(store)
            .environment(nav)
            .sheet(item: $notes) { b in
                WhatsNewSheet(build: b, earlier: ReleaseNotes.load().filter { $0.build != b.build }, detent: $notesDetent)
                    .presentationDetents([.medium, .large], selection: $notesDetent)
                    .presentationDragIndicator(.visible)
                    .environment(store).environment(nav)
            }
            .task(id: store.ready) {
                // Once per build, a moment after the app is up, never on a first install or with the notes off.
                guard store.ready else { return }
                try? await Task.sleep(for: .seconds(1))
                if let b = ReleaseNotes.check() { notesDetent = .medium; notes = b }
            }
            .watchesTouches()
            .task { Perf.start() }
            .task {
                // `-pair <link>` at launch pairs without typing, for the simulator and tests.
                if !store.ready, let link = UserDefaults.standard.string(forKey: "pair") {
                    await store.pair(link: link)
                } else {
                    store.start()
                }
            }
            .task {
                // `-dictate 1` listens for 8 s at launch and prints what it hears, to test dictation.
                guard UserDefaults.standard.bool(forKey: "dictate") else { return }
                let d = Dictation()
                await d.start()
                print("hub dictation: started=\(d.listening) error=\(d.error ?? "-")")
                try? await Task.sleep(for: .seconds(8))
                print("hub dictation: result '\(d.stop())'")
            }
            .onReceive(NotificationCenter.default.publisher(for: .openProject)) { note in
                if let p = note.object as? ProjectRef { nav.push(.project(p)) }
            }
            .onOpenURL { url in
                // hub://thread/<id> from a notice, hub://inbox from the test one.
                guard url.scheme == "hub" else { return }
                nav.tab = 0
                if url.host == "thread", let id = url.pathComponents.dropFirst().first { nav.inbox = [.thread(id)] } else { nav.inbox = [] }
            }
            // While the app is in front the machines hold back notices about the thread on screen
            // (`Store.look`): said now, again every minute, at once when another thread opens, and
            // taken back when the app goes to the background.
            .task(id: phase) {
                guard phase == .active else { return }
                while !Task.isCancelled {
                    await store.look(active: true)
                    try? await Task.sleep(for: .seconds(60))
                }
            }
            .onChange(of: store.openThread) { if phase == .active { Task { await store.look(active: true) } } }
            .onChange(of: phase) {
                // iOS drops the sockets in the background: back in front, reconnect and catch up.
                if phase == .active { store.start() }
                if phase == .background { Task { await store.look(active: false) }; store.stopAll() }
            }
        }
    }

    private var main: some View {
        WorldLayer {
            ZStack(alignment: .bottom) {
                ZStack {
                    if nav.world == .dev {
                        InboxView().opacity(nav.tab == 0 ? 1 : 0).allowsHitTesting(nav.tab == 0)
                        ProjectsView().opacity(nav.tab == 1 ? 1 : 0).allowsHitTesting(nav.tab == 1)
                        MachinesView().opacity(nav.tab == 2 ? 1 : 0).allowsHitTesting(nav.tab == 2)
                    } else {
                        ChatsView().opacity(nav.tab == 0 ? 1 : 0).allowsHitTesting(nav.tab == 0)
                        SpacesView().opacity(nav.tab == 1 ? 1 : 0).allowsHitTesting(nav.tab == 1)
                        ArtifactsView().opacity(nav.tab == 2 ? 1 : 0).allowsHitTesting(nav.tab == 2)
                    }
                    SettingsView().opacity(nav.tab == 3 ? 1 : 0).allowsHitTesting(nav.tab == 3)
                }
                if !nav.deep && !nav.picking {
                    FloatingBar().transition(.move(edge: .bottom).combined(with: .opacity))
                }
                if nav.penMenu && !nav.deep { PenMenu().zIndex(5) }
                if let t = nav.placesTab, !nav.deep { RecentPlaces(tab: t).zIndex(5) }
                if let m = nav.menu { HubMenuHost(request: m).id(m.id).zIndex(8) }
                if let c = nav.confirm { HubConfirmHost(request: c).id(c.id).zIndex(9) }
                if nav.talking { TalkIdea().zIndex(6).transition(.move(edge: .bottom).combined(with: .opacity)) }
            }
            .animation(.snappy, value: nav.talking)
            .animation(.snappy, value: nav.deep)
            .animation(.snappy, value: nav.picking)
            .watchesPlaces()
            .background(Hub.bg)
            .overlay(alignment: .top) {
                // A swipe down from the top edge changes world. Only this 30 pt strip takes the
                // gesture (a simultaneous one), the rest of the screen is never covered.
                if !nav.deep && nav.snapshot == nil {
                    Color.clear.frame(height: 30).frame(maxWidth: .infinity).contentShape(Rectangle())
                        .simultaneousGesture(DragGesture(minimumDistance: 24).onEnded { v in
                            if v.translation.height > 50, abs(v.translation.width) < v.translation.height { nav.switchWorld(tap: nil) }
                        })
                }
            }
        }
        .background(Hub.bg)
    }
}
