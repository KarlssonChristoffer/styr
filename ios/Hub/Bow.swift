import SwiftUI
import UIKit

// How Hub's own short sheets close by a pull down: like drawing a bow. A drag that scrolls the list is
// only a scroll, also when it brings the list back to its top. A fresh pull down while the list rests at
// its top draws the sheet down with growing resistance (it moves less and less the further you pull);
// past the line you feel a tick, and letting go there, or a flick down, throws it away. Let go before
// the line and it springs back. The list under it does not stretch (`NoBounce`), so only the sheet moves.

extension View {
    /// Keeps `atTop` true while this scroll view rests at its top.
    func trackTop(_ atTop: Binding<Bool>) -> some View {
        onScrollGeometryChange(for: Bool.self) { g in
            g.contentOffset.y + g.contentInsets.top <= 1
        } action: { _, top in
            atTop.wrappedValue = top
        }
    }

    /// Closes a sheet on a deliberate pull down only (see the note at the top of Bow.swift). `atTop`
    /// says whether its list rests at its top; a sheet without a list passes true.
    func bowDismiss(atTop: Bool, close: @escaping () -> Void) -> some View {
        modifier(BowDismiss(atTop: atTop, close: close))
    }
}

/// Turns off the rubber band of the scroll view it sits in, so a pull past the list's top moves only the
/// sheet: with the band on, the list stretched down under the thumb while the sheet moved too, and the two
/// fought (it jittered). Put it in the scroll view's content, as a background.
struct NoBounce: UIViewRepresentable {
    func makeUIView(context: Context) -> UIView {
        let probe = Probe()
        probe.isUserInteractionEnabled = false
        return probe
    }
    func updateUIView(_ uiView: UIView, context: Context) {}

    final class Probe: UIView {
        override func didMoveToWindow() {
            super.didMoveToWindow()
            DispatchQueue.main.async { [weak self] in
                var view = self?.superview
                while let v = view, !(v is UIScrollView) { view = v.superview }
                guard let scroll = view as? UIScrollView else { return }
                scroll.bounces = false
                scroll.alwaysBounceVertical = false
            }
        }
    }
}

struct BowDismiss: ViewModifier {
    let atTop: Bool
    let close: () -> Void

    /// How far the finger must pull before letting go closes the sheet.
    static let line: CGFloat = 130
    /// How far the sheet itself can move at most, however far you pull.
    static let reach: CGFloat = 110

    @State private var pull: CGFloat = 0
    /// Decided once per drag: nil before it is known, true for a pull that draws the bow.
    @State private var drawing: Bool?
    @State private var drawn = false

    func body(content: Content) -> some View {
        content
            .offset(y: Self.give(pull))
            .simultaneousGesture(
                DragGesture(minimumDistance: 8, coordinateSpace: .global)
                    .onChanged { v in
                        if drawing == nil {
                            // Only a pull that starts down, mostly straight, with the list already at its
                            // top. Anything else is a scroll and stays one for the whole drag.
                            drawing = atTop && v.translation.height > 0 && v.translation.height > abs(v.translation.width)
                        }
                        guard drawing == true else { return }
                        if pull == 0 { Haptics.ready(); Perf.note("sheet pulled down") }
                        pull = max(0, v.translation.height)
                        let now = pull > Self.line
                        if now != drawn {
                            drawn = now
                            if now { Haptics.firm() } else { Haptics.gentle() }
                        }
                    }
                    .onEnded { v in
                        let was = drawing == true
                        drawing = nil
                        drawn = false
                        guard was else { return }
                        TouchGuard.swiped()
                        let flick = v.predictedEndTranslation.height - v.translation.height > 240
                        if pull > Self.line || (flick && pull > 30) {
                            close()
                            pull = 0
                        } else {
                            withAnimation(.spring(response: 0.38, dampingFraction: 0.62)) { pull = 0 }
                        }
                    }
            )
    }

    /// The bow's give: close to the finger at first, then stiffer, never past `reach`.
    static func give(_ t: CGFloat) -> CGFloat {
        guard t > 0 else { return 0 }
        return reach * (1 - exp(-t / (reach * 1.6)))
    }
}
