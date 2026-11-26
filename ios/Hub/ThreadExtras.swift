import SwiftUI

// What sheet "Tråden" adds to a thread: slash commands above the box, a plan to approve, and
// quoting a message. Everything here works with the calls the phone's token already has.

/// A command typed after `/` in the reply box. `run` does it on the phone; `fill` writes text in
/// the box for you to read and send (Return never sends, only the arrow does).
struct SlashCommand: Identifiable {
    var name: String
    var detail: String
    var fill: String?
    var run: (() -> Void)?
    var id: String { name }
}

/// The commands rise above the box and narrow as you type (sketch "Tråden", screen 1).
struct SlashPopover: View {
    let query: String
    let items: [SlashCommand]
    let pick: (SlashCommand) -> Void

    var body: some View {
        let q = query.lowercased()
        let list = items.filter { q.isEmpty || $0.name.hasPrefix(q) }
        VStack(alignment: .leading, spacing: 0) {
            if list.isEmpty {
                Text("No command called /\(query)").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(14)
            }
            ForEach(Array(list.enumerated()), id: \.element.id) { i, c in
                Button { pick(c) } label: {
                    HStack(spacing: 10) {
                        Text("/\(c.name)").font(.system(size: 14, design: .monospaced)).foregroundStyle(Hub.accent)
                        Text(c.detail).font(.system(size: 14.5)).foregroundStyle(Hub.text2).lineLimit(1)
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 14).frame(height: 46).frame(maxWidth: .infinity, alignment: .leading)
                    .background(i == 0 ? Hub.accentBg : .clear)
                    .overlay(alignment: .bottom) { if i < list.count - 1 { Hub.line.frame(height: 1) } }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .background(RoundedRectangle(cornerRadius: 18).fill(Hub.surface))
        .clipShape(RoundedRectangle(cornerRadius: 18))
        .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(Hub.lineStrong))
        .shadow(color: .black.opacity(0.5), radius: 18, y: 8)
    }
}

extension Store {
    /// The agent's plan when it has planned first and nothing has begun: the thread waits for a go.
    func planToApprove(_ t: HubThread) -> [PlanStep]? {
        guard t.status != .running, let f = folded[t.id], !f.plan.isEmpty, f.planTurn == (f.turns.last ?? 0),
              f.plan.allSatisfy({ $0.status == "pending" }) else { return nil }
        return f.plan
    }
}

/// A plan to approve (sketch "Tråden", screen 2): the steps as a card above the box. Approve starts
/// it; Change opens the box with the plan quoted, so you say what is wrong and it plans again.
struct PlanApprovalCard: View {
    let steps: [PlanStep]
    let approve: () -> Void
    let change: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Image(systemName: "list.bullet").font(.system(size: 11))
                Text("Plan · \(steps.count) step\(steps.count == 1 ? "" : "s")").font(.system(size: 12.5, weight: .semibold))
            }
            .foregroundStyle(Hub.muted).padding(.bottom, 8)
            ForEach(Array(steps.enumerated()), id: \.offset) { i, s in
                HStack(alignment: .top, spacing: 10) {
                    RoundedRectangle(cornerRadius: 5).strokeBorder(Hub.lineStrong, lineWidth: 1.5).frame(width: 17, height: 17).padding(.top, 1)
                    Text(s.text).font(.system(size: 14)).foregroundStyle(Hub.text2).frame(maxWidth: .infinity, alignment: .leading)
                }
                .padding(.vertical, 7)
                .overlay(alignment: .bottom) { if i < steps.count - 1 { Hub.line.frame(height: 1) } }
            }
            HStack(spacing: 8) {
                Button(action: approve) { Text("Approve and run").frame(maxWidth: .infinity) }.buttonStyle(PillButtonStyle(kind: .primary))
                Button(action: change) { Text("Change").frame(maxWidth: .infinity) }.buttonStyle(PillButtonStyle())
            }
            .padding(.top, 10)
        }
        .padding(.horizontal, 14).padding(.vertical, 12)
        .background(RoundedRectangle(cornerRadius: 16).fill(Hub.surface))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(Hub.lineStrong))
        .padding(.horizontal, 12).padding(.bottom, 6)
    }
}

/// How a message gets quoted in the reply box: "> " before each line, then room to write.
enum Quote {
    static func text(_ message: String) -> String {
        let lines = message.trimmingCharacters(in: .whitespacesAndNewlines).split(separator: "\n", omittingEmptySubsequences: false)
        let short = lines.prefix(6).map { "> " + $0 } + (lines.count > 6 ? ["> …"] : [])
        return short.joined(separator: "\n") + "\n\n"
    }
}

private struct QuoteReplyKey: EnvironmentKey { static let defaultValue: ((String) -> Void)? = nil }
extension EnvironmentValues {
    /// Set by the thread page: puts a message, quoted, in the reply box.
    var quoteReply: ((String) -> Void)? {
        get { self[QuoteReplyKey.self] }
        set { self[QuoteReplyKey.self] = newValue }
    }
}

private struct SwipeToQuoteKey: EnvironmentKey { static let defaultValue = false }
extension EnvironmentValues {
    /// On in a chat (it has no settle or snooze): a swipe to the left on a message quotes it in the box.
    var swipeToQuote: Bool {
        get { self[SwipeToQuoteKey.self] }
        set { self[SwipeToQuoteKey.self] = newValue }
    }
}

/// Hold a message for Copy and Quote in reply. In a chat a swipe to the left on a message pulls out a
/// reply mark and quotes it (a swipe to the right on a dev thread settles it, so it does not quote there).
struct MessageActions: ViewModifier {
    @Environment(\.quoteReply) private var quote
    @Environment(\.swipeToQuote) private var swipes
    let text: String
    /// Opens the reaction sheet on a hold; without it a hold gives Copy and Quote in the plain menu.
    var react: (() -> Void)?
    @State private var dx: CGFloat = 0
    @State private var ours: Bool?
    @State private var armed = false

    private let line: CGFloat = 64

    func body(content: Content) -> some View {
        content
            .offset(x: dx)
            .overlay(alignment: .trailing) {
                if dx < -8 {
                    Image(systemName: "arrowshape.turn.up.left.fill").font(.system(size: 14))
                        .foregroundStyle(armed ? Hub.accent : Hub.muted)
                        .frame(width: 32, height: 32).background(Circle().fill(Hub.surface2))
                        .scaleEffect(armed ? 1.1 : 0.85).opacity(min(1, -dx / 40))
                        .offset(x: 40).animation(.snappy(duration: 0.15), value: armed)
                }
            }
            .simultaneousGesture(swipes && quote != nil ? drag : nil)
            .modifier(Holding(text: text, quote: quote, react: react))
    }

    private var drag: some Gesture {
        DragGesture(minimumDistance: 18, coordinateSpace: .global)
            .onChanged { v in
                if ours == nil {
                    // Only a clear swipe to the left, not from the right edge, and not in a code block or table.
                    ours = v.translation.width < 0 && abs(v.translation.width) > abs(v.translation.height) * 1.6
                        && !SwipeZones.holds(v.startLocation)
                    if ours == true { Haptics.ready() }
                }
                guard ours == true else { return }
                dx = max(-110, min(0, v.translation.width * 0.6))
                let reached = -dx > line * 0.6
                if reached != armed { armed = reached; Haptics.tick() }
            }
            .onEnded { _ in
                let mine = ours == true
                let go = mine && armed
                ours = nil; armed = false
                guard mine else { return }
                TouchGuard.swiped()
                withAnimation(.snappy) { dx = 0 }
                if go { Haptics.tap(); quote?(text) }
            }
    }
}

/// A hold on a message: the reaction sheet (`Reactions.swift`) when it can take one, else the plain menu.
private struct Holding: ViewModifier {
    let text: String
    let quote: ((String) -> Void)?
    let react: (() -> Void)?

    func body(content: Content) -> some View {
        if let react {
            content.onLongPressGesture(minimumDuration: 0.35) { Haptics.firm(); react() }
        } else {
            content.contextMenu {
                Button { UIPasteboard.general.string = text; Haptics.tap() } label: { Label("Copy", systemImage: "doc.on.doc") }
                if let quote { Button { quote(text) } label: { Label("Quote in reply", systemImage: "text.bubble") } }
            }
        }
    }
}

extension View {
    func messageActions(_ text: String, react: (() -> Void)? = nil) -> some View { modifier(MessageActions(text: text, react: react)) }
}

// MARK: Git in the thread (sheet "Git och granskning", screens 1 and 2)

/// A line under the thread's header that says where the work stands: its branch, whether it is
/// pushed, and lines added and removed, or its pull request. Only there when there is something to say.
struct GitLine: View {
    var branch: String?
    var pushWord: String?
    let files: Int
    let added: Int
    let removed: Int
    let pull: Int?
    let tap: () -> Void

    var body: some View {
        Button(action: tap) {
            HStack(spacing: 8) {
                Image(systemName: "arrow.triangle.branch").font(.system(size: 13)).foregroundStyle(Hub.muted)
                if let branch {
                    Text(branch).font(.system(size: 12.5, design: .monospaced)).foregroundStyle(Hub.text2).lineLimit(1)
                    if let pushWord { Text(pushWord).font(.system(size: 12.5)).foregroundStyle(Hub.amber).lineLimit(1) }
                } else {
                    Text("\(files) file\(files == 1 ? "" : "s") changed").font(.system(size: 12.5)).foregroundStyle(Hub.text2)
                }
                if let pull { Text("#\(pull)").font(.system(size: 12.5, design: .monospaced)).foregroundStyle(Hub.accent) }
                Spacer()
                if files > 0 {
                    Text("+\(added)").foregroundStyle(Hub.green)
                    Text("−\(removed)").foregroundStyle(Hub.red)
                }
            }
            .font(.system(size: 12.5, weight: .medium))
            .padding(.horizontal, 14).frame(height: 38)
            .background(Capsule().fill(Hub.surface))
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 12).padding(.bottom, 6)
        .accessibilityLabel("Git: what the thread changed")
    }
}

/// What the git line opens (sheet "Git och granskning", screen 2): review the changes, commit,
/// push, open a pull request. With `phone.git` Hub does them itself and writes the commit message
/// and the pull request's words from the thread; the one that fits the state is in violet. On a
/// hubd without it they ask the agent in the thread to do it.
struct GitPanel: View {
    let thread: HubThread
    let state: GitState?
    let files: Int
    let added: Int
    let removed: Int
    let pull: Int?
    let review: () -> Void
    let openPull: () -> Void
    let ask: (String) -> Void
    let changed: () async -> Void
    @Environment(Store.self) private var store
    @State private var busy: String?

    private var direct: Bool { store.has("phone.git", on: store.url(of: thread.host)) && state != nil }

    var body: some View {
        let dirty = state.map { !$0.files.isEmpty } ?? (files > 0)
        let unpushed = state.map { !$0.upstream || $0.ahead > 0 } ?? false
        let next = dirty ? "commit" : (unpushed ? "push" : (pull == nil ? "pr" : "view"))
        PanelGroup {
            PanelRow(title: state?.branch ?? "\(files) file\(files == 1 ? "" : "s") changed",
                     sub: [files > 0 ? "\(files) file\(files == 1 ? "" : "s") · +\(added) −\(removed)" : "Nothing uncommitted",
                           state.flatMap { $0.ahead > 0 ? "\($0.ahead) commit\($0.ahead == 1 ? "" : "s") ahead of \($0.default)" : nil }].compactMap { $0 }.joined(separator: " · "),
                     last: false) {
                Image(systemName: "arrow.triangle.branch").foregroundStyle(Hub.accent)
            } trailing: { EmptyView() } action: {}
                .disabled(true)
            PanelRow(title: "Review changes") {
                Image(systemName: "eye").foregroundStyle(Hub.text2)
            } trailing: {
                if files > 0 { Text("\(files) file\(files == 1 ? "" : "s")").font(.system(size: 12)).foregroundStyle(Hub.muted) }
            } action: { review() }
            PanelRow(title: "Commit", sub: direct ? "Message written for you" : "Asks the agent to commit", dim: direct && !dirty) {
                Image(systemName: busy == "commit" ? "hourglass" : "checkmark").foregroundStyle(next == "commit" ? Hub.accent : Hub.text2)
            } trailing: { EmptyView() } action: {
                if direct { run("commit") { c in try await c.ship(thread: thread.id, commit: true, message: await self.draft(c).message) } }
                else { ask("Commit the changes with a clear commit message.") }
            }
            PanelRow(title: "Push", sub: direct ? state?.pushWord : "Asks the agent to push the branch", dim: direct && !unpushed && !dirty) {
                Image(systemName: busy == "push" ? "hourglass" : "arrow.up").foregroundStyle(next == "push" ? Hub.accent : Hub.text2)
            } trailing: { EmptyView() } action: {
                if direct { run("push") { c in try await c.ship(thread: thread.id, commit: dirty, push: true, message: dirty ? await self.draft(c).message : nil) } }
                else { ask("Push the branch.") }
            }
            if let pull {
                PanelRow(title: "Pull request #\(pull)", last: true) {
                    Image(systemName: "arrow.triangle.pull").foregroundStyle(Hub.accent)
                } trailing: { Image(systemName: "chevron.right").font(.system(size: 12)).foregroundStyle(Hub.muted) } action: { openPull() }
            } else {
                PanelRow(title: "Open pull request", sub: direct ? "Title and text written from the thread" : "Asks the agent to write it from the thread", last: true) {
                    Image(systemName: busy == "pr" ? "hourglass" : "arrow.triangle.pull").foregroundStyle(next == "pr" ? Hub.accent : Hub.text2)
                } trailing: { EmptyView() } action: {
                    if direct {
                        run("pr") { c in
                            let d = await self.draft(c)
                            return try await c.ship(thread: thread.id, commit: dirty, push: true, pr: true, message: dirty ? d.message : nil, title: d.title, body: d.body)
                        }
                    } else { ask("Open a pull request for this work, with a title and a description written from this thread.") }
                }
            }
        }
        if !direct {
            Text("Hub doing the commit and push itself needs the next version of hubd.").font(.system(size: 11.5)).foregroundStyle(Hub.muted)
        }
    }

    private func draft(_ c: HubClient) async -> GitDraft {
        (try? await c.gitDraft(thread: thread.id)) ?? GitDraft(message: "Update", title: "Update", body: "")
    }

    private func run(_ what: String, _ job: @escaping (HubClient) async throws -> GitShipped) {
        guard busy == nil, let client = store.client(for: thread.id) else { return }
        busy = what
        Task {
            do {
                let done = try await job(client)
                Haptics.tap()
                store.flash(done.pr_url != nil ? "Pull request opened" : done.pushed == true ? "Pushed" : done.commit != nil ? "Committed \(done.commit ?? "")" : "Done")
                await changed()
            } catch { store.flash("Not done: \(error.localizedDescription)") }
            busy = nil
        }
    }
}
