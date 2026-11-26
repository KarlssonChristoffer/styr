import Foundation

// The Swift half of the server contract. The server half is `ios_contract::` in
// crates/hub-core/tests/it (every call the app makes is a route a phone may call).
// This reads what hubds in the field send (crates/hub-core/tests/compat/v1/*.json, the files the
// Rust `compat::` tests also have to read) with the app's own types, plain and with an unknown
// field added in every object, the way a newer hubd answers.
//
// Needs a Mac: Model.swift is part of the whole app target, so the test builds every app file as Mac
// Catalyst (runs natively, no simulator; `simctl spawn` has the wrong cwd). Run from the repo root,
// where the fixtures are read by relative path. iOS 18 is needed (`Bow.swift`). HubApp.swift has the
// app's @main, so a copy without that line stands in for it:
//   T=$(mktemp -d /tmp/hub-contract-XXXX); sed 's/^@main//' ios/Hub/HubApp.swift > $T/HubApp.swift
//   SDK=$(xcrun --sdk macosx --show-sdk-path)
//   xcrun --sdk macosx swiftc -parse-as-library -target $(uname -m)-apple-ios18.0-macabi \
//     -F $SDK/System/iOSSupport/System/Library/Frameworks -I $SDK/System/iOSSupport/usr/lib/swift \
//     $(ls ios/Hub/*.swift | grep -v HubApp.swift) $T/HubApp.swift ios/tests/ContractTests.swift -o $T/run && $T/run
//   rm -rf $T
@main struct ContractTests {
    static func fixture(_ name: String) throws -> Data {
        try Data(contentsOf: URL(fileURLWithPath: "crates/hub-core/tests/compat/v1/\(name)"))
    }

    /// The same JSON with an unknown field in every object.
    static func withExtras(_ value: Any) -> Any {
        if var object = value as? [String: Any] {
            for (k, v) in object { object[k] = withExtras(v) }
            object["added_in_a_later_version"] = ["anything": [1, 2, 3]]
            return object
        }
        if let list = value as? [Any] { return list.map(withExtras) }
        return value
    }

    static func both(_ name: String) throws -> [Data] {
        let plain = try fixture(name)
        let more = try JSONSerialization.data(withJSONObject: withExtras(JSONSerialization.jsonObject(with: plain)))
        return [plain, more]
    }

    static func main() throws {
        for data in try both("thread.json") + both("thread-full.json") {
            let thread = try JSONDecoder().decode(HubThread.self, from: data)
            assert(!thread.id.isEmpty)
        }
        // A list is read one element at a time: the odd thread is skipped, the rest stay.
        let odd = #"[{"broken":true}, \#(String(decoding: try fixture("thread.json"), as: UTF8.self))]"#
        let oddCount = try JSONDecoder().decode(Lossy<HubThread>.self, from: Data(odd.utf8)).items.count
        assert(oddCount == 1)
        // A status this build lacks must not cost the thread.
        var napping = try JSONSerialization.jsonObject(with: fixture("thread.json")) as! [String: Any]
        napping["status"] = "napping"
        let still = try JSONDecoder().decode(Lossy<HubThread>.self, from: JSONSerialization.data(withJSONObject: [napping]))
        assert(still.items.count == 1, "an unknown status dropped the thread")
        for data in try both("event.json") {
            let events = try JSONDecoder().decode(Lossy<HubEvent>.self, from: data).items
            assert(events.count == 6, "events lost: \(events.count) of 6")
            assert(events.allSatisfy { !$0.kind.isEmpty })
        }
        print("Contract tests passed")
    }
}
