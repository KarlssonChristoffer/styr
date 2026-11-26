import AVFoundation
import SwiftUI

/// First start (sketch 5, screens 3 and 4): scan the code from the PC, then a check of what usually
/// goes wrong, and into the inbox. None of it is needed again.
struct Onboarding: View {
    @Environment(Store.self) private var store
    @State private var typing = false
    @State private var link = ""
    @State private var busy = false
    @State private var paired = false
    var done: () -> Void

    var body: some View {
        Group {
            if paired { AlmostThere(done: done) } else { scan }
        }
        .background(Hub.bg)
    }

    private var scan: some View {
        VStack(spacing: 0) {
            Spacer().frame(height: 50)
            Image(systemName: "point.3.connected.trianglepath.dotted")
                .font(.system(size: 40, weight: .light)).foregroundStyle(Hub.accent)
            Text("Hub").font(.system(size: 32, weight: .bold)).padding(.top, 14)
            Text("Connect this phone to your machines.").font(.system(size: 16)).foregroundStyle(Hub.text3).padding(.top, 6)

            ZStack {
                RoundedRectangle(cornerRadius: 26).fill(Color.black)
                if !typing && !busy {
                    QRScanner { code in pair(code) }.clipShape(RoundedRectangle(cornerRadius: 26))
                }
                if busy { Working(tint: Hub.accent, size: 30) }
                Corners().stroke(Hub.accent, style: StrokeStyle(lineWidth: 4, lineCap: .round))
            }
            .frame(width: 240, height: 240)
            .padding(.top, 40)

            Text(LocalizedStringKey(store.error ?? "On the PC, open **Settings › Access** in Hub and point the camera at the code under **Your phone**."))
                .font(.system(size: 15)).foregroundStyle(store.error == nil ? Hub.text2 : Hub.red)
                .multilineTextAlignment(.center).padding(.horizontal, 36).padding(.top, 34)

            Spacer()
            if typing {
                HStack(spacing: 8) {
                    TextField("", text: $link, prompt: Text("https://…ts.net/phone#pair=…").foregroundStyle(Hub.muted))
                        .textInputAutocapitalization(.never).autocorrectionDisabled().font(.system(size: 14))
                    Button("Pair") { pair(link) }.foregroundStyle(Hub.accent).fontWeight(.semibold).disabled(link.isEmpty || busy)
                }
                .padding(.horizontal, 14).frame(height: 50)
                .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface2))
                .padding(.horizontal, 18).padding(.bottom, 10)
            }
            Button { withAnimation { typing.toggle() } } label: {
                Text(typing ? "Use the camera" : "Type the code instead").font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(Hub.text).frame(maxWidth: .infinity).frame(height: 54)
                    .background(RoundedRectangle(cornerRadius: 15).fill(Hub.surface2))
            }
            .padding(.horizontal, 18).padding(.bottom, 14)
            Button("Look around with made-up threads first") { store.useDemo(); done() }
                .font(.system(size: 13)).foregroundStyle(Hub.muted).padding(.bottom, 10)
        }
    }

    private func pair(_ code: String) {
        guard !busy else { return }
        busy = true
        Task {
            await store.pair(link: code)
            busy = false
            if !store.isDemo && store.home != nil { withAnimation { paired = true } }
        }
    }
}

/// The scanner's four corners.
struct Corners: Shape {
    func path(in r: CGRect) -> Path {
        var p = Path()
        let l: CGFloat = 44, c: CGFloat = 26
        for (x, y, dx, dy) in [(r.minX, r.minY, 1.0, 1.0), (r.maxX, r.minY, -1.0, 1.0), (r.minX, r.maxY, 1.0, -1.0), (r.maxX, r.maxY, -1.0, -1.0)] {
            p.move(to: CGPoint(x: x, y: y + dy * (c + l)))
            p.addLine(to: CGPoint(x: x, y: y + dy * c))
            p.addQuadCurve(to: CGPoint(x: x + dx * c, y: y), control: CGPoint(x: x, y: y))
            p.addLine(to: CGPoint(x: x + dx * (c + l), y: y))
        }
        return p
    }
}

/// "Almost there": what usually goes wrong, checked, then the inbox.
struct AlmostThere: View {
    @Environment(Store.self) private var store
    var done: () -> Void
    @State private var checked = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Almost there").font(.system(size: 29, weight: .bold)).padding(.top, 30)
            Text("Paired with Hub on \(store.machine ?? "your machine").").font(.system(size: 15)).foregroundStyle(Hub.text3).padding(.top, 4)
            Text("Checking").font(.system(size: 12, weight: .semibold)).foregroundStyle(Hub.muted).padding(.top, 26).padding(.leading, 12).padding(.bottom, 6)
            VStack(spacing: 0) {
                check("Tailscale is on", ok: store.anyLive || checked ? store.anyLive : nil)
                check("Same tailnet as \(store.machine ?? "the PC")", ok: store.anyLive || checked ? store.anyLive : nil)
                let reached = store.machines.filter { store.live[$0.url] == true }.count
                let known = max(store.machines.count, store.hosts.count)
                check("Reaches \(reached) of \(known) machines", ok: checked ? reached == known : nil)
                ForEach(store.hosts.filter { $0.state != "online" }) { h in
                    check("\(h.name) isn't reachable", ok: false, note: "later")
                }
            }
            .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
            Spacer()
            Button(action: done) {
                Text("Open the inbox").font(.system(size: 16, weight: .semibold)).foregroundStyle(Hub.bg)
                    .frame(maxWidth: .infinity).frame(height: 54)
                    .background(RoundedRectangle(cornerRadius: 15).fill(Hub.accent))
            }
            .padding(.bottom, 14)
        }
        .padding(.horizontal, 18)
        .task {
            await store.loadMachines()
            try? await Task.sleep(for: .seconds(3))
            checked = true
        }
    }

    private func check(_ label: String, ok: Bool?, note: String? = nil) -> some View {
        HStack(spacing: 12) {
            if let ok { Circle().fill(ok ? Hub.green : Hub.red).frame(width: 8, height: 8) }
            else { Working(tint: Hub.green, size: 11) }
            Text(label).font(.system(size: 15))
            Spacer()
            if let note { Text(note).font(.system(size: 13)).foregroundStyle(Hub.muted) }
        }
        .padding(.horizontal, 16).frame(minHeight: 48)
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 36) }
    }
}
