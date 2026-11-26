import SwiftUI

// The working indicator: the 2 by 2 grid of the desktop's System icon playing a little loop wherever
// something works (`WorkingLine`, the tool group, the inbox dot, web search, busy buttons). It is the
// same set as the desktop's `app/src/lib/workGrid.ts`, and each loop is a function of the time that
// repeats within 5.2 s, so nothing needs state. Grey is the grid and the tint is only what works.
// Pour plays everywhere (`WorkShape.pick`), as on the desktop by default. Reduce Motion holds it still.

/// One tile of the grid in a 24 by 24 box. `f` is how filled it is, `g` how much of its outline is
/// the tint, `o` its opacity.
struct WorkTile {
    var x: Double
    var y: Double
    var w: Double = 7
    var h: Double = 7
    var r: Double = 1.6
    var f: Double = 0
    var g: Double = 0
    var o: Double = 1
    /// A part of a tile at rest (Pour, Crumble): drawn clipped to that slot's rounded tile, so a tile
    /// made of parts has the same round corners as a whole one and no seams.
    var clip: Int? = nil
    /// A part, seated or on its way: drawn solid, with no outline.
    var part = false

    static let hidden = WorkTile(x: 0, y: 0, w: 0, h: 0, r: 0, f: 0, g: 0, o: 0)
}

enum WorkShape: String, CaseIterable {
    case pour, crumble, orbit, puzzle, breathe, hop, turn, micro, fill, flip, bounce, swap, shrink

    /// Every loop repeats within this many milliseconds.
    static let loop = 5200.0

    /// The loop something working plays: Pour, the desktop's default (Christoffer's pick). `seed`
    /// stays for a random pick per turn, which the desktop still offers in Settings.
    static func pick(seed: String) -> WorkShape { .pour }

    /// The outline's width in the 24 box; the micro loop has none.
    var stroke: Double {
        switch self {
        case .micro: return 0
        case .pour, .crumble: return 1.5
        default: return 1.7
        }
    }

    /// How far the whole grid is turned, in degrees.
    func turned(_ t: Double) -> Double {
        guard self == .turn else { return 0 }
        return 90 * (floor(t / 1300) + Self.back(Self.cl((t.truncatingRemainder(dividingBy: 1300)) / 520)))
    }

    // MARK: the loops

    func tiles(_ t: Double) -> [WorkTile] {
        let ring = Self.ring
        switch self {
        case .pour:
            return Self.parted(t, cells: true, stagger: 0.018, dur: 0.38, gap: Self.cellGap)
        case .crumble:
            return Self.parted(t, cells: false, stagger: 0.1, dur: 0.45, gap: 0.5)
        case .orbit:
            return (0..<4).map { (s: Int) -> WorkTile in
                var b = Self.tile(s)
                let k = Double(ring.firstIndex(of: s) ?? 0)
                var d = (t / 2600 * 4 - k).truncatingRemainder(dividingBy: 4)
                if d < 0 { d += 4 }
                let l = pow(max(0, 1 - d / 2.4), 1.2)
                b.f = l
                b.g = l
                return b
            }
        case .puzzle:
            // Three tiles and a gap: one tile slides into the gap at a time, clockwise.
            let n = Int(t / 650) % 4
            var pos = [0, 1, 2]
            var gap = 3
            for _ in 0..<n {
                if let i = pos.firstIndex(of: (gap + 3) % 4) { pos[i] = gap }
                gap = (gap + 3) % 4
            }
            let mover = pos.firstIndex(of: (gap + 3) % 4) ?? 0
            let f = Self.cl(((t.truncatingRemainder(dividingBy: 650)) / 650) / 0.65)
            let e = Self.back(f)
            return pos.enumerated().map { (pair: (offset: Int, element: Int)) -> WorkTile in
                let i = pair.offset
                let p = pair.element
                var b = Self.tile(ring[p])
                if i == mover {
                    let a = Self.slot[ring[p]]
                    let c = Self.slot[ring[gap]]
                    b.x = Self.lerp(a.0, c.0, e)
                    b.y = Self.lerp(a.1, c.1, e)
                    b.f = 0.3 * Self.bell(f)
                    b.g = Self.bell(f)
                }
                return b
            } + [WorkTile.hidden]
        case .breathe:
            // The grid turns tall, wide, soft and back; the corner tile breathes.
            let L = Self.loop / 6
            let i = Int(t / L) % 6
            let local = t.truncatingRemainder(dividingBy: L)
            let e = Self.back(Self.cl(local / 300))
            let a = Self.shapeState((i + 5) % 6)
            let b = Self.shapeState(i)
            return (0..<4).map { (k: Int) -> WorkTile in
                let A = a[k]
                let B = b[k]
                return WorkTile(x: Self.lerp(A.x, B.x, e), y: Self.lerp(A.y, B.y, e), w: Self.lerp(A.w, B.w, e), h: Self.lerp(A.h, B.h, e),
                                r: max(0, Self.lerp(A.r, B.r, e)), f: 0,
                                g: k == 3 ? 0.5 + 0.5 * sin(t / 2600 * 2 * Double.pi) : 0,
                                o: Self.lerp(A.o, B.o, Self.cl(local / 300 * 1.5)))
            } + [WorkTile.hidden]
        case .hop:
            // One lit tile hops from slot to slot around the grid.
            let k = Int(t / 650) % 4
            let f = Self.cl((t.truncatingRemainder(dividingBy: 650)) / 450)
            let e = Self.back(f)
            let a = Self.slot[ring[k]]
            let c = Self.slot[ring[(k + 1) % 4]]
            let lit = WorkTile(x: Self.lerp(a.0, c.0, e), y: Self.lerp(a.1, c.1, e) - 4 * Self.bell(f), f: 1, g: 1)
            return (0..<4).map { Self.tile($0) } + [lit]
        case .turn:
            var lit = Self.tile(1)
            lit.f = 1
            lit.g = 1
            return [Self.tile(0), lit, Self.tile(2), Self.tile(3), WorkTile.hidden]
        case .micro:
            func p(_ a: Double) -> Double { 0.2 + 0.8 * (0.5 + 0.5 * sin(t / 1300 * 2 * Double.pi + a)) }
            func d(_ x: Double, _ y: Double, _ a: Double) -> WorkTile { WorkTile(x: x, y: y, w: 8, h: 8, r: 2, f: 1, g: 1, o: p(a)) }
            return [d(3, 3, 0), d(13, 3, Double.pi), d(3, 13, Double.pi), d(13, 13, 0), WorkTile.hidden]
        case .fill:
            let tm = t.truncatingRemainder(dividingBy: 2600)
            return (0..<4).map { (s: Int) -> WorkTile in
                let k = Double(ring.firstIndex(of: s) ?? 0)
                let f = Self.cl((tm - k * 325) / 220) - Self.cl((tm - (4 + k) * 325) / 220)
                var b = Self.tile(s)
                b.f = f
                b.g = f
                return b
            } + [WorkTile.hidden]
        case .flip:
            let tm = t.truncatingRemainder(dividingBy: 2600)
            return (0..<4).map { (s: Int) -> WorkTile in
                let k = Double(ring.firstIndex(of: s) ?? 0)
                let pa = Self.cl((tm - k * 150) / 450)
                let pb = Self.cl((tm - 1300 - k * 150) / 450)
                let on: Double = (pa > 0.5 && pb <= 0.5) ? 1 : 0
                var b = Self.tile(s)
                b.f = on
                b.g = on
                return Self.scaled(b, max(abs(cos(Double.pi * pa)) * abs(cos(Double.pi * pb)), 0.02), 1)
            } + [WorkTile.hidden]
        case .bounce:
            let tm = t.truncatingRemainder(dividingBy: 1300)
            return (0..<4).map { (s: Int) -> WorkTile in
                let k = Double(ring.firstIndex(of: s) ?? 0)
                let b = Self.bell((tm - k * 140) / 520)
                var tile = Self.tile(s)
                tile.y = Self.slot[s].1 - 4.5 * b
                tile.f = 0.25 * b
                tile.g = b
                return tile
            } + [WorkTile.hidden]
        case .swap:
            // Two neighbours trade places and pass each other, once round the grid.
            let j = Int(t / 650) % 4
            let p = Self.cl((t.truncatingRemainder(dividingBy: 650)) / 520)
            let e = Self.back(p)
            let pairs = [(0, 1), (1, 3), (3, 2), (2, 0)]
            let (a, c) = pairs[j]
            let horizontal = Self.slot[a].1 == Self.slot[c].1
            var out = (0..<4).map { Self.tile($0) }
            func move(_ from: Int, _ to: Int, _ side: Double) -> WorkTile {
                let A = Self.slot[from]
                let B = Self.slot[to]
                let bump = 4 * Self.bell(p) * side
                var b = Self.tile(from)
                b.x = Self.lerp(A.0, B.0, e) + (horizontal ? 0 : bump)
                b.y = Self.lerp(A.1, B.1, e) + (horizontal ? bump : 0)
                b.f = 0.25 * Self.bell(p)
                b.g = Self.bell(p)
                return b
            }
            out[a] = move(a, c, -1)
            out[c] = move(c, a, 1)
            return out + [WorkTile.hidden]
        case .shrink:
            let tm = t.truncatingRemainder(dividingBy: 1300)
            return (0..<4).map { (s: Int) -> WorkTile in
                let k = Double(ring.firstIndex(of: s) ?? 0)
                let b = Self.bell((tm - k * 140) / 600)
                var tile = Self.tile(s)
                tile.r = Self.lerp(1.6, 3.5, b)
                tile.f = b * b
                tile.g = b
                return Self.scaled(tile, 1 - 0.62 * b, 1 - 0.62 * b)
            } + [WorkTile.hidden]
        }
    }

    // MARK: the parted loops

    /// One move of Pour or Crumble; four make a loop.
    private static let step = loop / 4
    private static let cellSize = 1.45
    private static let cellGap = 0.4
    private static let quarter = 3.25

    /// A tile split into 4 by 4 cells (Pour) or 2 by 2 quarters (Crumble), at the tile's place.
    private static func parts(_ bx: Double, _ by: Double, cells: Bool) -> [WorkTile] {
        if cells {
            return (0..<16).map { k in
                WorkTile(x: bx + Double(k % 4) * (cellSize + cellGap), y: by + Double(k / 4) * (cellSize + cellGap), w: cellSize, h: cellSize, r: 0.5, f: 1, g: 1)
            }
        }
        return (0..<4).map { k in
            WorkTile(x: bx + Double(k % 2) * (quarter + 0.5), y: by + Double(k / 2) * (quarter + 0.5), w: quarter, h: quarter, r: 0.9, f: 1, g: 1)
        }
    }

    /// A part at rest grows over the gaps, and a little over its neighbour so soft edges leave no
    /// seam, and is clipped to its tile.
    private static func seat(_ c: WorkTile, _ gap: Double) -> WorkTile {
        var o = c
        o.w = c.w + gap + 0.35
        o.h = c.h + gap + 0.35
        o.r = 0
        o.clip = (c.x >= 12 ? 1 : 0) + (c.y >= 12 ? 2 : 0)
        o.part = true
        return o
    }

    /// Three filled tiles and a gap. One tile at a time moves into the gap in parts, those nearest
    /// the gap first, and is one whole tile again where it lands.
    private static func parted(_ t: Double, cells: Bool, stagger: Double, dur: Double, gap: Double) -> [WorkTile] {
        let n = Int(t / step) % 4
        var pos = [0, 1, 2]
        var hole = 3
        for _ in 0..<n {
            if let i = pos.firstIndex(of: (hole + 3) % 4) { pos[i] = hole }
            hole = (hole + 3) % 4
        }
        let mover = pos.firstIndex(of: (hole + 3) % 4) ?? 0
        let p = t.truncatingRemainder(dividingBy: step) / step
        var out: [WorkTile] = []
        func whole(_ s: Int) -> WorkTile { var b = tile(s); b.f = 0.85; b.g = 1; return b }
        for (i, q) in pos.enumerated() {
            if i != mover { out.append(whole(ring[q])); continue }
            let (bx, by) = slot[ring[hole]]
            let A = parts(slot[ring[q]].0, slot[ring[q]].1, cells: cells)
            let B = parts(bx, by, cells: cells)
            let last = cl((p - Double(A.count - 1) * stagger) / dur)
            if p <= 0 { out.append(whole(ring[q])); continue }
            // A part close to its seat takes it, so a landing tile never shows the grid.
            if last >= 0.8 { out.append(whole(ring[hole])); continue }
            let order = A.indices.sorted { hypot(A[$0].x - bx, A[$0].y - by) < hypot(A[$1].x - bx, A[$1].y - by) }
            for (rank, k) in order.enumerated() {
                let f = cl((p - Double(rank) * stagger) / dur)
                if f <= 0 { out.append(seat(A[k], gap)); continue }
                if f >= 0.8 { out.append(seat(B[k], gap)); continue }
                let e = back(f)
                var m = A[k]
                m.x = lerp(A[k].x, B[k].x, e)
                m.y = lerp(A[k].y, B[k].y, e)
                m.w = A[k].w + gap + 0.35
                m.h = A[k].h + gap + 0.35
                m.r = gap > 0.45 ? 1.2 : 0.55
                m.part = true
                out.append(m)
            }
        }
        return out
    }

    // MARK: helpers

    /// Slots in the box: 0 top left, 1 top right, 2 bottom left, 3 bottom right.
    private static let slot: [(Double, Double)] = [(4, 4), (13, 4), (4, 13), (13, 13)]
    /// The slots clockwise.
    private static let ring = [0, 1, 3, 2]

    /// Where slot `s` sits in the 24 box, for drawing a part clipped to it.
    static func slotOrigin(_ s: Int) -> (Double, Double) { slot[s] }

    private static func tile(_ s: Int) -> WorkTile { WorkTile(x: slot[s].0, y: slot[s].1) }
    private static func lerp(_ a: Double, _ b: Double, _ t: Double) -> Double { a + (b - a) * t }
    private static func cl(_ x: Double) -> Double { max(0, min(1, x)) }
    /// 0 to 1 and back to 0 as `x` goes from 0 to 1.
    private static func bell(_ x: Double) -> Double { sin(Double.pi * cl(x)) }
    /// An ease that overshoots a little and settles.
    private static func back(_ t: Double) -> Double { 1 + 2.5 * pow(t - 1, 3) + 1.5 * pow(t - 1, 2) }
    private static func scaled(_ b: WorkTile, _ sx: Double, _ sy: Double) -> WorkTile {
        var o = b
        o.x = b.x + b.w * (1 - sx) / 2
        o.y = b.y + b.h * (1 - sy) / 2
        o.w = b.w * sx
        o.h = b.h * sy
        return o
    }

    /// The four tiles of the grid in the shape step `k` of the breathe loop: 1 tall, 3 wide, 5 soft, the rest the plain grid.
    private static func shapeState(_ k: Int) -> [WorkTile] {
        var g = (0..<4).map { tile($0) }
        switch k {
        case 1:
            g[0] = WorkTile(x: 4, y: 4, w: 7, h: 16)
            g[2].o = 0
        case 3:
            g[0] = WorkTile(x: 4, y: 4, w: 16, h: 7)
            g[1].o = 0
        case 5:
            for s in 0..<4 { g[s].r = s == 3 ? 3.5 : 2.6 }
        default:
            break
        }
        return g
    }
}

/// The grid, drawn at `size` points. `tint` is the colour of what works.
struct WorkGrid: View {
    let shape: WorkShape
    var tint: Color
    var size: CGFloat = 16
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(paused: reduceMotion)) { ctx in
            let t = reduceMotion ? 0 : (ctx.date.timeIntervalSinceReferenceDate * 1000).truncatingRemainder(dividingBy: WorkShape.loop)
            Canvas { gc, sz in
                let k = sz.width / 24
                var c = gc
                c.translateBy(x: sz.width / 2, y: sz.height / 2)
                c.rotate(by: .degrees(shape.turned(t)))
                c.translateBy(x: -sz.width / 2, y: -sz.height / 2)
                let half = shape.stroke / 2
                let tiles = shape.tiles(t)
                // Parts (Pour, Crumble) go in one layer: solid inside, so where two overlap there is no
                // seam, and the layer as a whole takes a whole tile's fill. A seated part is clipped to its tile.
                let parts = tiles.filter { $0.part && $0.w > 0 && $0.h > 0 }
                if !parts.isEmpty {
                    var layer = c
                    layer.opacity = 0.85
                    layer.drawLayer { l in
                        for b in parts {
                            var q = l
                            if let s = b.clip {
                                let o = WorkShape.slotOrigin(s)
                                q.clip(to: Path(roundedRect: CGRect(x: (o.0 - half) * k, y: (o.1 - half) * k, width: (7 + shape.stroke) * k, height: (7 + shape.stroke) * k), cornerRadius: (1.6 + half) * k))
                                let x = b.x <= o.0 + 0.01 ? b.x - half : b.x
                                let y = b.y <= o.1 + 0.01 ? b.y - half : b.y
                                let w = b.w + (b.x - x) + (b.x + b.w >= o.0 + 6.99 ? half : 0)
                                let h = b.h + (b.y - y) + (b.y + b.h >= o.1 + 6.99 ? half : 0)
                                q.fill(Path(CGRect(x: x * k, y: y * k, width: w * k, height: h * k)), with: .color(tint))
                            } else {
                                q.fill(Path(roundedRect: CGRect(x: b.x * k, y: b.y * k, width: b.w * k, height: b.h * k), cornerRadius: max(0, b.r) * k), with: .color(tint))
                            }
                        }
                    }
                }
                for b in tiles where !b.part && b.w > 0 && b.h > 0 && b.o > 0 {
                    let rect = CGRect(x: b.x * k, y: b.y * k, width: b.w * k, height: b.h * k)
                    let path = Path(roundedRect: rect, cornerRadius: max(0, b.r) * k)
                    if b.f > 0 { c.fill(path, with: .color(tint.opacity(min(1, b.f) * b.o))) }
                    if shape.stroke > 0 {
                        let line = StrokeStyle(lineWidth: shape.stroke * k)
                        c.stroke(path, with: .color(Hub.muted.opacity((1 - min(1, b.g)) * b.o)), style: line)
                        if b.g > 0 { c.stroke(path, with: .color(tint.opacity(min(1, b.g) * b.o)), style: line) }
                    }
                }
            }
            .frame(width: size, height: size)
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

/// "Something is going on here", the one way Hub says it: the grid playing Pour, in place of every
/// system spinner where something works (a send, a merge, a run, the inbox dot, the tool group).
/// The tint is green in Dev and the accent in Chat, unless a button asks for its own.
struct Working: View {
    var tint: Color? = nil
    var size: CGFloat = 14

    var body: some View {
        WorkGrid(shape: .pour, tint: tint ?? (Theme.world == .chat ? Hub.accent : Hub.green), size: size)
    }
}
