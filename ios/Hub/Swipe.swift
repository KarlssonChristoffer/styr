import SwiftUI

// Row swipes for lists that are not the inbox: Out of the inbox and the Chat list. The row follows the
// finger with resistance, a strip behind it says what letting go does, and past a line the strip turns
// solid with a tick (letting go before it springs back). The same shape as the thread swipes in the
// inbox, but each list brings its own two actions.

struct SwipeAction {
    let label: String
    let icon: String
    let tint: Color
    let run: () -> Void
}

struct StripSwipe<Content: View>: View {
    var right: SwipeAction?
    var left: SwipeAction?
    @ViewBuilder var content: () -> Content

    @State private var dx: CGFloat = 0
    @State private var locked = false
    @State private var armed = false
    /// Decided once per drag: nil before it is known, false when it is not ours.
    @State private var ours: Bool?

    private let line: CGFloat = 96
    private let hysteresis: CGFloat = 18
    private var shown: CGFloat { dx * 0.75 }

    var body: some View {
        ZStack {
            if shown > 0, let a = right { strip(a, width: shown, leading: true) }
            if shown < 0, let a = left { strip(a, width: -shown, leading: false) }
            content()
                .background(Hub.bg)
                .offset(x: shown)
                // A swipe never opens what is under the finger.
                .allowsHitTesting(!locked)
        }
        .contentShape(Rectangle())
        // The page's own sideways swipes (back, modes) stay out of a row that swipes.
        .swipeZone(right != nil || left != nil)
        .simultaneousGesture(drag)
    }

    private func strip(_ a: SwipeAction, width: CGFloat, leading: Bool) -> some View {
        ZStack {
            a.tint.opacity(armed ? 1 : 0.4)
            if width > 60 {
                Label(a.label, systemImage: a.icon).font(.system(size: 14, weight: .semibold)).foregroundStyle(.white)
                    .fixedSize().opacity(armed ? 1 : 0.8)
            }
        }
        .frame(width: max(0, width))
        .clipped()
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: leading ? .leading : .trailing)
        .animation(.easeOut(duration: 0.12), value: armed)
    }

    private var drag: some Gesture {
        DragGesture(minimumDistance: 16, coordinateSpace: .global)
            .onChanged { v in
                if ours == nil {
                    let dxv = v.translation.width
                    ours = v.startLocation.x > 24 && abs(dxv) > abs(v.translation.height) * 1.4
                        && ((dxv > 0 && right != nil) || (dxv < 0 && left != nil))
                    if ours == true { locked = true; Haptics.ready() }
                }
                guard ours == true else { return }
                let canGo = (v.translation.width > 0 && right != nil) || (v.translation.width < 0 && left != nil)
                dx = canGo ? v.translation.width : 0
                let reached = abs(shown) > (armed ? line - hysteresis : line)
                if reached != armed { armed = reached; Haptics.tick() }
            }
            .onEnded { _ in
                let mine = ours == true
                let go = mine && armed
                let side = dx
                ours = nil; armed = false
                guard mine else { return }
                TouchGuard.swiped()
                withAnimation(.snappy) { dx = 0 }
                locked = false
                guard go else { return }
                if side > 0 { right?.run() } else { left?.run() }
            }
    }
}
