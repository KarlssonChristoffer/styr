import SwiftUI
import UIKit

// The inbox as sketches 23 and 24 draw it: the desktop thread list's rows (project in its colour with
// the engine mark, the machine in a small chip and the state at the right, then one line with the dot
// and the title), the threads in groups that fold, each group one card, and a colour rail per project.
// A swipe to the right settles; a swipe to the left is the night swipe: a star sky and a moon that grows
// while you pull, three nights to pick (Store.snoozeChoices), a long pull takes the first, and a shooting
// star if you hold on too long. Two settings under Appearance switch the new look and the moon off.

// MARK: Groups

struct InboxGroupData: Identifiable {
    var id: String
    var title: String
    var tint: Color
    var threads: [HubThread]
    var settleAll = false
}

/// Which groups are folded, kept on the phone: From Hub and Snoozed start shut, the rest open.
enum InboxFolds {
    private static let key = "inbox.folded"
    static func load() -> Set<String> {
        if let a = UserDefaults.standard.stringArray(forKey: key) { return Set(a) }
        return ["hub", "snoozed"]
    }
    static func save(_ s: Set<String>) { UserDefaults.standard.set(Array(s), forKey: key) }
}

/// A group's heading: its name and count, a hairline, a chevron; a tap folds it.
struct GroupHeading: View {
    let title: String
    let count: Int
    var tint: Color = Hub.muted
    var folded: Bool
    var settleAll: (() -> Void)?
    let toggle: () -> Void

    var body: some View {
        Button(action: toggle) {
            HStack(spacing: 8) {
                Text(title).font(.system(size: 12.5, weight: .semibold)).foregroundStyle(tint)
                Text("\(count)").font(.system(size: 12.5)).foregroundStyle(Hub.muted)
                Rectangle().fill(Hub.line).frame(height: 1)
                if let settleAll, !folded {
                    Text("Settle all").font(.system(size: 12.5, weight: .medium)).foregroundStyle(Hub.accent)
                        .safeTap { settleAll() }
                }
                Image(systemName: folded ? "chevron.right" : "chevron.down").font(.system(size: 10, weight: .semibold)).foregroundStyle(Hub.muted)
            }
            .padding(.horizontal, 18).padding(.top, Metrics.sectionTop).padding(.bottom, Metrics.sectionBottom)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(title), \(count)")
        .accessibilityHint(folded ? "Unfolds the group" : "Folds the group")
    }
}

/// One group as a rounded card with its rows inside, a hairline between them.
struct GroupCard<Rows: View>: View {
    @ViewBuilder var rows: () -> Rows
    var body: some View {
        VStack(spacing: 0) { rows() }
            .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
            .clipShape(RoundedRectangle(cornerRadius: 14))
            .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(Hub.line))
            .padding(.horizontal, 12)
    }
}

// MARK: The order, shared with the thread's "next"

extension Store {
    /// The inbox's groups: what needs you, what is done, then what works (it needs nothing from
    /// you, so it sits low like on the desktop), Hub's own and the snoozed ones. Working always shows its
    /// threads, Hub's own too, so the "N working" in the header always has its rows.
    /// What needs you always stays in Needs you, wherever it came from. `match` narrows what the inbox shows.
    func inboxGroups(match: (HubThread) -> Bool = { _ in true }) -> [InboxGroupData] {
        func ours(_ t: HubThread) -> Bool { isInternal(projectRef(of: t)) }
        let needs = threads(in: .needsYou).filter(match)
        let working = threads(in: .working).filter(match)
        let done = threads(in: .done).filter(match)
        let hub = done.filter(ours).sorted { ($0.returned_at ?? $0.sent_at ?? $0.updated_at) > ($1.returned_at ?? $1.sent_at ?? $1.updated_at) }
        let now = Date().timeIntervalSince1970 * 1000
        let snoozed = threads.filter { t in
            guard let s = t.snoozed_until, s > now, !t.archived, t.settled_at == nil, !isChat(t) else { return false }
            return t.status != .waiting_on_you && t.status != .error
        }.filter(match)
        return [
            InboxGroupData(id: "needs", title: "Needs you", tint: Hub.amber, threads: needs),
            InboxGroupData(id: "done", title: "Done", tint: Hub.muted, threads: done.filter { !ours($0) }, settleAll: has("phone.tidy")),
            InboxGroupData(id: "working", title: "Working", tint: Hub.muted, threads: working),
            InboxGroupData(id: "hub", title: "From Hub", tint: Hub.muted, threads: hub),
            InboxGroupData(id: "snoozed", title: "Snoozed", tint: Hub.muted, threads: snoozed),
        ].filter { !$0.threads.isEmpty }
    }

    /// The thread after this one in the inbox's order (the grouped order when Inbox grouping is on, else
    /// Needs you, Done, Working), nil when it is the last or is not in the inbox.
    func nextThread(after id: String) -> HubThread? {
        let grouped = (UserDefaults.standard.object(forKey: "inbox.cards") as? Bool) ?? true
        let order: [HubThread] = grouped ? inboxGroups().flatMap(\.threads) : [InboxSection.needsYou, .done, .working].flatMap { threads(in: $0) }
        guard let i = order.firstIndex(where: { $0.id == id }), i + 1 < order.count else { return nil }
        return order[i + 1]
    }
}

// MARK: The row

/// A thread as the desktop list shows it: two lines and a rail in the project's colour.
struct ThreadCardRow: View {
    @Environment(Store.self) private var store
    let thread: HubThread
    var picking = false
    var picked = false
    /// What is typed in search: the same letters in the title are marked.
    var mark = ""

    /// The title with the searched letters marked (the first place they are).
    private func titled(_ title: String) -> AttributedString {
        var a = AttributedString(title)
        guard !mark.isEmpty, let r = title.range(of: mark, options: .caseInsensitive) else { return a }
        let from = title.distance(from: title.startIndex, to: r.lowerBound), len = title.distance(from: r.lowerBound, to: r.upperBound)
        let start = a.index(a.startIndex, offsetByCharacters: from), end = a.index(start, offsetByCharacters: len)
        a[start..<end].backgroundColor = Hub.accentBg
        return a
    }

    private var projectName: String { thread.isQuick ? "Quick" : thread.project }
    private var tint: Color { thread.isQuick ? Hub.accent : Hub.projectColor(thread.project) }

    var body: some View {
        HStack(spacing: 0) {
            Rectangle().fill(tint).frame(width: 3)
            if picking {
                Image(systemName: picked ? "checkmark.circle.fill" : "circle").font(.system(size: 20))
                    .foregroundStyle(picked ? Hub.accent : Hub.lineStrong).padding(.leading, 10)
            }
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Image(systemName: thread.isQuick ? "bolt.fill" : "folder").font(.system(size: 12)).foregroundStyle(tint)
                    Text(projectName).font(.system(size: 13.5, weight: .semibold)).foregroundStyle(tint).lineLimit(1)
                    EngineLogo(engine: thread.engine, size: 11)
                    Spacer(minLength: 6)
                    machine
                    state
                }
                HStack(spacing: 8) {
                    StatusDot(state: thread.dot)
                    Text(titled(thread.title.isEmpty ? "New thread" : thread.title))
                        .font(.system(size: 15, weight: thread.unread ? .semibold : .regular)).foregroundStyle(Hub.text).lineLimit(1)
                    if thread.pinned_at != nil { Image(systemName: "pin.fill").font(.system(size: 9)).foregroundStyle(Hub.muted) }
                    Spacer(minLength: 0)
                }
            }
            .padding(.horizontal, 12).padding(.vertical, 9)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
    }

    private var machine: some View {
        let name = thread.host ?? store.machine ?? ""
        // In the cloud the icon alone stands where a machine's name would; the model beside it says whose.
        return HStack(spacing: 4) {
            Image(systemName: thread.cloud == nil ? MachineIcon.name(thread.host, store: store) : "icloud.and.arrow.up").font(.system(size: 10))
            if thread.cloud == nil { Text(name).font(.system(size: 11)).lineLimit(1) }
        }
        .accessibilityLabel(thread.cloud.map { "Runs in the \($0.providerName) cloud" } ?? name)
        .foregroundStyle(Hub.muted).padding(.horizontal, 7).padding(.vertical, 2)
        .background(RoundedRectangle(cornerRadius: 6).fill(Hub.surface2))
    }

    /// Needs you: how long it has waited, amber. Working: the timer, green. Done and unread: "Done" in
    /// violet. Otherwise when it last said something.
    @ViewBuilder private var state: some View {
        switch thread.status {
        case .waiting_on_you:
            Text(relative(thread.returned_at ?? thread.updated_at)).font(.system(size: 12)).foregroundStyle(Hub.amber)
        case .running, .in_terminal:
            if let start = thread.sent_at {
                TimelineView(.periodic(from: .now, by: 1)) { ctx in
                    Text(Self.elapsed(since: start, now: ctx.date)).font(.system(size: 12).monospacedDigit()).foregroundStyle(Hub.green)
                }
            } else { Text("now").font(.system(size: 12)).foregroundStyle(Hub.green) }
        case .error:
            Text("Failed").font(.system(size: 12)).foregroundStyle(Hub.red)
        case .idle where thread.sideBusy != nil:
            // Its own turn is over, a side job still works: the timer runs from the side job's start.
            TimelineView(.periodic(from: .now, by: 1)) { ctx in
                Text("Side job · \(Self.elapsed(since: thread.sideBusy ?? 0, now: ctx.date))").font(.system(size: 12).monospacedDigit()).foregroundStyle(Hub.green)
            }
        case .idle:
            if thread.unread { Text("Done").font(.system(size: 12)).foregroundStyle(Hub.accent) }
            else { Text(relative(thread.returned_at ?? thread.updated_at)).font(.system(size: 12)).foregroundStyle(Hub.muted) }
        }
    }

    static func elapsed(since ms: Double, now: Date) -> String {
        let s = max(0, Int(now.timeIntervalSince1970 - ms / 1000))
        return s >= 3600 ? "\(s / 3600)h \(s % 3600 / 60)m" : "\(s / 60)m \(s % 60)s"
    }
}

// MARK: The night

/// A moon from a thin crescent (fill 0) to nearly full (fill 1).
struct Crescent: View {
    var size: CGFloat
    var fill: CGFloat
    var body: some View {
        ZStack {
            Circle().fill(Color(hex: 0xf3dfa8))
            Circle().fill(.black).frame(width: size * 0.92, height: size * 0.92)
                .offset(x: size * 0.46 * (1 - min(1, max(0, fill))), y: -size * 0.06 * (1 - min(1, max(0, fill))))
                .blendMode(.destinationOut)
        }
        .compositingGroup().frame(width: size, height: size)
    }
}

/// Stars that twinkle, at fixed places.
struct StarSky: View {
    private static let stars: [(x: CGFloat, y: CGFloat, r: CGFloat, phase: Double, speed: Double)] = {
        var seed: UInt64 = 7
        func next() -> Double { seed = seed &* 6364136223846793005 &+ 1442695040888963407; return Double(seed >> 33) / Double(1 << 31) }
        return (0..<26).map { _ in (CGFloat(next()), CGFloat(next()), CGFloat(0.6 + next() * 1.3), next() * 6.28, 1.2 + next() * 2.4) }
    }()

    var body: some View {
        TimelineView(.animation) { ctx in
            Canvas { g, size in
                let t = ctx.date.timeIntervalSinceReferenceDate
                for s in Self.stars {
                    let a = 0.35 + 0.65 * (0.5 + 0.5 * sin(t * s.speed + s.phase))
                    let rect = CGRect(x: s.x * size.width - s.r, y: s.y * size.height - s.r, width: s.r * 2, height: s.r * 2)
                    g.fill(Path(ellipseIn: rect), with: .color(.white.opacity(a)))
                }
            }
        }
        .allowsHitTesting(false)
    }
}

/// A shooting star across the sky, once.
struct ShootingStar: View {
    @State private var go = false
    var body: some View {
        GeometryReader { g in
            Capsule()
                .fill(LinearGradient(colors: [.white.opacity(0), .white], startPoint: .leading, endPoint: .trailing))
                .frame(width: 46, height: 2)
                .rotationEffect(.degrees(24))
                .offset(x: go ? g.size.width + 10 : -60, y: go ? g.size.height * 0.8 : 2)
                .opacity(go ? 0.0 : 1)
                .animation(.easeIn(duration: 0.7), value: go)
        }
        .onAppear { go = true }
        .allowsHitTesting(false)
    }
}

/// What shows in the strip a row leaves bare when it is pulled to the left: the night. The strip is exactly
/// `reveal` wide, so nothing is ever drawn behind the row itself. One layout the whole way, so no text ever
/// swaps while you pull: the three nights sit in the strip's right end and are uncovered as it widens (the
/// one furthest right first, as iOS does); once all three show, a moon grows in the sky to their left. Pull
/// past the line (`armed`, decided by the row with some slack so it never flickers) and the first night
/// grows over the whole strip: let go and that is the one chosen. Nothing else changes its words.
struct NightSky: View {
    let reveal: CGFloat
    let width: CGFloat
    let choices: [(label: String, sub: String, until: Double)]
    let moon: Bool
    let shoot: Int
    let armed: Bool
    /// The night just chosen, whose moon falls asleep.
    var asleep: Double?
    let pick: (Double) -> Void

    static let tile: CGFloat = 72
    private var tilesWidth: CGFloat { CGFloat(max(1, choices.count)) * Self.tile }
    /// The open sky to the left of the nights, where the moon grows.
    private var sky: CGFloat { armed ? 0 : max(0, reveal - tilesWidth) }

    var body: some View {
        ZStack(alignment: .trailing) {
            LinearGradient(colors: [Color(hex: 0x0c1230), Color(hex: 0x1b2352)], startPoint: .top, endPoint: .bottom)
            if moon { StarSky() }
            if moon && shoot > 0 { ShootingStar().id(shoot) }
            HStack(spacing: 0) {
                ZStack {
                    if moon && sky >= 40 {
                        let grow = min(1, (sky - 40) / 120)
                        Crescent(size: 18 + 14 * grow, fill: grow)
                    }
                }
                .frame(width: sky)
                ForEach(Array(choices.enumerated()), id: \.offset) { i, c in
                    Button { pick(c.until) } label: {
                        VStack(spacing: 2) {
                            Image(systemName: asleep == c.until ? "moon.zzz.fill" : "moon.fill").font(.system(size: 12)).foregroundStyle(Color(hex: 0xf3dfa8))
                                .contentTransition(.symbolEffect(.replace))
                                .symbolEffect(.bounce, value: asleep == c.until)
                                .scaleEffect(asleep == c.until ? 1.8 : armed && i == 0 ? 1.5 : 1)
                            Text(c.label).font(.system(size: 14, weight: .semibold))
                            Text(c.sub).font(.system(size: 10.5)).foregroundStyle(.white.opacity(0.65))
                        }
                        .fixedSize()
                        .foregroundStyle(.white)
                        .frame(width: tileWidth(i)).frame(maxHeight: .infinity)
                        .background(armed && i == 0 ? Color.white.opacity(0.08) : .clear)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .opacity(armed && i > 0 ? 0 : 1)
                    .clipped()
                }
            }
            .fixedSize(horizontal: true, vertical: false)
            .animation(.snappy(duration: 0.22), value: armed)
        }
        .frame(width: max(0, reveal), alignment: .trailing)
        .clipped()
    }

    /// Each night is a fixed tile; armed, the first one takes the whole strip and the others fold away.
    private func tileWidth(_ i: Int) -> CGFloat {
        guard armed else { return Self.tile }
        return i == 0 ? max(Self.tile, reveal) : 0
    }
}

// MARK: The swipe

/// A row that settles on a swipe to the right and snoozes on a swipe to the left (the night swipe).
struct NightSwipeRow<Content: View>: View {
    @Environment(Store.self) private var store
    let moon: Bool
    var enabled = true
    let onSettle: () -> Void
    let onSnooze: (Double) -> Void
    let onTap: () -> Void
    let onHold: () -> Void
    @ViewBuilder var content: () -> Content

    @State private var dx: CGFloat = 0
    @State private var width: CGFloat = 390
    @State private var locked = false
    @State private var open = false
    @State private var cancelled = false
    @State private var shoot = 0
    @State private var hold: Task<Void, Never>?
    @State private var lastDx: CGFloat = 0
    @State private var armed = false
    @State private var settleArmed = false
    @State private var pressing = false
    /// Let go past the line: the check is drawn on the strip before the row flies out.
    @State private var checked = false
    @State private var drawn = false
    /// The night chosen: its moon falls asleep before the row flies out.
    @State private var asleep: Double?
    /// The row is on its way out: the strip behind it fades with it, so nothing is left to be cut off.
    @State private var leaving = false

    /// Where the row rests when it is left open: exactly the three nights.
    private var tiles: CGFloat { CGFloat(max(1, store.snoozeChoices.count)) * NightSky.tile }
    /// How far a pull to the left must go before letting go takes the first night, and how far back it
    /// must come before that is undone again: the gap keeps a hand that shakes on the line from flickering.
    private var armOn: CGFloat { min(width - 24, max(tiles + 72, width * 0.72)) }
    private var armOff: CGFloat { armOn - 28 }
    private var settleAt: CGFloat { 90 }

    var body: some View {
        content()
            .background(Hub.surface)
            // Held, the card lightens and sinks a little (sketch "Känsla", 1); not while it is swiped.
            .pressed(pressing && dx == 0 && !open)
            .offset(x: dx)
            .safeTap { if open { close() } else { onTap() } }
            .onLongPressGesture(minimumDuration: 0.4, maximumDistance: 10) {
                if !open && dx == 0 { onHold() }
            } onPressingChanged: { pressing = $0 }
            .background(alignment: .leading) {
                if dx > 0 {
                    ZStack(alignment: .leading) {
                        Color(hex: 0x5f9f77).opacity(settleArmed || checked ? 1 : 0.55)
                        if checked {
                            CheckMark().trim(from: 0, to: drawn ? 1 : 0)
                                .stroke(.white, style: StrokeStyle(lineWidth: 3.2, lineCap: .round, lineJoin: .round))
                                .frame(width: 26, height: 26).padding(.leading, 20)
                                .scaleEffect(drawn ? 1 : 0.7, anchor: .center)
                        } else {
                            Label("Settle", systemImage: "checkmark").font(.system(size: 14, weight: .semibold)).foregroundStyle(.white)
                                .padding(.leading, 18).fixedSize()
                                .scaleEffect(settleArmed ? 1.08 : 1, anchor: .leading)
                        }
                    }
                    .animation(.snappy(duration: 0.18), value: settleArmed)
                    .frame(width: dx)
                    .clipped()
                    .opacity(leaving ? 0 : 1)
                }
            }
            .background(alignment: .trailing) {
                if dx < 0 {
                    NightSky(reveal: -dx, width: width, choices: store.snoozeChoices, moon: moon, shoot: shoot, armed: armed, asleep: asleep) { until in commitSnooze(until) }
                        .opacity(leaving ? 0 : 1)
                }
            }
            .background(GeometryReader { g in Color.clear.onAppear { width = g.size.width }.onChange(of: g.size.width) { width = g.size.width } })
            .clipped()
            .simultaneousGesture(enabled ? drag : nil)
    }

    private var drag: some Gesture {
        DragGesture(minimumDistance: 14)
            .onChanged { v in
                if !locked {
                    guard abs(v.translation.width) > abs(v.translation.height) * 1.4 else { return }
                    locked = true; cancelled = false; lastDx = dx
                    Haptics.ready()
                    startHold()
                }
                guard !cancelled else { return }
                let base: CGFloat = open ? -tiles : 0
                let next = min(width, max(-width, base + v.translation.width))
                if abs(next - lastDx) > 6 { lastDx = next; startHold() }
                dx = next
                // The lines, with slack both ways and a tick when one is crossed, so you feel when letting go
                // will act.
                let pull = -next
                let nowArmed = armed ? pull > armOff : pull > armOn
                if nowArmed != armed { armed = nowArmed; Haptics.tick() }
                let nowSettle = settleArmed ? next > settleAt - 20 : next > settleAt
                if nowSettle != settleArmed { settleArmed = nowSettle; Haptics.tick() }
            }
            .onEnded { _ in
                if locked { TouchGuard.swiped() }
                hold?.cancel()
                let wasCancelled = cancelled
                let wasArmed = armed, wasSettle = settleArmed
                // A pull that was past the line stays drawn that way while the row flies out, so the words
                // do not change under your thumb at the end; commitSnooze clears it.
                locked = false; cancelled = false; settleArmed = false
                if !wasArmed || wasCancelled { armed = false }
                if wasCancelled { return }
                if wasSettle { commitSettle() }
                else if wasArmed { commitSnooze(store.snoozeChoices.first?.until) }
                else if dx < -60 { withAnimation(.snappy) { dx = -tiles; open = true } }
                else { close() }
            }
    }

    private func close() { withAnimation(.snappy) { dx = 0; open = false; armed = false } }

    /// Holding the swipe still for about a second and a half is the shooting star: no night is chosen
    /// and the row springs back.
    private func startHold() {
        hold?.cancel()
        hold = Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(1500))
            // Not past the line: there you have chosen, and a pause before letting go must not undo it.
            guard !Task.isCancelled, locked, dx < -40, !armed, moon else { return }
            cancelled = true
            shoot += 1
            Haptics.tap()
            try? await Task.sleep(for: .milliseconds(750))
            withAnimation(.spring(response: 0.4, dampingFraction: 0.72)) { dx = 0; open = false }
        }
    }

    /// The strip holds while a check is drawn on it (sketch "Känsla", 2), then the row flies out.
    private func commitSettle() {
        checked = true
        withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) { dx = max(dx, min(width * 0.46, 180)) }
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(120))
            withAnimation(.spring(response: 0.28, dampingFraction: 0.55)) { drawn = true }
            Haptics.done()
            try? await Task.sleep(for: .milliseconds(380))
            withAnimation(.easeIn(duration: 0.28)) { dx = width; leaving = true }
            try? await Task.sleep(for: .milliseconds(300))
            onSettle()
            // Gone by now; if it is still listed (the tidy was refused) it comes back gently, never in a jump.
            try? await Task.sleep(for: .milliseconds(80))
            withAnimation(.snappy) { dx = 0; open = false; checked = false; drawn = false; leaving = false }
        }
    }

    private func commitSnooze(_ until: Double?) {
        guard let until else { close(); return }
        // The chosen night's moon falls asleep (sketch "Känsla", 3), then the row glides out.
        withAnimation(.spring(response: 0.3, dampingFraction: 0.6)) { asleep = until }
        Haptics.snooze()
        Task { @MainActor in
            // Long enough to see the moon fall asleep; then the row and the night fade out together.
            try? await Task.sleep(for: .milliseconds(700))
            withAnimation(.easeIn(duration: 0.3)) { dx = -width; leaving = true }
            try? await Task.sleep(for: .milliseconds(320))
            onSnooze(until)
            try? await Task.sleep(for: .milliseconds(80))
            withAnimation(.snappy) { dx = 0; open = false; armed = false; asleep = nil; leaving = false }
        }
    }
}


/// A check drawn as one stroke, for the settle strip.
struct CheckMark: Shape {
    func path(in r: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: r.minX + r.width * 0.16, y: r.minY + r.height * 0.53))
        p.addLine(to: CGPoint(x: r.minX + r.width * 0.40, y: r.minY + r.height * 0.75))
        p.addLine(to: CGPoint(x: r.minX + r.width * 0.84, y: r.minY + r.height * 0.28))
        return p
    }
}
