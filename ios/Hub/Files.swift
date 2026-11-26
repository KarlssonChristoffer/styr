import SwiftUI

// Sheet "Filer": a project's files as a mode of the project, read-only. A folder opens in place
// with its path as a row you can tap to jump up; a file opens on its own page; the search finds
// files by name. Works with `GET /api/phone/files/tree`, `/file`, `/raw` and `/search` (feature
// `phone.files`); on a hubd without it the tab says so.

struct TreeEntry: Decodable, Hashable, Identifiable {
    var name: String
    var rel: String
    var dir: Bool
    var id: String { rel }
}

struct FileText: Decodable {
    var rel: String
    var text: String
    var truncated: Bool
}

extension HubClient {
    func tree(path: String, rel: String, host: String?) async throws -> [TreeEntry] {
        var q = ["path": path, "rel": rel]
        if let host { q["host"] = host }
        return try JSONDecoder().decode([TreeEntry].self, from: try await call("GET", "api/phone/files/tree", query: q))
    }

    func file(path: String, rel: String, host: String?) async throws -> FileText {
        var q = ["path": path, "rel": rel]
        if let host { q["host"] = host }
        return try JSONDecoder().decode(FileText.self, from: try await call("GET", "api/phone/files/file", query: q))
    }

    func rawFile(path: String, rel: String, host: String?) async throws -> Data {
        var q = ["path": path, "rel": rel]
        if let host { q["host"] = host }
        return try await call("GET", "api/phone/files/raw", query: q)
    }

    /// Files whose name matches, shortest path first (the words go in `rel`).
    func findFiles(path: String, q: String, host: String?) async throws -> [TreeEntry] {
        var query = ["path": path, "rel": q]
        if let host { query["host"] = host }
        return try JSONDecoder().decode([TreeEntry].self, from: try await call("GET", "api/phone/files/search", query: query))
    }
}

enum FileKind {
    static let images: Set<String> = ["png", "jpg", "jpeg", "gif", "webp", "heic"]
    static func ext(_ rel: String) -> String { (rel as NSString).pathExtension.lowercased() }
    static func isImage(_ rel: String) -> Bool { images.contains(ext(rel)) }
    static func isMarkdown(_ rel: String) -> Bool { ["md", "markdown"].contains(ext(rel)) }
}

/// The project's Files mode.
struct FilesTab: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    let project: ProjectRef
    @State private var dir = ""
    @State private var entries: [TreeEntry]?
    @State private var failed: String?
    @State private var query = ""
    @State private var hits: [String] = []
    /// The folders already listed, so the one above is there to slide in when a swipe takes you up.
    @State private var listings: [String: [TreeEntry]] = [:]
    @State private var pull: CGFloat = 0
    @State private var pulling: Bool?
    @State private var armed = false

    private var available: Bool { store.isDemo || store.has("phone.files", on: store.url(of: project.host)) }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if !available {
                Text("\(project.host ?? store.machine ?? "Hub") needs the next version of hubd to show files on the phone.")
                    .font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18)
            } else {
                search
                if !query.trimmingCharacters(in: .whitespaces).isEmpty { results } else { browser }
            }
        }
        .task(id: dir) { await load() }
        .onChange(of: dir, initial: true) { PageSwipe.rightGoesUp = !dir.isEmpty }
        .onDisappear { PageSwipe.rightGoesUp = false }
    }

    /// The folder above `dir`.
    private var parent: String { (dir as NSString).deletingLastPathComponent }

    // MARK: Browsing

    @ViewBuilder private var browser: some View {
        crumbs
        // A swipe to the right takes you up one folder: the folder above waits behind and slides in as
        // this one goes, and only at the top of the project does the swipe leave Files (the pager's).
        ZStack(alignment: .topLeading) {
            if pull > 0, let above = listings[parent], !dir.isEmpty {
                VStack(alignment: .leading, spacing: 0) { ForEach(above) { e in staticRow(e) } }
                    .offset(x: -80 + min(pull, 400) * 0.2).opacity(0.5 + Double(min(pull, 200) / 400))
            }
            VStack(alignment: .leading, spacing: 0) { list }
                .background(Hub.bg)
                .offset(x: pull)
                .allowsHitTesting(pulling != true)
        }
        .simultaneousGesture(dir.isEmpty ? nil : up)
    }

    @ViewBuilder private var list: some View {
        if let failed { Text(failed).font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18) }
        else if entries == nil { ProgressView().frame(maxWidth: .infinity).padding(30) }
        else if entries?.isEmpty == true { Text("Nothing here.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18) }
        ForEach(entries ?? []) { e in
            if e.dir {
                Button { guard TouchGuard.ok else { return }; withAnimation(.snappy) { dir = e.rel } } label: { row(e.name, icon: "folder", tint: Hub.accent, chevron: true) }.buttonStyle(.pressFlat)
            } else {
                NavigationLink(value: Route.file(project, e.rel)) { row(e.name, icon: FileKind.isMarkdown(e.rel) ? "doc.text" : FileKind.isImage(e.rel) ? "photo" : "doc", tint: Hub.muted, chevron: false) }
                    .buttonStyle(.pressFlat)
                    .contextMenu {
                        Button { UIPasteboard.general.string = e.rel; Haptics.tap() } label: { Label("Copy path", systemImage: "doc.on.doc") }
                        Button { mention(e.rel) } label: { Label("Mention in a new thread", systemImage: "at") }
                    }
            }
        }
    }

    private func staticRow(_ e: TreeEntry) -> some View {
        row(e.name, icon: e.dir ? "folder" : "doc", tint: e.dir ? Hub.accent : Hub.muted, chevron: e.dir)
    }

    /// Up one folder: the page follows the finger, past a line (or a flick) the folder above takes over.
    private var up: some Gesture {
        DragGesture(minimumDistance: 20, coordinateSpace: .global)
            .onChanged { v in
                if pulling == nil {
                    pulling = v.translation.width > 0 && v.translation.width > abs(v.translation.height) * 1.5
                        && v.startLocation.x > 24 && !SwipeZones.holds(v.startLocation)
                    if pulling == true { Haptics.ready() }
                }
                guard pulling == true else { return }
                pull = max(0, v.translation.width) * 0.9
                let reached = pull > 90
                if reached != armed { armed = reached; Haptics.tick() }
            }
            .onEnded { v in
                let mine = pulling == true
                pulling = nil
                guard mine else { return }
                TouchGuard.swiped()
                let go = pull > 90 || v.predictedEndTranslation.width > 220
                armed = false
                if go {
                    // The folder above, already listed, is in place before this one is gone.
                    pull = 0
                    withAnimation(.snappy) { dir = parent }
                } else {
                    withAnimation(.snappy) { pull = 0 }
                }
            }
    }

    /// "hub › crates › src": every part jumps up to that folder.
    private var crumbs: some View {
        let parts = dir.split(separator: "/").map(String.init)
        return ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                Button(project.name) { withAnimation(.snappy) { dir = "" } }.foregroundStyle(parts.isEmpty ? Hub.text : Hub.muted)
                ForEach(Array(parts.enumerated()), id: \.offset) { i, p in
                    Image(systemName: "chevron.right").font(.system(size: 9)).foregroundStyle(Hub.lineStrong)
                    Button(p) { withAnimation(.snappy) { dir = parts.prefix(i + 1).joined(separator: "/") } }
                        .foregroundStyle(i == parts.count - 1 ? Hub.text : Hub.muted).fontWeight(i == parts.count - 1 ? .semibold : .regular)
                }
            }
            .font(.system(size: 13.5)).padding(.horizontal, 18).padding(.vertical, 8)
        }
        .swipeZone()
    }

    private func row(_ name: String, icon: String, tint: Color, chevron: Bool) -> some View {
        HStack(spacing: 12) {
            Image(systemName: icon).foregroundStyle(tint).frame(width: 22)
            Text(name).font(.system(size: 15)).lineLimit(1)
            Spacer()
            if chevron { Image(systemName: "chevron.right").font(.system(size: 11)).foregroundStyle(Hub.lineStrong) }
        }
        .padding(.horizontal, 18).frame(height: 48).contentShape(Rectangle())
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
    }

    // MARK: Find a file (names only)

    private var search: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").foregroundStyle(Hub.muted)
            TextField("", text: $query, prompt: Text("Find a file").foregroundStyle(Hub.muted)).autocorrectionDisabled().textInputAutocapitalization(.never)
            if !query.isEmpty { Button { query = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(Hub.muted) }.buttonStyle(.plain) }
        }
        .font(.system(size: 15)).padding(.horizontal, 14).frame(height: 42)
        .background(Capsule().fill(Hub.surface)).overlay(Capsule().strokeBorder(Hub.lineStrong))
        .padding(.horizontal, 14).padding(.top, 4)
        .onChange(of: query) { Task { await find() } }
    }

    private var results: some View {
        VStack(alignment: .leading, spacing: 0) {
            if hits.isEmpty { Text("No file with that name.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18) }
            ForEach(hits, id: \.self) { rel in
                NavigationLink(value: Route.file(project, rel)) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text((rel as NSString).lastPathComponent).font(.system(size: 15))
                        Text(Self.shortened((rel as NSString).deletingLastPathComponent)).font(.system(size: 11.5, design: .monospaced)).foregroundStyle(Hub.muted).lineLimit(1)
                    }
                    .padding(.horizontal, 18).padding(.vertical, 9).frame(maxWidth: .infinity, alignment: .leading)
                    .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
                }
                .buttonStyle(.pressFlat)
            }
        }
    }

    /// The middle of a long path shortened: "crates/…/src/phone".
    static func shortened(_ path: String) -> String {
        let parts = path.split(separator: "/")
        return parts.count > 4 ? ([parts[0]] + ["…"] + parts.suffix(2)).joined(separator: "/") : path
    }

    // MARK: Loading

    private func load() async {
        // A folder already listed shows at once and is read again behind it.
        entries = listings[dir]; failed = nil
        if store.isDemo { entries = dir.isEmpty ? [TreeEntry(name: "crates", rel: "crates", dir: true), TreeEntry(name: "CLAUDE.md", rel: "CLAUDE.md", dir: false)] : []; return }
        guard available, let t = store.target(project.host) else { return }
        do { entries = try await t.client.tree(path: project.path, rel: dir, host: t.host); listings[dir] = entries }
        catch let e as HubClient.Failure where e.tooOld { failed = "This machine needs the next version of hubd to show files." }
        catch { failed = error.localizedDescription }
    }

    private func find() async {
        let q = query.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty, !store.isDemo, let t = store.target(project.host) else { hits = []; return }
        try? await Task.sleep(for: .milliseconds(250))
        guard q == query.trimmingCharacters(in: .whitespaces) else { return }
        // Names only; the machine answers up to 40, shortest path first.
        let found = (try? await t.client.findFiles(path: project.path, q: q, host: t.host)) ?? []
        hits = found.filter { !$0.dir }.map(\.rel)
    }

    private func mention(_ rel: String) {
        Drafts.set("new", "@\(rel) ")
        nav.push(.new(project, talk: false))
    }
}

/// A file, read-only (sheet "Filer", screens 3 to 5): code with line numbers, markdown rendered
/// with Source one tap away, a picture shown large. Ask about this starts a thread with the file
/// named; nothing here edits a file, edits are the agent's job.
struct FileView: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @Environment(\.dismiss) private var dismiss
    let project: ProjectRef
    let rel: String
    @State private var file: FileText?
    @State private var image: UIImage?
    @State private var failed: String?
    @State private var source = false

    var body: some View {
        VStack(spacing: 0) {
            HubHeader(parent: "Files", title: (rel as NSString).lastPathComponent,
                      sub: Self.folder(rel), back: { dismiss() })
            if FileKind.isMarkdown(rel) && file != nil {
                ModeSwitch(selection: Binding(get: { source ? 1 : 0 }, set: { source = $0 == 1 }), items: ["Read", "Source"])
            }
            ModePager(selection: Binding(get: { source ? 1 : 0 }, set: { source = $0 == 1 }),
                      count: FileKind.isMarkdown(rel) && file != nil ? 2 : 1) { i in
                ScrollView([.vertical]) {
                    content(source: i == 1).padding(.horizontal, 12).padding(.bottom, 20)
                }
            }
            HStack(spacing: 10) {
                Button { ask() } label: { Label("Ask about this", systemImage: "bubble.left").frame(maxWidth: .infinity) }
                    .buttonStyle(PillButtonStyle(kind: .primary))
                Button { UIPasteboard.general.string = file?.text ?? rel; Haptics.tap(); store.flash("Copied") } label: {
                    Label("Copy", systemImage: "doc.on.doc").frame(maxWidth: .infinity)
                }
                .buttonStyle(PillButtonStyle())
            }
            .padding(.horizontal, 14).padding(.top, 6).padding(.bottom, 10)
        }
        .background(Hub.bg)
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack(enabled: !source) { dismiss() }
        .task { await load() }
    }

    static func folder(_ rel: String) -> String {
        let d = (rel as NSString).deletingLastPathComponent
        return d.isEmpty ? "project root" : d
    }

    @ViewBuilder private func content(source: Bool) -> some View {
        if let failed { Text(failed).font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18) }
        else if FileKind.isImage(rel) {
            if let image { Image(uiImage: image).resizable().scaledToFit().clipShape(RoundedRectangle(cornerRadius: 12)) }
            else { ProgressView().frame(maxWidth: .infinity).padding(30) }
        } else if let file {
            if FileKind.isMarkdown(rel) && !source {
                Prose(text: file.text).padding(.horizontal, 6).padding(.top, 4)
            } else {
                code(file)
            }
            if file.truncated { Text("Only the start of the file is shown.").font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.top, 8) }
        } else { ProgressView().frame(maxWidth: .infinity).padding(30) }
    }

    private func code(_ f: FileText) -> some View {
        let lines = f.text.split(separator: "\n", omittingEmptySubsequences: false).prefix(3000)
        return ScrollView(.horizontal, showsIndicators: false) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(lines.enumerated()), id: \.offset) { i, line in
                    HStack(alignment: .top, spacing: 0) {
                        Text("\(i + 1)").font(.system(size: 11, design: .monospaced)).foregroundStyle(Hub.muted)
                            .frame(width: 40, alignment: .trailing).padding(.trailing, 10)
                        Text(line.isEmpty ? " " : String(line)).font(.system(size: 12, design: .monospaced)).foregroundStyle(Hub.text2)
                            .fixedSize(horizontal: true, vertical: false)
                    }
                    .frame(minHeight: 18)
                }
            }
            .padding(.vertical, 8)
        }
        .swipeZone()
        .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
    }

    private func load() async {
        if store.isDemo { file = FileText(rel: rel, text: "// demo file\nfn main() {}\n", truncated: false); return }
        guard let t = store.target(project.host) else { return }
        do {
            if FileKind.isImage(rel) { image = UIImage(data: try await t.client.rawFile(path: project.path, rel: rel, host: t.host)); if image == nil { failed = "Could not read the picture." } }
            else { file = try await t.client.file(path: project.path, rel: rel, host: t.host) }
        } catch { failed = error.localizedDescription }
    }

    private func ask() {
        Drafts.set("new", "About `\(rel)`: ")
        nav.push(.new(project, talk: false))
    }
}
