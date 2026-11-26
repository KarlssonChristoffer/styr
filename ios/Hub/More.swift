import SwiftUI

extension Store {
    /// When the nearest plan limit resets, for "snooze until the limits reset".
    var limitsReset: Double? {
        let all = (usage?.plan?.limits ?? []) + (usage?.others.flatMap { $0.plan?.limits ?? [] } ?? [])
        return all.compactMap(\.resets_at).filter { $0 > Date().timeIntervalSince1970 * 1000 }.min()
    }

    /// Snooze times as the sketch's swipe shows them: an hour, tomorrow 08:00, the limits' reset.
    var snoozeChoices: [(label: String, sub: String, until: Double)] {
        var list = Store.snoozeTimes().map { t in (t.label, t.label == "1 h" ? Store.clock(t.until) : "tomorrow", t.until) }
        if let r = limitsReset { list.append(("Reset", Store.clock(r), r)) }
        return list
    }

    func rename(_ t: HubThread, to title: String) async {
        await patch(t, ["title": title], done: "Renamed")
    }

    func setEngine(_ t: HubThread, _ engine: String) async {
        await patch(t, ["engine": engine], done: "Carries on with \(engine.capitalized)")
    }

    private func patch(_ t: HubThread, _ body: [String: Any], done: String) async {
        if isDemo { flash(done); return }
        guard needs("phone.move", on: t.host) else { return }
        do {
            try await client(for: t.id)?.patch(thread: t.id, body)
            Haptics.tap()
            flash(done)
        } catch let e as HubClient.Failure where e.tooOld {
            flash("\(t.host ?? machine ?? "Hub") needs a newer hubd for this.")
        } catch {
            flash("Not saved: \(error.localizedDescription)")
        }
    }

    func move(_ t: HubThread, to machine: String) async {
        guard let client = homeClient, needs("phone.move") else { return }
        do {
            try await client.move(thread: t.id, to: machine, stop: t.status == .running)
            flash("Moving to \(machine)")
        } catch { flash("Not moved: \(error.localizedDescription)") }
    }

    /// The pull request a thread watches, asked of the machine the thread runs on.
    func pullNumber(for t: HubThread) async -> Int? {
        if isDemo { return t.id == "t7" ? 273 : nil }
        guard has("phone.github", on: url(of: t.host)) else { return nil }
        guard let target = target(t.host) else { return nil }
        return (try? await target.client.watches(host: target.host))?[t.id]
    }
}

// MARK: Pull request (sketch 2, screen 7)

struct PullView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    let thread: HubThread
    @State private var number: Int?
    @State private var detail: PullDetail?
    @State private var note: String?
    @State private var merging = false
    @State private var confirm = false
    @State private var mode = 0
    @State private var diffText: String?
    @State private var openFiles: Set<String> = []

    var body: some View {
        VStack(spacing: 0) {
            HubHeader(parent: "Thread", title: number.map { "Pull request #\($0)" } ?? "Pull request", sub: status,
                      back: { dismiss() }) {
                if let d = detail {
                    HubMenu {
                        if let u = URL(string: d.pull.url) { Button { openURL(u) } label: { Label("Open on GitHub", systemImage: "arrow.up.right") } }
                        if d.pull.state == "open" {
                            Button { act("request_changes", body: "Please have a look at the thread in Hub.") } label: { Label("Ask for changes", systemImage: "arrow.uturn.backward") }
                        }
                    } label: { HeaderFace(icon: "ellipsis") }
                    .accessibilityLabel("More")
                }
            }
            if detail != nil { ModeSwitch(selection: $mode, items: ["Overview", "Files", "Discussion"]) }
            ModePager(selection: $mode, count: detail == nil ? 1 : 3) { i in
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        if let d = detail {
                            switch i {
                            case 0: overview(d)
                            case 1: files(d)
                            default: discussion(d)
                            }
                        }
                        else { Text(note ?? "Looking for its pull request…").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18) }
                    }
                    .padding(.bottom, 20)
                }
            }
            if let d = detail, d.pull.state == "open" {
                VStack(spacing: 6) {
                    HStack(spacing: 10) {
                        Button { act("approve") } label: { Text("Approve").font(.system(size: 16, weight: .medium)).frame(maxWidth: .infinity).frame(height: 52) }
                            .buttonStyle(.plain).foregroundStyle(Hub.text2)
                            .background(Capsule().fill(Hub.surface)).overlay(Capsule().strokeBorder(Hub.lineStrong))
                        Button { confirm = true } label: {
                            Group { if merging { Working(tint: Hub.bg, size: 18) } else { Text("Merge") } }
                                .font(.system(size: 16, weight: .semibold)).foregroundStyle(Hub.bg)
                                .frame(maxWidth: .infinity).frame(height: 52)
                                .background(Capsule().fill(canMerge(d) ? Hub.accent : Hub.lineStrong))
                        }
                        .disabled(merging)
                    }
                    Text(canMerge(d) ? "Merge is for pull requests you take yourself; Hub's own are merged by the thread that opened them." : "GitHub says: \(d.merge_state). Merge anyway only if you know why.")
                        .font(.system(size: 12)).foregroundStyle(Hub.muted).multilineTextAlignment(.center)
                }
                .padding(.horizontal, 14).padding(.bottom, 10)
                .hubConfirm("Merge #\(d.pull.number)?", message: "Merge with \(d.methods.first ?? "squash").", confirm: "Merge", isPresented: $confirm) { merge(d) }
            }
        }
        .background(Hub.bg)
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack(enabled: mode == 0) { dismiss() }
        .task { await load() }
    }

    private var path: String { Store.projectRoot(thread.cwd) }

    private func canMerge(_ d: PullDetail) -> Bool { ["clean", "unstable", "has_hooks"].contains(d.merge_state) }

    /// "Open · checks green", the status line under the title.
    private var status: String? {
        guard let d = detail else { return nil }
        let state = d.pull.draft ? "Draft" : d.pull.state.capitalized
        let bad = d.checks.contains { $0.state == "fail" }
        let waiting = d.checks.contains { $0.state == "pending" }
        let checks = d.checks.isEmpty ? nil : bad ? "checks failing" : waiting ? "checks running" : "checks green"
        return [state, checks].compactMap { $0 }.joined(separator: " · ")
    }

    private func load() async {
        guard let n = await store.pullNumber(for: thread) else { note = "This thread has no pull request yet."; return }
        number = n
        if store.isDemo { note = "Pull request #\(n) (demo)"; return }
        guard let t = store.target(thread.host) else { return }
        do { detail = try await t.client.pull(path: path, number: n, host: t.host) }
        catch let e as HubClient.Failure where e.tooOld { note = "\(thread.host ?? store.machine ?? "Hub") needs a newer hubd to show pull requests." }
        catch { note = error.localizedDescription }
    }

    private func merge(_ d: PullDetail) {
        guard let t = store.target(thread.host) else { return }
        merging = true
        Task {
            do {
                try await t.client.pullAction(path: path, number: d.pull.number, action: "merge", method: d.methods.first, host: t.host)
                Haptics.tap()
                store.flash("Merged #\(d.pull.number)")
                await load()
            } catch { store.flash("Not merged: \(error.localizedDescription)") }
            merging = false
        }
    }

    // MARK: The three modes

    @ViewBuilder private func overview(_ d: PullDetail) -> some View {
        HStack(spacing: 6) {
            Image(systemName: "arrow.triangle.pull")
            Text("\(d.pull.head) → \(d.pull.base)").lineLimit(1)
        }
        .font(.system(size: 12.5)).foregroundStyle(Hub.muted).padding(.horizontal, 18)
        Text(d.pull.title).font(.system(size: 22, weight: .bold)).padding(.horizontal, 18).padding(.top, 4)
        HStack(spacing: 8) {
            let open = d.pull.state == "open"
            Text(d.pull.draft ? "Draft" : d.pull.state.capitalized).font(.system(size: 11.5, weight: .semibold))
                .foregroundStyle(open ? Hub.green : Hub.accent).padding(.horizontal, 8).padding(.vertical, 2)
                .background(Capsule().fill(open ? Hub.greenBg : Hub.accentBg))
            Text("+\(d.pull.additions)").foregroundStyle(Hub.green)
            Text("−\(d.pull.deletions)").foregroundStyle(Hub.red)
        }
        .font(.system(size: 12.5)).padding(.horizontal, 18).padding(.top, 6)
        GroupLabel(text: "Checks")
        VStack(spacing: 0) {
            if d.checks.isEmpty { line("No checks", dot: Hub.muted) }
            ForEach(d.checks, id: \.self) { c in
                line(c.name, dot: c.state == "pass" ? Hub.green : c.state == "fail" ? Hub.red : c.state == "pending" ? Hub.amber : Hub.muted)
            }
        }
        .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface)).padding(.horizontal, 14)
        if !d.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            GroupLabel(text: "What it does")
            Prose(text: d.body).padding(.horizontal, 18)
        }
        if let commits = d.commits, !commits.isEmpty {
            GroupLabel(text: "Commits · \(commits.count)")
            VStack(spacing: 0) {
                ForEach(commits, id: \.self) { c in
                    HStack(spacing: 10) {
                        Text(String(c.oid.prefix(7))).font(.system(size: 12, design: .monospaced)).foregroundStyle(Hub.muted)
                        Text(c.headline).font(.system(size: 14)).lineLimit(1)
                        Spacer()
                    }
                    .padding(.horizontal, 13).frame(minHeight: 40)
                    .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 13) }
                }
            }
            .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface)).padding(.horizontal, 14)
        }
    }

    @ViewBuilder private func files(_ d: PullDetail) -> some View {
        let list = d.files ?? []
        let canDiff = !store.isDemo && store.has("phone.git", on: store.url(of: thread.host))
        if list.isEmpty {
            Text("No file list from this machine yet.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18)
        }
        VStack(spacing: 8) {
            ForEach(list, id: \.self) { f in
                let name = f.path ?? f.filename ?? "?"
                VStack(alignment: .leading, spacing: 0) {
                    Button {
                        guard canDiff else { return }
                        withAnimation(.snappy) { if openFiles.contains(name) { openFiles.remove(name) } else { openFiles.insert(name) } }
                        if diffText == nil { Task { await loadDiff(d) } }
                    } label: {
                        HStack(spacing: 10) {
                            Image(systemName: "doc").foregroundStyle(Hub.muted)
                            Text(name).font(.system(size: 14)).lineLimit(1).truncationMode(.head)
                            Spacer()
                            Text("+\(f.additions ?? 0)").foregroundStyle(Hub.green)
                            Text("−\(f.deletions ?? 0)").foregroundStyle(Hub.red)
                            if canDiff { Image(systemName: openFiles.contains(name) ? "chevron.up" : "chevron.right").font(.system(size: 11)).foregroundStyle(Hub.muted) }
                        }
                        .font(.system(size: 13, weight: .medium))
                        .padding(.horizontal, 13).frame(minHeight: 44).contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    if openFiles.contains(name) {
                        if let section = Self.section(of: name, in: diffText) { DiffView(diff: section) }
                        else if diffText == nil { ProgressView().frame(maxWidth: .infinity).padding(14) }
                        else { Text("No diff for this file.").font(.system(size: 12.5)).foregroundStyle(Hub.muted).padding(13) }
                    }
                }
                .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
                .clipShape(RoundedRectangle(cornerRadius: 12))
            }
            if !canDiff && !list.isEmpty && !store.isDemo {
                Text("Reading the diff here needs the next version of hubd.").font(.system(size: 11.5)).foregroundStyle(Hub.muted)
            }
        }
        .padding(.horizontal, 14)
    }

    private func loadDiff(_ d: PullDetail) async {
        guard let t = store.target(thread.host) else { return }
        diffText = (try? await t.client.githubDiff(path: path, number: d.pull.number, host: t.host)) ?? ""
    }

    /// One file's part of a whole pull request diff ("diff --git a/x b/x" starts each).
    private static func section(of name: String, in text: String?) -> String? {
        guard let text, !text.isEmpty else { return nil }
        for part in text.components(separatedBy: "diff --git ").dropFirst() {
            let head = part.prefix { $0 != "\n" }
            if head.hasSuffix(" b/\(name)") { return "diff --git " + part }
        }
        return nil
    }

    @ViewBuilder private func discussion(_ d: PullDetail) -> some View {
        let list = d.comments ?? []
        VStack(spacing: 8) {
            if !d.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !d.checks.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    let bad = d.checks.filter { $0.state == "fail" }.count
                    let ok = d.checks.filter { $0.state == "pass" }.count
                    HStack(spacing: 6) {
                        Circle().fill(bad > 0 ? Hub.red : Hub.green).frame(width: 8, height: 8)
                        Text(d.checks.isEmpty ? "No checks" : "Checks \(bad > 0 ? "failing" : "passed") · \(ok) of \(d.checks.count)").font(.system(size: 12.5)).foregroundStyle(Hub.muted)
                    }
                    Text(d.pull.title).font(.system(size: 15, weight: .semibold))
                    Text("\(d.pull.head) into \(d.pull.base) · +\(d.pull.additions) −\(d.pull.deletions)").font(.system(size: 13)).foregroundStyle(Hub.text3)
                }
                .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
            }
            if list.isEmpty { Text("No comments yet.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.top, 10) }
            ForEach(Array(list.enumerated()), id: \.offset) { _, c in
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 6) {
                        Text(c.author).fontWeight(.medium)
                        if let at = c.at { Text("· \(relative(at < 4_000_000_000 ? at * 1000 : at))") }
                        if let v = c.verdict { Text("· \(v.replacingOccurrences(of: "_", with: " "))") }
                    }
                    .font(.system(size: 12)).foregroundStyle(Hub.muted)
                    if let p = c.path { Text("\(p)\(c.line.map { " · line \($0)" } ?? "")").font(.system(size: 11.5, design: .monospaced)).foregroundStyle(Hub.text3) }
                    if !c.body.isEmpty { Text(c.body).font(.system(size: 14.5)) }
                }
                .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
            }
        }
        .padding(.horizontal, 14)
    }

    private func act(_ action: String, body: String? = nil) {
        guard let d = detail, let t = store.target(thread.host) else { return }
        Task {
            do { try await t.client.pullAction(path: path, number: d.pull.number, action: action, body: body, host: t.host); store.flash("Done") }
            catch { store.flash("Not done: \(error.localizedDescription)") }
        }
    }

    private func line(_ text: String, dot: Color) -> some View {
        HStack(spacing: 11) {
            Circle().fill(dot).frame(width: 8, height: 8)
            Text(text).font(.system(size: 14.5)).lineLimit(1)
            Spacer()
        }
        .padding(.horizontal, 13).frame(minHeight: 44)
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 32) }
    }
}

struct PullRoute: View {
    @Environment(Store.self) private var store
    let id: String
    var body: some View {
        if let t = store.thread(id) { PullView(thread: t) } else { ThreadGone(parent: "Thread", title: "Pull request") }
    }
}

// MARK: Adding a project (sketch 5, the + on Projects)

struct AddProjectSheet: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var path = ""
    @State private var machine: String?
    @State private var create = false
    @State private var busy = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Button("Cancel") { dismiss() }.foregroundStyle(Hub.accent)
                Spacer()
                Text("Add a project").font(.system(size: 15, weight: .semibold))
                Spacer()
                Button("Add") { add() }.foregroundStyle(Hub.accent).fontWeight(.semibold).disabled(path.isEmpty || busy)
            }
            Text("Folder on the machine").font(.system(size: 12, weight: .semibold)).foregroundStyle(Hub.muted)
            TextField("", text: $path, prompt: Text("~/dev/my-app").foregroundStyle(Hub.muted))
                .textInputAutocapitalization(.never).autocorrectionDisabled()
                .padding(.horizontal, 12).frame(height: 44).background(RoundedRectangle(cornerRadius: 10).fill(Hub.surface2))
            Text("Machine").font(.system(size: 12, weight: .semibold)).foregroundStyle(Hub.muted)
            FlowLayout(spacing: 7) {
                ForEach([store.machine ?? "This machine"] + store.hosts.filter { !$0.local && $0.state == "online" }.map(\.name), id: \.self) { m in
                    Button(m) { machine = m == store.machine ? nil : m }
                        .buttonStyle(PillButtonStyle(kind: (machine ?? store.machine) == m ? .primary : .plain))
                }
            }
            Toggle("Make the folder, git init and start the plan", isOn: $create).tint(Hub.green).font(.system(size: 14.5))
            Spacer()
        }
        .padding(18)
        .background(Hub.side)
    }

    private func add() {
        guard let t = store.target(machine) else { return }
        busy = true
        Task {
            do {
                try await t.client.addProject(path: path, create: create, host: t.host)
                store.flash("Added \(path)")
                await store.loadProjects()
                dismiss()
            } catch { store.flash("Not added: \(error.localizedDescription)") }
            busy = false
        }
    }
}

extension Store {
    /// The settings an engine offered on any thread the phone has read, newest first: what to show
    /// for an engine before this thread has run on it.
    func knownOptions(_ engine: String) -> [EngineOption] {
        let ids = threads.filter { $0.engine == engine }.sorted { $0.updated_at > $1.updated_at }.map(\.id)
        for id in ids { if let o = folded[id]?.options, !o.isEmpty { return o } }
        return []
    }

    /// Sets one of the engine's own settings on a thread, and shows it at once; answers whether the
    /// machine took it (the caller keeps its own pick in front until the machine says the same).
    @discardableResult
    func setOption(_ t: HubThread, id: String, value: String) async -> Bool {
        if var f = folded[t.id], let i = f.options.firstIndex(where: { $0.id == id }) {
            f.options[i].value = value
            folded[t.id] = f
        }
        if isDemo { return true }
        guard needs("phone.move", on: t.host), let client = client(for: t.id) else { return false }
        let v: Any
        switch value { case "true": v = true; case "false": v = false; default: v = value }
        do { try await client.setOption(thread: t.id, id: id, value: v); return true }
        catch { flash("Not changed: \(error.localizedDescription)"); return false }
    }
}
