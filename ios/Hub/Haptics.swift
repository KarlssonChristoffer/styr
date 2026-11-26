// Haptics and the keychain, kept apart from the store.
import UIKit
import Security

/// Haptics from generators kept and prepared, not made anew at every tick: a generator that was not
/// prepared makes the thumb wait a moment for the engine to wake, which showed as a small stutter just
/// as a swipe crossed its line. `ready()` wakes them when a drag begins.
@MainActor
enum Haptics {
    private static let light = UIImpactFeedbackGenerator(style: .light)
    private static let medium = UIImpactFeedbackGenerator(style: .medium)
    private static let soft = UIImpactFeedbackGenerator(style: .soft)
    private static let select = UISelectionFeedbackGenerator()
    private static let rigid = UIImpactFeedbackGenerator(style: .rigid)
    private static let note = UINotificationFeedbackGenerator()

    static func tap() { light.impactOccurred(); light.prepare() }
    static func firm() { medium.impactOccurred(); medium.prepare() }
    static func gentle() { soft.impactOccurred(); soft.prepare() }
    /// The tick a swipe or a pull gives when it crosses its line.
    static func tick() { select.selectionChanged(); select.prepare() }
    /// A drag has begun: have the engine ready for the tick.
    static func ready() { select.prepare(); medium.prepare() }

    // What an act feels like, so each one answers in its own way and not all with the same light tap.

    /// Done and gone: a thread settled, a question answered, a test ticked, an idea kept.
    static func done() { guard fresh("done") else { return }; note.notificationOccurred(.success); note.prepare() }
    /// Put to sleep: a soft tap and a softer one after it, like a lid closing.
    static func snooze() {
        guard fresh("snooze") else { return }
        soft.impactOccurred(intensity: 1)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.11) { soft.impactOccurred(intensity: 0.45); soft.prepare() }
    }
    /// A message left: one crisp tap the moment you send, not when the machine answers.
    static func sent() { rigid.impactOccurred(intensity: 0.75); rigid.prepare() }
    /// Something did not work.
    static func fail() { note.notificationOccurred(.error); note.prepare() }
    /// It needs you: an agent asks for permission.
    static func asks() { note.notificationOccurred(.warning); note.prepare() }
    private static var lastFelt: [String: Date] = [:]
    /// False when the same feel was given a moment ago: a row that buzzes as its check is drawn is not
    /// buzzed again when the settle reaches the store.
    private static func fresh(_ key: String) -> Bool {
        let now = Date()
        defer { lastFelt[key] = now }
        return now.timeIntervalSince(lastFelt[key] ?? .distantPast) > 0.8
    }

    /// An agent finished its turn in a thread you have.
    static func arrived() { soft.impactOccurred(intensity: 0.8); soft.prepare() }
}

/// The phone tokens live in the Keychain, never in UserDefaults.
enum Keychain {
    private static func query(_ account: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: "se.karlsson.hub",
         kSecAttrAccount as String: account]
    }

    private struct Old: Codable { var url: String; var token: String; var machine: String }

    @discardableResult
    static func save(_ machines: [Machine]) -> OSStatus {
        SecItemDelete(query("machines") as CFDictionary)
        var add = query("machines")
        add[kSecValueData as String] = try? JSONEncoder().encode(machines)
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        return SecItemAdd(add as CFDictionary, nil)
    }

    static func load() -> [Machine] {
        if let data = read("machines"), let list = try? JSONDecoder().decode([Machine].self, from: data) { return list }
        // The first build kept one machine under "pairing".
        if let data = read("pairing"), let old = try? JSONDecoder().decode(Old.self, from: data) {
            let list = [Machine(id: nil, name: old.machine, url: old.url, token: old.token, home: true)]
            save(list)
            SecItemDelete(query("pairing") as CFDictionary)
            return list
        }
        return []
    }

    private static func read(_ account: String) -> Data? {
        var q = query(account)
        q[kSecReturnData as String] = true
        var out: AnyObject?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess else { return nil }
        return out as? Data
    }

    static func clear() {
        SecItemDelete(query("machines") as CFDictionary)
        SecItemDelete(query("pairing") as CFDictionary)
    }
}
