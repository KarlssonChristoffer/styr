import SwiftUI

/// A row of modes in one capsule instead of a tab bar (sketch "Git och granskning", screens 3
/// and 6): the chosen one on a lighter pill.
struct ModeSwitch: View {
    @Binding var selection: Int
    let items: [String]
    @Namespace private var pill

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(items.enumerated()), id: \.offset) { i, item in
                Button { withAnimation(.snappy) { selection = i }; Haptics.tick() } label: {
                    Text(item).font(.system(size: 14, weight: selection == i ? .semibold : .regular))
                        .foregroundStyle(selection == i ? Hub.text : Hub.muted)
                        .frame(maxWidth: .infinity).frame(height: 36)
                        .background {
                            if selection == i { Capsule().fill(Hub.lineStrong.opacity(0.8)).matchedGeometryEffect(id: "pill", in: pill) }
                        }
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
            }
        }
        // The pill glides to the mode whether a tap or a swipe on the page chose it.
        .animation(.snappy, value: selection)
        .padding(3)
        .background(Capsule().fill(Hub.surface))
        .padding(.horizontal, 12).padding(.bottom, 8)
    }
}

/// A comment on a line of a file's diff, kept until you send them all together.
struct LineComment: Identifiable, Hashable {
    var id = UUID()
    var path: String
    var line: Int
    var text: String
}

/// Reviewing what a thread changed (sketch "Git och granskning", screens 3 to 5): Files, Diff and
/// Comments as one mode switch. A tap on a line number puts a comment under the line; the
/// comments gather in their own list and go to the thread as one message that names each file
/// and line.
struct ChangesView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    let thread: HubThread
    @State private var changes: [ThreadChange]?
    @State private var failed: String?
    @State private var note = ""
    @State private var asking = false
    @State private var mode = 0
    @State private var selected: String?
    @State private var comments: [LineComment] = []

    var body: some View {
        VStack(spacing: 0) {
            HubHeader(parent: "Thread", title: "Changes", sub: sub, back: { dismiss() })
            ModeSwitch(selection: $mode, items: ["Files", "Diff", comments.isEmpty ? "Comments" : "Comments \(comments.count)"])
            ModePager(selection: $mode, count: 3) { i in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 8) {
                        if let failed { Text(failed).font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18) }
                        else if changes == nil { ProgressView().frame(maxWidth: .infinity).padding(30) }
                        else if changes?.isEmpty == true { Text("Nothing changed yet.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18) }
                        else {
                            switch i {
                            case 0: ForEach(changes ?? []) { c in fileRow(c) }
                            case 1: diff
                            default: commentList
                            }
                        }
                    }
                    .padding(.horizontal, 12).padding(.bottom, 20)
                }
                .scrollDismissesKeyboard(.interactively)
            }
            bottom
        }
        .background(Hub.bg)
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack(enabled: mode == 0) { dismiss() }
        .task { await load() }
    }

    private var sub: String? {
        if mode == 2 { return "\(comments.count) comment\(comments.count == 1 ? "" : "s")" }
        return changes.map { c in "\(c.count) file\(c.count == 1 ? "" : "s") · +\(c.reduce(0) { $0 + $1.added }) −\(c.reduce(0) { $0 + $1.removed })" }
    }

    // MARK: Modes

    private func fileRow(_ c: ThreadChange) -> some View {
        Button { selected = c.id; withAnimation(.snappy) { mode = 1 } } label: {
            HStack(spacing: 10) {
                Image(systemName: "doc").foregroundStyle(Hub.muted)
                Text(c.name).font(.system(size: 14.5)).lineLimit(1)
                Spacer()
                Text("+\(c.added)").foregroundStyle(Hub.green)
                Text("−\(c.removed)").foregroundStyle(Hub.red)
                Image(systemName: "chevron.right").font(.system(size: 11)).foregroundStyle(Hub.muted)
            }
            .font(.system(size: 13, weight: .medium))
            .padding(.horizontal, 12).frame(minHeight: 46)
            .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder private var diff: some View {
        let list = changes ?? []
        let c = list.first { $0.id == selected } ?? list.first
        if let c {
            if list.count > 1 {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(list) { f in
                            Button(f.name) { selected = f.id }
                                .buttonStyle(PillButtonStyle(kind: f.id == c.id ? .primary : .plain))
                        }
                    }
                }
                .swipeZone()
                .padding(.bottom, 4)
            }
            CommentableDiff(path: c.path, name: c.name, diff: c.diff, comments: $comments)
        }
    }

    @ViewBuilder private var commentList: some View {
        if comments.isEmpty {
            Text("Tap a line number in Diff to leave a comment. They are sent together.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18)
        }
        ForEach(comments) { c in
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Image(systemName: "text.bubble").font(.system(size: 11))
                    Text("\(c.path.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? c.path) · line \(c.line)").font(.system(size: 12))
                }
                .foregroundStyle(Hub.muted)
                Text(c.text).font(.system(size: 15))
            }
            .padding(12).frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
            .contextMenu { Button(role: .destructive) { comments.removeAll { $0.id == c.id } } label: { Label("Remove", systemImage: "trash") } }
        }
    }

    // MARK: The bottom: a verdict on Files, send on Comments

    @ViewBuilder private var bottom: some View {
        if mode == 2 {
            Button { sendComments() } label: {
                Text(comments.isEmpty ? "No comments yet" : "Send \(comments.count) comment\(comments.count == 1 ? "" : "s") to the agent")
                    .font(.system(size: 16, weight: .semibold)).foregroundStyle(comments.isEmpty ? Hub.muted : Hub.bg)
                    .frame(maxWidth: .infinity).frame(height: 52)
                    .background(Capsule().fill(comments.isEmpty ? Hub.lineStrong : Hub.accent))
            }
            .disabled(comments.isEmpty)
            .padding(.horizontal, 14).padding(.top, 6).padding(.bottom, 10)
        } else {
            if asking {
                HStack(spacing: 8) {
                    TextField("", text: $note, prompt: Text("What should change?").foregroundStyle(Hub.muted), axis: .vertical)
                        .font(.system(size: 15)).lineLimit(1...5)
                    DictationButton(text: $note)
                    Button("Send") { send(note) }.foregroundStyle(Hub.accent).fontWeight(.semibold).disabled(note.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                .padding(.horizontal, 14).padding(.vertical, 10)
                .background(RoundedRectangle(cornerRadius: 20).fill(Hub.surface2))
                .padding(.horizontal, 10).padding(.bottom, 6)
            }
            HStack(spacing: 10) {
                Button { withAnimation { asking.toggle() } } label: { Label("Ask for changes", systemImage: "mic").frame(maxWidth: .infinity) }
                    .buttonStyle(PillButtonStyle())
                Button { send("Looks good.") } label: { Text("Looks good").frame(maxWidth: .infinity) }
                    .buttonStyle(PillButtonStyle(kind: .primary))
            }
            .padding(.horizontal, 14).padding(.top, 6).padding(.bottom, 10)
        }
    }

    private func load() async {
        if store.isDemo { changes = Demo.changes; return }
        guard let client = store.client(for: thread.id) else { return }
        guard store.has("phone.changes", on: store.url(of: thread.host)) else {
            failed = "\(thread.host ?? store.machine ?? "Hub") needs a newer Hub to show changes on the phone."
            return
        }
        do { changes = try await client.changes(thread: thread.id) }
        catch let e as HubClient.Failure where e.tooOld { failed = "\(thread.host ?? store.machine ?? "Hub") needs a newer hubd to show changes on the phone." }
        catch { failed = error.localizedDescription }
    }

    private func send(_ text: String) {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty else { return }
        Task { await store.send(t, to: thread); dismiss() }
    }

    /// One message that names every file and line.
    private func sendComments() {
        let lines = comments.map { "- \($0.path), line \($0.line): \($0.text)" }
        let message = "Review comments on the changes:\n\n" + lines.joined(separator: "\n") + "\n\nPlease deal with each one."
        Task { await store.send(message, to: thread); dismiss() }
    }
}

/// One file's diff with a tappable number for each line that exists in the new file. A tap opens a
/// box under the line; what you write is kept with the file and the line.
struct CommentableDiff: View {
    let path: String
    let name: String
    let diff: String
    @Binding var comments: [LineComment]
    @State private var composing: Int?
    @State private var text = ""
    @FocusState private var focused: Bool

    private struct Row { var number: Int?; var text: String; var kind: Character }

    private var rows: [Row] {
        var out: [Row] = []
        var line = 0
        for raw in diff.split(separator: "\n", omittingEmptySubsequences: false).prefix(1500) {
            let s = String(raw)
            if s.hasPrefix("diff ") || s.hasPrefix("index ") || s.hasPrefix("--- ") || s.hasPrefix("+++ ") { continue }
            if s.hasPrefix("@@") {
                // @@ -a,b +c,d @@: the new file continues at c.
                if let plus = s.split(separator: " ").first(where: { $0.hasPrefix("+") }) {
                    line = Int(plus.dropFirst().split(separator: ",").first ?? "") ?? 1
                }
                out.append(Row(number: nil, text: s, kind: "@"))
            } else if s.hasPrefix("+") {
                out.append(Row(number: line, text: s, kind: "+")); line += 1
            } else if s.hasPrefix("-") {
                out.append(Row(number: nil, text: s, kind: "-"))
            } else {
                out.append(Row(number: line, text: s, kind: " ")); line += 1
            }
        }
        return out
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(name).font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.horizontal, 12).padding(.vertical, 8)
            ForEach(Array(rows.enumerated()), id: \.offset) { _, r in
                line(r)
                if let n = r.number {
                    ForEach(comments.filter { $0.path == path && $0.line == n }) { c in
                        Label(c.text, systemImage: "text.bubble").font(.system(size: 13)).foregroundStyle(Hub.text2)
                            .padding(10).frame(maxWidth: .infinity, alignment: .leading)
                            .background(RoundedRectangle(cornerRadius: 10).fill(Hub.surface2)).padding(.horizontal, 8).padding(.vertical, 3)
                    }
                    if composing == n { composer(n) }
                }
            }
        }
        .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
        .clipShape(RoundedRectangle(cornerRadius: 12))
    }

    private func line(_ r: Row) -> some View {
        HStack(alignment: .top, spacing: 0) {
            Button {
                guard let n = r.number else { return }
                Haptics.tap(); text = ""; composing = composing == n ? nil : n; focused = composing != nil
            } label: {
                Text(r.number.map(String.init) ?? "").font(.system(size: 11, design: .monospaced)).foregroundStyle(Hub.muted)
                    .frame(width: 38, alignment: .trailing).padding(.trailing, 6).frame(minHeight: 20)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain).disabled(r.number == nil)
            .accessibilityLabel(r.number.map { "Comment on line \($0)" } ?? "")
            Text(r.text.isEmpty ? " " : r.text).font(.system(size: 11.5, design: .monospaced))
                .foregroundStyle(r.kind == "@" ? Hub.muted : r.kind == "+" ? Hub.green : r.kind == "-" ? Hub.red : Hub.text2)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 1)
        }
        .background(r.kind == "+" ? Hub.greenBg : r.kind == "-" ? Hub.redBg : .clear)
    }

    private func composer(_ n: Int) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Label("Comment on line \(n)", systemImage: "text.bubble").font(.system(size: 12, weight: .semibold)).foregroundStyle(Hub.accent)
            TextField("", text: $text, prompt: Text("What should change here?").foregroundStyle(Hub.muted), axis: .vertical)
                .font(.system(size: 15)).lineLimit(1...5).focused($focused)
            HStack {
                Spacer()
                Button("Cancel") { composing = nil }.foregroundStyle(Hub.muted)
                Button("Add") {
                    let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
                    guard !t.isEmpty else { return }
                    comments.append(LineComment(path: path, line: n, text: t)); composing = nil; text = ""
                }
                .fontWeight(.semibold).foregroundStyle(Hub.accent)
            }
            .font(.system(size: 14))
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 12).fill(Hub.bg))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Hub.accent.opacity(0.6)))
        .padding(.horizontal, 8).padding(.vertical, 4)
    }
}

/// A unified diff, line by line: added on green, removed on red, hunk headers grey.
struct DiffView: View {
    let diff: String

    var body: some View {
        let lines = diff.split(separator: "\n", omittingEmptySubsequences: false).prefix(800)
        ScrollView(.horizontal, showsIndicators: false) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                    let s = String(line)
                    if !(s.hasPrefix("diff ") || s.hasPrefix("index ") || s.hasPrefix("--- ") || s.hasPrefix("+++ ")) {
                        Text(s.isEmpty ? " " : s)
                            .font(.system(size: 11.5, design: .monospaced))
                            .foregroundStyle(s.hasPrefix("@@") ? Hub.muted : s.hasPrefix("+") ? Hub.green : s.hasPrefix("-") ? Hub.red : Hub.text2)
                            .padding(.horizontal, 10).padding(.vertical, 1)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(s.hasPrefix("+") ? Hub.greenBg : s.hasPrefix("-") ? Hub.redBg : .clear)
                    }
                }
            }
            .padding(.vertical, 6)
        }
        .swipeZone()
        .background(Hub.bg)
    }
}
