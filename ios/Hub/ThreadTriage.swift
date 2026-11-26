import SwiftUI
import UIKit

// Going through the inbox from inside a thread, like TikTok. A swipe to the right settles the thread and
// opens the next one; a swipe to the left snoozes it (the first of the snooze choices, an hour) and opens
// the next; pulling up past the end of the feed opens the next one without doing anything to this one.
// The left edge stays the system's back to the inbox. Each gesture has a line: past it the finger feels
// a tick and the strip turns solid, letting go there does it, letting go before springs back.

/// The strip a swipe to the right draws from the left edge.
struct SettleStrip: View {
    let width: CGFloat
    let armed: Bool
    var body: some View {
        ZStack(alignment: .leading) {
            Color(hex: 0x5f9f77).opacity(armed ? 1 : 0.4)
            if width > 60 {
                Label("Settle", systemImage: "checkmark").font(.system(size: 15, weight: .semibold)).foregroundStyle(.white)
                    .padding(.leading, 20).fixedSize().opacity(armed ? 1 : 0.8)
            }
        }
        .frame(width: max(0, width)).clipped()
        .animation(.easeOut(duration: 0.12), value: armed)
    }
}

/// The strip a swipe to the left draws from the right edge: the night, with the moon and what snoozing does.
struct SnoozeStrip: View {
    let width: CGFloat
    let armed: Bool
    let choice: (label: String, sub: String, until: Double)?

    var body: some View {
        ZStack {
            LinearGradient(colors: [Color(hex: 0x0c1230), Color(hex: 0x1b2352)], startPoint: .top, endPoint: .bottom)
            StarSky()
            if width > 70 {
                VStack(spacing: 6) {
                    Crescent(size: armed ? 36 : 26, fill: armed ? 0.85 : 0.3)
                    if let c = choice {
                        VStack(spacing: 1) {
                            Text("Snooze \(c.label)").font(.system(size: 14, weight: .semibold))
                            Text("until \(c.sub)").font(.system(size: 11)).foregroundStyle(.white.opacity(0.7))
                        }
                        .foregroundStyle(.white).opacity(armed ? 1 : 0.75)
                    }
                }
                .fixedSize()
            }
        }
        .frame(width: max(0, width)).clipped()
        .animation(.easeOut(duration: 0.12), value: armed)
    }
}

/// The swipes in an open thread.
struct ThreadTriage: ViewModifier {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    let thread: HubThread?
    let dismiss: () -> Void

    @State private var dx: CGFloat = 0
    @State private var locked = false
    @State private var armed = false

    /// How far the finger goes before letting go does it, and how far back before it is undone again.
    private let line: CGFloat = 110
    private let hysteresis: CGFloat = 20
    /// The page follows the finger with resistance.
    private var shown: CGFloat { dx * 0.62 }

    func body(content: Content) -> some View {
        content
            .offset(x: shown)
            .background(alignment: .leading) { if dx > 0 { SettleStrip(width: shown, armed: armed) } }
            .background(alignment: .trailing) { if dx < 0 { SnoozeStrip(width: -shown, armed: armed, choice: store.snoozeChoices.first) } }
            .simultaneousGesture(thread == nil ? nil : drag)
    }

    private var drag: some Gesture {
        DragGesture(minimumDistance: 16, coordinateSpace: .global)
            .onChanged { v in
                if !locked {
                    // From the left edge it is the system's back; only a clearly sideways drag counts.
                    // A drag that starts on a strip or code block that scrolls sideways, or in the box, is theirs.
                    guard v.startLocation.x > 24, !SwipeZones.holds(v.startLocation),
                          abs(v.translation.width) > abs(v.translation.height) * 1.4 else { return }
                    locked = true
                    Haptics.ready()
                    Perf.note("thread swipe starts")
                }
                dx = v.translation.width
                let reached = abs(dx) > (armed ? line - hysteresis : line)
                if reached != armed {
                    armed = reached
                    Haptics.tick()
                }
            }
            .onEnded { _ in
                let done = locked && armed
                if locked { TouchGuard.swiped(); Perf.note("thread swipe ends\(done ? ", acts" : ", springs back")") }
                let direction = dx
                locked = false; armed = false
                guard done, let t = thread else { withAnimation(.snappy) { dx = 0 }; return }
                if direction > 0 { go(t, settle: true) } else { go(t, settle: false) }
            }
    }

    /// Settle or snooze the thread and open the next one; with none left, back to the inbox.
    private func go(_ t: HubThread, settle: Bool) {
        let next = store.nextThread(after: t.id)
        if settle { store.tidy([t], .settle) }
        else if let first = store.snoozeChoices.first { store.tidy([t], .snooze(until: first.until)) }
        else { withAnimation(.snappy) { dx = 0 }; return }
        dx = 0
        if let next { nav.replaceTop(.thread(next.id)) } else { dismiss() }
    }
}

/// The next thread's card, peeking up from the bottom while you pull past the end of the feed.
struct NextPeek: View {
    @Environment(Store.self) private var store
    let thread: HubThread
    let armed: Bool

    private var tint: Color { thread.isQuick ? Hub.accent : Hub.projectColor(thread.project) }

    var body: some View {
        HStack(spacing: 0) {
            Rectangle().fill(tint).frame(width: 3)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.up").font(.system(size: 10, weight: .bold)).foregroundStyle(armed ? Hub.accent : Hub.muted)
                    Text(armed ? "Let go for the next thread" : "Next thread").font(.system(size: 11.5)).foregroundStyle(armed ? Hub.accent : Hub.muted)
                    Spacer()
                }
                HStack(spacing: 8) {
                    Text(thread.isQuick ? "Quick" : thread.project).font(.system(size: 13, weight: .semibold)).foregroundStyle(tint).lineLimit(1)
                    StatusDot(state: thread.dot)
                    Text(thread.title.isEmpty ? "New thread" : thread.title).font(.system(size: 14.5)).foregroundStyle(Hub.text).lineLimit(1)
                    Spacer(minLength: 0)
                }
            }
            .padding(.horizontal, 12).padding(.vertical, 8)
        }
        .frame(height: 64)
        .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
        .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(armed ? Hub.accent.opacity(0.6) : Hub.lineStrong))
        .clipShape(RoundedRectangle(cornerRadius: 14))
        .padding(.horizontal, 12)
    }
}
