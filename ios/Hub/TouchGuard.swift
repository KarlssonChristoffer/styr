import SwiftUI
import UIKit

// Telling a tap from a touch that only stopped or moved something, the way iOS's own lists do.
//
// A touch that lands while a list still glides only stops the list; it does not open the row under the
// finger. A touch that travels more than a few points is a scroll or a swipe, never a tap. And a touch
// right after one of Hub's own swipes (a row swiped away, a page swiped sideways) is part of that swipe.
// `TouchWatch` sits on the window and sees every touch without taking any; rows ask `TouchGuard.ok`
// before they act (`safeTap`, `PanelRow`).

@MainActor
enum TouchGuard {
    /// This touch began while a list glided or right after a swipe of ours.
    fileprivate(set) static var stopped = false
    /// This touch went further than a tap may.
    fileprivate(set) static var travelled = false
    private static var swipedAt = Date.distantPast

    /// How far a finger may drift and still tap, and how long after a swipe a touch still belongs to it.
    static let slop: CGFloat = 10
    static let settle: TimeInterval = 0.3

    /// Whether the touch that just ended may act as a tap.
    static var ok: Bool { !stopped && !travelled }

    /// One of Hub's own swipes ended or claimed the finger; a touch in the next moment is part of it.
    static func swiped() { swipedAt = Date() }

    fileprivate static func began(stopping: Bool) {
        stopped = stopping || Date().timeIntervalSince(swipedAt) < settle
        travelled = false
    }

    fileprivate static func moved(_ distance: CGFloat) {
        if distance > slop { travelled = true }
    }
}

extension View {
    /// `onTapGesture` that does nothing for a touch that stopped a list, came right after a swipe or drifted.
    func safeTap(count: Int = 1, perform action: @escaping () -> Void) -> some View {
        onTapGesture(count: count) { if TouchGuard.ok { action() } }
    }

    /// A row or card you tap: a button that answers the press (`PressStyle`) and acts only on a real tap.
    func tapRow(flat: Bool = false, perform action: @escaping () -> Void) -> some View {
        Button { if TouchGuard.ok { action() } } label: { self }
            .buttonStyle(PressStyle(sink: !flat))
    }

    /// Puts the touch watcher on the window this view is in (once).
    func watchesTouches() -> some View { background(TouchWatch.Installer().allowsHitTesting(false)) }
}

/// Sees every touch on the window. It never takes one, except a touch that lands on a gliding list: that
/// touch is claimed so that only the list (which stops) has it, as in iOS's own lists.
final class TouchWatch: UIGestureRecognizer, UIGestureRecognizerDelegate {
    private weak var first: UITouch?
    private var start: CGPoint = .zero

    override init(target: Any?, action: Selector?) {
        super.init(target: target, action: action)
        cancelsTouchesInView = true
        delaysTouchesBegan = false
        delaysTouchesEnded = false
        delegate = self
    }

    /// Nothing shuts the watcher out, so it sees each touch to its end.
    override func canBePrevented(by preventing: UIGestureRecognizer) -> Bool { false }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesBegan(touches, with: event)
        guard first == nil, let touch = touches.first else { return }
        first = touch
        start = touch.location(in: nil)
        let gliding = Self.gliding(above: touch.view)
        MainActor.assumeIsolated { TouchGuard.began(stopping: gliding) }
        // Claim the touch so the row under it does not get it; the list's own pan goes on with it.
        if gliding { state = .began }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesMoved(touches, with: event)
        guard let touch = first, touches.contains(touch) else { return }
        let p = touch.location(in: nil)
        let d = hypot(p.x - start.x, p.y - start.y)
        MainActor.assumeIsolated { TouchGuard.moved(d) }
        if state == .began { state = .changed }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesEnded(touches, with: event)
        guard let touch = first, touches.contains(touch) else { return }
        finish(ended: true)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesCancelled(touches, with: event)
        guard let touch = first, touches.contains(touch) else { return }
        finish(ended: false)
    }

    private func finish(ended: Bool) {
        first = nil
        switch state {
        case .began, .changed: state = ended ? .ended : .cancelled
        default: state = .failed
        }
    }

    override func reset() {
        super.reset()
        first = nil
    }

    /// Only a claimed touch shuts other gestures out, and never the list's own pan and pinch.
    func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        other.view is UIScrollView
    }

    /// Whether a list above this view still glides (or is being dragged), so a touch on it only stops it.
    private static func gliding(above view: UIView?) -> Bool {
        var v = view
        while let current = v {
            if let scroll = current as? UIScrollView, scroll.isDecelerating { return true }
            v = current.superview
        }
        return false
    }

    /// A probe that puts one watcher on its window.
    struct Installer: UIViewRepresentable {
        func makeUIView(context: Context) -> UIView { Probe() }
        func updateUIView(_ uiView: UIView, context: Context) {}

        final class Probe: UIView {
            override func didMoveToWindow() {
                super.didMoveToWindow()
                guard let window, !(window.gestureRecognizers ?? []).contains(where: { $0 is TouchWatch }) else { return }
                window.addGestureRecognizer(TouchWatch(target: nil, action: nil))
            }
        }
    }
}
