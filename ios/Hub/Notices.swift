import Foundation

/// The phone keeps its own notice settings and hands them to every machine it follows. The
/// ntfy topic is made once, on the phone, and never changes, so the subscription in the ntfy app
/// keeps working when a machine is updated, reinstalled, was asleep while the settings changed,
/// or is new.
extension Store {
    private static let noticesKey = "notices.prefs"

    /// What the phone has chosen, or nil before notices were ever set up on it.
    var notices: NotifyPrefs? {
        guard let data = UserDefaults.standard.data(forKey: Self.noticesKey) else { return nil }
        return try? JSONDecoder().decode(NotifyPrefs.self, from: data)
    }

    func keepNotices(_ n: NotifyPrefs) {
        var n = n
        n.utc_offset_min = TimeZone.current.secondsFromGMT() / 60
        if let data = try? JSONEncoder().encode(n) { UserDefaults.standard.set(data, forKey: Self.noticesKey) }
        if !n.topic.isEmpty { UserDefaults.standard.set(n.topic, forKey: "notices.topic") }
        noticesSent = [:]
    }

    /// The machines that should tell the phone about their threads: the paired one and every
    /// other machine that is up.
    var noticeMachines: [String?] {
        Array(Set([machine] + hosts.filter { $0.state == "online" }.map(\.name)))
    }

    /// Gives each machine the phone's settings where they differ, at most every ten minutes per
    /// machine. Answers the machines it could not reach, with why.
    @discardableResult
    func syncNotices(force: Bool = false) async -> [String] {
        guard !isDemo else { return [] }
        guard var mine = notices else {
            // A phone from before the phone kept them: take over what the paired machine has, so
            // the subscription already in ntfy keeps working.
            if has("phone.notify"), let theirs = try? await homeClient?.notify(host: nil), !theirs.topic.isEmpty { keepNotices(theirs) }
            return []
        }
        mine.utc_offset_min = TimeZone.current.secondsFromGMT() / 60
        var errors: [String] = []
        for name in noticeMachines {
            let key = name ?? ""
            if !force, let at = noticesSent[key], Date().timeIntervalSince(at) < 600 { continue }
            guard let t = target(name) else { continue }
            if t.host == nil && !has("phone.notify", on: url(of: name)) { continue }
            do {
                if (try? await t.client.notify(host: t.host)) != mine { try await t.client.setNotify(mine, host: t.host) }
                noticesSent[key] = Date()
            } catch {
                errors.append("\(name ?? "Hub"): \(error.localizedDescription)")
            }
        }
        return errors
    }

    /// Tells every machine that sends notices what the app has on screen: while it is in front, no
    /// notice about the open thread (you see the reply come in) and no "Done" for the others (the
    /// inbox shows it). A machine keeps the word for 90 seconds, so a phone that went away without
    /// saying so hears about everything again soon. Machines without `phone.looking` are skipped.
    func look(active: Bool) async {
        guard !isDemo, notices != nil else { return }
        let thread = active ? openThread : nil
        for name in noticeMachines {
            guard has("phone.looking", on: url(of: name)), let t = target(name) else { continue }
            try? await t.client.looking(active: active, thread: thread, host: t.host)
        }
    }
}
