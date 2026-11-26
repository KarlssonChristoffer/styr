import SwiftUI

/// Status dot as on the PC: amber ring waits on you, green ring works, red failed, violet done
/// and unread.
struct StatusDot: View {
    let state: DotState
    var body: some View {
        switch state {
        case .waiting:
            Circle().strokeBorder(Hub.amber, lineWidth: 1.5).frame(width: 8, height: 8)
        case .running:
            // Working is the working grid, as everywhere in Hub (`Working` in WorkGrid.swift).
            Working(tint: Hub.green, size: 11)
        case .error:
            Circle().fill(Hub.red).frame(width: 8, height: 8)
        case .unread:
            Circle().fill(Hub.accent).frame(width: 8, height: 8)
        case .idle:
            Circle().strokeBorder(Hub.muted, lineWidth: 1.5).frame(width: 8, height: 8)
        }
    }
}

/// "▢ Hub  ▭ PC": project with its coloured folder, then the machine.
struct ThreadMeta: View {
    @Environment(Store.self) private var store
    let thread: HubThread
    var body: some View {
        HStack(spacing: 10) {
            if thread.isQuick {
                // A quick thread has no project: "Quick · PC".
                HStack(spacing: 4) {
                    Image(systemName: "bolt.fill").foregroundStyle(Hub.accent)
                    Text("Quick · \(thread.host ?? store.machine ?? "this machine")")
                }
            } else {
                HStack(spacing: 4) {
                    Image(systemName: "folder").foregroundStyle(Hub.projectColor(thread.project))
                    Text(thread.project)
                }
                HStack(spacing: 4) {
                    Image(systemName: "desktopcomputer")
                    Text(thread.host ?? store.machine ?? "this machine")
                }
            }
        }
        .font(.system(size: 12))
        .foregroundStyle(Hub.muted)
        .lineLimit(1)
    }
}

/// Round 32 pt buttons, at most one filled per row.
struct PillButtonStyle: ButtonStyle {
    enum Kind { case primary, plain, danger }
    var kind: Kind = .plain

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: kind == .primary ? .semibold : .medium))
            .padding(.horizontal, 13)
            .frame(height: 32)
            .foregroundStyle(kind == .primary ? Hub.bg : kind == .danger ? Hub.red : Hub.text2)
            .background(Capsule().fill(kind == .primary ? Hub.accent : Hub.surface2))
            .overlay(Capsule().strokeBorder(kind == .primary ? Hub.accent : Hub.lineStrong))
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}

/// A row or card that answers a press (sketch "Känsla", 1): it lightens and sinks a little while held,
/// and springs back. `.pressFlat` only lightens, for dense lists (files, settings, pickers), as iOS's own do.
struct PressStyle: ButtonStyle {
    var sink = true

    func makeBody(configuration: Configuration) -> some View {
        configuration.label.pressed(configuration.isPressed, sink: sink)
    }
}

/// The send button: it dips under the thumb and springs back (sketch "Känsla", 4).
struct DipStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.82 : 1)
            .animation(configuration.isPressed ? .easeOut(duration: 0.07) : .spring(response: 0.3, dampingFraction: 0.45), value: configuration.isPressed)
    }
}

extension ButtonStyle where Self == PressStyle {
    static var press: PressStyle { PressStyle() }
    static var pressFlat: PressStyle { PressStyle(sink: false) }
}

extension View {
    /// How a held row looks: a touch lighter, and with `sink` a little smaller, springing back on release.
    func pressed(_ on: Bool, sink: Bool = true) -> some View {
        brightness(on ? 0.045 : 0)
            .scaleEffect(sink && on ? 0.975 : 1)
            .animation(on ? .easeOut(duration: 0.08) : .spring(response: 0.32, dampingFraction: 0.55), value: on)
    }
}

func relative(_ ms: Double?) -> String {
    guard let ms else { return "" }
    let minutes = max(0, Int((Date().timeIntervalSince1970 * 1000 - ms) / 60_000))
    if minutes < 1 { return "now" }
    if minutes < 60 { return "\(minutes) min" }
    if minutes < 60 * 24 { return "\(minutes / 60) h" }
    return "\(minutes / (60 * 24)) d"
}

/// The top of a pushed page (sketch "Ramen", screens 5 and 6): the pill steps aside for a round
/// back button, a capsule with the title and a status line, and the page's round buttons.
struct HubHeader<Trailing: View>: View {
    var parent: String?
    let title: String
    var sub: String?
    var back: (() -> Void)?
    @ViewBuilder var trailing: () -> Trailing

    var body: some View {
        HStack(spacing: 8) {
            if let back {
                Button(action: back) {
                    Image(systemName: "chevron.left").font(.system(size: 16, weight: .semibold)).foregroundStyle(Hub.text)
                        .frame(width: 42, height: 42).background(Circle().fill(Hub.surface2))
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Back to \(parent ?? "the last page")")
            }
            VStack(spacing: 1) {
                Text(title).font(.system(size: 14, weight: .semibold)).lineLimit(1).truncationMode(.tail)
                if let sub, !sub.isEmpty { Text(sub).font(.system(size: 11)).foregroundStyle(Hub.muted).lineLimit(1) }
            }
            .padding(.horizontal, 14).frame(maxWidth: .infinity, minHeight: 42)
            .background(Capsule().fill(Hub.surface2))
            HStack(spacing: 6) { trailing() }
        }
        .padding(.horizontal, 12).padding(.top, 4).padding(.bottom, 8)
    }
}

extension HubHeader where Trailing == EmptyView {
    init(parent: String? = nil, title: String, sub: String? = nil, back: (() -> Void)? = nil) {
        self.init(parent: parent, title: title, sub: sub, back: back) { EmptyView() }
    }
}

/// A round button for a pushed page's action next to the title capsule; with a label it is a capsule.
struct HeaderButton: View {
    let icon: String
    var label: String?
    var accessibility: String
    let action: () -> Void

    init(_ icon: String, label: String? = nil, accessibility: String, action: @escaping () -> Void) {
        self.icon = icon; self.label = label; self.accessibility = accessibility; self.action = action
    }

    var body: some View {
        Button(action: action) { face }.buttonStyle(.plain).accessibilityLabel(accessibility)
    }

    private var face: some View { HeaderFace(icon: icon, label: label) }
}

/// The look of a `HeaderButton`, also used as the label of a `Menu`.
struct HeaderFace: View {
    let icon: String
    var label: String?

    var body: some View {
        HStack(spacing: 5) {
            Image(systemName: icon).font(.system(size: 16, weight: .semibold))
            if let label { Text(label).font(.system(size: 12.5, weight: .medium)) }
        }
        .foregroundStyle(Hub.text)
        .padding(.horizontal, label == nil ? 0 : 12).frame(minWidth: 42, minHeight: 42)
        .background(Capsule().fill(Hub.surface2))
        .contentShape(Capsule())
    }
}

extension Nav {
    /// The tab's name, for the start of a page's path.
    var tabName: String { (world == .dev ? ["Inbox", "Projects", "Machines", "Settings"] : ["Chats", "Spaces", "Artifacts", "Settings"])[min(max(tab, 0), 3)] }
}
