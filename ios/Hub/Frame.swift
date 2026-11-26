import SwiftUI

/// The frame of the four root pages (sketch "Ramen"): one floating pill with the four places,
/// a round search button beside it on the inbox, and the violet pen, the only coloured button.
/// A tap on the icon you are on scrolls that page to its top. On a pushed page the pill steps
/// aside (`HubHeader` draws the round back button, the title capsule and the round menu).
///
/// Its widest form (Dev's inbox: pill, search and pen) must fit the screen: a bar wider than the
/// screen makes the whole root wider, so the pill and the pages slide out on both sides whenever you
/// come to that page or world. Pill 232 + 8 + 56 + 8 + 56 + 2 × 14 = 388 pt, which fits a 390 pt
/// iPhone; it was 406 pt, and Dev sat 6.5 pt further out than Chat.
struct FloatingBar: View {
    @Environment(Nav.self) private var nav
    @Environment(Store.self) private var store
    @State private var worldFrame: CGRect = .zero
    /// Where each pill icon is on screen (global), for sliding a finger across them.
    @State private var itemFrames: [Int: CGRect] = [:]
    @State private var scrubbing = false
    @State private var downAt: CGPoint?
    @State private var holdTask: Task<Void, Never>?

    var body: some View {
        HStack(spacing: 0) {
            pill
            Spacer(minLength: 8)
            HStack(spacing: 8) {
                if nav.world == .dev && nav.tab == 0 && nav.searching {
                    RoundButton(icon: "xmark", label: "Close search") { withAnimation(.snappy) { nav.searching = false } }
                } else {
                    if nav.world == .dev && nav.tab == 0 {
                        RoundButton(icon: "magnifyingglass", label: "Search") { withAnimation(.snappy) { nav.searching = true } }
                    }
                    PenButton()
                }
            }
        }
        .padding(.horizontal, 14).padding(.top, 24).padding(.bottom, 8)
        .background(LinearGradient(colors: [Hub.bg.opacity(0), Hub.bg.opacity(0.94)], startPoint: .top, endPoint: .center).ignoresSafeArea(edges: .bottom))
    }

    private var pill: some View {
        HStack(spacing: 2) {
            worldButton
            if nav.world == .dev {
                item(0, "Inbox", "tray", dot: store.threads(in: .needsYou).count > 0)
                item(1, "Projects", "folder", tint: nav.focus.map { Hub.projectColor($0.name) }, locked: nav.focus != nil)
                item(2, "Machines", "server.rack")
            } else {
                item(0, "Chats", "bubble.left", dot: store.chats.contains { $0.unread })
                item(1, "Spaces", "square.grid.2x2")
                item(2, "Artifacts", "square.stack.3d.up")
            }
            item(3, "Settings", "gearshape")
        }
        .padding(5)
        .background(Capsule().fill(Hub.surface))
        .overlay(Capsule().strokeBorder(Hub.lineStrong))
        .shadow(color: .black.opacity(0.45), radius: 12, y: 6)
        .simultaneousGesture(slide)
    }

    /// The pill answers a finger that stays on it: slide across the icons and the page changes under it
    /// with a tick for each icon, and you land where you let go; hold on an icon and its last places
    /// are listed (`Places.swift`). A plain tap works as before.
    private var slide: some Gesture {
        DragGesture(minimumDistance: 0, coordinateSpace: .global)
            .onChanged { v in
                if downAt == nil {
                    downAt = v.startLocation
                    scrubbing = false
                    startHold(at: v.startLocation)
                }
                if nav.placesTab != nil { nav.placesFinger = v.location; return }
                if !scrubbing, abs(v.translation.width) > 14, abs(v.translation.width) > abs(v.translation.height) {
                    scrubbing = true
                    holdTask?.cancel()
                    Haptics.ready()
                }
                guard scrubbing, let i = item(at: v.location), i != nav.tab else { return }
                nav.tab = i
                Haptics.tick()
            }
            .onEnded { _ in
                holdTask?.cancel()
                if nav.placesTab != nil { nav.placesLetGo += 1; nav.placesFinger = nil }
                if scrubbing { TouchGuard.swiped() }
                downAt = nil
                scrubbing = false
            }
    }

    private func item(at p: CGPoint) -> Int? {
        itemFrames.first { $0.value.insetBy(dx: 0, dy: -24).contains(p) }?.key
    }

    private func startHold(at p: CGPoint) {
        holdTask?.cancel()
        holdTask = Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(450))
            guard !Task.isCancelled, !scrubbing, downAt != nil, let i = item(at: p), !nav.recents(i).isEmpty else { return }
            Haptics.firm()
            withAnimation(.snappy) { nav.placesTab = i }
        }
    }

    /// The first thing in the pill: a dashed circle that goes to the other world (a speech bubble in
    /// Dev, a terminal in Chat).
    private var worldButton: some View {
        Button {
            Haptics.tap()
            nav.switchWorld(tap: CGPoint(x: worldFrame.midX, y: worldFrame.midY))
        } label: {
            Image(systemName: nav.world == .dev ? "bubble.left.fill" : "terminal.fill").font(.system(size: 14))
                .foregroundStyle(Hub.accent)
                .frame(width: 36, height: 36)
                .overlay(Circle().strokeBorder(Hub.accent.opacity(0.7), style: StrokeStyle(lineWidth: 1.2, dash: [3, 3])))
                .padding(.horizontal, 4)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { worldFrame = $0 }
        .accessibilityLabel(nav.world == .dev ? "Go to Chat" : "Go to Dev")
    }

    private func item(_ i: Int, _ title: String, _ icon: String, dot: Bool = false, tint: Color? = nil, locked: Bool = false) -> some View {
        Button {
            guard TouchGuard.ok, nav.placesTab == nil else { return }
            if locked, nav.world == .dev, i == 1 {
                // Focused: the project button goes in, from the list too; inside the project it scrolls to the top.
                if nav.tab == 1, nav.projects == [Route.project(nav.focus!)] { nav.topTick[i] += 1 } else { nav.enterFocus() }
            } else if nav.tab == i { nav.topTick[i] += 1 } else { nav.tab = i }
            Haptics.tap()
        } label: {
            Image(systemName: icon).font(.system(size: 19, weight: .regular))
                .foregroundStyle(tint ?? (nav.tab == i ? Hub.text : Hub.muted))
                .frame(width: 40, height: 46)
                .background(Circle().fill(tint.map { $0.opacity(nav.tab == i ? 0.22 : 0.12) } ?? (nav.tab == i ? Hub.lineStrong.opacity(0.8) : .clear)))
                .overlay { if let tint, locked { Circle().strokeBorder(tint.opacity(nav.tab == i ? 0.9 : 0.5), lineWidth: 1.5) } }
                .overlay(alignment: .bottomTrailing) {
                    if locked {
                        Image(systemName: "lock.fill").font(.system(size: 7, weight: .bold)).foregroundStyle(Hub.bg)
                            .frame(width: 14, height: 14).background(Circle().fill(Hub.accent))
                            .overlay(Circle().strokeBorder(Hub.bg, lineWidth: 2)).offset(x: -1, y: -3)
                    }
                }
                .overlay(alignment: .topTrailing) {
                    if dot { Circle().fill(nav.world == .dev ? Hub.amber : Hub.accent).frame(width: 9, height: 9).offset(x: -6, y: 8) }
                }
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { itemFrames[i] = $0 }
        .accessibilityLabel(title + (dot ? ", something waits for you" : ""))
        .accessibilityAddTraits(nav.tab == i ? .isSelected : [])
    }
}

/// A round 56 pt button beside the pill, as tall as the pill.
struct RoundButton: View {
    let icon: String
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: icon).font(.system(size: 18, weight: .regular)).foregroundStyle(Hub.text2)
                .frame(width: 56, height: 56)
                .background(Circle().fill(Hub.surface))
                .overlay(Circle().strokeBorder(Hub.lineStrong))
                .shadow(color: .black.opacity(0.45), radius: 12, y: 6)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

/// The violet pen: a tap is a new thread in the project you used last. A hold opens `PenMenu` and goes
/// on as one move: slide to a row and let go on it to choose, let go outside to close.
struct PenButton: View {
    @Environment(Nav.self) private var nav

    var body: some View {
        Image(systemName: "square.and.pencil").font(.system(size: 20, weight: .medium)).foregroundStyle(Hub.bg)
            .frame(width: 56, height: 56)
            .background(Circle().fill(Hub.accent))
            .shadow(color: .black.opacity(0.45), radius: 12, y: 6)
            .contentShape(Circle())
            .safeTap {
                if nav.penHeld { nav.penHeld = false; return }
                Haptics.tap(); if nav.world == .chat { nav.push(.newChat) } else { nav.push(.new(nil, talk: false)) }
            }
            .simultaneousGesture(hold)
            .accessibilityAddTraits(.isButton)
            .accessibilityLabel("New thread")
    }

    private var hold: some Gesture {
        LongPressGesture(minimumDuration: 0.35)
            .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .global))
            .onChanged { value in
                // A hold: the quick choices (another project, no project, talk an idea). Dev only.
                guard nav.world == .dev, case .second(true, let drag) = value else { return }
                if !nav.penHeld {
                    nav.penHeld = true
                    Haptics.firm()
                    withAnimation(.snappy) { nav.penMenu = true }
                }
                if let drag { nav.penFinger = drag.location }
            }
            .onEnded { _ in
                guard nav.penHeld else { return }
                nav.penLetGo += 1
                nav.penFinger = nil
                // The tap that follows the let-go is not a tap.
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { nav.penHeld = false }
            }
    }
}

/// Tap on the current pill icon: the page scrolls to its top.
struct ScrollsToTop: ViewModifier {
    @Environment(Nav.self) private var nav
    let tab: Int
    @State private var position = ScrollPosition(edge: .top)

    func body(content: Content) -> some View {
        content
            .scrollPosition($position)
            .onChange(of: nav.topTick[tab]) { withAnimation(.snappy) { position.scrollTo(edge: .top) } }
    }
}

extension View {
    func scrollsToTop(_ tab: Int) -> some View { modifier(ScrollsToTop(tab: tab)) }
}
