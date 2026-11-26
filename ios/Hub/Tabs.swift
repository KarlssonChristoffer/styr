import SwiftUI

/// A plain list of threads that opens each, for the shelf and a project's page.
struct ThreadList: View {
    @Environment(Store.self) private var store
    let threads: [HubThread]
    /// What a swipe to the right and to the left does to a row, when the list swipes (`StripSwipe`).
    var swipes: ((HubThread) -> (right: SwipeAction?, left: SwipeAction?))?

    var body: some View {
        ForEach(threads) { t in
            if let s = swipes?(t) {
                StripSwipe(right: s.right, left: s.left) { link(t) }
            } else {
                link(t)
            }
        }
    }

    private func link(_ t: HubThread) -> some View {
        NavigationLink(value: Route.thread(t.id)) {
            HStack(spacing: 10) {
                StatusDot(state: t.dot)
                VStack(alignment: .leading, spacing: 2) {
                    Text(t.title.isEmpty ? "New thread" : t.title).font(.system(size: 14.5)).lineLimit(1)
                    HStack(spacing: 6) {
                        Text(t.project)
                        if let h = t.host { Text("·"); Text(h) }
                        if t.archived { Text("· archived") }
                        else if t.settled_at != nil { Text("· settled") }
                        else if let s = t.snoozed_until, s > Date().timeIntervalSince1970 * 1000 { Text("· until \(Store.clock(s))") }
                    }
                    .font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(1)
                }
                Spacer()
                Text(relative(t.returned_at ?? t.sent_at ?? t.updated_at)).font(.system(size: 12)).foregroundStyle(Hub.muted)
            }
            .padding(.horizontal, 18).padding(.vertical, 10)
            .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
        }
        .buttonStyle(.press)
    }
}

struct ShelfView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var which = 0

    var body: some View {
        let now = Date().timeIntervalSince1970 * 1000
        let list = store.threads.filter { t in
            switch which {
            case 0: return t.settled_at != nil && !t.archived
            case 1: return (t.snoozed_until ?? 0) > now && t.settled_at == nil && !t.archived
            default: return t.archived
            }
        }.sorted { ($0.settled_at ?? $0.updated_at) > ($1.settled_at ?? $1.updated_at) }
        return VStack(spacing: 0) {
            HubHeader(parent: "Inbox", title: "Out of the inbox", back: { dismiss() })
            Tabs(selection: $which, items: ["Settled", "Snoozed", "Archived"]).padding(.horizontal, 14).padding(.vertical, 8)
            ScrollView {
                LazyVStack(spacing: 0) {
                    if list.isEmpty { Text("Nothing here.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.top, 30) }
                    ThreadList(threads: Array(list.prefix(200)), swipes: swipes)
                }
            }
        }
        .background(Hub.bg)
        .toolbar(.hidden, for: .navigationBar)
        .swipeBack { dismiss() }
    }

    /// Settled: right brings it back to the inbox, left archives it. Snoozed: right wakes it now, left
    /// snoozes it until tomorrow. Archived: right restores it.
    private func swipes(_ t: HubThread) -> (right: SwipeAction?, left: SwipeAction?) {
        let night = Color(hex: 0x1b2352), stone = Color(hex: 0x6a625a)
        switch which {
        case 0:
            return (SwipeAction(label: "Back to inbox", icon: "arrow.uturn.backward", tint: Hub.accent) { store.tidy([t], .reopen) },
                    SwipeAction(label: "Archive", icon: "archivebox", tint: stone) { store.tidy([t], .archive) })
        case 1:
            let later = Store.snoozeTimes().last?.until ?? Date().timeIntervalSince1970 * 1000 + 86_400_000
            return (SwipeAction(label: "Wake now", icon: "sun.max", tint: Hub.accent) { store.tidy([t], .wake(was: t.snoozed_until ?? 0)) },
                    SwipeAction(label: "Later", icon: "moon.zzz", tint: night) { store.tidy([t], .snooze(until: later)) })
        default:
            return (SwipeAction(label: "Restore", icon: "arrow.uturn.backward", tint: Hub.accent) { store.tidy([t], .unarchive) }, nil)
        }
    }
}


