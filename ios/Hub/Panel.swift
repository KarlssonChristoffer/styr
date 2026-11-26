import SwiftUI

/// Hub's own bottom panel, instead of an Apple sheet with a grab handle: the page dims, a card
/// rises from the bottom with ✕ and a title, and a tap on the dimmed page or ✕ closes it.
struct HubPanel<Content: View>: View {
    let title: String
    var trailing: AnyView? = nil
    let close: () -> Void
    @ViewBuilder var content: () -> Content
    @State private var atTop = true

    var body: some View {
        ZStack(alignment: .bottom) {
            Color.black.opacity(0.45).ignoresSafeArea().onTapGesture(perform: close)
            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Button(action: close) {
                        Image(systemName: "xmark").font(.system(size: 12.5, weight: .semibold)).foregroundStyle(Hub.text)
                            .frame(width: 30, height: 30).background(Circle().fill(Hub.surface2))
                    }
                    .buttonStyle(.plain).accessibilityLabel("Close")
                    Spacer()
                    Text(title).font(.system(size: 14.5, weight: .semibold))
                    Spacer()
                    if let trailing { trailing } else { Color.clear.frame(width: 30, height: 30) }
                }
                .padding(.horizontal, 12).padding(.top, 10).padding(.bottom, 6)
                ScrollView { VStack(alignment: .leading, spacing: 8) { content() }.padding(.horizontal, 12).padding(.bottom, 14).background(NoBounce()) }
                    .scrollBounceBehavior(.basedOnSize)
                    .trackTop($atTop)
                    .frame(maxHeight: 440)
            }
            .background(
                UnevenRoundedRectangle(topLeadingRadius: 22, topTrailingRadius: 22)
                    .fill(Hub.side)
                    .overlay(UnevenRoundedRectangle(topLeadingRadius: 22, topTrailingRadius: 22).strokeBorder(Hub.lineStrong, lineWidth: 1))
                    .ignoresSafeArea(edges: .bottom)
            )
            .transition(.move(edge: .bottom))
            // A deliberate pull down closes it, like a tap on the dimmed page (Bow.swift).
            .bowDismiss(atTop: atTop, close: close)
        }
        .onAppear { Perf.note("panel opened: \(title)") }
        .onDisappear { Perf.note("panel closed: \(title)") }
    }
}

extension View {
    /// Shows a `HubPanel` over the page while `isPresented`.
    func hubPanel<C: View>(_ title: String, isPresented: Binding<Bool>, @ViewBuilder content: @escaping () -> C) -> some View {
        // The animation sits on the overlay alone. Put after `overlay`, as it was, it covered the whole
        // page under it too, so a panel opening or closing made the thread's own layout animate with it.
        overlay {
            ZStack {
                if isPresented.wrappedValue {
                    HubPanel(title: title, close: { withAnimation(.snappy) { isPresented.wrappedValue = false } }, content: content)
                        .zIndex(10)
                }
            }
            .animation(.snappy, value: isPresented.wrappedValue)
        }
    }
}

/// A grouped box of rows inside a panel.
struct PanelGroup<C: View>: View {
    var label: String?
    @ViewBuilder var content: () -> C
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let label { Text(label).font(.system(size: 12, weight: .semibold)).foregroundStyle(Hub.muted).padding(.leading, 2) }
            VStack(spacing: 0) { content() }.background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
        }
    }
}

struct PanelRow<Leading: View, Trailing: View>: View {
    let title: String
    var sub: String?
    var dim = false
    var last = false
    @ViewBuilder var leading: () -> Leading
    @ViewBuilder var trailing: () -> Trailing
    let action: () -> Void

    var body: some View {
        Button { if TouchGuard.ok { action() } } label: {
            HStack(spacing: 10) {
                leading()
                VStack(alignment: .leading, spacing: 1) {
                    Text(title).font(.system(size: 14, weight: .medium))
                    if let sub { Text(sub).font(.system(size: 11)).foregroundStyle(Hub.muted).lineLimit(2) }
                }
                Spacer(minLength: 6)
                trailing()
            }
            .padding(.horizontal, 12).padding(.vertical, 5).frame(minHeight: 40).contentShape(Rectangle())
            .overlay(alignment: .bottom) { if !last { Hub.line.frame(height: 1).padding(.leading, 13) } }
            .opacity(dim ? 0.45 : 1)
        }
        .buttonStyle(.pressFlat)
        .disabled(dim)
    }
}

/// The kind of machine, for its icon: what its hubd says it is, else a good guess from its name.
@MainActor
enum MachineIcon {
    static func name(_ machine: String?, store: Store) -> String {
        let n = (machine ?? store.machine ?? "").lowercased()
        let kind = store.vitals[machine ?? store.machine ?? ""]?.about?.kind
        switch kind {
        case "laptop": return "laptopcomputer"
        case "desktop": return "desktopcomputer"
        case "board": return "cpu"
        case "vps": return "cloud"
        case "server": return "server.rack"
        default: break
        }
        if n.contains("vps") || n.contains("cloud") || n.contains("hetzner") { return "cloud" }
        if n.contains("pi") || n.contains("raspberry") { return "cpu" }
        if n.contains("macbook") || n.contains("laptop") { return "laptopcomputer" }
        if n == "pc" || n.contains("desktop") { return "desktopcomputer" }
        return "server.rack"
    }

    /// Up when it is followed live, or hubd says it is online.
    static func up(_ machine: String?, store: Store) -> Bool {
        let name = machine ?? store.machine
        if let h = store.hosts.first(where: { $0.name == name }) { return h.state == "online" }
        if let m = store.machines.first(where: { $0.name == name }) { return store.live[m.url] == true }
        return store.anyLive || store.isDemo
    }
}

/// The dial for an effort level: a ring filled as far as the level goes.
struct EffortDial: View {
    let fraction: Double
    var body: some View {
        ZStack {
            Circle().stroke(Hub.lineStrong, lineWidth: 3)
            Circle().trim(from: 0, to: max(0.08, fraction)).stroke(Hub.accent, style: StrokeStyle(lineWidth: 3, lineCap: .round)).rotationEffect(.degrees(-90))
        }
        .frame(width: 24, height: 24)
    }
}
