import SwiftUI

/// "What's new" (sketch "Release notes", C): a half sheet after an update with the three newest changes,
/// dragged up it shows all of them and the builds before. × or a drag down closes it, "Turn off" ends it.
struct WhatsNewSheet: View {
    @Environment(Store.self) private var store
    @Environment(Nav.self) private var nav
    @Environment(\.dismiss) private var dismiss
    let build: ReleaseBuild
    let earlier: [ReleaseBuild]
    @Binding var detent: PresentationDetent

    private var full: Bool { detent == .large }
    private var shown: [ReleaseChange] { full ? build.changes : Array(build.changes.prefix(3)) }
    private var hidden: Int { build.changes.count - shown.count }

    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .top, spacing: 10) {
                VStack(alignment: .leading, spacing: 3) {
                    Text("What’s new").font(.system(size: 17, weight: .semibold))
                    Text("Build \(build.when) · \(build.count)").font(.system(size: 12)).foregroundStyle(Hub.muted)
                }
                Spacer()
                Button { dismiss() } label: {
                    Image(systemName: "xmark").font(.system(size: 12, weight: .bold)).foregroundStyle(Hub.text3)
                        .frame(width: 30, height: 30).background(Circle().fill(Hub.surface2)).contentShape(Circle())
                }
                .buttonStyle(.plain).accessibilityLabel("Close")
            }
            .padding(.horizontal, 18).padding(.top, 18)

            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(shown) { ChangeLine(change: $0) }
                    if hidden > 0 {
                        Button { Haptics.tick(); withAnimation(.snappy) { detent = .large } } label: {
                            Text("Show \(hidden) more ↑").font(.system(size: 12.5)).foregroundStyle(Hub.accent)
                                .padding(.leading, 16).padding(.vertical, 8).contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                    if full && !earlier.isEmpty {
                        Text("Before").font(.system(size: 12, weight: .semibold)).foregroundStyle(Hub.muted).padding(.top, 14).padding(.bottom, 2)
                        ForEach(earlier.prefix(3)) { b in
                            HStack {
                                Text("\(b.when) · \(b.count)").font(.system(size: 12.5)).foregroundStyle(Hub.text3)
                                Spacer()
                                Image(systemName: "chevron.right").font(.system(size: 11, weight: .semibold)).foregroundStyle(Hub.muted)
                            }
                            .padding(.vertical, 8).contentShape(Rectangle())
                            .safeTap { openUpdates() }
                        }
                    }
                }
                .padding(.horizontal, 18).padding(.top, 6)
            }
            .scrollBounceBehavior(.basedOnSize)

            HStack {
                Button { turnOff() } label: {
                    Text("Turn off").font(.system(size: 12)).foregroundStyle(Hub.muted).frame(height: 38).contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                Spacer()
                Button("Got it") { Haptics.done(); dismiss() }
                    .buttonStyle(PillButtonStyle(kind: .primary))
            }
            .padding(.horizontal, 18).padding(.top, 8).padding(.bottom, 10)
        }
        .background(Hub.surface)
        .presentationBackground(Hub.surface)
    }

    private func turnOff() {
        ReleaseNotes.enabled = false
        store.flash("Update notes are off. Settings > Updates turns them on again.")
        dismiss()
    }

    private func openUpdates() {
        dismiss()
        nav.tab = 3
        nav.settings = [.settings(.updates)]
    }
}

struct ChangeLine: View {
    let change: ReleaseChange
    var size: CGFloat = 13.5
    var pad: CGFloat = 8

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Circle().fill(Hub.accent).frame(width: 6, height: 6).padding(.top, size * 0.5)
            (Text(change.title) + (change.pr.map { Text(" #\($0)").foregroundColor(Hub.muted) } ?? Text("")))
                .font(.system(size: size)).lineSpacing(2).fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
        .padding(.vertical, pad)
    }
}

/// One build on Settings > Updates: a card that opens in place; the running one starts open.
struct BuildCard: View {
    let build: ReleaseBuild
    let running: Bool
    @State var open: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Text(build.when).font(.system(size: 14.5, weight: .semibold))
                if running {
                    Text("Running").font(.system(size: 11.5, weight: .semibold)).foregroundStyle(Hub.green)
                        .padding(.horizontal, 8).padding(.vertical, 2).background(Capsule().fill(Hub.greenBg))
                }
                Spacer()
                Text(build.count).font(.system(size: 12)).foregroundStyle(Hub.muted)
                Image(systemName: "chevron.right").font(.system(size: 11, weight: .semibold)).foregroundStyle(Hub.muted)
                    .rotationEffect(.degrees(open ? 90 : 0))
            }
            .contentShape(Rectangle())
            .safeTap { Haptics.tick(); withAnimation(.snappy) { open.toggle() } }
            if open {
                if build.changes.isEmpty {
                    Text("Nothing was recorded for this build.").font(.system(size: 12.5)).foregroundStyle(Hub.muted).padding(.top, 8)
                } else {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(build.changes) { ChangeLine(change: $0, size: 12.8, pad: 6) }
                    }
                    .padding(.top, 4)
                }
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 12)
        .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
        .padding(.horizontal, 14).padding(.bottom, 8)
    }
}
