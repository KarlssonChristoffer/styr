// The pictures and files an agent's answer can hold: pictures it made, files sent and file pages.
import SwiftUI
import WebKit

/// A picture the agent made, straight on the chat with rounded corners and no frame. Tap opens it
/// big (pinch to zoom); a hold has Copy, Share and Save (through the share sheet).
struct PictureView: View {
    let picture: Picture
    @State private var image: UIImage?
    @State private var big = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: "photo").font(.system(size: 11.5))
                Text("Picture").lineLimit(1)
            }
            .font(.system(size: 12)).foregroundStyle(Hub.muted)
            if let image {
                Image(uiImage: image).resizable().scaledToFit()
                    .clipShape(RoundedRectangle(cornerRadius: 14))
                    .safeTap { big = true; Haptics.tap() }
                    .contextMenu {
                        Button { UIPasteboard.general.image = image } label: { Label("Copy image", systemImage: "doc.on.doc") }
                        ShareLink(item: Image(uiImage: image), preview: SharePreview("Picture", image: Image(uiImage: image))) {
                            Label("Share or save", systemImage: "square.and.arrow.up")
                        }
                        Button { big = true } label: { Label("Open big", systemImage: "arrow.up.left.and.arrow.down.right") }
                    }
                if let prompt = picture.prompt, !prompt.isEmpty {
                    Text(prompt).font(.system(size: 11.5)).foregroundStyle(Hub.muted).lineLimit(2)
                }
            } else {
                RoundedRectangle(cornerRadius: 14).fill(Hub.surface).frame(height: 180).overlay(ProgressView())
            }
        }
        .task(id: picture.id) {
            let base64 = picture.base64
            image = await Task.detached(priority: .userInitiated) { Data(base64Encoded: base64).flatMap(UIImage.init(data:)) }.value
        }
        .sheet(isPresented: $big) {
            if let image { PictureSheet(image: image) }
        }
    }
}

struct PictureSheet: View {
    let image: UIImage
    @Environment(\.dismiss) private var dismiss
    @State private var zoom: CGFloat = 1
    @State private var base: CGFloat = 1

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Button { dismiss() } label: {
                    Image(systemName: "chevron.down").font(.system(size: 15, weight: .semibold)).foregroundStyle(Hub.text)
                        .frame(width: 36, height: 36).background(Circle().fill(Hub.surface))
                }
                .buttonStyle(.plain)
                Spacer()
                ShareLink(item: Image(uiImage: image), preview: SharePreview("Picture", image: Image(uiImage: image))) {
                    Image(systemName: "square.and.arrow.up").font(.system(size: 15)).foregroundStyle(Hub.text2)
                        .frame(width: 36, height: 36).background(Circle().fill(Hub.surface))
                }
            }
            .padding(.horizontal, 14).padding(.top, 14).padding(.bottom, 8)
            GeometryReader { geo in
                ScrollView([.horizontal, .vertical], showsIndicators: false) {
                    Image(uiImage: image).resizable().scaledToFit()
                        .frame(width: geo.size.width * zoom, height: geo.size.height * zoom)
                }
                .gesture(MagnifyGesture().onChanged { zoom = max(1, min(base * $0.magnification, 5)) }.onEnded { _ in base = zoom })
                .onTapGesture(count: 2) { withAnimation(.snappy) { zoom = zoom > 1 ? 1 : 2.5; base = zoom } }
            }
        }
        .background(Hub.bg.ignoresSafeArea())
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
    }
}

// MARK: - Pictures named in a reply

/// A picture the agent saved itself and only named in its answer ("saved as D:\Dev\chats\…\gw.png"):
/// the log holds no picture, only the path. Each path that ends in a picture's type is fetched from the
/// thread's machine (`GET /api/phone/threads/{id}/picture`, only inside the thread's folder) and shown
/// under the answer, a tap opens it big. Nothing shows for a path the machine will not give (an older
/// hubd, a file outside the folder or gone).
enum NamedPictures {
    static func paths(in text: String) -> [String] {
        let pattern = #"[^\s`"'<>()\[\]|*?]+\.(?:png|jpe?g|gif|webp|avif|bmp)\b"#
        guard let re = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]) else { return [] }
        let range = NSRange(text.startIndex..., in: text)
        var seen = Set<String>()
        return re.matches(in: text, range: range).compactMap { m in
            guard let r = Range(m.range, in: text) else { return nil }
            let path = String(text[r])
            // A web address is a link, not a file of the thread's.
            guard !path.contains("://"), !path.hasPrefix("www."), seen.insert(path).inserted else { return nil }
            return path
        }
        .prefix(4).map { $0 }
    }
}

struct ReplyPictures: View {
    let text: String
    let threadId: String
    @Environment(Store.self) private var store
    @State private var images: [String: UIImage] = [:]
    @State private var big: UIImage?

    var body: some View {
        let paths = NamedPictures.paths(in: text)
        VStack(alignment: .leading, spacing: 8) {
            ForEach(paths.filter { images[$0] != nil }, id: \.self) { path in
                if let image = images[path] {
                    Image(uiImage: image).resizable().scaledToFit()
                        .frame(maxHeight: 360, alignment: .leading)
                        .clipShape(RoundedRectangle(cornerRadius: 14))
                        .safeTap { big = image; Haptics.tap() }
                        .contextMenu {
                            Button { UIPasteboard.general.image = image } label: { Label("Copy image", systemImage: "doc.on.doc") }
                            ShareLink(item: Image(uiImage: image), preview: SharePreview("Picture", image: Image(uiImage: image))) {
                                Label("Share or save", systemImage: "square.and.arrow.up")
                            }
                        }
                        .transition(.opacity)
                }
            }
        }
        .animation(.easeOut(duration: 0.25), value: images.count)
        .task(id: paths) { await load(paths) }
        .sheet(item: Binding(get: { big.map(Shown.init) }, set: { big = $0?.image })) { PictureSheet(image: $0.image) }
    }

    private struct Shown: Identifiable { let image: UIImage; var id: ObjectIdentifier { ObjectIdentifier(image) } }

    private func load(_ paths: [String]) async {
        guard !paths.isEmpty, let thread = store.thread(threadId),
              store.has("phone.threadpicture", on: store.url(of: thread.host)),
              let client = store.client(for: threadId) else { return }
        for path in paths where images[path] == nil {
            guard let data = try? await client.call("GET", "api/phone/threads/\(threadId)/picture", query: ["path": path]),
                  let image = await Task.detached(priority: .userInitiated, operation: { UIImage(data: data) }).value else { continue }
            images[path] = image
        }
    }
}

// MARK: - What you sent

/// What you sent with a message, above your words: each photo as a small square you tap to see it big,
/// a file or link as a small chip with its name. Small on purpose: it is there to find again, the
/// message is what you said.
struct SentFiles: View {
    let files: [SentFile]

    var body: some View {
        // Right-aligned like your message; a long row scrolls sideways.
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(Array(files.enumerated()), id: \.offset) { _, file in
                    if let p = file.picture { SentThumb(picture: p) }
                    else { chip(file) }
                }
            }
        }
        .fixedSize(horizontal: files.count <= 4, vertical: true)
        .swipeZone()
    }

    @ViewBuilder private func chip(_ file: SentFile) -> some View {
        let label = Label(file.name, systemImage: file.link != nil ? "link" : file.folder ? "folder" : "doc")
            .font(.system(size: 11.5)).foregroundStyle(Hub.text3).lineLimit(1)
            .padding(.horizontal, 8).frame(height: 26).frame(maxWidth: 180)
            .background(Capsule().fill(Hub.surface2)).overlay(Capsule().strokeBorder(Hub.line))
        if let s = file.link, let url = URL(string: s), ["http", "https"].contains(url.scheme ?? "") {
            Button { UIApplication.shared.open(url); Haptics.tap() } label: { label }.buttonStyle(.plain)
        } else {
            label
        }
    }
}

private struct SentThumb: View {
    let picture: Picture
    @State private var image: UIImage?
    @State private var big = false

    var body: some View {
        Group {
            if let image {
                Image(uiImage: image).resizable().scaledToFill()
            } else {
                Hub.surface2
            }
        }
        .frame(width: 44, height: 44)
        .clipShape(RoundedRectangle(cornerRadius: 9))
        .overlay(RoundedRectangle(cornerRadius: 9).strokeBorder(Hub.line))
        .contentShape(Rectangle())
        .safeTap { if image != nil { big = true; Haptics.tap() } }
        .accessibilityLabel("Picture you sent")
        .task(id: picture.id) {
            let base64 = picture.base64
            image = await Task.detached(priority: .utility) { () -> UIImage? in
                // A small copy for the square; the sheet shows the whole picture.
                guard let full = Data(base64Encoded: base64).flatMap(UIImage.init(data:)) else { return nil }
                let scale = 132 / max(1, min(full.size.width, full.size.height))
                return full.preparingThumbnail(of: CGSize(width: full.size.width * scale, height: full.size.height * scale)) ?? full
            }.value
        }
        .sheet(isPresented: $big) {
            if let full = Data(base64Encoded: picture.base64).flatMap(UIImage.init(data:)) { PictureSheet(image: full) }
        }
    }
}

// MARK: - HTML files as pages

/// An HTML file the agent wrote (a chart saved as `.html`), named in its answer or written in the turn,
/// shows as a page card like a page it showed, instead of only its name. The core turns the file into a
/// page (`POST /api/phone/threads/{id}/page`, feature `phone.threadpage`, only files in the thread's own
/// folder) and serves it under the pseudo-thread `files`. A Hub without the call leaves the name as it was.
enum FilePages {
    /// The page each file became, so a turn drawn again does not ask again.
    nonisolated(unsafe) static var made: [String: String] = [:]

    static func paths(in items: [Item]) -> [String] {
        var texts: [String] = []
        for item in items {
            switch item {
            case let .reply(text): texts.append(text)
            case let .work(steps):
                for step in steps where step.kind == "edit" || step.title.lowercased().hasPrefix("write") {
                    texts.append(step.title)
                    if let c = step.command { texts.append(c) }
                }
            default: break
            }
        }
        guard let re = try? NSRegularExpression(pattern: #"[^\s`"'<>()\[\]|*?]+\.html?\b"#, options: [.caseInsensitive]) else { return [] }
        var names = Set<String>()
        var out: [String] = []
        for text in texts {
            for m in re.matches(in: text, range: NSRange(text.startIndex..., in: text)) {
                guard let r = Range(m.range, in: text) else { continue }
                let path = String(text[r])
                // A web address is a link, not a file of the thread's; the same file named twice
                // (relative and absolute) is one page.
                guard !path.contains("://"), !path.hasPrefix("www."), names.insert(name(path)).inserted else { continue }
                out.append(path)
            }
        }
        return Array(out.prefix(3))
    }

    static func name(_ path: String) -> String {
        String(path.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last ?? Substring(path))
    }
}

struct FilePagesView: View {
    let paths: [String]
    let threadId: String
    @Environment(Store.self) private var store
    @State private var pages: [String: String] = [:]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(paths.filter { pages[$0] != nil }, id: \.self) { path in
                if let id = pages[path] {
                    PageCard(page: PageItem(id: id, title: FilePages.name(path), height: nil), threadId: threadId, servedFrom: "files")
                }
            }
        }
        .task(id: paths) { await load() }
    }

    private func load() async {
        for path in paths {
            if let id = FilePages.made["\(threadId)|\(path)"] { pages[path] = id }
        }
        guard paths.contains(where: { pages[$0] == nil }), let thread = store.thread(threadId),
              store.has("phone.threadpage", on: store.url(of: thread.host)),
              let client = store.client(for: threadId) else { return }
        for path in paths where pages[path] == nil {
            guard let data = try? await client.call("POST", "api/phone/threads/\(threadId)/page", body: ["path": path]),
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let id = json["page"] as? String, !id.isEmpty else { continue }
            FilePages.made["\(threadId)|\(path)"] = id
            pages[path] = id
        }
    }
}
