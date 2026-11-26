import QuartzCore
import SwiftUI

// A quiet meter for where the app stutters. It draws nothing: it keeps `perf.log` in the app's
// Documents folder (`xcrun devicectl device copy from --domain-type appDataContainer ... --source
// Documents/perf.log`), a few lines at a time:
//   - "hitch 58 ms · ThreadView.body×3 Prose.parse×12": a frame that came late, and what had run in the
//     last second (how many times a page was rebuilt, a text was parsed, the log was folded);
//   - "slow fold 14 ms (3200 events)": one piece of work that took longer than it should;
//   - notes where a panel opened or closed, a swipe began or ended, a thread opened, so a hitch can be
//     matched with what you were doing.
// It costs a counter per rebuild and one display link, and keeps the log below 400 KB.

enum PerfLog {
    static let url = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("perf.log")
    private static let queue = DispatchQueue(label: "hub.perf", qos: .utility)

    static func append(_ line: String) {
        queue.async {
            if let handle = try? FileHandle(forWritingTo: url) {
                handle.seekToEndOfFile()
                handle.write(Data(line.utf8))
                try? handle.close()
            } else {
                try? Data(line.utf8).write(to: url)
            }
        }
    }

    /// A log over 400 KB moves aside (one old copy is kept) when the app starts.
    static func rotate() {
        queue.async {
            let size = (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? Int) ?? 0
            guard size > 400_000 else { return }
            let old = url.deletingLastPathComponent().appendingPathComponent("perf.old.log")
            try? FileManager.default.removeItem(at: old)
            try? FileManager.default.moveItem(at: url, to: old)
        }
    }
}

@MainActor
enum Perf {
    private static var counts: [String: Int] = [:]
    private static var windowStart = CACurrentMediaTime()
    private static var ticker: Ticker?
    private static let clock: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss.SSS"
        return f
    }()

    static func start() {
        guard ticker == nil else { return }
        PerfLog.rotate()
        let t = Ticker()
        let link = CADisplayLink(target: t, selector: #selector(Ticker.tick(_:)))
        // The meter asks for 60 Hz so it never holds a ProMotion screen at 120 by itself; it still sees
        // every frame while an animation runs the screen faster, and judges each against its own target.
        link.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: 120, preferred: 60)
        link.add(to: .main, forMode: .common)
        ticker = t
        let build = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "?"
        note("start, build \(build)")
    }

    /// One more of something that ran (a page rebuilt, a text parsed).
    static func count(_ key: String) { counts[key, default: 0] += 1 }

    /// A line in the log: what you were doing.
    static func note(_ text: String) {
        PerfLog.append("\(clock.string(from: Date()))  \(text)\n")
    }

    /// Runs `body` and says so in the log when it took `over` milliseconds or more.
    @discardableResult
    static func time<T>(_ key: String, over: Double = 6, detail: @autoclosure () -> String = "", _ body: () -> T) -> T {
        let began = CACurrentMediaTime()
        let result = body()
        let took = (CACurrentMediaTime() - began) * 1000
        counts[key, default: 0] += 1
        if took >= over {
            let d = detail()
            note("slow \(key) \(Int(took)) ms\(d.isEmpty ? "" : " (\(d))")")
        }
        return result
    }

    fileprivate static func tick(gap: Double, expected: Double) {
        let now = CACurrentMediaTime()
        // The window is the last second or so: what ran just before a late frame.
        if now - windowStart > 1 { counts = [:]; windowStart = now }
        guard gap < 1, gap > max(0.030, expected * 2.5) else { return }
        let busy = counts.sorted { $0.value > $1.value }.prefix(6).map { "\($0.key)×\($0.value)" }.joined(separator: " ")
        note("hitch \(Int(gap * 1000)) ms\(busy.isEmpty ? "" : " · " + busy)")
    }

    @MainActor
    final class Ticker: NSObject {
        private var last: CFTimeInterval = 0

        @objc func tick(_ link: CADisplayLink) {
            defer { last = link.timestamp }
            guard last > 0 else { return }
            Perf.tick(gap: link.timestamp - last, expected: link.targetTimestamp - link.timestamp)
        }
    }
}
