import SwiftUI

/// The reply box every thread and a new thread share (sketch 2; T3 Code's composer): it grows to
/// five lines, keeps a draft, and turns into a dictation bar while you talk. Send only ever goes on
/// a tap of the arrow: Return makes a new line, and the arrow is hidden while dictating.
struct Composer: View {
    @Binding var text: String
    var placeholder: String
    /// The agent works: an empty box offers Stop, a written one queues for after the turn.
    var running = false
    var sending = false
    var startDictating = false
    var focus: FocusState<Bool>.Binding
    var onSend: (String) -> Void
    var onStop: (() -> Void)?
    /// While the agent works, a long press on send offers this: stop it and take the message now.
    var onSteer: ((String) -> Void)?
    /// + for context, the engine's name to tap for its settings, @ mentions, and what is attached.
    var onPlus: (() -> Void)?
    var engineLabel: String?
    /// The engine company's mark, shown before its name in the box.
    var engineLogo: AnyView?
    var onEngine: (() -> Void)?
    var mentions: [Mention] = []
    /// Typed after `/`: what the phone can do with it (`SlashCommand`).
    var commands: [SlashCommand] = []
    var attachments: Binding<[Attachment]>?

    @State private var dictation = Dictation()
    @State private var dictating = false
    @State private var before = ""
    @State private var levels: [Float] = Array(repeating: 0, count: 28)

    private var empty: Bool { text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let q = slashQuery, !commands.isEmpty {
                SlashPopover(query: q, items: commands) { c in
                    Haptics.tap()
                    if let run = c.run { text = ""; run() } else { text = c.fill ?? "/\(c.name) " }
                }
            } else if let q = mentionQuery, !mentions.isEmpty {
                MentionPopover(query: q, items: mentions) { m in
                    text = String(text.dropLast(q.count + 1)) + m.insert + " "
                    Haptics.tap()
                }
                .padding(.horizontal, -10)
            }
            if let attachments, !attachments.wrappedValue.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(attachments.wrappedValue) { a in
                            ZStack(alignment: .topTrailing) {
                                Group {
                                    if let img = a.preview { Image(uiImage: img).resizable().scaledToFill() }
                                    else { VStack { Image(systemName: "doc"); Text(a.name).font(.system(size: 9)).lineLimit(1) }.foregroundStyle(Hub.text2).padding(4) }
                                }
                                .frame(width: 54, height: 54).background(Hub.surface2).clipShape(RoundedRectangle(cornerRadius: 10))
                                Button { attachments.wrappedValue.removeAll { $0.id == a.id } } label: {
                                    Image(systemName: "xmark.circle.fill").font(.system(size: 16)).foregroundStyle(.white, Hub.bg)
                                }
                                .offset(x: 5, y: -5).accessibilityLabel("Remove \(a.name)")
                            }
                        }
                    }
                    .padding(.horizontal, 6).padding(.top, 6)
                }
                .swipeZone()
            }
            if dictating, !dictation.text.isEmpty {
                Text(dictation.text).font(.system(size: 15)).foregroundStyle(Hub.text2)
                    .padding(.horizontal, 16).frame(maxWidth: .infinity, alignment: .leading)
                    .transition(.opacity)
            }
            Group {
                if dictating { dictationBar } else { editorRow }
            }
            .padding(.leading, 6).padding(.trailing, 6).padding(.vertical, 6)
            .background(RoundedRectangle(cornerRadius: 24).fill(Hub.surface2))
            .overlay(RoundedRectangle(cornerRadius: 24).strokeBorder(dictating ? Hub.accent.opacity(0.5) : Hub.lineStrong))
            .animation(.snappy(duration: 0.26), value: dictating)
        }
        .padding(.horizontal, 10).padding(.top, 4).padding(.bottom, 8)
        // The box is its own: a sideways drag in it moves the cursor or scrolls the strip, never the page.
        .swipeZone()
        .onChange(of: dictation.level) { levels.removeFirst(); levels.append(dictation.level) }
        .onChange(of: dictation.listening) { if !dictation.listening && dictating { finish() } }
        .onChange(of: dictation.error) { if dictation.error != nil { dictating = false } }
        .task { if startDictating { begin() } }
    }

    /// What follows a "/" that starts the box, while it is one word.
    private var slashQuery: String? {
        guard text.hasPrefix("/"), !text.contains(" "), !text.contains("\n") else { return nil }
        return String(text.dropFirst())
    }

    /// The word after an @ being typed, if the last word is one.
    private var mentionQuery: String? {
        guard let last = text.split(separator: " ", omittingEmptySubsequences: false).last, last.hasPrefix("@"),
              !text.hasSuffix(" ") else { return nil }
        return String(last.dropFirst())
    }

    private var editorRow: some View {
        HStack(alignment: .bottom, spacing: 8) {
            if let onPlus {
                Button(action: onPlus) {
                    Image(systemName: "plus").font(.system(size: 16, weight: .semibold)).foregroundStyle(Hub.text)
                        .frame(width: 34, height: 34).background(Circle().fill(Hub.bg))
                }
                .buttonStyle(.borderless).accessibilityLabel("Add a picture, file or thread")
            }
            TextField("", text: $text, prompt: Text(placeholder).foregroundStyle(Hub.muted), axis: .vertical)
                .font(.system(size: 15.5)).lineLimit(1...5)
                .focused(focus)
                .padding(.leading, 10).padding(.vertical, 7)
            if let engineLabel, let onEngine, empty {
                Button(action: onEngine) {
                    HStack(spacing: 5) {
                        if let engineLogo { engineLogo }
                        Text(engineLabel).font(.system(size: 12, weight: .medium)).foregroundStyle(Hub.text3).lineLimit(1)
                        Image(systemName: "chevron.right").font(.system(size: 9, weight: .semibold)).foregroundStyle(Hub.muted)
                    }
                    .frame(height: 34)
                }
                .buttonStyle(.borderless).accessibilityLabel("Engine and effort: \(engineLabel)")
            }
            Button { begin() } label: {
                Image(systemName: "mic").font(.system(size: 17)).foregroundStyle(Hub.text3).frame(width: 34, height: 34)
            }
            .buttonStyle(.borderless)
            .accessibilityLabel("Dictate")
            sendButton
        }
    }

    @ViewBuilder private var sendButton: some View {
        if running && empty, let onStop {
            Button { onStop() } label: {
                Image(systemName: "stop.fill").font(.system(size: 13)).foregroundStyle(.white)
                    .frame(width: 34, height: 34).background(Circle().fill(Hub.red))
            }
            .buttonStyle(.borderless)
            .accessibilityLabel("Stop the agent")
        } else {
            Button {
                let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !t.isEmpty, !sending else { return }
                onSend(t)
            } label: {
                Group {
                    if sending { Working(tint: Hub.bg, size: 17) }
                    else { Image(systemName: running ? "text.line.last.and.arrowtriangle.forward" : "arrow.up").font(.system(size: 15, weight: .semibold)) }
                }
                .foregroundStyle(empty ? Hub.muted : Hub.bg)
                .frame(width: 34, height: 34)
                .background(Circle().fill(empty ? Hub.lineStrong : Hub.accent))
            }
            .buttonStyle(DipStyle())
            .disabled(empty || sending)
            .accessibilityLabel(running ? "Queue for after this turn" : "Send")
            .contextMenu {
                if running, let onSteer, !empty {
                    Button { onSend(text.trimmingCharacters(in: .whitespacesAndNewlines)) } label: {
                        Label("Queue · after the current turn", systemImage: "text.line.last.and.arrowtriangle.forward")
                    }
                    Button { onSteer(text.trimmingCharacters(in: .whitespacesAndNewlines)) } label: {
                        Label("Steer now · stop it and take this next", systemImage: "arrow.uturn.forward")
                    }
                }
            }
        }
    }

    /// ✕, the level and time, ✓ (T3 Code's dictation toolbar). The words show above as you talk.
    private var dictationBar: some View {
        HStack(spacing: 10) {
            Button { cancel() } label: {
                Image(systemName: "xmark").font(.system(size: 15, weight: .semibold)).foregroundStyle(Hub.text2)
                    .frame(width: 34, height: 34).background(Circle().fill(Hub.lineStrong))
            }
            .buttonStyle(.borderless)
            .accessibilityLabel("Cancel dictation")
            HStack(spacing: 3) {
                ForEach(Array(levels.enumerated()), id: \.offset) { _, l in
                    Capsule().fill(Hub.accent.opacity(0.25 + Double(l) * 0.75))
                        .frame(width: 2.5, height: max(3, CGFloat(l) * 26))
                }
            }
            .frame(maxWidth: .infinity, minHeight: 34)
            TimelineView(.periodic(from: .now, by: 1)) { _ in
                Text(Self.clock(Date().timeIntervalSince(dictation.started))).font(.system(size: 13).monospacedDigit()).foregroundStyle(Hub.muted)
            }
            Button { finish() } label: {
                Image(systemName: "checkmark").font(.system(size: 15, weight: .bold)).foregroundStyle(Hub.bg)
                    .frame(width: 34, height: 34).background(Circle().fill(Hub.accent))
            }
            .buttonStyle(.borderless)
            .accessibilityLabel("Finish dictation")
        }
    }

    static func clock(_ s: TimeInterval) -> String { String(format: "%d:%02d", Int(s) / 60, Int(s) % 60) }

    private func begin() {
        before = text
        focus.wrappedValue = false
        dictating = true
        Task { await dictation.start(); if !dictation.listening { dictating = false } }
    }

    private func finish() {
        let said = dictation.stop()
        dictating = false
        guard !said.isEmpty else { return }
        text = before.isEmpty ? said : before + (before.hasSuffix(" ") || before.hasSuffix("\n") ? "" : " ") + said
        Haptics.tap()
    }

    private func cancel() {
        dictation.cancel()
        dictating = false
        text = before
    }
}

/// A draft per thread that outlives leaving the thread and the app.
enum Drafts {
    static func get(_ key: String) -> String { UserDefaults.standard.string(forKey: "draft." + key) ?? "" }
    static func set(_ key: String, _ text: String) {
        if text.isEmpty { UserDefaults.standard.removeObject(forKey: "draft." + key) }
        else { UserDefaults.standard.set(text, forKey: "draft." + key) }
    }
}
