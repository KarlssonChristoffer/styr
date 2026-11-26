import SwiftUI

// Sheet "Maskiner och användning": Usage over time (`GET /api/stats`, feature `phone.stats`) and
// the devices paired with this Hub (`GET /api/devices`, feature `phone.devices`, read only). Hub
// runs on subscriptions, so the page talks about answers and tokens, never money.

struct UsageStats: Decodable {
    struct Model: Decodable, Hashable { var model: String; var answers: Int; var tokens: Double }
    struct Project: Decodable, Hashable { var name: String; var tokens: Double }
    var days: Int
    var answers: Int
    var tokens: Double
    var models: [Model]
    var projects: [Project]
}

struct PairedDevice: Decodable, Hashable, Identifiable {
    var id: String
    var name: String
    var created_at: Double
    var last_seen: Double?
    var phone: Bool?
}

extension HubClient {
    func stats(days: Int, host: String?) async throws -> UsageStats {
        var q = ["days": "\(days)", "tz": "\(TimeZone.current.secondsFromGMT() / 60)"]
        if let host { q["host"] = host }
        return try JSONDecoder().decode(UsageStats.self, from: try await call("GET", "api/stats", query: q))
    }

    func devices(host: String?) async throws -> [PairedDevice] {
        try JSONDecoder().decode(Lossy<PairedDevice>.self, from: try await call("GET", "api/devices", query: host.map { ["host": $0] } ?? [:])).items
    }
}

/// Tokens as 1.2 k or 3.4 M.
func shortCount(_ n: Double) -> String {
    n >= 1_000_000 ? String(format: "%.1f M", n / 1_000_000) : n >= 1000 ? String(format: "%.0f k", n / 1000) : "\(Int(n))"
}

struct UsagePage: View {
    @Environment(Store.self) private var store
    @State private var days = 1
    @State private var stats: UsageStats?
    @State private var failed: String?
    private let choices = [7, 14, 30]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ModeSwitch(selection: $days, items: choices.map { "\($0) days" })
            if let failed { Text(failed).font(.system(size: 13)).foregroundStyle(Hub.muted).padding(18) }
            else if let s = stats {
                summary(s)
                bars("By model", s.models.map { ($0.model, "\($0.answers) answers", $0.tokens) }, color: Hub.accent)
                bars("By project", s.projects.prefix(8).map { ($0.name, shortCount($0.tokens), $0.tokens) }, color: Hub.green)
                Text("Claude Code's answers, read from its own logs. Hub runs on subscriptions, so this is plan and pace, not money.")
                    .font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.horizontal, 18).padding(.top, 14)
            } else { ProgressView().frame(maxWidth: .infinity).padding(30) }
        }
        .task(id: days) { await load() }
    }

    private func summary(_ s: UsageStats) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 18) {
            VStack(alignment: .leading, spacing: 2) { Text("\(s.answers)").font(.system(size: 28, weight: .bold)); Text("answers").font(.system(size: 12)).foregroundStyle(Hub.muted) }
            VStack(alignment: .leading, spacing: 2) { Text(shortCount(s.tokens)).font(.system(size: 28, weight: .bold)); Text("tokens, last \(s.days) days").font(.system(size: 12)).foregroundStyle(Hub.muted) }
            Spacer()
        }
        .padding(.horizontal, 18).padding(.vertical, 10)
    }

    private func bars(_ title: String, _ rows: [(String, String, Double)], color: Color) -> some View {
        let top = max(1, rows.map(\.2).max() ?? 1)
        return VStack(alignment: .leading, spacing: 0) {
            GroupLabel(text: title)
            VStack(spacing: 0) {
                if rows.isEmpty { Text("Nothing in this time.").font(.system(size: 13)).foregroundStyle(Hub.muted).padding(14) }
                ForEach(Array(rows.enumerated()), id: \.offset) { _, r in
                    VStack(alignment: .leading, spacing: 6) {
                        HStack { Text(r.0).font(.system(size: 14.5)).lineLimit(1); Spacer(); Text(r.1).font(.system(size: 12.5)).foregroundStyle(Hub.muted) }
                        ProgressBar(value: r.2 / top, color: color)
                    }
                    .padding(.horizontal, 13).padding(.vertical, 10)
                    .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
                }
            }
            .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface)).padding(.horizontal, 14)
        }
    }

    private func load() async {
        stats = nil; failed = nil
        guard !store.isDemo else { failed = "No usage in the demo."; return }
        guard store.has("phone.stats"), let t = store.target(nil) else { failed = "\(store.machine ?? "Hub") needs the next version of hubd to show usage on the phone."; return }
        do { stats = try await t.client.stats(days: choices[days], host: t.host) }
        catch { failed = error.localizedDescription }
    }
}

/// Every phone or browser that holds a token for this Hub, and when it last spoke (read only: it is
/// cut off on the PC).
struct PairedDevices: View {
    @Environment(Store.self) private var store
    @State private var list: [PairedDevice]?

    var body: some View {
        Group {
            if store.has("phone.devices"), !store.isDemo {
                GroupLabel(text: "Paired with \(store.machine ?? "this Hub")")
                SBox {
                    if list == nil { ProgressView().frame(maxWidth: .infinity).padding(14) }
                    ForEach(list ?? []) { d in
                        HStack(spacing: 12) {
                            Image(systemName: d.phone == true ? "iphone" : "laptopcomputer").foregroundStyle(Hub.muted).frame(width: 20)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(d.name).font(.system(size: 15)).lineLimit(1)
                                Text("Paired \(Self.date(d.created_at))\(d.last_seen.map { " · seen \(relative($0)) ago" } ?? "")").font(.system(size: 12)).foregroundStyle(Hub.muted)
                            }
                            Spacer()
                            if d.name == UIDevice.current.name { Text("this one").font(.system(size: 11.5, weight: .semibold)).foregroundStyle(Hub.green).padding(.horizontal, 7).padding(.vertical, 2).background(Capsule().fill(Hub.greenBg)) }
                        }
                        .padding(.horizontal, 13).frame(minHeight: 56)
                        .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 45) }
                    }
                }
                Text("Remove a device on the PC, under Settings; its token then stops working on every machine.").font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.horizontal, 18).padding(.top, 6)
            }
        }
        .task {
            guard store.has("phone.devices"), !store.isDemo, let t = store.target(nil) else { return }
            list = (try? await t.client.devices(host: t.host)) ?? []
        }
    }

    static func date(_ ms: Double) -> String {
        let f = DateFormatter(); f.dateFormat = "d MMM"
        return f.string(from: Date(timeIntervalSince1970: ms > 4_000_000_000 ? ms / 1000 : ms))
    }
}
