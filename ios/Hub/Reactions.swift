import PhotosUI
import SwiftUI

// Reactions on messages (docs/ARCHITECTURE.md, "Reactions"). An emoji on a message is a `reaction`
// event; the fold (`Folded`, Model.swift) turns them into chips by the seq of the message. As in
// Claude Code the chips are soft grey capsules right under the message: under your bubble on the
// right, under a reply on the left. Yours is tinted with the accent. A tap puts yours on or takes it
// away, a hold on a chip says who put it there, a double tap on a message puts a 👍 on it, and a
// hold on a message opens the sheet with your most used emojis, all the others, and your own.

/// One emoji on one message, and who put it there.
struct ReactionChip: Hashable {
    var emoji: String
    var you: Bool
    var agent: Bool
    var count: Int { (you ? 1 : 0) + (agent ? 1 : 0) }
    var who: String { you && agent ? "You and the agent" : you ? "You" : "The agent" }
}

/// The user's own emojis (small pictures) and the ones used most, as `GET /api/emoji` gives them.
struct EmojiBook {
    var quick = ["👍", "❤️", "🎉", "👀", "🚀", "🙏"]
    var custom: [String: UIImage] = [:]
    var loaded = false
}

extension Store {
    /// Reactions need a hubd that has them (`chat.reactions`); an older one shows none.
    var reacts: Bool { isDemo || has("chat.reactions") }

    func loadEmoji() async {
        guard !isDemo, !emoji.loaded, reacts, let client = home.flatMap(HubClient.init) else { return }
        if let state = try? await client.emoji() {
            var pictures: [String: UIImage] = [:]
            for c in state.custom { if let d = Data(base64Encoded: c.data), let img = UIImage(data: d) { pictures[c.name] = img } }
            emoji.custom = pictures
            if !state.quick.isEmpty { emoji.quick = state.quick }
            emoji.loaded = true
        }
    }

    /// Puts your emoji on a message, or takes it away. The event comes back on the stream.
    func react(_ thread: String, seq: Int, emoji pick: String, on: Bool) {
        if isDemo { return }
        if on { emoji.quick = Array(([pick] + emoji.quick.filter { $0 != pick }).prefix(7)) }
        Haptics.tap()
        Task {
            guard let client = client(for: thread) else { return }
            do { try await client.react(thread: thread, seq: seq, emoji: pick, on: on) }
            catch { Haptics.fail(); flash("Not reacted: \(error.localizedDescription)") }
        }
    }

    func addEmoji(name: String, png: Data) async -> String? {
        guard let client = home.flatMap(HubClient.init) else { return "Not connected" }
        do {
            try await client.putEmoji(name: name, mime: "image/png", data: png.base64EncodedString())
            if let img = UIImage(data: png) { emoji.custom[name] = img }
            return nil
        } catch { return error.localizedDescription }
    }

    func removeEmoji(_ name: String) async {
        guard let client = home.flatMap(HubClient.init) else { return }
        do { try await client.deleteEmoji(name: name); emoji.custom[name] = nil }
        catch { flash("Not removed: \(error.localizedDescription)") }
    }
}

struct EmojiItem: Hashable { var e: String; var n: String }
struct EmojiGroup { var name: String; var items: [EmojiItem] }

/// The Unicode emojis (`EmojiData`) read once: "emoji name" per line.
enum EmojiList {
    static let groups: [EmojiGroup] = EmojiData.groups.map { g in
        EmojiGroup(name: g.name, items: g.text.split(separator: "\n").compactMap { line in
            guard let space = line.firstIndex(of: " ") else { return nil }
            return EmojiItem(e: String(line[..<space]), n: String(line[line.index(after: space)...]))
        })
    }
}

/// One emoji: Unicode as text, or one of your own (`:name:`) as its picture.
struct EmojiGlyph: View {
    @Environment(Store.self) private var store
    let emoji: String
    var size: CGFloat = 16

    var body: some View {
        if emoji.hasPrefix(":"), emoji.hasSuffix(":"), emoji.count > 2,
           let img = store.emoji.custom[String(emoji.dropFirst().dropLast())] {
            Image(uiImage: img).resizable().scaledToFit().frame(width: size, height: size)
        } else {
            Text(emoji).font(.system(size: size))
        }
    }
}

/// The chips under a message, with the round smiley button that opens the sheet.
struct ReactionRow: View {
    @Environment(Store.self) private var store
    let threadId: String
    let seq: Int
    let chips: [ReactionChip]
    var trailing = false
    let add: () -> Void

    var body: some View {
        if store.reacts, !chips.isEmpty {
            HStack(spacing: 5) {
                ForEach(chips, id: \.emoji) { chip in
                    HStack(spacing: 5) {
                        EmojiGlyph(emoji: chip.emoji, size: 15)
                        Text("\(chip.count)").font(.system(size: 12, weight: .medium).monospacedDigit())
                    }
                    .foregroundStyle(chip.you ? Hub.accent : Hub.text2)
                    .padding(.horizontal, 10).frame(height: 28)
                    .background(Capsule().fill(chip.you ? Hub.accentBg : Hub.surface2))
                    .contentShape(Capsule())
                    .onTapGesture { store.react(threadId, seq: seq, emoji: chip.emoji, on: !chip.you) }
                    .onLongPressGesture(minimumDuration: 0.35) { Haptics.firm(); store.flash(chip.who) }
                    .accessibilityLabel("\(chip.emoji), \(chip.who)")
                    .accessibilityAddTraits(.isButton)
                }
                Button(action: add) {
                    Image(systemName: "face.smiling").font(.system(size: 14)).foregroundStyle(Hub.text2)
                        .frame(width: 30, height: 28)
                        .background(Circle().fill(Hub.surface2))
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Add a reaction")
            }
            .frame(maxWidth: .infinity, alignment: trailing ? .trailing : .leading)
        }
    }
}

/// What a hold on a message opens: your most used emojis in a row, "+" for all of them and your
/// own, and Copy and Quote in reply under them.
struct ReactSheet: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    let threadId: String
    let seq: Int
    let chips: [ReactionChip]
    let text: String
    var quote: ((String) -> Void)?

    @State private var more = false
    @State private var query = ""
    @State private var group = 0
    @State private var photo: PhotosPickerItem?
    @State private var note = ""

    private var names: [String] { store.emoji.custom.keys.sorted() }
    private var groups: [EmojiGroup] { EmojiList.groups }

    var body: some View {
        VStack(spacing: 14) {
            HStack(spacing: 0) {
                ForEach(store.emoji.quick.prefix(6), id: \.self) { e in
                    Button { pick(e) } label: {
                        EmojiGlyph(emoji: e, size: 28)
                            .frame(maxWidth: .infinity).frame(height: 48)
                            .background(Circle().fill(mine(e) ? Hub.accentBg : .clear).frame(width: 46, height: 46))
                    }
                    .buttonStyle(.plain)
                }
                Button { withAnimation(.snappy) { more.toggle() } } label: {
                    Image(systemName: more ? "chevron.down" : "plus").font(.system(size: 17, weight: .medium)).foregroundStyle(Hub.text2)
                        .frame(maxWidth: .infinity).frame(height: 48)
                        .background(Circle().fill(Hub.surface2).frame(width: 40, height: 40))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("More emojis")
            }
            if more { full } else { actions }
        }
        .padding(.horizontal, 14).padding(.top, 22).padding(.bottom, 10)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(Hub.bg)
        .presentationDetents(more ? Set<PresentationDetent>([.large]) : Set<PresentationDetent>([.height(230)]))
        .presentationDragIndicator(.visible)
        .task { await store.loadEmoji() }
    }

    private var actions: some View {
        VStack(spacing: 0) {
            if !text.isEmpty {
                row("Copy", "doc.on.doc") { UIPasteboard.general.string = text; Haptics.tap(); dismiss() }
                if let quote { row("Quote in reply", "arrowshape.turn.up.left") { dismiss(); quote(text) } }
            }
        }
        .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
    }

    private func row(_ title: String, _ symbol: String, _ run: @escaping () -> Void) -> some View {
        Button(action: run) {
            HStack(spacing: 12) {
                Image(systemName: symbol).font(.system(size: 15)).foregroundStyle(Hub.muted).frame(width: 22)
                Text(title).font(.system(size: 16)).foregroundStyle(Hub.text)
                Spacer()
            }
            .padding(.horizontal, 14).frame(height: 48).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private var full: some View {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased().trimmingCharacters(in: CharacterSet(charactersIn: ":"))
        let all = groups
        return VStack(spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").font(.system(size: 14)).foregroundStyle(Hub.muted)
                TextField("Search emojis", text: $query).textInputAutocapitalization(.never).autocorrectionDisabled()
                    .font(.system(size: 16)).foregroundStyle(Hub.text)
            }
            .padding(.horizontal, 12).frame(height: 40)
            .background(RoundedRectangle(cornerRadius: 11).fill(Hub.surface))
            if q.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 4) {
                        tab(0, AnyView(Image(systemName: "plus.circle").font(.system(size: 17))))
                        ForEach(Array(all.enumerated()), id: \.offset) { i, g in tab(i + 1, AnyView(Text(g.items.first?.e ?? "").font(.system(size: 20)))) }
                    }
                }
            }
            ScrollView {
                LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 2), count: 8), spacing: 2) {
                    if q.isEmpty {
                        if group == 0 {
                            PhotosPicker(selection: $photo, matching: .images) {
                                Image(systemName: "plus").font(.system(size: 16)).foregroundStyle(Hub.muted)
                                    .frame(maxWidth: .infinity).frame(height: 42)
                                    .overlay(RoundedRectangle(cornerRadius: 9).strokeBorder(Hub.lineStrong, style: StrokeStyle(lineWidth: 1, dash: [3])))
                            }
                            ForEach(names, id: \.self) { n in
                                cell(":\(n):", size: 26)
                                    .contextMenu { Button(role: .destructive) { Task { await store.removeEmoji(n) } } label: { Label("Remove :\(n):", systemImage: "trash") } }
                            }
                        } else if group - 1 < all.count {
                            ForEach(all[group - 1].items, id: \.e) { i in cell(i.e, size: 28) }
                        }
                    } else {
                        let own = names.filter { $0.contains(q) }.map { ":\($0):" }
                        ForEach(own, id: \.self) { cell($0, size: 26) }
                        ForEach(Array(all.flatMap { g in g.items.filter { $0.n.contains(q) } }.prefix(160)), id: \.e) { item in cell(item.e, size: 28) }
                    }
                }
                if q.isEmpty, group == 0, names.isEmpty {
                    Text("Add a picture and it becomes an emoji you can react with.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.top, 10)
                }
                if !note.isEmpty { Text(note).font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.top, 6) }
            }
        }
        .onChange(of: photo) { _, item in
            guard let item else { return }
            Task { await add(item) }
        }
        .onAppear { if names.isEmpty { group = 1 } }
    }

    private func tab(_ i: Int, _ label: AnyView) -> some View {
        Button { group = i } label: {
            label.frame(width: 38, height: 34)
                .foregroundStyle(group == i ? Hub.text : Hub.muted)
                .background(RoundedRectangle(cornerRadius: 9).fill(group == i ? Hub.surface2 : .clear))
        }
        .buttonStyle(.plain)
    }

    private func cell(_ e: String, size: CGFloat) -> some View {
        Button { pick(e) } label: { EmojiGlyph(emoji: e, size: size).frame(maxWidth: .infinity).frame(height: 42).contentShape(Rectangle()) }
            .buttonStyle(.plain)
    }

    private func mine(_ e: String) -> Bool { chips.contains { $0.emoji == e && $0.you } }

    private func pick(_ e: String) {
        store.react(threadId, seq: seq, emoji: e, on: !mine(e))
        dismiss()
    }

    /// A picture becomes an emoji: scaled down to at most 96 px, named after nothing but a counter.
    private func add(_ item: PhotosPickerItem) async {
        defer { photo = nil }
        guard let data = try? await item.loadTransferable(type: Data.self), let img = UIImage(data: data) else { note = "Could not read that picture"; return }
        let k = min(1, 96 / max(img.size.width, img.size.height))
        let size = CGSize(width: max(1, (img.size.width * k).rounded()), height: max(1, (img.size.height * k).rounded()))
        let fmt = UIGraphicsImageRendererFormat(); fmt.scale = 1
        let small = UIGraphicsImageRenderer(size: size, format: fmt).image { _ in img.draw(in: CGRect(origin: .zero, size: size)) }
        guard let png = small.pngData() else { note = "Could not read that picture"; return }
        var n = 1
        while store.emoji.custom["emoji\(n)"] != nil { n += 1 }
        let name = "emoji\(n)"
        if let err = await store.addEmoji(name: name, png: png) { note = err } else { note = "Added :\(name):"; group = 0 }
    }
}

extension View {
    /// Double tap puts a 👍 on a message (or takes it away); not on a message with nothing to react to.
    func likeOnDoubleTap(_ thread: String, seq: Int?, chips: [ReactionChip], store: Store) -> some View {
        simultaneousGesture(TapGesture(count: 2).onEnded {
            guard let seq, store.reacts else { return }
            let mine = chips.contains { $0.emoji == "👍" && $0.you }
            store.react(thread, seq: seq, emoji: "👍", on: !mine)
        })
    }
}
