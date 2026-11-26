import Foundation

// Run on a machine with Swift:
// swiftc ios/Hub/ReleaseNotes.swift ios/tests/ReleaseNotesTests.swift -o /tmp/hub-notes-tests
// /tmp/hub-notes-tests
@main struct ReleaseNotesTests {
    static func main() throws {
        let data = Data(#"""
        [{"build":"202611030853","commit":"abc1234","changes":[{"title":"Rows press and settle","pr":488},{"title":"No number","extra":1}],"future":true},
         {"build":"202611031015","changes":[{"title":"Image viewer","pr":503}]},
         {"build":"202611030755"}]
        """#.utf8)
        let builds = ReleaseNotes.decode(data)
        assert(builds.map(\.build) == ["202611031015", "202611030853", "202611030755"], "newest first")
        assert(builds[1].changes.count == 2 && builds[1].changes[1].pr == nil)
        assert(builds[2].changes.isEmpty && builds[2].commit == nil)
        assert(builds[1].when == "3 Nov 08:53" && builds[1].time == "08:53")
        assert(ReleaseBuild(build: "1").when == "1", "a build from Xcode keeps its plain number")
        assert(ReleaseNotes.decode(Data("not json".utf8)).isEmpty)

        // First install: nothing to show, but it counts as seen.
        var r = ReleaseNotes.decide(builds: builds, current: "202611031015", seen: nil, enabled: true)
        assert(r.show == nil && r.markSeen)
        // New build after one that was seen: show it once.
        r = ReleaseNotes.decide(builds: builds, current: "202611031015", seen: "202611030853", enabled: true)
        assert(r.show?.build == "202611031015" && r.markSeen)
        // Already seen: nothing, nothing to write.
        r = ReleaseNotes.decide(builds: builds, current: "202611031015", seen: "202611031015", enabled: true)
        assert(r.show == nil && !r.markSeen)
        // Switched off: nothing, but the build counts as seen so turning it on later does not replay it.
        r = ReleaseNotes.decide(builds: builds, current: "202611031015", seen: "202611030853", enabled: false)
        assert(r.show == nil && r.markSeen)
        // A build with no entry, or an entry with no changes: nothing.
        r = ReleaseNotes.decide(builds: builds, current: "202611040000", seen: "202611031015", enabled: true)
        assert(r.show == nil && r.markSeen)
        r = ReleaseNotes.decide(builds: builds, current: "202611030755", seen: "202611030000", enabled: true)
        assert(r.show == nil && r.markSeen)
        print("ReleaseNotes: ok")
    }
}
