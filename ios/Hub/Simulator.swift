import SwiftUI

// The iPhone simulator of a Mac, live, on the phone (feature `sim.view`, docs/ARCHITECTURE.md "The
// simulator view"). A thread whose machine has a simulator running gets a round phone button in its
// top; it opens the simulator full screen on near-black, bare and with quiet buttons (home, close).
// The screen takes touches like a phone: a finger down, dragging and up are sent to the Mac's hubd,
// which passes them on to the simulator, so a tap is a tap and a drag is a swipe. The picture is asked
// for about five times a second: hubd keeps the newest of the stream and hands it out as one JPEG.

/// What a thread's machine says about its simulator, asked every few seconds while its page is open.
struct SimState: Equatable {
    var udid: String
    var name: String
    var live: Bool
}

@MainActor
enum SimAsk {
    /// The client for a thread's machine: its own hubd when followed directly (no `host` needed), else the
    /// paired one, which passes the call on with `host`.
    static func route(_ store: Store, _ thread: HubThread) -> (client: HubClient, query: [String: String])? {
        if let c = store.client(machine: thread.host) { return (c, [:]) }
        if let c = store.homeClient, let h = thread.host { return (c, ["host": h]) }
        return nil
    }

    /// The simulator that is running on the thread's machine, or nil (no Mac, no feature, none booted).
    static func state(_ store: Store, _ thread: HubThread) async -> SimState? {
        guard !store.isDemo, store.has("sim.view", on: store.url(of: thread.host)),
              let (client, query) = route(store, thread),
              let data = try? await client.call("GET", "api/simview", query: query),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              root["available"] as? Bool == true else { return nil }
        let live = root["live"] as? [String: Any]
        let devices = root["devices"] as? [[String: Any]] ?? []
        let watched = live?["udid"] as? String
        let pick = devices.first { ($0["udid"] as? String) == watched } ?? devices.first { $0["booted"] as? Bool == true }
        guard let udid = pick?["udid"] as? String else { return nil }
        return SimState(udid: udid, name: pick?["name"] as? String ?? "iPhone", live: live?["phase"] as? String == "live")
    }
}

/// The simulator, full screen. Closing it stops the live picture; the simulator keeps running.
struct SimulatorView: View {
    @Environment(Store.self) private var store
    let thread: HubThread
    let sim: SimState
    let close: () -> Void

    @State private var image: UIImage?
    @State private var note = "Starting…"
    @State private var pump = TouchPump()

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            Group {
                if let image {
                    Image(uiImage: image).resizable().aspectRatio(contentMode: .fit)
                        .clipShape(RoundedRectangle(cornerRadius: 38, style: .continuous))
                        .padding(.horizontal, 10)
                        .overlay(
                            GeometryReader { g in
                                Color.clear.contentShape(Rectangle())
                                    .gesture(DragGesture(minimumDistance: 0, coordinateSpace: .local)
                                        .onChanged { v in pump.move(v.location, in: g.size) }
                                        .onEnded { v in pump.end(v.location, in: g.size) })
                            }
                            .padding(.horizontal, 10)
                        )
                } else {
                    Text(note).font(.system(size: 13)).foregroundStyle(Hub.muted).multilineTextAlignment(.center).padding(30)
                }
            }
            top
        }
        .task { await run() }
    }

    private var top: some View {
        VStack {
            HStack(spacing: 8) {
                HStack(spacing: 6) {
                    Circle().fill(image == nil ? Hub.muted : Hub.green).frame(width: 7, height: 7)
                    Text(sim.name).font(.system(size: 12.5, weight: .medium)).foregroundStyle(.white.opacity(0.85))
                }
                .padding(.horizontal, 11).frame(height: 30).background(Capsule().fill(.black.opacity(0.4)))
                Spacer()
                Button(action: { Haptics.tap(); pump.home() }) {
                    Image(systemName: "house").font(.system(size: 13, weight: .semibold)).foregroundStyle(.white.opacity(0.9))
                        .frame(width: 32, height: 32).background(Circle().fill(.black.opacity(0.4)))
                }
                .buttonStyle(.plain).accessibilityLabel("Home")
                Button(action: { Haptics.tap(); close() }) {
                    Image(systemName: "xmark").font(.system(size: 13, weight: .semibold)).foregroundStyle(.white.opacity(0.9))
                        .frame(width: 32, height: 32).background(Circle().fill(.black.opacity(0.4)))
                }
                .buttonStyle(.plain).accessibilityLabel("Close the simulator")
            }
            .padding(.horizontal, 14).padding(.top, 6)
            Spacer()
        }
    }

    /// Starts the live picture, asks for pictures until the page goes, then stops it again.
    private func run() async {
        guard let (client, query) = SimAsk.route(store, thread) else { note = "This machine cannot be reached."; return }
        let start = query.merging(["udid": sim.udid]) { a, _ in a }
        pump.attach(client: client, query: query, udid: sim.udid)
        _ = try? await client.call("POST", "api/simview/live", query: query, body: ["udid": sim.udid, "on": true])
        defer { Task { _ = try? await client.call("POST", "api/simview/live", query: query, body: ["udid": "", "on": false]) } }
        while !Task.isCancelled {
            let began = Date()
            do {
                let data = try await client.call("GET", "api/simview/frame", query: start)
                if !data.isEmpty, let next = await Task.detached(priority: .userInitiated, operation: { UIImage(data: data) }).value {
                    image = next
                } else if image == nil {
                    note = "Waiting for the first picture…"
                }
            } catch {
                if image == nil { note = "The simulator does not answer." }
            }
            let spent = Date().timeIntervalSince(began)
            try? await Task.sleep(nanoseconds: UInt64(max(0.04, 0.2 - spent) * 1_000_000_000))
        }
    }
}

/// Sends the finger to the simulator: a begin at the first place, the moves collected a moment and sent
/// together in order, and an end. One request at a time, so the order holds.
@MainActor
final class TouchPump {
    private var client: HubClient?
    private var query: [String: String] = [:]
    private var udid = ""
    private var down = false
    private var queue: [[String: Any]] = []
    private var sending = false
    private var lastMove = Date.distantPast
    private var unsupported = false

    func attach(client: HubClient, query: [String: String], udid: String) {
        self.client = client
        self.query = query
        self.udid = udid
    }

    private func norm(_ p: CGPoint, in size: CGSize) -> (Double, Double) {
        (Double(min(1, max(0, p.x / max(1, size.width)))), Double(min(1, max(0, p.y / max(1, size.height)))))
    }

    func move(_ p: CGPoint, in size: CGSize) {
        let (x, y) = norm(p, in: size)
        if !down {
            down = true
            lastMove = Date()
            queue.append(["phase": "begin", "x": x, "y": y])
            flush()
        } else if Date().timeIntervalSince(lastMove) > 0.03 {
            lastMove = Date()
            queue.append(["phase": "move", "x": x, "y": y])
            flush()
        }
    }

    func end(_ p: CGPoint, in size: CGSize) {
        guard down else { return }
        down = false
        let (x, y) = norm(p, in: size)
        queue.append(["phase": "end", "x": x, "y": y])
        flush()
    }

    func home() {
        guard let client, !unsupported else { return }
        let q = query, body: [String: Any] = ["udid": udid, "button": "home"]
        Task { _ = try? await client.call("POST", "api/simview/input", query: q, body: body) }
    }

    private func flush() {
        guard !sending, !queue.isEmpty, let client, !unsupported else { return }
        sending = true
        let batch = Array(queue.prefix(40))
        queue.removeFirst(batch.count)
        let q = query, body: [String: Any] = ["udid": udid, "touches": batch]
        Task {
            do { _ = try await client.call("POST", "api/simview/input", query: q, body: body) }
            catch { if case HubClient.Failure.http(404, _) = error { unsupported = true } }
            sending = false
            flush()
        }
    }
}
