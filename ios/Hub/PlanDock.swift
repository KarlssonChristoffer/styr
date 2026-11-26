import SwiftUI

// The agent's plan for the turn now (sketch steg-ruta 7, as in Claude's iOS app): a card above the
// reply box, ticked off as it works. A swipe down folds it to a pill with the step now (the choice is
// kept); a swipe up on the pill, or on the card's grab bar, opens it again, and a tap on the pill
// slides the whole list up as a sheet with how long each step took. When the turn is over it is a
// small "3/3 steps" pill, with a tick when every step is done, that opens the same sheet, until you
// write again. The step at work carries the thread's one moving cube; the working line at the end of
// the feed goes without while the card or the pill shows.

struct PlanDock: View {
    let steps: [PlanStep]
    let running: Bool
    /// When the plan came (ms): the sheet's total time counts from here.
    let start: Double
    /// The working line's seed, so the cube here is the same loop the line would draw.
    let seed: String
    @AppStorage("plan.folded") private var folded = false
    @State private var sheet = false
    @State private var drag: CGFloat = 0

    private var tint: Color { Theme.world == .chat ? Hub.accent : Hub.green }
    private var done: Int { steps.filter { $0.status == "completed" }.count }
    /// The step at work: the one in progress, else the first not done.
    private var now: Int { steps.firstIndex { $0.status == "in_progress" } ?? steps.firstIndex { $0.status != "completed" } ?? max(0, steps.count - 1) }

    var body: some View {
        Group {
            if !running { over }
            else if folded { pill }
            else { card }
        }
        .padding(.horizontal, 12).padding(.bottom, 6)
        .sheet(isPresented: $sheet) {
            PlanSheet(steps: steps, start: start, running: running, seed: seed)
                .presentationDetents([.medium, .large])
                .presentationDragIndicator(.visible)
                .presentationBackground(Hub.surface)
        }
    }

    // MARK: The card

    private var card: some View {
        VStack(alignment: .leading, spacing: 0) {
            Capsule().fill(Hub.lineStrong).frame(width: 30, height: 4)
                .frame(maxWidth: .infinity).padding(.top, 7).padding(.bottom, 9)
            if steps.count > 6 {
                ScrollViewReader { proxy in
                    ScrollView { rows.padding(.bottom, 2) }
                        .frame(maxHeight: 210)
                        .scrollBounceBehavior(.basedOnSize)
                        .onAppear { proxy.scrollTo(now, anchor: .center) }
                        .onChange(of: now) { withAnimation(.snappy) { proxy.scrollTo(now, anchor: .center) } }
                }
            } else {
                rows
            }
        }
        .padding(.horizontal, 14).padding(.bottom, 12)
        .background(RoundedRectangle(cornerRadius: 16).fill(Hub.surface))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(Hub.line))
        .offset(y: max(0, drag))
        .contentShape(RoundedRectangle(cornerRadius: 16))
        .gesture(
            DragGesture(minimumDistance: 8)
                .onChanged { drag = $0.translation.height }
                .onEnded { v in
                    if v.translation.height > 40 || v.predictedEndTranslation.height > 110 {
                        Haptics.tick()
                        withAnimation(.snappy) { folded = true; drag = 0 }
                    } else {
                        withAnimation(.snappy) { drag = 0 }
                    }
                }
        )
        .transition(.move(edge: .bottom).combined(with: .opacity))
        .accessibilityAction(named: "Fold the steps") { folded = true }
    }

    private var rows: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(steps.enumerated()), id: \.offset) { i, s in
                HStack(alignment: .top, spacing: 10) {
                    PlanMark(step: s, current: i == now, tint: tint, seed: seed)
                    Text(s.text).font(.system(size: 14.5))
                        .foregroundStyle(s.status == "completed" ? Hub.muted : i == now ? Hub.text : Hub.text3)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .id(i)
            }
        }
    }

    // MARK: The pill while it works

    private var pill: some View {
        Button { Haptics.tap(); sheet = true } label: {
            HStack(spacing: 10) {
                PlanMark(step: steps[now], current: true, tint: tint, seed: seed)
                Text(steps[now].text).font(.system(size: 14.5)).foregroundStyle(Hub.text).lineLimit(1)
                Spacer(minLength: 6)
                Text("\(now + 1) of \(steps.count)").font(.system(size: 12.5).monospacedDigit()).foregroundStyle(Hub.muted)
            }
            .padding(.horizontal, 14).frame(height: 44)
            .background(Capsule().fill(Hub.surface))
            .overlay(Capsule().strokeBorder(Hub.line))
            .contentShape(Capsule())
        }
        .buttonStyle(.press)
        .simultaneousGesture(
            DragGesture(minimumDistance: 12).onEnded { v in
                if v.translation.height < -24 || v.predictedEndTranslation.height < -80 {
                    Haptics.tick()
                    withAnimation(.snappy) { folded = false }
                }
            }
        )
        .transition(.move(edge: .bottom).combined(with: .opacity))
        .accessibilityHint("Shows every step")
        .accessibilityAction(named: "Open the steps here") { folded = false }
    }

    // MARK: After the turn

    private var over: some View {
        HStack {
            Button { Haptics.tap(); sheet = true } label: {
                HStack(spacing: 6) {
                    if done == steps.count {
                        Image(systemName: "checkmark.circle.fill").font(.system(size: 13)).foregroundStyle(Hub.muted)
                    }
                    Text("\(done)/\(steps.count) steps").font(.system(size: 12.5).monospacedDigit())
                    Image(systemName: "chevron.up").font(.system(size: 9.5, weight: .semibold))
                }
                .foregroundStyle(Hub.text3)
                .padding(.horizontal, 11).padding(.vertical, 6)
                .background(Capsule().fill(Hub.surface))
                .overlay(Capsule().strokeBorder(Hub.line))
                .contentShape(Capsule())
            }
            .buttonStyle(.press)
            .accessibilityHint("Shows every step")
            Spacer()
        }
        .padding(.leading, 4)
    }
}

/// A step's mark: ticked when done, the moving cube when it is the one at work, an empty ring before.
struct PlanMark: View {
    let step: PlanStep
    let current: Bool
    let tint: Color
    let seed: String
    var animated = true

    var body: some View {
        Group {
            if step.status == "completed" {
                Image(systemName: "checkmark.circle.fill").font(.system(size: 16)).foregroundStyle(Hub.muted)
            } else if current, animated {
                WorkGrid(shape: WorkShape.pick(seed: seed), tint: tint, size: 15)
            } else if current {
                Image(systemName: "square.grid.2x2").font(.system(size: 13)).foregroundStyle(Hub.text3)
            } else {
                Circle().strokeBorder(Hub.lineStrong, lineWidth: 1.5).frame(width: 15, height: 15)
            }
        }
        .frame(width: 18, height: 18)
        .padding(.top, 1)
    }
}

/// Every step with how long it took, from the pill: "Steps · 2 of 3 · 13s". A swipe down on the grab
/// bar closes it.
struct PlanSheet: View {
    let steps: [PlanStep]
    let start: Double
    let running: Bool
    let seed: String

    var body: some View {
        let done = steps.filter { $0.status == "completed" }.count
        let now = steps.firstIndex { $0.status == "in_progress" } ?? steps.firstIndex { $0.status != "completed" }
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline) {
                Text("Steps").font(.system(size: 18, weight: .semibold))
                Spacer()
                TimelineView(.periodic(from: .now, by: 1)) { _ in
                    Text("\(done) of \(steps.count)\(total.map { " · " + ThreadView.duration($0) } ?? "")")
                        .font(.system(size: 13).monospacedDigit()).foregroundStyle(Hub.muted)
                }
            }
            .padding(.top, 26).padding(.bottom, 12)
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(Array(steps.enumerated()), id: \.offset) { i, s in
                        HStack(alignment: .top, spacing: 12) {
                            PlanMark(step: s, current: i == now, tint: Theme.world == .chat ? Hub.accent : Hub.green, seed: seed, animated: false)
                            Text(s.text).font(.system(size: 15))
                                .foregroundStyle(s.status == "completed" ? Hub.muted : i == now ? Hub.text : Hub.text3)
                                .frame(maxWidth: .infinity, alignment: .leading)
                            if let took = s.took {
                                Text(ThreadView.duration(took)).font(.system(size: 12.5).monospacedDigit()).foregroundStyle(Hub.muted).padding(.top, 2)
                            }
                        }
                        .padding(.vertical, 12)
                        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
                    }
                }
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .padding(.horizontal, 20)
    }

    /// From the plan to now while it works, or to the last step done after.
    private var total: Double? {
        guard start > 0 else { return nil }
        let end = running ? Date().timeIntervalSince1970 * 1000 : (steps.compactMap(\.finished).max() ?? 0)
        return end > start ? end - start : nil
    }
}
