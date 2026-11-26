import SwiftUI

// Switching between the two worlds, Dev and Chat. One app, one skeleton, two themes; the world
// button first in the pill (or a swipe down from the top edge) changes between them. Four
// animations to choose from under Settings > Appearance, each worked from a picture of the old
// world (`Snap`) and the new world drawn live in its own colours.

enum SwitchAnimation: String, CaseIterable {
    case slide, reveal, lift, cascade
}

/// What the setting stores: one animation for both gestures, or a split (A for a swipe, B for a tap).
enum SwitchStyle: String, CaseIterable, Identifiable {
    case split, slide, reveal, lift, cascade
    var id: String { rawValue }

    var title: String {
        switch self {
        case .split: return "Split · A for a swipe, B for a tap"
        case .slide: return "A · Slide sideways"
        case .reveal: return "B · Circle reveal"
        case .lift: return "C · Soft lift"
        case .cascade: return "D · Cascade"
        }
    }

    var detail: String {
        switch self {
        case .split: return "The default: the page slides when you swipe down from the top, a circle grows from the button when you tap it."
        case .slide: return "The new world comes in from the right (back from the left), the old one moves a little and darkens. Best with a swipe."
        case .reveal: return "A circle grows from the world button and repaints the page and the pill in the new theme. Best with a tap."
        case .lift: return "The new content fades in rising and sharpening while the old one fades out, shrinking and blurring. The calmest."
        case .cascade: return "The rows of the old page leave from the top down while the colours change, then the new rows come in the same order."
        }
    }

    static var current: SwitchStyle {
        get { SwitchStyle(rawValue: UserDefaults.standard.string(forKey: "switchStyle") ?? "") ?? .split }
        set { UserDefaults.standard.set(newValue.rawValue, forKey: "switchStyle") }
    }

    func animation(tap: Bool) -> SwitchAnimation {
        switch self {
        case .split: return tap ? .reveal : .slide
        case .slide: return .slide
        case .reveal: return .reveal
        case .lift: return .lift
        case .cascade: return .cascade
        }
    }
}

/// A picture of what is on screen, taken just before the world changes.
@MainActor
enum Snap {
    static func capture() -> UIImage? {
        guard let window = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).flatMap(\.windows).first(where: \.isKeyWindow) else { return nil }
        let r = UIGraphicsImageRenderer(size: window.bounds.size)
        return r.image { _ in window.drawHierarchy(in: window.bounds, afterScreenUpdates: false) }
    }
}

/// Draws the new world live and the old one as a picture, mixed by `progress` (0 to 1, animated).
struct WorldTransition<Content: View>: View, Animatable {
    var progress: CGFloat
    let snapshot: UIImage?
    let anim: SwitchAnimation
    let origin: CGPoint
    let forward: Bool
    let content: Content

    init(progress: CGFloat, snapshot: UIImage?, anim: SwitchAnimation, origin: CGPoint, forward: Bool, @ViewBuilder content: () -> Content) {
        self.progress = progress; self.snapshot = snapshot; self.anim = anim; self.origin = origin; self.forward = forward; self.content = content()
    }

    var animatableData: CGFloat {
        get { progress }
        set { progress = newValue }
    }

    /// The layers are always the same three (below, content, above), so the content keeps its place
    /// and its state when the transition is over.
    var body: some View {
        let size = UIScreen.main.bounds.size
        let w = size.width, h = size.height
        let active = snapshot != nil && progress < 1
        let p = progress
        let q = max(0, (p - 0.4) / 0.6)
        ZStack {
            Group {
                if active, let snapshot {
                    switch anim {
                    case .slide: old(snapshot).offset(x: (forward ? -1 : 1) * 0.28 * w * p).brightness(-0.45 * p)
                    case .reveal: old(snapshot)
                    case .cascade: Hub.bg.ignoresSafeArea()
                    case .lift: Color.clear
                    }
                }
            }
            content
                .offset(x: active && anim == .slide ? (forward ? 1 : -1) * w * (1 - p) : 0,
                        y: active ? (anim == .lift ? 40 * (1 - p) : anim == .cascade ? 14 * (1 - q) : 0) : 0)
                .opacity(active ? (anim == .lift ? p : anim == .cascade ? q : 1) : 1)
                .blur(radius: active && anim == .lift ? 6 * (1 - p) : 0)
                .shadow(color: .black.opacity(active && anim == .slide ? 0.35 * (1 - p) : 0), radius: 18)
                .mask {
                    if active && anim == .reveal {
                        let far = max(hypot(origin.x, origin.y), hypot(w - origin.x, origin.y), hypot(origin.x, h - origin.y), hypot(w - origin.x, h - origin.y))
                        Circle().frame(width: far * 2 * max(p, 0.001), height: far * 2 * max(p, 0.001)).position(x: origin.x, y: origin.y)
                            .frame(maxWidth: .infinity, maxHeight: .infinity).ignoresSafeArea()
                    } else {
                        Rectangle().ignoresSafeArea()
                    }
                }
            Group {
                if active, let snapshot {
                    switch anim {
                    case .lift: old(snapshot).opacity(1 - p).scaleEffect(1 - 0.04 * p).blur(radius: 5 * p)
                    case .cascade: strips(snapshot, w: w, h: h, p: p)
                    default: Color.clear
                    }
                }
            }
            .allowsHitTesting(false)
        }
    }

    private func old(_ image: UIImage) -> some View {
        Image(uiImage: image).resizable().ignoresSafeArea().allowsHitTesting(false)
    }

    /// The old page in bands that leave from the top down, one after the other.
    private func strips(_ image: UIImage, w: CGFloat, h: CGFloat, p: CGFloat) -> some View {
        let n = 16
        let bh = h / CGFloat(n)
        return ZStack(alignment: .topLeading) {
            ForEach(0..<n, id: \.self) { i in
                let q = min(1, max(0, (p - 0.025 * CGFloat(i)) / 0.2))
                Image(uiImage: image).resizable().frame(width: w, height: h)
                    .offset(y: -CGFloat(i) * bh)
                    .frame(width: w, height: bh, alignment: .top).clipped()
                    .offset(y: CGFloat(i) * bh - 10 * q)
                    .opacity(1 - q)
            }
        }
        .frame(width: w, height: h, alignment: .topLeading)
        .ignoresSafeArea()
    }
}

/// Wraps the app's body: runs the transition when `Nav.switchWorld` has taken its picture.
struct WorldLayer<C: View>: View {
    @Environment(Nav.self) private var nav
    @ViewBuilder var content: () -> C

    var body: some View {
        WorldTransition(progress: nav.wProgress, snapshot: nav.snapshot, anim: nav.wAnim, origin: nav.wOrigin, forward: nav.wForward) {
            content().id(nav.world)
        }
        .onChange(of: nav.wToken) {
            withAnimation(.timingCurve(0.65, 0, 0.35, 1, duration: 0.8)) { nav.wProgress = 1 }
        }
        // Never leave a transition running: whatever happened, it is over on appear and 1.5 s after it began.
        .onAppear { nav.snapshot = nil; nav.wProgress = 1 }
        .task(id: nav.wToken) {
            guard nav.snapshot != nil else { return }
            let token = nav.wToken
            try? await Task.sleep(for: .milliseconds(1500))
            guard token == nav.wToken else { return }
            nav.snapshot = nil
            nav.wProgress = 1
        }
    }
}

@MainActor
extension Nav {
    /// Changes world. `origin` is where a tap came from (the circle grows from there); nil is a swipe.
    func switchWorld(tap origin: CGPoint?) {
        // A tap or swipe while a change runs is ignored (a light tap says so), so the end state is
        // always the world the last accepted tap asked for.
        guard snapshot == nil else { Haptics.tap(); return }
        let anim = SwitchStyle.current.animation(tap: origin != nil)
        snapshot = Snap.capture()
        wAnim = anim
        wOrigin = origin ?? CGPoint(x: UIScreen.main.bounds.midX, y: 0)
        wForward = world == .dev
        saveWorld()
        let next: World = world == .dev ? .chat : .dev
        world = next
        Theme.world = next
        loadWorld()
        searching = false; picking = false; penMenu = false; talking = false
        wProgress = 0
        wToken += 1
        let token = wToken
        Task {
            try? await Task.sleep(for: .milliseconds(950))
            // Only the change that started this timer may end it.
            guard token == wToken else { return }
            snapshot = nil
            wProgress = 1
        }
    }
}

/// Settings > Appearance: how the app changes between Dev and Chat, with a button to try it.
struct AppearancePage: View {
    @Environment(Nav.self) private var nav
    @AppStorage("inbox.cards") private var inboxCards = true
    @AppStorage("snooze.moon") private var snoozeMoon = true
    @State private var chosen = SwitchStyle.current
    @State private var tryFrame: CGRect = .zero

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            GroupLabel(text: "Inbox")
            SBox {
                SToggle(title: "Inbox grouping", sub: "Threads in cards per state with a colour rail per project, as the desktop list. Off: the old rows.", on: $inboxCards)
                SToggle(title: "Snooze swipe: moon", sub: "A star sky and a growing moon when you swipe a row left. Off: plain buttons.", on: $snoozeMoon)
            }
            GroupLabel(text: "Switching between Dev and Chat")
            SBox {
                ForEach(SwitchStyle.allCases) { a in
                    Button { chosen = a; SwitchStyle.current = a; Haptics.tap() } label: {
                        HStack(alignment: .top, spacing: 12) {
                            Image(systemName: chosen == a ? "largecircle.fill.circle" : "circle").foregroundStyle(chosen == a ? Hub.accent : Hub.lineStrong).padding(.top, 2)
                            VStack(alignment: .leading, spacing: 3) {
                                Text(a.title).font(.system(size: 15))
                                Text(a.detail).font(.system(size: 12)).foregroundStyle(Hub.muted).multilineTextAlignment(.leading)
                            }
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 13).padding(.vertical, 10).frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                        .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 38) }
                    }
                    .buttonStyle(.plain)
                }
            }
            HStack(spacing: 10) {
                Button {
                    nav.switchWorld(tap: CGPoint(x: tryFrame.midX, y: tryFrame.midY))
                } label: {
                    Text("Try with a tap").font(.system(size: 15, weight: .semibold)).foregroundStyle(Hub.bg)
                        .frame(maxWidth: .infinity).frame(height: 48).background(Capsule().fill(Hub.accent))
                }
                .buttonStyle(.plain)
                .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { tryFrame = $0 }
                Button { nav.switchWorld(tap: nil) } label: {
                    Text("Try as a swipe").font(.system(size: 15, weight: .semibold)).foregroundStyle(Hub.text2)
                        .frame(maxWidth: .infinity).frame(height: 48).background(Capsule().fill(Hub.surface2))
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 14).padding(.top, 18)
            Text("Swipe down from the top edge, or tap the dashed button first in the pill, to change world. Each try goes to the other world; change back the same way.")
                .font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.horizontal, 18).padding(.top, 8)
        }
    }
}
