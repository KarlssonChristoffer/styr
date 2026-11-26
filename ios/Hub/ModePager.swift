import SwiftUI

/// The modes of a page (a project's Focus, Threads, Ideas…; a pull request's Overview, Files…) as
/// panes you swipe between. Only the pane you are on is built; a neighbour is added while a finger
/// drags and while the page settles, so a page with five modes costs one pane at rest.
///
/// The pane follows the finger, a long or quick enough swipe goes on to the next mode and a short one
/// springs back. At the last mode it gives a little and springs back. A swipe to the right at the first
/// mode is not ours: the page's `swipeBack` takes it, so put `swipeBack(enabled: selection == 0)` on
/// the page. A drag that starts in a `swipeZone` (a code block, the mode pills) is left alone, and so
/// is one that starts in the 24 pt at the left edge, which the system's own back swipe takes.
///
/// While a swipe goes on and the page settles, the panes take no touches, so a row under the finger is not
/// opened by the swipe.
///
/// A mode chosen another way (a tap on a pill) slides in the same way, from the side it lies on. Panes
/// never fade: they come and go without a transition, so two never show through each other.
///
/// Each pane brings its own `ScrollView`, so every mode scrolls on its own. A pane that is built
/// again starts at its top.
struct ModePager<Page: View>: View {
    @Binding var selection: Int
    let count: Int
    @ViewBuilder let page: (Int) -> Page

    @State private var drag: CGFloat = 0
    /// True from the first move of a drag until the page has settled.
    @State private var settling = false
    /// Decided once per drag: nil before it is known, false when it is not ours.
    @State private var ours: Bool?
    /// The mode drawn. It follows `selection`, but a change from outside slides in instead of jumping.
    @State private var current: Int?
    /// The mode a slide started from, kept drawn until the page has settled.
    @State private var from: Int?

    var body: some View {
        GeometryReader { geo in
            let w = max(geo.size.width, 1)
            let here = min(max(current ?? selection, 0), max(count - 1, 0))
            ZStack(alignment: .topLeading) {
                ForEach(shown(here), id: \.self) { i in
                    page(i)
                        .frame(width: w, height: geo.size.height)
                        // A swipe never presses what is under the finger: from the first move of a drag
                        // until the page has settled the rows and buttons do not take touches.
                        .allowsHitTesting(!settling)
                        .offset(x: CGFloat(i - here) * w + drag)
                        .transition(.identity)
                }
            }
            .frame(width: w, height: geo.size.height, alignment: .topLeading)
            .clipped()
            .contentShape(Rectangle())
            .simultaneousGesture(
                DragGesture(minimumDistance: 20, coordinateSpace: .global)
                    .onChanged { v in changed(v, here: here, width: w) }
                    .onEnded { v in ended(v, here: here, width: w) }
            )
            .onAppear { if current == nil { current = selection } }
            .onChange(of: selection) { old, now in slide(from: current ?? old, to: now, width: w) }
        }
    }

    /// A mode chosen from outside (a pill): the new pane takes its place beside this one and glides home.
    private func slide(from was: Int, to now: Int, width: CGFloat) {
        guard now != was, now >= 0, now < count else { current = now; return }
        withTransaction(Transaction(animation: nil)) {
            from = was
            current = now
            drag += CGFloat(now - was) * width
            settling = true
        }
        settle()
    }

    /// The panes glide home; the neighbours go once the glide is over.
    private func settle() {
        withAnimation(.snappy, completionCriteria: .removed) {
            drag = 0
        } completion: {
            guard drag == 0, ours != true else { return }
            settling = false
            from = nil
        }
    }

    private func shown(_ here: Int) -> [Int] {
        guard count > 1, settling || drag != 0 else { return [here] }
        var list = [here - 1, here, here + 1]
        if let from, !list.contains(from) { list.append(from) }
        return list.filter { $0 >= 0 && $0 < count }
    }

    private func changed(_ v: DragGesture.Value, here: Int, width: CGFloat) {
        if ours == nil {
            let dx = v.translation.width, dy = v.translation.height
            // Sideways clearly, outside a zone and the left edge, and a way to go (the first mode's
            // swipe to the right is the page's back swipe).
            ours = count > 1
                && abs(dx) > abs(dy) * 1.5
                && v.startLocation.x > 24
                && !SwipeZones.holds(v.startLocation)
                && !(dx > 0 && here == 0)
                && !(dx > 0 && PageSwipe.rightGoesUp)
            if ours == true { settling = true }
        }
        guard ours == true else { return }
        let dx = v.translation.width
        let open = (dx > 0 && here > 0) || (dx < 0 && here < count - 1)
        drag = open ? dx : dx * 0.15
    }

    private func ended(_ v: DragGesture.Value, here: Int, width: CGFloat) {
        let mine = ours == true
        ours = nil
        if mine { TouchGuard.swiped() }
        guard mine else { return }
        let dx = v.translation.width
        let far = abs(dx) > width * 0.33 || abs(v.predictedEndTranslation.width) > width * 0.6
        let step = dx < 0 ? 1 : -1
        let next = here + step
        if far, next >= 0, next < count {
            // The new mode takes over where the finger left the pane, then glides home.
            let plain = Transaction(animation: nil)
            withTransaction(plain) {
                current = next
                selection = next
                drag += CGFloat(step) * width
            }
            Haptics.tick()
        }
        settle()
    }
}
