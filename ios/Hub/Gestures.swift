import SwiftUI
import UIKit

/// The pages draw their own top bar (Hub's look, not Apple's), and hiding the system bar also
/// turns off the swipe from the left edge that goes back. This turns it on again, everywhere.
///
/// Taking over the swipe's delegate also takes away what UIKit's own delegate did to let it win, so
/// that is done here too. Without it the edge swipe had to race everything else under the finger and
/// often lost (it took many tries to go back): the page's scroll view started scrolling first, and
/// the pages' own sideways drags (settle, quote, the mode pager), which leave the edge alone in
/// their logic but still claimed the touch, cancelled it.
extension UINavigationController: @retroactive UIGestureRecognizerDelegate {
    override open func viewDidLoad() {
        super.viewDidLoad()
        interactivePopGestureRecognizer?.delegate = self
    }

    public func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        viewControllers.count > 1
    }

    /// The pages' own gestures run beside the edge swipe instead of cancelling it; each of them
    /// leaves a drag that starts at the edge alone. A scroll view waits instead (below).
    public func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        g == interactivePopGestureRecognizer && !(other.view is UIScrollView)
    }

    /// A scroll view waits until the edge swipe has failed, as with UIKit's own delegate: a drag that
    /// starts at the edge and goes sideways is back, one that goes up or down still scrolls.
    public func gestureRecognizer(_ g: UIGestureRecognizer, shouldBeRequiredToFailBy other: UIGestureRecognizer) -> Bool {
        g == interactivePopGestureRecognizer && other.view is UIScrollView
    }
}

/// Places on a page where a sideways drag belongs to something else: a code block or a row of chips
/// that scrolls, the machine line on a new thread that changes machine, the reply box. The swipes that
/// work anywhere on a page (`SwipeNav`, `ThreadTriage`) leave a drag that starts in one alone, so reading
/// code to the left and back to the right no longer closes the page.
enum SwipeZones {
    private static var frames: [UUID: CGRect] = [:]
    private static var asleep: Set<UUID> = []
    static func set(_ id: UUID, _ frame: CGRect) { frames[id] = frame }
    static func setActive(_ id: UUID, _ active: Bool) { if active { asleep.remove(id) } else { asleep.insert(id) } }
    static func remove(_ id: UUID) { frames[id] = nil; asleep.remove(id) }
    /// Whether a point (in the window's coordinates) lies in a zone that is on.
    static func holds(_ point: CGPoint) -> Bool { frames.contains { !asleep.contains($0.key) && $0.value.contains(point) } }
}

/// A page inside a mode of a page (Files, one folder deep) takes a swipe to the right itself: it goes up
/// a folder. The mode pager then leaves a swipe to the right alone.
enum PageSwipe {
    static var rightGoesUp = false
}

struct SwipeZone: ViewModifier {
    var active = true
    @State private var id = UUID()

    func body(content: Content) -> some View {
        content
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { SwipeZones.set(id, $0) }
            .onChange(of: active, initial: true) { SwipeZones.setActive(id, active) }
            .onDisappear { SwipeZones.remove(id) }
    }
}

extension View {
    /// Marks this view as one whose sideways drags are its own: the page's swipes do not start in it.
    /// `active: false` takes the mark away again (a page that sleeps until tapped).
    func swipeZone(_ active: Bool = true) -> some View { modifier(SwipeZone(active: active)) }
}

/// A sideways swipe anywhere on a page (sketch "Gester" 1 and 2): right goes back, not only from
/// the edge; left goes on to the next thread where there is one. It only counts when clearly
/// sideways, so scrolling is left alone, and the page follows the finger a little. A drag that starts
/// in a `swipeZone` is not ours.
struct SwipeNav: ViewModifier {
    let back: () -> Void
    /// Off on the thread: a swipe to the right there quotes a message, and the edge swipe goes back.
    /// Also off while a sheet stands over the page, so a sideways swipe on the dimmed page does not
    /// close the whole page.
    var backEnabled = true
    var next: (() -> Void)?
    @State private var offset: CGFloat = 0
    /// Decided once per drag: nil before it is known, false when it started in a zone.
    @State private var ours: Bool?

    func body(content: Content) -> some View {
        content
            .offset(x: offset)
            .simultaneousGesture(
                DragGesture(minimumDistance: 24, coordinateSpace: .global)
                    .onChanged { v in
                        if ours == nil { ours = v.startLocation.x > 24 && !SwipeZones.holds(v.startLocation) }
                        guard ours == true, abs(v.translation.width) > abs(v.translation.height) * 1.5 else { return }
                        let canGo = (v.translation.width > 0 && backEnabled) || (v.translation.width < 0 && next != nil)
                        offset = v.translation.width * (canGo ? 0.4 : 0.08)
                    }
                    .onEnded { v in
                        let mine = ours == true
                        ours = nil
                        defer { withAnimation(.snappy) { offset = 0 } }
                        guard mine, abs(v.translation.width) > abs(v.translation.height) * 1.5,
                              abs(v.predictedEndTranslation.width) > 110 else { return }
                        if v.translation.width > 0 { if backEnabled { Haptics.tap(); back() } }
                        else if let next { Haptics.tap(); next() }
                    }
            )
    }
}

extension View {
    /// Swipe right anywhere to go back (`enabled: false` while something stands over the page).
    func swipeBack(enabled: Bool = true, _ back: @escaping () -> Void) -> some View { modifier(SwipeNav(back: back, backEnabled: enabled)) }
}

/// In a chat a swipe to the right anywhere is back (a dev thread uses it to settle, see `ThreadTriage`).
struct ChatBack: ViewModifier {
    let on: Bool
    let back: () -> Void

    @ViewBuilder func body(content: Content) -> some View {
        if on { content.swipeBack(back) } else { content }
    }
}
