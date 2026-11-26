import SwiftUI

// Hub's own menu, in place of Apple's dropdown: a small dark card that grows out of the control you
// touched (below it, or above it when the control is low on the screen) over a light dim, with the same
// rows and look everywhere. It is a drop-in for `Menu { Button… } label: { … }`: the buttons inside keep
// their own actions and `role`, and `HubMenuRow` draws them. The card is drawn once, at the top of the app
// (`HubMenuHost`), so no scroll view or clip can cut it off.

struct HubMenuRequest: Identifiable {
    let id = UUID()
    let anchor: CGRect
    let content: AnyView
}

struct HubMenu<Label: View, Content: View>: View {
    @Environment(Nav.self) private var nav
    @ViewBuilder var content: () -> Content
    @ViewBuilder var label: () -> Label
    @State private var frame: CGRect = .zero

    var body: some View {
        Button {
            Haptics.tap()
            withAnimation(.snappy(duration: 0.2)) { nav.menu = HubMenuRequest(anchor: frame, content: AnyView(content())) }
        } label: { label() }
            .buttonStyle(.plain)
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { frame = $0 }
    }
}

/// A row in the card: icon and words, a press lights it, and the card closes once the finger is lifted.
struct HubMenuRow: ButtonStyle {
    let close: () -> Void

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 15))
            .foregroundStyle(configuration.role == .destructive ? Hub.red : Hub.text)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 14).frame(minHeight: 46)
            .background(configuration.isPressed ? Hub.lineStrong.opacity(0.6) : .clear)
            .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 14) }
            .contentShape(Rectangle())
            .onChange(of: configuration.isPressed) { was, now in
                if was && !now { DispatchQueue.main.async(execute: close) }
            }
    }
}

struct HubMenuHost: View {
    @Environment(Nav.self) private var nav
    let request: HubMenuRequest

    private func close() { withAnimation(.snappy(duration: 0.18)) { nav.menu = nil } }

    var body: some View {
        GeometryReader { geo in
            let width: CGFloat = 250
            let a = request.anchor
            let x = min(max(12, a.minX), geo.size.width - width - 12)
            let below = a.midY < geo.size.height * 0.6
            ZStack(alignment: below ? .topLeading : .bottomLeading) {
                Color.black.opacity(0.28).contentShape(Rectangle()).onTapGesture(perform: close)
                VStack(spacing: 0) { request.content }
                    .buttonStyle(HubMenuRow(close: close))
                    .frame(width: width)
                    .background(RoundedRectangle(cornerRadius: 16).fill(Hub.surface2))
                    .clipShape(RoundedRectangle(cornerRadius: 16))
                    .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(Hub.lineStrong))
                    .shadow(color: .black.opacity(0.5), radius: 18, y: 8)
                    .padding(.leading, x)
                    .padding(below ? .top : .bottom, below ? a.maxY + 6 : geo.size.height - a.minY + 6)
                    .transition(.scale(scale: 0.92, anchor: below ? .topLeading : .bottomLeading).combined(with: .opacity))
            }
            .frame(width: geo.size.width, height: geo.size.height)
        }
        .ignoresSafeArea()
    }
}
