import SwiftUI

// The one line a running thread shows (sketches 21 and 25, option A): at the end of the feed, where the
// answer grows, a small grid playing Pour (`WorkGrid.swift`, the same loop as everywhere something works), what the agent is doing in plain words and the time, in grey.
// It replaces the capsule above the reply box; Stop is only the box's red button. In Chat the tool rows
// are not shown while it works: after the turn they are one grey chip, "Worked 38s · 4 steps", that
// opens "What it did".

/// "◌ Building the page…  1m 12s", the line at the end of a running thread's feed.
struct WorkingLine: View {
    let words: String
    let start: Double
    /// The thread and the turn: the loop of the grid is drawn from it, so it stays while the turn runs.
    var seed: String = ""
    /// The grid; off while the plan above the box shows, whose step at work has it instead.
    var cube = true

    private var tint: Color { Theme.world == .chat ? Hub.accent : Hub.green }

    var body: some View {
        HStack(spacing: 9) {
            if cube { WorkGrid(shape: WorkShape.pick(seed: seed), tint: tint) }
            Text(words).font(.system(size: 14)).foregroundStyle(Theme.world == .chat ? Hub.accent : Hub.text3).lineLimit(1)
            if start > 0 {
                TimelineView(.periodic(from: .now, by: 1)) { _ in
                    Text(ThreadView.duration(Date().timeIntervalSince1970 * 1000 - start))
                        .font(.system(size: 13).monospacedDigit()).foregroundStyle(Hub.muted)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }
}

/// After a Chat turn: one small grey chip with how long it took and how many steps, that opens what it did.
struct DidChip: View {
    let seconds: Double
    let steps: Int
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 6) {
                Text("Worked \(ThreadView.duration(seconds))\(steps > 0 ? " · \(steps) step\(steps == 1 ? "" : "s")" : "")").font(.system(size: 12.5).monospacedDigit())
                Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold))
            }
            .foregroundStyle(Hub.muted)
            .padding(.horizontal, 10).padding(.vertical, 5)
            .background(Capsule().fill(Hub.surface))
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityHint("Shows what it did")
    }
}

/// "What it did": the turn's tool groups, exactly as the Dev view shows them.
struct WhatItDid: View {
    let turn: ThreadView.Turn?

    var body: some View {
        let groups: [[Step]] = (turn?.items ?? []).compactMap { if case let .work(s) = $0 { return s }; return nil }
        if groups.isEmpty { Text("Nothing to show.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.vertical, 8) }
        ForEach(Array(groups.enumerated()), id: \.offset) { _, steps in WorkGroup(steps: steps) }
    }
}
