// The pieces a thread is built from: the ask card, turns, items, work groups and live steps.

import SwiftUI

/// An ask docked above the box with big buttons for the thumb, and a line on why it asks.
struct AskCard: View {
    @Environment(Store.self) private var store
    let thread: HubThread
    let ask: Ask
    @State private var answering = false
    @State private var armed: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            HStack(spacing: 7) {
                Image(systemName: "hand.raised.fill").font(.system(size: 12)).foregroundStyle(Hub.amber)
                Text(ask.command == nil ? ask.title : "Run this command?").font(.system(size: 14.5, weight: .semibold))
            }
            if let command = ask.command {
                ScrollView(.horizontal, showsIndicators: false) {
                    Text(command).font(.system(size: 12.5, design: .monospaced)).foregroundStyle(Hub.text2).textSelection(.enabled)
                }
                .swipeZone()
                .padding(10).frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 9).fill(Hub.bg))
            }
            Text(ask.risky ? "This deletes or rewrites history. Allow takes a second tap." :
                    "In \(thread.project) on \(thread.host ?? store.machine ?? "this machine"). Settings › Safety asks before \(ask.command == nil ? "this" : "commands").")
                .font(.system(size: 11.5)).foregroundStyle(ask.risky ? Hub.red : Hub.muted)
            HStack(spacing: 8) {
                ForEach(Array(ask.options.prefix(3).enumerated()), id: \.element) { i, o in
                    let allow = o.kind.hasPrefix("allow")
                    let main = allow && i == 0 && !ask.risky
                    Button {
                        if allow && ask.risky && armed != o.id { armed = o.id; Haptics.tap(); return }
                        answer(o.id)
                    } label: {
                        Text(armed == o.id ? "Sure? \(o.name)" : o.name).font(.system(size: 14, weight: .semibold)).lineLimit(1).minimumScaleFactor(0.8)
                            .frame(maxWidth: .infinity).frame(height: 42)
                            .foregroundStyle(main ? Hub.bg : allow ? Hub.text : Hub.red)
                            .background(RoundedRectangle(cornerRadius: 12).fill(main ? Hub.accent : Hub.surface2))
                    }
                    .buttonStyle(.plain)
                }
                if !ask.hasReject {
                    Button { answer(nil) } label: {
                        Text("Deny").font(.system(size: 14, weight: .semibold)).frame(maxWidth: .infinity).frame(height: 42)
                            .foregroundStyle(Hub.red).background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface2))
                    }
                    .buttonStyle(.plain)
                }
            }
            .disabled(answering)
            .opacity(answering ? 0.5 : 1)
        }
        .padding(13)
        .background(RoundedRectangle(cornerRadius: 16).fill(Hub.surface))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(Hub.amber.opacity(0.4)))
    }

    private func answer(_ option: String?) {
        answering = true
        Task { await store.answer(thread, ask: ask, option: option); answering = false }
    }
}

/// One turn in the feed. An earlier, finished turn shows your message, one folded line for the
/// work, and the agent's last words; the latest shows everything, its work in groups.
struct TurnView: View {
    let turn: ThreadView.Turn
    let latest: Bool
    let running: Bool
    let open: Bool
    let threadId: String
    var chat = false
    /// How much of the work shows (`ChatDetail`); nil on a hubd without it, which draws as it always did.
    var detail: ChatDetail?
    /// The tables and drawings the turn's reply wrote whole (`Folded.made`), as cards after it.
    var made: [MadeFile] = []
    var did: ((ThreadView.Turn) -> Void)?
    let toggle: () -> Void

    var body: some View {
        // The "New" break after a long pause sits above the message it came before.
        // A break before the message (a long pause, a switch of engine) sits above it.
        let heads: Set<String> = ["time", "engine"]
        let users = turn.items.filter { if case .user = $0 { return true }; if case .sent = $0 { return true }; if case .hub = $0 { return true }; if case let .brk(b) = $0, heads.contains(b.kind) { return true }; return false }
        let rest = turn.items.filter { if case .user = $0 { return false }; if case .sent = $0 { return false }; if case .hub = $0 { return false }; if case let .brk(b) = $0, heads.contains(b.kind) { return false }; return true }
        let lastReplyIndex = rest.lastIndex { if case .reply = $0 { return true }; return false }
        let steps = rest.reduce(0) { n, i in if case let .work(s) = i { return n + s.count }; return n }
        let web = Self.web(users)
        VStack(alignment: .leading, spacing: 12) {
            ForEach(Array(users.enumerated()), id: \.offset) { _, item in ItemView(item: item, threadId: threadId, time: turn.start, seq: turn.userSeq, chips: turn.userSeq.flatMap { turn.reactions[$0] } ?? []) }
            if let detail {
                levelRows(detail, rest: rest, steps: steps, web: web)
            } else if chat {
                // Chat has no tool rows: the answer, and after the turn one chip that opens what it did.
                let shown = rest.filter { if case .work = $0 { return false }; return true }
                ForEach(Array(shown.enumerated()), id: \.offset) { i, item in
                    ItemView(item: item, threadId: threadId, live: running && i == shown.count - 1, web: web, seq: replySeq(i, shown), chips: replyChips(i, shown))
                }
                if steps > 0 && !(latest && running) {
                    DidChip(seconds: max(0, turn.end - turn.start), steps: steps) { did?(turn) }
                }
            } else if latest || open || steps == 0 {
                ForEach(Array(rest.enumerated()), id: \.offset) { i, item in
                    ItemView(item: item, threadId: threadId, live: running && i == rest.count - 1, web: web, seq: replySeq(i, rest), chips: replyChips(i, rest))
                }
                if !latest && open && steps > 0 { foldButton(steps: steps, open: true) }
            } else {
                foldButton(steps: steps, open: false)
                ForEach(Array(rest.enumerated()), id: \.offset) { i, item in
                    if i == lastReplyIndex || isTail(item) { ItemView(item: item, threadId: threadId, web: web, seq: replySeq(i, rest), chips: replyChips(i, rest)) }
                }
            }
            if !(latest && running) {
                let files = FilePages.paths(in: rest)
                if !files.isEmpty { FilePagesView(paths: files, threadId: threadId) }
                if !made.isEmpty { MadeFilesView(files: made) }
                let named = NamedFiles.paths(in: rest, skipping: made)
                if !named.isEmpty { NamedFilesView(paths: named, threadId: threadId) }
                let sources = Sources.of(rest)
                if web {
                    // The answer's own links are its cards; without any, the pages it read are.
                    let said = rest.contains { if case let .reply(t) = $0 { return !Web.answer(t).links.isEmpty }; return false }
                    if !said, !sources.isEmpty { SourceCards(urls: sources) }
                } else if !sources.isEmpty { SourcesLine(urls: sources) }
            }
        }
    }

    /// The turn end a reaction to the reply points at: only the turn's last reply, once the turn is over.
    private func replySeq(_ i: Int, _ list: [Item]) -> Int? {
        guard let end = turn.endSeq, i == list.lastIndex(where: { if case .reply = $0 { return true }; return false }) else { return nil }
        return end
    }

    private func replyChips(_ i: Int, _ list: [Item]) -> [ReactionChip] {
        replySeq(i, list).flatMap { turn.reactions[$0] } ?? []
    }

    /// A turn you asked as a web search.
    static func web(_ items: [Item]) -> Bool {
        items.contains { if case let .user(t) = $0 { return Web.asked(t) != nil }; return false }
    }

    /// What stays visible in a folded turn besides the last words: failures, files and asks.
    private func isTail(_ item: Item) -> Bool {
        switch item { case .error, .files, .ask, .page, .brk, .picture, .side: return true; default: return false }
    }

    /// The turn's rows at a level (mirrors `Transcript.svelte`'s rows). Minimal, Quiet and Normal tuck the
    /// turn's work and notes into one fold row, keeping what is happening now (Quiet: the live step;
    /// Normal: also the latest note and the running command's last lines) and, once done, the answer.
    /// Full leaves everything in place with every group open. Questions, failures, files and pages are
    /// never tucked.
    @ViewBuilder private func levelRows(_ d: ChatDetail, rest: [Item], steps: Int, web: Bool) -> some View {
        let live = latest && running
        let lastReply = rest.lastIndex { if case .reply = $0 { return true }; return false }
        let lastWork = rest.lastIndex { if case .work = $0 { return true }; return false }
        let tucked: [Int] = d == .all ? [] : rest.indices.filter { i in
            switch rest[i] {
            case .work: return true
            case .reply: return live ? !(d == .normal && i == lastReply) : i != lastReply
            default: return false
            }
        }
        let lastGroup: [Step] = lastWork.flatMap { if case let .work(s) = rest[$0] { return s }; return nil } ?? []
        let failed = !live && lastGroup.last?.status == "failed"
        ForEach(Array(rest.enumerated()), id: \.offset) { i, item in
            let hidden = tucked.contains(i) && !open
            if i == tucked.first, d != .minimal {
                foldButton(label: foldLabel(d, steps: steps, live: live), open: open, failed: failed)
            }
            if !hidden {
                if d == .all, case let .work(s) = item {
                    WorkGroup(steps: s, live: live && i == rest.count - 1, expanded: true)
                } else {
                    ItemView(item: item, threadId: threadId, live: live && i == rest.count - 1, web: web, seq: replySeq(i, rest), chips: replyChips(i, rest))
                }
            } else if live, i == lastWork, d == .quiet || d == .normal, let step = lastGroup.last {
                LiveStep(step: step, tail: d == .normal)
            }
        }
        if d == .minimal && steps > 0 && !live {
            DidChip(seconds: max(0, turn.end - turn.start), steps: steps) { did?(turn) }
        }
    }

    private func foldLabel(_ d: ChatDetail, steps: Int, live: Bool) -> String {
        let took = ThreadView.duration(max(0, turn.end - turn.start))
        if live { return "So far" }
        if d == .normal { return "Worked \(took)\(steps > 0 ? " · \(steps) step\(steps == 1 ? "" : "s")" : "")" }
        return "Worked for \(took)"
    }

    private func foldButton(steps: Int, open: Bool) -> some View {
        foldButton(label: "Worked \(ThreadView.duration(max(0, turn.end - turn.start)))\(steps > 0 ? " · \(steps) step\(steps == 1 ? "" : "s")" : "")", open: open, failed: false)
    }

    private func foldButton(label: String, open: Bool, failed: Bool) -> some View {
        Button { withAnimation(.snappy) { toggle() }; Haptics.tap() } label: {
            HStack(spacing: 6) {
                Image(systemName: failed ? "xmark.circle" : "checkmark.circle").font(.system(size: 12))
                Text(label)
                Image(systemName: open ? "chevron.up" : "chevron.down").font(.system(size: 10, weight: .semibold))
            }
            .font(.system(size: 12.5).monospacedDigit()).foregroundStyle(failed ? Hub.red : Hub.muted)
            .padding(.vertical, 4)
        }
        .buttonStyle(.plain)
    }
}

struct ItemView: View {
    @Environment(Store.self) private var store
    let item: Item
    var threadId = ""
    var time: Double = 0
    var live = false
    /// The turn is a web search: the answer's sources show as cards.
    var web = false
    /// What a reaction to this message points at, and the emojis already on it (`Reactions.swift`).
    var seq: Int?
    var chips: [ReactionChip] = []
    @State private var copied = false
    @State private var reacting = false
    @Environment(\.quoteReply) private var quote

    var body: some View {
        switch item {
        case let .user(said):
            // A web search shows your own words with a globe, never the instruction Hub sent.
            let asked = Web.asked(said)
            let text = asked ?? said
            VStack(alignment: .trailing, spacing: 4) {
                VStack(alignment: .leading, spacing: 3) {
                    if asked != nil { WebTag() }
                    Folding(text: text)
                }
                .padding(.horizontal, 14).padding(.vertical, 10)
                .background(RoundedRectangle(cornerRadius: 20).fill(Hub.surface2))
                if let seq { ReactionRow(threadId: threadId, seq: seq, chips: chips, trailing: true) { reacting = true } }
                HStack(spacing: 10) {
                    if time > 0 { Text(Store.clock(time)).font(.system(size: 11)).foregroundStyle(Hub.muted) }
                    copyButton(text)
                }
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
            .padding(.leading, 40)
            .likeOnDoubleTap(threadId, seq: seq, chips: chips, store: store)
            .messageActions(text, react: reactor)
            .sheet(isPresented: $reacting) { ReactSheet(threadId: threadId, seq: seq ?? 0, chips: chips, text: text, quote: quote) }
        case let .hub(text):
            Label(text, systemImage: "sparkles").font(.system(size: 11)).foregroundStyle(Hub.muted.opacity(0.7)).lineLimit(1)
        case let .sent(files):
            SentFiles(files: files)
                .frame(maxWidth: .infinity, alignment: .trailing)
                .padding(.leading, 40)
        case let .reply(text):
            let found = web ? Web.answer(text) : (body: text, links: [URL]())
            VStack(alignment: .leading, spacing: 4) {
                Prose(text: found.body)
                if !live { ReplyPictures(text: text, threadId: threadId) }
                if !live && !found.links.isEmpty { SourceCards(urls: found.links).padding(.top, 4) }
                if !live, let seq { ReactionRow(threadId: threadId, seq: seq, chips: chips) { reacting = true } }
                if !live { copyButton(text) }
            }
            .likeOnDoubleTap(threadId, seq: seq, chips: chips, store: store)
            .messageActions(text, react: reactor)
            .sheet(isPresented: $reacting) { ReactSheet(threadId: threadId, seq: seq ?? 0, chips: chips, text: text, quote: quote) }
        case let .work(steps):
            WorkGroup(steps: steps, live: live)
        case let .ask(ask):
            if ask.resolved {
                let picked = ask.options.first { $0.id == ask.answer }?.name
                Label("\(ask.title): \(ask.answer == nil ? "Declined" : picked ?? "Answered")", systemImage: "checkmark.shield")
                    .font(.system(size: 12.5)).foregroundStyle(Hub.muted)
            }
            // An open ask is docked above the box, not repeated in the feed.
        case let .error(text):
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "exclamationmark.circle.fill").foregroundStyle(Hub.red)
                Text(text).font(.system(size: 13.5)).foregroundStyle(Hub.red).textSelection(.enabled)
            }
            .padding(11).frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 11).fill(Hub.redBg))
        case let .brk(brk):
            BreakView(brk: brk)
        case let .side(job):
            SideJobView(job: job, from: store.thread(threadId)?.engine ?? "")
        case let .page(page):
            PageCard(page: page, threadId: threadId)
        case let .picture(picture):
            PictureView(picture: picture)
        case let .files(names):
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 7) {
                    ForEach(names, id: \.self) { name in
                        Label(name, systemImage: "doc").font(.system(size: 12.5)).foregroundStyle(Hub.text2)
                            .padding(.horizontal, 10).frame(height: 30)
                            .background(Capsule().fill(Hub.surface2)).overlay(Capsule().strokeBorder(Hub.lineStrong))
                    }
                    NavigationLink(value: Route.changes(threadId)) {
                        Label("Changes", systemImage: "plus.forwardslash.minus").font(.system(size: 12.5)).foregroundStyle(Hub.accent)
                            .padding(.horizontal, 10).frame(height: 30).background(Capsule().fill(Hub.accentBg))
                    }
                }
            }
            .swipeZone()
        }
    }

    /// A hold on the message opens the reaction sheet, when there is something to react to.
    private var reactor: (() -> Void)? { seq != nil && store.reacts ? { reacting = true } : nil }

    private func copyButton(_ text: String) -> some View {
        Button {
            UIPasteboard.general.string = text
            Haptics.tap()
            copied = true
            Task { try? await Task.sleep(for: .seconds(1.2)); copied = false }
        } label: {
            Image(systemName: copied ? "checkmark" : "doc.on.doc").font(.system(size: 11)).foregroundStyle(Hub.muted)
                .frame(width: 28, height: 22).contentShape(Rectangle())
        }
        .buttonStyle(.borderless)
        .accessibilityLabel("Copy")
    }
}

/// The agent's steps as one grey line you can open (T3 Code's work group): each step a row with an
/// icon for its kind, and a tap on a step shows its command and what came back.
struct WorkGroup: View {
    let steps: [Step]
    var live = false
    /// Full detail: the group and every step with a command or output start open (a tap still closes them).
    var expanded = false
    @State private var open = false
    @State private var shown: Set<Int> = []
    private var isOpen: Bool { open != expanded }
    private func isShown(_ i: Int) -> Bool {
        guard expanded else { return shown.contains(i) }
        return steps[i].command != nil || steps[i].output != nil ? !shown.contains(i) : shown.contains(i)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button { withAnimation(.snappy) { open.toggle() }; Haptics.tap() } label: {
                HStack(spacing: 7) {
                    Image(systemName: Self.icon(steps.last)).font(.system(size: 12)).frame(width: 16)
                    Text(summary).lineLimit(1)
                    if live && !isOpen { Working(size: 13) }
                    Image(systemName: isOpen ? "chevron.up" : "chevron.down").font(.system(size: 9, weight: .semibold))
                    Spacer(minLength: 0)
                }
                .font(.system(size: 12.5)).foregroundStyle(failed > 0 ? Hub.red : Hub.muted)
                .padding(.vertical, 3).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityHint("Shows \(steps.count) steps")
            if isOpen {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(Array(steps.enumerated()), id: \.offset) { i, step in row(i, step) }
                }
                .padding(.leading, 6).padding(.top, 4)
                .overlay(alignment: .leading) { Hub.line.frame(width: 1).padding(.vertical, 4) }
            }
        }
    }

    private var failed: Int { steps.filter { $0.status == "failed" }.count }

    private var summary: String {
        let last = steps.last?.title ?? ""
        if steps.count == 1 { return last }
        return "\(steps.count) steps\(failed > 0 ? ", \(failed) failed" : "") · \(last)"
    }

    private func row(_ i: Int, _ step: Step) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Button { withAnimation(.snappy) { if shown.contains(i) { shown.remove(i) } else { shown.insert(i) } } } label: {
                HStack(spacing: 7) {
                    Image(systemName: Self.icon(step)).font(.system(size: 11.5)).frame(width: 16)
                    Text(step.title).lineLimit(isShown(i) ? nil : 1)
                    Spacer(minLength: 0)
                    if step.status == "failed" { Image(systemName: "xmark").font(.system(size: 10, weight: .bold)).foregroundStyle(Hub.red) }
                }
                .font(.system(size: 12.5)).foregroundStyle(step.status == "failed" ? Hub.red : Hub.text3)
                .padding(.leading, 8).frame(minHeight: 28).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .contextMenu {
                Button { UIPasteboard.general.string = step.command ?? step.title } label: { Label("Copy", systemImage: "doc.on.doc") }
                if let out = step.output { Button { UIPasteboard.general.string = out } label: { Label("Copy the output", systemImage: "doc.plaintext") } }
            }
            if isShown(i) {
                VStack(alignment: .leading, spacing: 6) {
                    if let c = step.command { Text("$ " + c).font(.system(size: 11.5, design: .monospaced)).foregroundStyle(Hub.text2) }
                    if let out = step.output {
                        ScrollView { Text(out).font(.system(size: 11, design: .monospaced)).foregroundStyle(Hub.text3).frame(maxWidth: .infinity, alignment: .leading) }
                            .frame(maxHeight: 220)
                    } else if step.command == nil {
                        Text("No output.").font(.system(size: 11.5)).foregroundStyle(Hub.muted)
                    }
                }
                .textSelection(.enabled)
                .padding(10).frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 9).fill(Hub.side))
                .padding(.leading, 8)
            }
        }
    }

    static func icon(_ step: Step?) -> String {
        guard let step else { return "circle" }
        if step.status == "failed" { return "xmark.circle" }
        if step.status == "in_progress" || step.status == "pending" { return "circle.dotted" }
        let t = step.title.lowercased()
        if step.command != nil || t.hasPrefix("ran") || t.hasPrefix("run") || t.contains("`") { return "terminal" }
        if t.hasPrefix("read") || t.hasPrefix("search") || t.hasPrefix("find") || t.hasPrefix("grep") || t.hasPrefix("list") || t.hasPrefix("view") { return "eye" }
        if t.hasPrefix("edit") || t.hasPrefix("write") || t.hasPrefix("creat") || t.hasPrefix("updat") || t.hasPrefix("delet") { return "pencil" }
        if t.hasPrefix("fetch") || t.contains("http") { return "globe" }
        return "circle.fill"
    }
}

/// A long message of yours, as markdown, folded to a few lines with "Show more" under it.
struct Folding: View {
    let text: String
    @State private var open = false
    private var long: Bool { text.count > 360 || text.filter { $0 == "\n" }.count > 6 }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if long && !open {
                Text((try? AttributedString(markdown: text, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(text))
                    .font(.system(size: 15)).foregroundStyle(Hub.text).lineLimit(6)
            } else {
                Prose(text: text).textSelection(.enabled)
            }
            if long {
                Button(open ? "Show less" : "Show more") { withAnimation(.snappy) { open.toggle() } }
                    .font(.system(size: 13, weight: .medium)).foregroundStyle(Hub.accent).buttonStyle(.plain)
            }
        }
    }
}

/// Quiet and Normal while a turn runs: the one step it is on, as a line; Normal adds the last lines of a
/// command that is still running.
struct LiveStep: View {
    let step: Step
    var tail = false

    private var failed: Bool { step.status == "failed" }
    private var lines: String? {
        guard tail, step.command != nil, step.status == "in_progress", let out = step.output else { return nil }
        let last = out.split(separator: "\n", omittingEmptySubsequences: true).suffix(3).joined(separator: "\n")
        return last.isEmpty ? nil : last
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 7) {
                Image(systemName: WorkGroup.icon(step)).font(.system(size: 12)).frame(width: 16)
                Text(step.title).lineLimit(1)
                Spacer(minLength: 0)
            }
            .font(.system(size: 12.5)).foregroundStyle(failed ? Hub.red : Hub.muted)
            if let lines {
                Text(lines).font(.system(size: 11, design: .monospaced)).foregroundStyle(Hub.text3).lineLimit(3)
                    .padding(8).frame(maxWidth: .infinity, alignment: .leading)
                    .background(RoundedRectangle(cornerRadius: 9).fill(Hub.side))
                    .padding(.leading, 23)
            }
        }
        .padding(.vertical, 3)
        .accessibilityElement(children: .combine)
    }
}
