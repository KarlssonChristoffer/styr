import SwiftUI

// Hub's own confirmation, in place of Apple's grey sheet: a dark card rises from the bottom over a dim,
// with a line on what happens. A harmless act has a plain button; one that stops things is held until the
// button fills, so a stray tap does nothing. A swipe down or a tap outside cancels. The card is drawn once
// at the top of the app (`HubConfirmHost`), like the menu, and `hubConfirm` is the drop-in for
// `.confirmationDialog` on any view.

struct HubConfirmRequest: Identifiable {
    let id = UUID()
    let title: String
    let message: String?
    let confirm: String
    let hold: Bool
    let destructive: Bool
    let run: () -> Void
    let dismiss: () -> Void
}

private struct ConfirmBridge: ViewModifier {
    @Environment(Nav.self) private var nav
    @Binding var isPresented: Bool
    let title: String
    let message: String?
    let confirm: String
    let hold: Bool
    let destructive: Bool
    let action: () -> Void

    func body(content: Content) -> some View {
        content.onChange(of: isPresented) { _, now in
            guard now else { return }
            let flag = $isPresented
            withAnimation(.snappy) {
                nav.confirm = HubConfirmRequest(title: title, message: message, confirm: confirm, hold: hold, destructive: destructive,
                                                run: action, dismiss: { flag.wrappedValue = false })
            }
        }
    }
}

extension View {
    func hubConfirm(_ title: String, message: String? = nil, confirm: String, hold: Bool = false, destructive: Bool = false,
                    isPresented: Binding<Bool>, action: @escaping () -> Void) -> some View {
        modifier(ConfirmBridge(isPresented: isPresented, title: title, message: message, confirm: confirm, hold: hold, destructive: destructive, action: action))
    }
}

struct HubConfirmHost: View {
    @Environment(Nav.self) private var nav
    let request: HubConfirmRequest
    @State private var progress: CGFloat = 0
    @State private var holding: Task<Void, Never>?
    private let holdFor = 0.9

    private func close() {
        holding?.cancel()
        request.dismiss()
        withAnimation(.snappy) { nav.confirm = nil }
    }

    private func finish() {
        Haptics.firm()
        let run = request.run
        close()
        run()
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            Color.black.opacity(0.5).ignoresSafeArea().onTapGesture(perform: close)
            VStack(alignment: .leading, spacing: 0) {
                Capsule().fill(Hub.lineStrong).frame(width: 36, height: 5).frame(maxWidth: .infinity).padding(.top, 2).padding(.bottom, 10)
                Text(request.title).font(.system(size: 17, weight: .semibold))
                if let m = request.message {
                    Text(m).font(.system(size: 13.5)).foregroundStyle(Hub.text3).lineSpacing(2).padding(.top, 5)
                }
                HStack(spacing: 10) {
                    Button(action: close) {
                        Text("Cancel").font(.system(size: 15)).frame(maxWidth: .infinity).frame(height: 46)
                            .background(Capsule().fill(Hub.surface)).overlay(Capsule().strokeBorder(Hub.lineStrong))
                    }
                    .buttonStyle(.plain)
                    confirmButton.frame(maxWidth: .infinity)
                }
                .padding(.top, 16)
            }
            .padding(.horizontal, 16).padding(.top, 10).padding(.bottom, 16)
            .background(RoundedRectangle(cornerRadius: 24).fill(Hub.surface2).overlay(RoundedRectangle(cornerRadius: 24).strokeBorder(Hub.lineStrong)))
            .shadow(color: .black.opacity(0.6), radius: 24, y: -6)
            .padding(.horizontal, 12).padding(.bottom, 22)
            .transition(.move(edge: .bottom).combined(with: .opacity))
            .bowDismiss(atTop: true, close: close)
        }
    }

    @ViewBuilder private var confirmButton: some View {
        let tint = request.destructive ? Hub.red : Hub.accent
        if request.hold {
            ZStack {
                Capsule().fill(request.destructive ? Hub.redBg : Hub.accentBg)
                GeometryReader { g in
                    Capsule().fill(tint.opacity(0.5)).frame(width: g.size.width * progress)
                }
                .clipShape(Capsule())
                Text("Hold to \(request.confirm.lowercased())").font(.system(size: 15, weight: .semibold)).foregroundStyle(tint)
            }
            .frame(height: 46)
            .overlay(Capsule().strokeBorder(tint.opacity(0.5)))
            .contentShape(Capsule())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in
                        guard holding == nil else { return }
                        Haptics.tick()
                        withAnimation(.linear(duration: holdFor)) { progress = 1 }
                        holding = Task { @MainActor in
                            try? await Task.sleep(for: .seconds(holdFor))
                            guard !Task.isCancelled else { return }
                            finish()
                        }
                    }
                    .onEnded { _ in
                        holding?.cancel(); holding = nil
                        withAnimation(.easeOut(duration: 0.2)) { progress = 0 }
                    }
            )
            .accessibilityLabel(request.confirm)
            .accessibilityAddTraits(.isButton)
            .accessibilityAction { finish() }
        } else {
            Button(action: finish) {
                Text(request.confirm).font(.system(size: 15, weight: .semibold)).foregroundStyle(request.destructive ? Hub.red : Hub.bg)
                    .frame(maxWidth: .infinity).frame(height: 46)
                    .background(Capsule().fill(request.destructive ? Hub.redBg : Hub.accent))
            }
            .buttonStyle(.plain)
        }
    }
}
