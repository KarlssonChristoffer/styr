import Foundation

// Run on a machine with Swift:
// swiftc ios/Hub/Updates.swift ios/tests/UpdatesTests.swift -o /tmp/hub-updates-tests
// /tmp/hub-updates-tests
@main struct UpdatesTests {
    static func main() throws {
        let data = Data(#"""
        {"hubd":"92cea798a21b","machines":[
          {"machine":"PC","local":true,"online":true,"hubd":"c301302",
           "hubd_state":{"version":"c301302","state":"ready","target":"92cea798a21b"}},
          {"machine":"mac","local":false,"online":true,"hubd":"c301302"},
          {"machine":"clanker","local":false,"online":true,"hubd":"92cea798a21b"},
          {"machine":"vps","local":false,"online":true,"hubd":"c301302",
           "hubd_note":"building hubd 92cea798a21b for x86_64-unknown-linux-gnu on PC"},
          {"machine":"offline","local":false,"online":false,"hubd":"c301302"},
          {"machine":"fresh","local":false,"online":true,"hubd":"c301302",
           "hubd_state":{"version":"92cea798a21b","state":"idle"}},
          {"machine":"future","local":false,"online":true,
           "hubd_state":{"version":"92cea798a21b","state":"future-state","extra":true}}
        ],"extra":"ignored"}
        """#.utf8)
        let fleet = try JSONDecoder().decode(UpdateFleet.self, from: data)
        let m = fleet.machines
        // c sorts after 9, but the shipping target is 92cea…; never choose .max().
        assert(m[1].behind(fleet.hubd) && m[1].canUpdate(fleet.hubd))
        assert(!m[2].behind(fleet.hubd) && !m[2].canUpdate(fleet.hubd))
        assert(m[0].label(fleet.hubd) == "ready to install" && !m[0].canUpdate(fleet.hubd))
        assert(m[3].working && !m[3].canUpdate(fleet.hubd))
        assert(m[4].label(fleet.hubd) == "offline" && !m[4].canUpdate(fleet.hubd))
        assert(!m[5].behind(fleet.hubd)) // updater is fresher than host inventory
        assert(m[6].label(fleet.hubd) == "up to date") // future states/fields decode
        assert(!m[1].behind(nil) && !m[1].canUpdate(nil)) // unknown target is not guessed
        print("Updates regression tests passed")
    }
}
