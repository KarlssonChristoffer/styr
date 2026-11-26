import SwiftUI

// "Om tråden" (sketch 33b): what the thread is, in a card that drops out of its title. The title
// pill opens it with a tap or a pull down, a hold opens it ready to rename. The ⋯ menu keeps what
// you do with the thread now (settle, pin, snooze, review, pull request, archive).
// No keyboard comes up until you tap the pencil.

struct ThreadAbout: View {
    @Environment(Store.self) private var store
    let thread: HubThread
    /// Opened by a hold on the title: the name is a field from the start.
    var editing: Bool
    let close: () -> Void
    let engine: () -> Void
    let move: () -> Void

    @State private var renaming = false
    @State private var title = ""
    @State private var drag: CGFloat = 0
    @FocusState private var focused: Bool

    var body: some View {
        ZStack(alignment: .top) {
            Color.black.opacity(0.45).ignoresSafeArea()
                .onTapGesture { done() }
            card
                .offset(y: min(0, drag))
                .gesture(
                    DragGesture(minimumDistance: 10)
                        .onChanged { drag = $0.translation.height }
                        .onEnded { v in
                            if v.translation.height < -50 || v.predictedEndTranslation.height < -120 { done() }
                            else { withAnimation(.snappy) { drag = 0 } }
                        }
                )
                .transition(.move(edge: .top).combined(with: .opacity))
        }
        .onAppear {
            title = thread.title
            if editing { startRename() }
        }
    }

    private var card: some View {
        VStack(spacing: 0) {
            head
                .padding(.horizontal, 14).padding(.top, 12).padding(.bottom, 10)
            Hub.line.frame(height: 1)
            row("Engine", "sparkle", value: EnginePanel.label(thread.engine), action: engine)
            if let cloud = thread.cloud {
                row("Runs in", "icloud.and.arrow.up", value: "\(cloud.providerName) cloud · \(cloud.stateWord)", action: nil)
                if let branch = cloud.branch { row("Branch", "arrow.triangle.branch", value: branch, action: nil) }
                if let note = cloud.note { row("Note", "exclamationmark.circle", value: note, action: nil) }
            } else {
                row("Machine", MachineIcon.name(thread.host, store: store), value: thread.host ?? store.machine ?? "", action: move)
            }
            row("Project", "folder", value: store.isChat(thread) ? "Chat" : thread.isQuick ? "Quick" : thread.project, action: nil)
            row("Started", "clock", value: started, action: nil, last: true)
            Capsule().fill(Hub.lineStrong).frame(width: 36, height: 4).padding(.vertical, 8)
        }
        .background(RoundedRectangle(cornerRadius: 22).fill(Hub.surface))
        .overlay(RoundedRectangle(cornerRadius: 22).strokeBorder(Hub.lineStrong))
        .shadow(color: .black.opacity(0.5), radius: 20, y: 10)
        .padding(.horizontal, 10).padding(.top, 4)
    }

    @ViewBuilder private var head: some View {
        if renaming {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    TextField("", text: $title, prompt: Text("Title").foregroundStyle(Hub.muted))
                        .font(.system(size: 15.5)).focused($focused).submitLabel(.done).onSubmit(save)
                        .padding(.horizontal, 11).frame(height: 40)
                        .background(RoundedRectangle(cornerRadius: 11).fill(Hub.bg))
                        .overlay(RoundedRectangle(cornerRadius: 11).strokeBorder(Hub.accent))
                    Button(action: save) {
                        Image(systemName: "checkmark").font(.system(size: 14, weight: .bold)).foregroundStyle(Hub.bg)
                            .frame(width: 36, height: 36).background(Circle().fill(Hub.accent))
                    }
                    .buttonStyle(.plain).accessibilityLabel("Save the name")
                    .disabled(title.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                if title != thread.title, !thread.title.isEmpty {
                    Button { title = thread.title; Haptics.tap() } label: {
                        Label("Was: \(thread.title)", systemImage: "arrow.uturn.backward").font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(1)
                    }
                    .buttonStyle(.plain)
                }
            }
        } else {
            HStack(spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(thread.title.isEmpty ? "New thread" : thread.title).font(.system(size: 16, weight: .semibold)).lineLimit(2)
                    Text(summary).font(.system(size: 11.5)).foregroundStyle(Hub.muted).lineLimit(1)
                }
                Spacer(minLength: 6)
                Button { startRename(); Haptics.tap() } label: {
                    Image(systemName: "pencil").font(.system(size: 14, weight: .medium)).foregroundStyle(Hub.text2)
                        .frame(width: 36, height: 36).background(Circle().fill(Hub.surface2))
                }
                .buttonStyle(.plain).accessibilityLabel("Rename")
            }
        }
    }

    private func row(_ title: String, _ icon: String, value: String, action: (() -> Void)?, last: Bool = false) -> some View {
        Button { if let action, TouchGuard.ok { action() } } label: {
            HStack(spacing: 10) {
                Image(systemName: icon).font(.system(size: 14)).foregroundStyle(Hub.muted).frame(width: 20)
                Text(title).font(.system(size: 14, weight: .medium)).foregroundStyle(Hub.text)
                Spacer(minLength: 6)
                Text(value).font(.system(size: 13)).foregroundStyle(Hub.muted).lineLimit(1)
                if action != nil { Image(systemName: "chevron.right").font(.system(size: 11)).foregroundStyle(Hub.muted) }
            }
            .padding(.horizontal, 14).frame(minHeight: 42).contentShape(Rectangle())
            .overlay(alignment: .bottom) { if !last { Hub.line.frame(height: 1).padding(.leading, 44) } }
        }
        .buttonStyle(.pressFlat)
        .disabled(action == nil)
    }

    private var summary: String {
        [thread.statusWord, thread.host ?? store.machine].compactMap { $0 }.joined(separator: " · ")
    }

    private var started: String {
        guard let first = store.folded[thread.id]?.times.first(where: { $0 > 0 }) else { return "–" }
        let date = Date(timeIntervalSince1970: first / 1000)
        let f = DateFormatter()
        f.dateFormat = Calendar.current.isDateInToday(date) ? "HH:mm" : "d MMM, HH:mm"
        return f.string(from: date)
    }

    private func startRename() {
        withAnimation(.snappy) { renaming = true }
        focused = true
    }

    private func save() {
        let t = title.trimmingCharacters(in: .whitespaces)
        guard !t.isEmpty else { return }
        if t != thread.title { Task { await store.rename(thread, to: t) } }
        Haptics.tap()
        done()
    }

    private func done() {
        focused = false
        close()
    }
}
