import SwiftUI

/// Settings (sketch 4): three groups, you, threads and machines, each row with what holds now, so
/// you seldom need to go in. What only the PC may change says so on its page.
struct SettingsView: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @State private var search = ""

    var body: some View {
        NavigationStack(path: Bindable(nav).settings) {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    PageTitle(title: "Settings") { EmptyView() }
                    HStack(spacing: 8) {
                        Image(systemName: "magnifyingglass").foregroundStyle(Hub.muted)
                        TextField("", text: $search, prompt: Text("Search settings").foregroundStyle(Hub.muted)).autocorrectionDisabled()
                    }
                    .font(.system(size: 15)).padding(.horizontal, 12).frame(height: 38)
                    .background(RoundedRectangle(cornerRadius: 10).fill(Hub.surface2))
                    .padding(.horizontal, 14)

                    if store.watchAvailable {
                        section("Watch", [
                            row("Problems", "exclamationmark.triangle", value: store.openProblems.isEmpty ? "None" : "\(store.openProblems.count) open", route: .problems),
                            row("Routines", "clock", value: RoutinesView.summary(store.watchRoutines), route: .routines),
                        ])
                    }
                    section("You", [
                        row("Notifications", "bell", value: "ntfy", page: .notifications),
                        row("Devices", "iphone", value: "\(store.machines.count)", page: .devices),
                        row("Appearance", "circle.lefthalf.filled", value: "Dev and Chat", page: .appearance),
                        row("Dictation", "mic", value: Dictation.language.hasPrefix("sv") ? "Svenska" : "English", page: .dictation),
                    ])
                    section("Threads", [
                        row("New threads", "pencil", value: "Last used", page: .newThreads),
                        row("Safety", "shield", value: "Ask first", page: .safety),
                        row("Usage", "chart.bar", value: store.has("phone.stats") && !store.isDemo ? "Claude Code" : "", page: .usage),
                        row("Chat detail", "text.alignleft", value: store.has("phone.chatdetail") ? (ChatDetail(rawValue: store.chatDetailPrefs?.default ?? "") ?? .quiet).name : "", page: .chatDetail),
                        row("Inbox and limits", "tray", value: store.usage?.plan?.limits.first.map { "\(Int($0.percent)) %" } ?? "", page: .limits),
                    ])
                    let online = store.hosts.filter { $0.state == "online" }.count
                    section("Machines", [
                        row("Updates", "arrow.down.circle", value: updatesValue, page: .updates),
                        row("Machines", "server.rack", value: store.hosts.isEmpty ? "\(store.machines.count)" : "\(online) of \(store.hosts.count)", page: .machines),
                        row("Engines", "sparkle", value: "\(store.engines.count)", page: .engines),
                    ])
                    Button { nav.settings.append(.settings(.updates)) } label: {
                        Text("Hub \(version) ›").font(.system(size: 12)).foregroundStyle(Hub.accent)
                            .padding(.horizontal, 30).padding(.vertical, 12).contentShape(Rectangle())
                    }
                    .buttonStyle(.plain).padding(.top, 10)
                }
                .padding(.bottom, 30)
            }
            .contentMargins(.bottom, 96, for: .scrollContent)
            .scrollsToTop(3)
            .background(Hub.bg)
            .toolbar(.hidden, for: .navigationBar)
            .navigationDestination(for: Route.self) { RouteView(route: $0) }
            .task { await store.loadMachines(); await store.loadWatch() }
            .onAppear {
                // `-page notifications` at launch opens that page, for screenshots.
                if nav.settings.isEmpty, let p = UserDefaults.standard.string(forKey: "page"),
                   let page = [("notifications", SettingsPage.notifications), ("safety", .safety), ("limits", .limits), ("updates", .updates), ("devices", .devices)].first(where: { $0.0 == p })?.1 {
                    nav.settings = [.settings(page)]
                }
            }
        }
    }

    private var version: String {
        "\(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?") (\(Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "?"))"
    }

    private var updatesValue: String {
        // The build this app is, so it reads without opening the page; a build from Xcode falls back to the machines.
        let me = ReleaseBuild(build: ReleaseNotes.current)
        if me.date != nil { return "Build \(me.time)" }
        let versions = Set(store.hosts.compactMap(\.version))
        guard !versions.isEmpty else { return "not read" }
        return versions.count == 1 ? "1 running version" : "\(versions.count) running versions"
    }

    private struct Entry: Identifiable { var id: String { title }; var title: String; var icon: String; var value: String; var target: Route }

    private func row(_ title: String, _ icon: String, value: String, page: SettingsPage) -> Entry { Entry(title: title, icon: icon, value: value, target: .settings(page)) }
    private func row(_ title: String, _ icon: String, value: String, route: Route) -> Entry { Entry(title: title, icon: icon, value: value, target: route) }

    @ViewBuilder private func section(_ title: String, _ entries: [Entry]) -> some View {
        let q = search.lowercased()
        let shown = q.isEmpty ? entries : entries.filter { $0.title.lowercased().contains(q) }
        if !shown.isEmpty {
            GroupLabel(text: title)
            VStack(spacing: 0) {
                ForEach(shown) { e in
                    NavigationLink(value: e.target) {
                        HStack(spacing: 12) {
                            Image(systemName: e.icon).foregroundStyle(Hub.muted).frame(width: 20)
                            Text(e.title).font(.system(size: 15))
                            Spacer()
                            if e.value.hasSuffix("behind") {
                                Text(e.value).font(.system(size: 11.5, weight: .semibold)).foregroundStyle(Hub.amber)
                                    .padding(.horizontal, 7).padding(.vertical, 2).background(Capsule().fill(Hub.amberBg))
                            } else {
                                Text(e.value).font(.system(size: 13.5)).foregroundStyle(Hub.muted)
                            }
                            Image(systemName: "chevron.right").font(.system(size: 11)).foregroundStyle(Hub.muted)
                        }
                        .padding(.horizontal, 13).frame(minHeight: 46)
                        .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 45) }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.pressFlat)
                }
            }
            .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
            .padding(.horizontal, 14)
        }
    }
}

enum SettingsPage: Hashable { case notifications, devices, dictation, newThreads, safety, limits, updates, machines, engines, usage, appearance, chatDetail }

struct SettingsPageView: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @Environment(\.dismiss) private var dismiss
    let page: SettingsPage
    @State private var language = Dictation.language
    @State private var confirmForget = false

    var body: some View {
        VStack(spacing: 0) {
            HubHeader(parent: "Settings", title: title, back: { dismiss() })
            ScrollView { VStack(alignment: .leading, spacing: 0) { content }.padding(.bottom, 30) }
        }
        .background(Hub.bg)
        .swipeBack { dismiss() }
        .toolbar(.hidden, for: .navigationBar)
    }

    private var title: String {
        switch page {
        case .notifications: return "Notifications"
        case .devices: return "Devices"
        case .dictation: return "Dictation"
        case .newThreads: return "New threads"
        case .safety: return "Safety"
        case .limits: return "Inbox and limits"
        case .updates: return "Updates"
        case .machines: return "Machines"
        case .engines: return "Engines"
        case .usage: return "Usage"
        case .appearance: return "Appearance"
        case .chatDetail: return "Chat detail"
        }
    }

    @ViewBuilder private var content: some View {
        switch page {
        case .devices:
            GroupLabel(text: "This iPhone")
            box {
                line(UIDevice.current.name, sub: "paired with \(store.machine ?? "Hub") · phone key in the Keychain")
                line("Talks straight to", value: "\(store.machines.filter { store.live[$0.url] == true }.count) machines")
            }
            GroupLabel(text: "How it reaches each machine")
            box {
                ForEach(store.machines, id: \.url) { m in
                    HStack(spacing: 11) {
                        Circle().fill(store.live[m.url] == true ? Hub.green : Hub.red).frame(width: 8, height: 8)
                        Text(m.name).font(.system(size: 15))
                        Spacer()
                        Text(store.live[m.url] == true ? (m.home ? "Tailscale · paired" : "Tailscale") : "through \(store.machine ?? "the PC")")
                            .font(.system(size: 13)).foregroundStyle(Hub.muted)
                    }
                    .padding(.horizontal, 13).frame(minHeight: 46)
                    .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 32) }
                }
            }
            note("The phone doesn't go through the PC, so it works while the PC sleeps.")
            PairedDevices()
            Button { confirmForget = true } label: {
                Text("Forget these machines").font(.system(size: 15, weight: .semibold)).foregroundStyle(Hub.red)
                    .frame(maxWidth: .infinity).frame(height: 48).background(RoundedRectangle(cornerRadius: 13).fill(Hub.redBg))
            }
            .padding(.horizontal, 14).padding(.top, 20)
            .hubConfirm("Forget the machines?", message: "You scan a new code to pair again.", confirm: "Forget", hold: true, destructive: true, isPresented: $confirmForget) { store.forget(); nav.settings = []; nav.tab = 0 }
        case .dictation:
            GroupLabel(text: "Language")
            box {
                ForEach([("sv-SE", "Svenska"), ("en-US", "English")], id: \.0) { code, name in
                    Button { language = code; Dictation.language = code } label: {
                        HStack {
                            Text(name).font(.system(size: 15))
                            Spacer()
                            if language == code { Image(systemName: "checkmark").foregroundStyle(Hub.accent) }
                        }
                        .padding(.horizontal, 13).frame(minHeight: 46).contentShape(Rectangle())
                        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
                    }
                    .buttonStyle(.plain)
                }
            }
            note("Swedish goes to Apple's recognizer on its servers, which hears it far better than the phone's own model. English stays on the phone.")
        case .notifications: NotificationsPage()
        case .safety: SafetyPage()
        case .limits: InboxLimitsPage()
        case .usage: UsagePage()
        case .appearance: AppearancePage()
        case .chatDetail: ChatDetailPage()
        case .updates: UpdatesPage()
        case .machines, .engines, .newThreads:
            GroupLabel(text: title)
            box {
                switch page {
                case .machines: ForEach(store.hosts) { h in line(h.name, sub: h.error ?? h.address, value: h.state) }
                case .engines: ForEach(store.engines, id: \.self) { e in line(e.capitalized, value: "\(store.threads.filter { $0.engine == e }.count) threads") }
                case .newThreads: line("Project, machine and engine", sub: "the ones you used last, changed on the line above the box")
                case .safety:
                    line("Ask first", sub: "asks before commands and edits, as each project's safety says")
                    line("The panic button", sub: "under Machines, pauses every agent everywhere")
                default: line("Not on the phone yet", sub: "Hub can send notices through the ntfy app; that comes next")
                }
            }
            note("These are set in Hub on the PC. The phone shows what holds.")
        }
    }

    private func box<C: View>(@ViewBuilder _ c: () -> C) -> some View {
        VStack(spacing: 0) { c() }.background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface)).padding(.horizontal, 14)
    }

    private func line(_ title: String, sub: String? = nil, value: String? = nil) -> some View {
        HStack(spacing: 11) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 15))
                if let sub { Text(sub).font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(2) }
            }
            Spacer()
            if let value { Text(value).font(.system(size: 13.5)).foregroundStyle(Hub.muted) }
        }
        .padding(.horizontal, 13).padding(.vertical, 10).frame(minHeight: 46)
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
    }

    private func note(_ text: String) -> some View {
        Text(text).font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.horizontal, 30).padding(.top, 6)
    }
}
