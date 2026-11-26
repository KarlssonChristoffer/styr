import SwiftUI

/// Rows the settings pages share: a grouped box, a toggle row, a choice row with a check.
struct SBox<C: View>: View {
    @ViewBuilder var content: () -> C
    var body: some View { VStack(spacing: 0) { content() }.background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface)).padding(.horizontal, 14) }
}

struct SToggle: View {
    let title: String
    var sub: String?
    @Binding var on: Bool
    var body: some View {
        Toggle(isOn: $on) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 15))
                if let sub { Text(sub).font(.system(size: 12)).foregroundStyle(Hub.muted) }
            }
        }
        .tint(Hub.green)
        .padding(.horizontal, 13).padding(.vertical, 8).frame(minHeight: 46)
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
    }
}

struct SChoice: View {
    let title: String
    var sub: String?
    let on: Bool
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.system(size: 15))
                    if let sub { Text(sub).font(.system(size: 12)).foregroundStyle(Hub.muted) }
                }
                Spacer()
                if on { Image(systemName: "checkmark").foregroundStyle(Hub.accent) }
            }
            .padding(.horizontal, 13).padding(.vertical, 8).frame(minHeight: 46).contentShape(Rectangle())
            .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
        }
        .buttonStyle(.plain)
    }
}

struct SNote: View {
    let text: String
    var body: some View { Text(text).font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.horizontal, 30).padding(.top, 6).frame(maxWidth: .infinity, alignment: .leading) }
}

// MARK: Notifications (sketch 4, screen 2)

/// Which notices reach the phone, set on every machine the phone follows, through the ntfy app.
struct NotificationsPage: View {
    @Environment(Store.self) private var store
    @Environment(\.openURL) private var openURL
    @State private var n = NotifyPrefs()
    @State private var loaded = false
    @State private var quiet = false
    @State private var failed: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            GroupLabel(text: "Sent through")
            SBox {
                SChoice(title: "ntfy app", sub: n.topic.isEmpty ? "not set up yet" : "topic \(n.topic)", on: !n.topic.isEmpty) { setUp() }
                SChoice(title: "Only inside Hub", sub: "nothing on the lock screen", on: n.topic.isEmpty) { n.topic = ""; save() }
            }
            HStack(spacing: 4) {
                Text("Apple only lets paid apps push, so Hub goes through ntfy.").font(.system(size: 12)).foregroundStyle(Hub.muted)
                if !n.topic.isEmpty { Button("Send a test") { test() }.font(.system(size: 12)).foregroundStyle(Hub.accent) }
            }
            .padding(.horizontal, 30).padding(.top, 6)
            if !n.topic.isEmpty {
                SBox {
                    Button { openURL(URL(string: "https://apps.apple.com/app/ntfy/id1625396347")!) } label: {
                        row("Get the ntfy app", sub: "free, from the App Store", value: "Get", icon: "bell")
                    }.buttonStyle(.plain)
                    Button { subscribe() } label: {
                        row("Subscribe to Hub", sub: "copies the topic: in ntfy tap +, paste, Subscribe", value: "Copy", icon: "link")
                    }.buttonStyle(.plain)
                }
                .padding(.top, 10)
            }
            GroupLabel(text: "Tell me when a thread")
            SBox {
                SToggle(title: "Needs an approval", on: bind(\.approvals))
                SToggle(title: "Asks a question", on: bind(\.questions))
                SToggle(title: "Fails or hits a limit", on: bind(\.failures))
                SToggle(title: "Is done", sub: "held back in quiet hours", on: bind(\.done))
            }
            GroupLabel(text: "Also")
            SBox {
                SToggle(title: "Quiet hours", sub: "23–07, approvals still come through", on: Binding(get: { n.quiet_from != nil }, set: { on in
                    n.quiet_from = on ? 23 : nil; n.quiet_to = on ? 7 : nil; save()
                }))
                SToggle(title: "Quiet while I'm at the PC", sub: "nothing about the thread you have open there", on: Binding(get: { n.quiet_at_pc ?? true }, set: { n.quiet_at_pc = $0; save() }))
            }
            if let failed { SNote(text: failed) }
        }
        .task { await load() }
    }

    private var host: String { URL(string: n.server)?.host ?? "ntfy.sh" }

    /// ntfy for iPhone takes no link to a topic, so the topic goes on the clipboard and ntfy is
    /// subscribed by hand: + in ntfy, paste, Subscribe.
    private func subscribe() {
        UIPasteboard.general.string = n.topic
        Haptics.tap()
        store.flash("Topic copied. In ntfy, tap +, paste it and tap Subscribe.")
    }

    private func row(_ title: String, sub: String, value: String, icon: String) -> some View {
        HStack(spacing: 12) {
            Image(systemName: icon).foregroundStyle(Hub.muted).frame(width: 20)
            VStack(alignment: .leading, spacing: 2) { Text(title).font(.system(size: 15)); Text(sub).font(.system(size: 12)).foregroundStyle(Hub.muted) }
            Spacer()
            Text(value).font(.system(size: 14, weight: .semibold)).foregroundStyle(Hub.accent)
        }
        .padding(.horizontal, 13).padding(.vertical, 8).frame(minHeight: 50).contentShape(Rectangle())
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
    }

    private func bind(_ key: WritableKeyPath<NotifyPrefs, Bool>) -> Binding<Bool> {
        Binding(get: { n[keyPath: key] }, set: { n[keyPath: key] = $0; save() })
    }

    private func setUp() {
        if n.topic.isEmpty, let last = UserDefaults.standard.string(forKey: "notices.topic") {
            // Back on after "Only inside Hub": the same topic, so ntfy needs nothing new.
            n.topic = last
        } else if n.topic.isEmpty {
            // A topic nobody guesses: ntfy topics are open to whoever knows the name.
            let words = (store.machine ?? "hub").lowercased().filter { $0.isLetter }
            n.topic = "hub-\(words.prefix(8))-\(UUID().uuidString.prefix(8).lowercased())"
        }
        save()
    }

    /// The phone's own settings first (`Notices.swift`), so they show and save even while a
    /// machine is away or too old; then every machine gets them.
    private func load() async {
        guard !store.isDemo else { loaded = true; return }
        if store.notices == nil { await store.syncNotices() }
        if let mine = store.notices { n = mine }
        loaded = true
        if !store.has("phone.notify") { failed = "\(store.machine ?? "Hub") needs a newer Hub to send notices. They start once it is updated."; return }
        await push()
    }

    /// The same notices on every machine the phone follows, so each tells of its own threads.
    private func save() {
        guard loaded, !store.isDemo else { return }
        store.keepNotices(n)
        Task { await push() }
    }

    private func push() async {
        let errors = await store.syncNotices(force: true)
        if store.has("phone.notify") { failed = errors.isEmpty ? nil : "Not saved on " + errors.joined(separator: "; ") }
    }

    /// A test from every machine, so each one's path to ntfy is checked.
    private func test() {
        Task {
            _ = await store.syncNotices(force: true)
            var sent: [String] = [], failedOn: [String] = []
            for name in store.noticeMachines {
                guard let t = store.target(name) else { continue }
                do { try await t.client.testNotify(host: t.host); sent.append(name ?? "Hub") }
                catch { failedOn.append("\(name ?? "Hub"): \(error.localizedDescription)") }
            }
            if failedOn.isEmpty { store.flash("Sent a test from \(sent.joined(separator: ", "))") }
            else { failed = "Test not sent from " + failedOn.joined(separator: "; ") ; store.flash("Some tests were not sent") }
        }
    }
}

// MARK: Safety (sketch 4, screen 4)

struct SafetyPage: View {
    @Environment(Store.self) private var store
    @State private var prefs: PermissionPrefs?
    @State private var failed: String?
    @State private var confirmPause = false

    private let levels: [(String, String, String)] = [
        ("read", "Read only", "looks and reports"),
        ("ask", "Ask first", "asks before commands and edits"),
        ("edit", "Edits are fine", "asks before commands"),
        ("everything", "Full access", "never asks"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            GroupLabel(text: "New threads start at")
            SBox {
                ForEach(levels, id: \.0) { l in
                    SChoice(title: l.1, sub: l.2, on: prefs?.level == l.0) { set(l.0) }
                }
            }
            if let p = prefs, !p.deny.isEmpty || !p.protected.isEmpty {
                GroupLabel(text: "Always ask, whatever the level")
                SBox {
                    ForEach(p.deny + p.protected, id: \.self) { rule in
                        Text(rule).font(.system(size: 13, design: .monospaced)).frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 13).frame(minHeight: 42).overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
                    }
                }
            }
            if let failed { SNote(text: failed) }
            Button { confirmPause = true } label: {
                Label("Pause every agent, everywhere", systemImage: "pause.fill").font(.system(size: 15, weight: .semibold)).foregroundStyle(Hub.red)
                    .frame(maxWidth: .infinity).frame(height: 48).background(RoundedRectangle(cornerRadius: 13).fill(Hub.redBg))
            }
            .padding(.horizontal, 14).padding(.top, 22)
            .hubConfirm("Pause every agent?", message: "Every agent on every machine stops where it is. Nothing is lost, and you start them again from here.", confirm: "Pause", hold: true, destructive: true, isPresented: $confirmPause) { Task { await store.setPaused(true) } }
        }
        .task {
            guard !store.isDemo, let c = store.homeClient else { prefs = PermissionPrefs(level: "ask", allow: [], deny: ["git push --force"], protected: []); return }
            do { prefs = try await c.permissions() } catch let e as HubClient.Failure where e.tooOld { failed = "Needs a newer hubd." } catch { failed = error.localizedDescription }
        }
    }

    private func set(_ level: String) {
        guard var p = prefs else { return }
        p.level = level
        prefs = p
        Haptics.tap()
        guard !store.isDemo, let c = store.homeClient else { return }
        Task { do { try await c.setPermissions(p) } catch { failed = "Not saved: \(error.localizedDescription)" } }
    }
}

// MARK: Inbox and limits (sketch 4, screen 5)

struct InboxLimitsPage: View {
    @Environment(Store.self) private var store
    @State private var prefs: ThreadPrefs?
    @State private var failed: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            let plans: [(String, UsageReport.Plan?)] = (store.usage.map { u in [("Claude", u.plan)] + u.others.map { ($0.label, $0.plan) } }) ?? []
            GroupLabel(text: "Plans")
            SBox {
                ForEach(Array(plans.enumerated()), id: \.offset) { _, entry in
                    ForEach(entry.1?.limits ?? [], id: \.self) { limit in LimitRow(name: entry.0, limit: limit) }
                }
                if plans.isEmpty { Text("No limits read yet").font(.system(size: 14)).foregroundStyle(Hub.muted).padding(13) }
            }
            GroupLabel(text: "Inbox")
            SBox {
                SToggle(title: "Settle when the PR is merged", on: Binding(get: { prefs?.settle_on_merge ?? false }, set: { prefs?.settle_on_merge = $0; save() }))
                Stepper(value: Binding(get: { prefs?.settle_after_days ?? 0 }, set: { prefs?.settle_after_days = $0; save() }), in: 0...30) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Settle quiet threads after").font(.system(size: 15))
                        Text((prefs?.settle_after_days ?? 0) == 0 ? "never" : "\(prefs?.settle_after_days ?? 0) days").font(.system(size: 12)).foregroundStyle(Hub.muted)
                    }
                }
                .padding(.horizontal, 13).padding(.vertical, 6)
            }
            if let failed { SNote(text: failed) }
        }
        .task {
            await store.loadMachines()
            guard !store.isDemo, let c = store.homeClient else { prefs = ThreadPrefs(settle_after_days: 3, settle_on_merge: true); return }
            do { prefs = try await c.threadPrefs() } catch let e as HubClient.Failure where e.tooOld { failed = "Needs a newer hubd." } catch { failed = error.localizedDescription }
        }
    }

    private func save() {
        guard let p = prefs, !store.isDemo, let c = store.homeClient else { return }
        Task { do { try await c.setThreadPrefs(p) } catch { failed = "Not saved: \(error.localizedDescription)" } }
    }
}

// MARK: Chat detail

/// How much of the agent's work threads show, for every thread without a level of its own (a thread's
/// own is in its menu). Chat starts at Minimal until one is chosen here.
struct ChatDetailPage: View {
    @Environment(Store.self) private var store

    var body: some View {
        let on = store.has("phone.chatdetail")
        let chosen = ChatDetail(rawValue: store.chatDetailPrefs?.default ?? "")
        VStack(alignment: .leading, spacing: 0) {
            GroupLabel(text: "Default")
            SBox {
                ForEach(ChatDetail.allCases) { level in
                    Button { Haptics.tap(); store.setDefaultDetail(level) } label: {
                        HStack(spacing: 10) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(level.name).font(.system(size: 15))
                                Text(level.blurb).font(.system(size: 12)).foregroundStyle(Hub.muted).multilineTextAlignment(.leading)
                            }
                            Spacer(minLength: 6)
                            if (chosen ?? .quiet) == level { Image(systemName: "checkmark").foregroundStyle(Hub.accent) }
                        }
                        .padding(.horizontal, 13).padding(.vertical, 8).frame(minHeight: 46).contentShape(Rectangle())
                        .overlay(alignment: .bottom) { if level != .all { Hub.line.frame(height: 1).padding(.leading, 13) } }
                    }
                    .buttonStyle(.plain)
                    .disabled(!on)
                }
            }
            SNote(text: on ? "Questions for you and failures always show. A thread can have its own level in its menu." : "Needs a newer hubd.")
        }
        .task { await store.loadChatDetail() }
    }
}

// MARK: Updates (sketch 4, screen 6)

struct UpdatesPage: View {
    @Environment(Store.self) private var store
    @Environment(\.scenePhase) private var scenePhase
    @State private var updating: Set<String> = []
    @State private var fleet: UpdateFleet?
    @State private var failed: String?
    @State private var reading = false
    @AppStorage(ReleaseNotes.enabledKey) private var notesOn = true
    private let builds = ReleaseNotes.load()

    /// This app's own line: the build, when it was made and from which commit.
    private var appLine: String {
        let me = builds.first { $0.build == ReleaseNotes.current } ?? ReleaseBuild(build: ReleaseNotes.current)
        var parts = ["built \(me.when)"]
        if let c = me.commit { parts.append(String(c.prefix(7))) }
        parts.append("signed for 7 days, the Mac renews it over Wi-Fi")
        return parts.joined(separator: " · ")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            GroupLabel(text: "This app")
            SBox {
                SToggle(title: "Tell me what’s new", sub: "a half sheet after each update", on: $notesOn)
                HStack(spacing: 12) {
                    Image(systemName: "iphone").foregroundStyle(Hub.muted).frame(width: 20)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Build \(ReleaseNotes.current)").font(.system(size: 15))
                        Text(appLine).font(.system(size: 12)).foregroundStyle(Hub.muted).fixedSize(horizontal: false, vertical: true)
                    }
                    Spacer()
                }
                .padding(.horizontal, 13).padding(.vertical, 10).frame(minHeight: 48)
            }
            GroupLabel(text: "Patch notes")
            if builds.isEmpty {
                SNote(text: "Nothing recorded yet. Builds installed with the deploy script list what changed in each, newest first.")
            }
            ForEach(builds) { b in BuildCard(build: b, running: b.build == ReleaseNotes.current, open: b.build == ReleaseNotes.current) }
            GroupLabel(text: "Running now")
            if let failed { SNote(text: failed) }
            if fleet == nil && failed == nil { SNote(text: "Reading updates…") }
            SBox {
                ForEach(fleet?.machines ?? []) { h in
                    VStack(alignment: .leading, spacing: 5) {
                        HStack(spacing: 12) {
                            Image(systemName: "server.rack").foregroundStyle(Hub.muted).frame(width: 20)
                            Text(h.machine).font(.system(size: 15)).frame(maxWidth: .infinity, alignment: .leading)
                            if h.canUpdate(fleet?.hubd) {
                                Button(updating.contains(h.machine) ? "Updating…" : "Update") { update(h.machine) }
                                    .buttonStyle(PillButtonStyle(kind: .primary)).disabled(updating.contains(h.machine))
                            }
                            Text(h.version.map { String($0.prefix(7)) } ?? "unknown")
                                .font(.system(size: 13))
                                .foregroundStyle(h.online && (h.behind(fleet?.hubd) || h.hubd_state?.state == "ready") ? Hub.amber : Hub.muted)
                        }
                        Text(h.label(fleet?.hubd)).font(.system(size: 12)).foregroundStyle(Hub.muted)
                        if let detail = h.hubd_state?.step ?? h.hubd_state?.message ?? h.hubd_note {
                            Text(detail).font(.system(size: 12)).foregroundStyle(Hub.muted)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    .padding(.horizontal, 13).padding(.vertical, 10)
                    .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 45) }
                }
            }
            SNote(text: "Update sends the paired Hub’s version, just as on that Hub’s desktop. Ready versions are installed from desktop Updates; threads carry on.")
            Button("Check now") { Task { await refresh() } }.padding(.horizontal, 18).padding(.vertical, 10)
        }
        .task(id: scenePhase) {
            guard scenePhase == .active else { return }
            while !Task.isCancelled {
                await refresh()
                do { try await Task.sleep(for: .seconds(5)) } catch { return }
            }
        }
    }

    @MainActor private func refresh() async {
        guard !reading else { return }
        reading = true
        defer { reading = false }
        guard store.has("phone.hosts"), let client = store.homeClient else {
            failed = "The paired Hub needs a newer version to list updates."
            return
        }
        do {
            let next = try await client.updates()
            guard !Task.isCancelled else { return }
            fleet = next
            failed = nil
        } catch is CancellationError {} catch {
            failed = "Could not refresh updates; shown statuses may be old. \(error.localizedDescription)"
        }
    }

    private func update(_ name: String) {
        updating.insert(name)
        Task {
            do {
                guard let client = store.homeClient else { updating.remove(name); return }
                try await client.updateHubd(machine: name)
                store.flash("Updating \(name)")
            }
            catch { store.flash("Not started: \(error.localizedDescription)") }
            try? await Task.sleep(for: .seconds(5))
            await refresh()
            updating.remove(name)
        }
    }
}
