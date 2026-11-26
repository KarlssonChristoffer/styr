import Foundation

/// What changed in each build of the app, for the "What's new" sheet and Settings > Updates.
/// `ios/scripts/deploy.sh` writes `ReleaseNotes.json` into the app from the merged pull requests since the
/// previous build; a build that was never deployed from there has no entry and says nothing.
struct ReleaseChange: Codable, Hashable, Identifiable {
    var title: String
    var pr: Int?
    var id: String { "\(pr ?? 0)|\(title)" }
}

struct ReleaseBuild: Codable, Hashable, Identifiable {
    var build: String
    var commit: String?
    var changes: [ReleaseChange]
    var id: String { build }

    init(build: String, commit: String? = nil, changes: [ReleaseChange] = []) {
        self.build = build; self.commit = commit; self.changes = changes
    }

    // Unknown or missing fields never stop the file from reading (a newer deploy script may add more).
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        build = try c.decode(String.self, forKey: .build)
        commit = try c.decodeIfPresent(String.self, forKey: .commit)
        changes = try c.decodeIfPresent([ReleaseChange].self, forKey: .changes) ?? []
    }
    private enum CodingKeys: String, CodingKey { case build, commit, changes }

    /// The moment the build was made. The build number is that moment as yyyyMMddHHmm.
    var date: Date? { ReleaseNotes.parse(build) }
    /// "3 Nov 10:15", or the plain build number when it is not a time (a build from Xcode).
    var when: String { date.map { ReleaseNotes.format($0, "d MMM HH:mm") } ?? build }
    /// "10:15", for the Updates row.
    var time: String { date.map { ReleaseNotes.format($0, "HH:mm") } ?? build }
    var count: String { changes.count == 1 ? "1 change" : "\(changes.count) changes" }
}

enum ReleaseNotes {
    static let enabledKey = "releaseNotes.enabled"
    static let seenKey = "releaseNotes.seen"

    /// On unless switched off in Settings > Updates.
    static var enabled: Bool {
        get { UserDefaults.standard.object(forKey: enabledKey) as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: enabledKey) }
    }

    static var current: String { Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "" }

    /// Newest first.
    static func decode(_ data: Data) -> [ReleaseBuild] {
        ((try? JSONDecoder().decode([ReleaseBuild].self, from: data)) ?? []).sorted { $0.build > $1.build }
    }

    static func load(_ bundle: Bundle = .main) -> [ReleaseBuild] {
        guard let url = bundle.url(forResource: "ReleaseNotes", withExtension: "json"), let data = try? Data(contentsOf: url) else { return [] }
        return decode(data)
    }

    /// Whether the sheet shows on this launch, and whether this build counts as seen. A first install, a build
    /// with nothing recorded and a switched-off setting all say nothing; a build is never offered twice.
    static func decide(builds: [ReleaseBuild], current: String, seen: String?, enabled: Bool) -> (show: ReleaseBuild?, markSeen: Bool) {
        if seen == current || current.isEmpty { return (nil, false) }
        guard seen != nil, enabled, let b = builds.first(where: { $0.build == current }), !b.changes.isEmpty else { return (nil, true) }
        return (b, true)
    }

    /// What the app launch does: reads what was seen, asks `decide`, remembers.
    static func check() -> ReleaseBuild? {
        let d = UserDefaults.standard
        let r = decide(builds: load(), current: current, seen: d.string(forKey: seenKey), enabled: enabled)
        if r.markSeen { d.set(current, forKey: seenKey) }
        return r.show
    }

    static func parse(_ build: String) -> Date? {
        guard build.count == 12, build.allSatisfy(\.isNumber) else { return nil }
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyyMMddHHmm"
        return f.date(from: build)
    }

    static func format(_ date: Date, _ pattern: String) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = pattern
        return f.string(from: date)
    }
}
