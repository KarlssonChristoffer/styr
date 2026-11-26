import SwiftUI

// The picture viewer (sketch 20, with the gestures of sketch 29): full screen on near-black, the picture
// first, a close, a count and a share on top and, with more than one, a strip of thumbnails at the foot.
// Swipe sideways between the pictures of the thread; pinch or double tap to zoom (one finger pans while
// zoomed, and the page does not turn or close then); pull down at fit size and the picture follows the
// finger and shrinks, the black fades into the chat, and past a line it goes. One tap hides the buttons.

struct ImageViewer: View {
    @Environment(Store.self) private var store
    let project: ProjectRef
    let items: [LibraryItem]
    let start: Int
    let close: () -> Void

    @State private var index = 0
    @State private var zoomed = false
    @State private var chrome = true
    @State private var dragY: CGFloat = 0
    @State private var pulling: Bool?
    @State private var armed = false
    @State private var current: UIImage?

    private var fade: Double { 1 - min(0.85, Double(max(0, dragY)) / 360) }

    var body: some View {
        ZStack {
            Color.black.opacity(fade).ignoresSafeArea()
            GeometryReader { geo in
                ScrollView(.horizontal, showsIndicators: false) {
                    LazyHStack(spacing: 0) {
                        ForEach(Array(items.enumerated()), id: \.offset) { i, item in
                            ZoomablePicture(project: project, item: item, zoomed: i == index ? $zoomed : .constant(false), tapped: { withAnimation(.easeOut(duration: 0.18)) { chrome.toggle() } },
                                            loaded: { if i == index { current = $0 } })
                                .frame(width: geo.size.width, height: geo.size.height)
                                .id(i)
                        }
                    }
                    .scrollTargetLayout()
                }
                .scrollTargetBehavior(.paging)
                .scrollPosition(id: Binding(get: { index }, set: { if let v = $0 { index = v } }))
                .scrollDisabled(zoomed)
            }
            .scaleEffect(1 - min(0.35, max(0, dragY) / 900))
            .offset(y: dragY)
            .simultaneousGesture(zoomed ? nil : pullDown)
            top.opacity(chrome && dragY == 0 ? 1 : 0)
            if items.count > 1 { strip.opacity(chrome && dragY == 0 && !zoomed ? 1 : 0) }
        }
        .onAppear { index = min(max(start, 0), max(items.count - 1, 0)) }
        .onChange(of: index) { current = nil; zoomed = false }
        .statusBarHidden()
    }

    // MARK: Chrome

    private var top: some View {
        VStack {
            HStack {
                Button(action: close) {
                    Image(systemName: "xmark").font(.system(size: 15, weight: .semibold)).foregroundStyle(.white)
                        .frame(width: 38, height: 38).background(Circle().fill(.white.opacity(0.14)))
                }
                .accessibilityLabel("Close")
                Spacer()
                if items.count > 1 {
                    Text("\(index + 1) of \(items.count)").font(.system(size: 13)).foregroundStyle(.white.opacity(0.85))
                        .padding(.horizontal, 12).padding(.vertical, 6).background(Capsule().fill(.white.opacity(0.12)))
                }
                Spacer()
                if let image = current, index < items.count {
                    ShareLink(item: Image(uiImage: image), preview: SharePreview(items[index].title, image: Image(uiImage: image))) {
                        Image(systemName: "square.and.arrow.up").font(.system(size: 15, weight: .semibold)).foregroundStyle(.white)
                            .frame(width: 38, height: 38).background(Circle().fill(.white.opacity(0.14)))
                    }
                } else {
                    Color.clear.frame(width: 38, height: 38)
                }
            }
            .padding(.horizontal, 14).padding(.top, 8)
            Spacer()
        }
    }

    private var strip: some View {
        VStack {
            Spacer()
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(Array(items.enumerated()), id: \.offset) { i, item in
                        PictureThumb(project: project, item: item)
                            .frame(width: 42, height: 42)
                            .clipShape(RoundedRectangle(cornerRadius: 9))
                            .overlay(RoundedRectangle(cornerRadius: 9).strokeBorder(i == index ? Hub.accent : .clear, lineWidth: 2))
                            .opacity(i == index ? 1 : 0.55)
                            .onTapGesture { Haptics.tick(); withAnimation(.snappy) { index = i } }
                    }
                }
                .padding(.horizontal, 16)
            }
            .frame(height: 46).padding(.bottom, 26)
        }
    }

    // MARK: Pull down to close

    private var pullDown: some Gesture {
        DragGesture(minimumDistance: 14, coordinateSpace: .global)
            .onChanged { v in
                if pulling == nil {
                    pulling = v.translation.height > 0 && v.translation.height > abs(v.translation.width) * 1.2
                    if pulling == true { Haptics.ready() }
                }
                guard pulling == true else { return }
                dragY = max(0, v.translation.height)
                let reached = dragY > 120
                if reached != armed { armed = reached; Haptics.tick() }
            }
            .onEnded { v in
                let mine = pulling == true
                pulling = nil
                guard mine else { return }
                TouchGuard.swiped()
                armed = false
                if dragY > 120 || v.predictedEndTranslation.height > 420 {
                    withAnimation(.easeIn(duration: 0.18)) { dragY = 700 }
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.16) { close() }
                } else {
                    withAnimation(.snappy) { dragY = 0 }
                }
            }
    }
}

/// One picture: it loads over the tailnet, fits the screen, zooms by pinch up to 8x and by double tap
/// between fit and 2.5x on the spot tapped, and pans with one finger once zoomed.
private struct ZoomablePicture: View {
    @Environment(Store.self) private var store
    let project: ProjectRef
    let item: LibraryItem
    @Binding var zoomed: Bool
    let tapped: () -> Void
    let loaded: (UIImage) -> Void

    @State private var image: UIImage?
    @State private var scale: CGFloat = 1
    @State private var base: CGFloat = 1
    @State private var offset: CGSize = .zero
    @State private var baseOffset: CGSize = .zero

    var body: some View {
        GeometryReader { geo in
            ZStack {
                if let image {
                    Image(uiImage: image).resizable().scaledToFit()
                        .frame(width: geo.size.width, height: geo.size.height)
                        .scaleEffect(scale)
                        .offset(offset)
                        .gesture(scale > 1.01 ? pan(geo.size) : nil)
                        .simultaneousGesture(pinch(geo.size))
                        .gesture(
                            SpatialTapGesture(count: 2).onEnded { v in doubleTap(at: v.location, in: geo.size) }
                                .exclusively(before: TapGesture(count: 1).onEnded { tapped() })
                        )
                } else {
                    ProgressView().tint(.white.opacity(0.7))
                }
            }
            .frame(width: geo.size.width, height: geo.size.height)
        }
        .task {
            image = await store.picture(project, item.id)
            if let image { loaded(image) }
        }
        .onChange(of: scale) { zoomed = scale > 1.01 }
        .onAppear { if let image { loaded(image) } }
    }

    private func clamp(_ o: CGSize, _ size: CGSize, _ s: CGFloat) -> CGSize {
        let mx = size.width * (s - 1) / 2, my = size.height * (s - 1) / 2
        return CGSize(width: min(max(o.width, -mx), mx), height: min(max(o.height, -my), my))
    }

    private func pinch(_ size: CGSize) -> some Gesture {
        MagnifyGesture()
            .onChanged { v in
                scale = min(8, max(1, base * v.magnification))
                offset = clamp(offset, size, scale)
            }
            .onEnded { _ in
                base = scale
                if scale < 1.05 { reset() } else { baseOffset = offset }
            }
    }

    private func pan(_ size: CGSize) -> some Gesture {
        DragGesture()
            .onChanged { v in
                offset = clamp(CGSize(width: baseOffset.width + v.translation.width, height: baseOffset.height + v.translation.height), size, scale)
            }
            .onEnded { _ in baseOffset = offset }
    }

    private func doubleTap(at p: CGPoint, in size: CGSize) {
        Haptics.tick()
        withAnimation(.snappy) {
            if scale > 1.01 { reset(animated: false) }
            else {
                scale = 2.5; base = 2.5
                // The spot tapped comes to the middle, as far as the edges allow.
                offset = clamp(CGSize(width: (size.width / 2 - p.x) * 2.5, height: (size.height / 2 - p.y) * 2.5), size, 2.5)
                baseOffset = offset
            }
        }
    }

    private func reset(animated: Bool = true) {
        let apply = { scale = 1; base = 1; offset = .zero; baseOffset = .zero }
        if animated { withAnimation(.snappy) { apply() } } else { apply() }
    }
}

private struct PictureThumb: View {
    @Environment(Store.self) private var store
    let project: ProjectRef
    let item: LibraryItem
    @State private var image: UIImage?

    var body: some View {
        Group {
            if let image { Image(uiImage: image).resizable().scaledToFill() } else { Color.white.opacity(0.08) }
        }
        .task { image = await store.picture(project, item.id) }
    }
}
