import SwiftUI

extension Store {
    /// A project's library, kept a minute so a thread and its project share one read.
    func library(_ p: ProjectRef) async -> [LibraryItem] {
        if let cached = libraryCache[p.id], Date().timeIntervalSince(cached.at) < 60 { return cached.items }
        guard !isDemo, let t = target(p.host), has("phone.library", on: url(of: p.host)) else { return [] }
        let items = (try? await t.client.library(project: p.path, host: t.host)) ?? []
        libraryCache[p.id] = (Date(), items)
        return items
    }

    func picture(_ p: ProjectRef, _ id: String) async -> UIImage? {
        let key = p.id + "#" + id
        if let img = pictures.object(forKey: key as NSString) { return img }
        guard let t = target(p.host), let data = try? await t.client.picture(project: p.path, id: id, host: t.host),
              let img = UIImage(data: data) else { return nil }
        pictures.setObject(img, forKey: key as NSString)
        return img
    }

    func projectRef(of t: HubThread) -> ProjectRef {
        ProjectRef(path: Store.projectRoot(t.cwd), name: t.project, host: t.host)
    }
}

/// A picture from the library, loaded over the tailnet, a tap away from full screen.
struct LibraryPicture: View {
    @Environment(Store.self) private var store
    let project: ProjectRef
    let item: LibraryItem
    var height: CGFloat = 180
    /// The pictures this one is shown with: the viewer swipes between them.
    var group: [LibraryItem] = []
    @State private var image: UIImage?
    @State private var full = false

    var body: some View {
        Group {
            if let image {
                Image(uiImage: image).resizable().scaledToFill()
            } else {
                ZStack { Hub.surface2; ProgressView().tint(Hub.muted) }
            }
        }
        .frame(height: height).frame(maxWidth: .infinity)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Hub.lineStrong))
        .contentShape(Rectangle())
        .safeTap { if image != nil { full = true } }
        .task { image = await store.picture(project, item.id) }
        .fullScreenCover(isPresented: $full) {
            ImageViewer(project: project, items: group.isEmpty ? [item] : group, start: group.firstIndex { $0.id == item.id } ?? 0) { full = false }
                .presentationBackground(.clear)
        }
        .accessibilityLabel(item.title)
    }
}

/// The pictures a thread made, under its conversation.
struct ThreadPictures: View {
    @Environment(Store.self) private var store
    let thread: HubThread
    @State private var items: [LibraryItem] = []

    var body: some View {
        let pics = items.filter { $0.kind == "picture" }
        Group {
            if !pics.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    Label("Made in this thread", systemImage: "photo.on.rectangle").font(.system(size: 12, weight: .semibold)).foregroundStyle(Hub.muted)
                    ForEach(pics.prefix(6)) { item in
                        LibraryPicture(project: store.projectRef(of: thread), item: item, height: 220, group: Array(pics))
                    }
                }
            }
        }
        .task {
            items = await store.library(store.projectRef(of: thread)).filter { $0.thread == thread.id }
        }
    }
}

/// A project's library tab: pictures in a grid, docs and pages as rows that open their thread.
struct LibraryTab: View {
    @Environment(Store.self) private var store
    let project: ProjectRef
    @State private var items: [LibraryItem]?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let items {
                let pics = items.filter { $0.kind == "picture" }
                let rest = items.filter { $0.kind != "picture" && $0.kind != "server" }
                if items.isEmpty {
                    Text(store.has("phone.library", on: store.url(of: project.host)) ? "Nothing made here yet." : "\(project.host ?? store.machine ?? "Hub") needs a newer Hub to show the library.")
                        .font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.horizontal, 18)
                }
                if !pics.isEmpty {
                    LazyVGrid(columns: [GridItem(.flexible(), spacing: 8), GridItem(.flexible(), spacing: 8)], spacing: 8) {
                        ForEach(pics) { LibraryPicture(project: project, item: $0, height: 150, group: pics) }
                    }
                    .padding(.horizontal, 14)
                }
                ForEach(rest) { item in
                    Group {
                        if let thread = item.thread {
                            NavigationLink(value: Route.thread(thread)) { row(item) }
                        } else { row(item) }
                    }
                    .buttonStyle(.press)
                }
            } else {
                ProgressView().frame(maxWidth: .infinity).padding(20)
            }
        }
        .task { items = await store.library(project) }
    }

    private func row(_ item: LibraryItem) -> some View {
        HStack(spacing: 11) {
            Image(systemName: item.kind == "page" ? "doc.richtext" : item.kind == "doc" ? "doc.text" : "doc").foregroundStyle(Hub.muted)
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title).font(.system(size: 14.5)).lineLimit(1)
                Text(item.thread_title ?? item.kind).font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(1)
            }
            Spacer()
            Text(relative(item.made_at)).font(.system(size: 12)).foregroundStyle(Hub.muted)
        }
        .padding(.horizontal, 18).padding(.vertical, 9)
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
        .contentShape(Rectangle())
    }
}
