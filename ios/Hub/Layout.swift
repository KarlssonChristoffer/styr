import SwiftUI

/// What the two worlds share to the point: the title block, the chips row and the section headers
/// sit at the same place and size in Dev and Chat. Only colours, the title's typeface and the
/// content differ. The pill and its buttons are drawn by one `FloatingBar` for both.
enum Metrics {
    static let titleTop: CGFloat = 8
    static let titleRow: CGFloat = 42
    static let subRow: CGFloat = 18
    static let titleBottom: CGFloat = 4
    static let chipsVertical: CGFloat = 6
    static let chipHeight: CGFloat = 34
    static let sectionTop: CGFloat = 14
    static let sectionBottom: CGFloat = 6
    static let side: CGFloat = 18
}

/// A page's big title with its status line: a fixed 42 pt row and an 18 pt line, so a serif title
/// (Chat) and a bold one (Dev) leave the first row at the same Y.
struct PageHead<Trailing: View, Sub: View>: View {
    let title: String
    var reserveSub = true
    @ViewBuilder var sub: () -> Sub
    @ViewBuilder var trailing: () -> Trailing

    private var font: Font {
        Theme.world == .chat ? .system(size: 33, weight: .medium, design: .serif) : .system(size: 29, weight: .bold)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Text(title).font(font).tracking(Theme.world == .chat ? -0.3 : -0.4).lineLimit(1)
                Spacer(minLength: 8)
                trailing().font(.system(size: 20)).foregroundStyle(Hub.accent)
            }
            .frame(height: Metrics.titleRow, alignment: .center)
            if reserveSub {
                sub().frame(height: Metrics.subRow, alignment: .leading)
            }
        }
        .padding(.horizontal, Metrics.side).padding(.top, Metrics.titleTop).padding(.bottom, Metrics.titleBottom)
    }
}

extension PageHead where Trailing == EmptyView {
    init(title: String, reserveSub: Bool = true, @ViewBuilder sub: @escaping () -> Sub) {
        self.init(title: title, reserveSub: reserveSub, sub: sub, trailing: { EmptyView() })
    }
}

/// The wing of Hermes, drawn as strokes on a 24 x 24 grid: a leading edge and three feathers.
struct HermesWing: Shape {
    func path(in rect: CGRect) -> Path {
        let k = min(rect.width, rect.height) / 24
        let ox = rect.minX + (rect.width - 24 * k) / 2, oy = rect.minY + (rect.height - 24 * k) / 2
        func P(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: ox + x * k, y: oy + y * k) }
        var p = Path()
        p.move(to: P(3.5, 20))
        p.addCurve(to: P(21, 4), control1: P(3.5, 9.5), control2: P(11, 3.5))
        p.move(to: P(6, 16.5))
        p.addCurve(to: P(20.5, 9), control1: P(9, 10.5), control2: P(14.5, 8.5))
        p.move(to: P(9.5, 19))
        p.addCurve(to: P(21, 14), control1: P(12.5, 14.5), control2: P(17, 13.5))
        p.move(to: P(13.5, 20.5))
        p.addCurve(to: P(20, 18.5), control1: P(15.5, 18.5), control2: P(18, 18))
        return p
    }
}
