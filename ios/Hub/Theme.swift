import SwiftUI

/// Hub's tokens (app/src/app.css and the iPhone sketches in iphone-app/putsad): dark, calm and
/// dense, colour only for state.
extension Color {
    init(hex: UInt32, opacity: Double = 1) {
        self.init(.sRGB,
                  red: Double((hex >> 16) & 0xff) / 255,
                  green: Double((hex >> 8) & 0xff) / 255,
                  blue: Double(hex & 0xff) / 255,
                  opacity: opacity)
    }
}

/// The two worlds of the app: Dev as it is, Chat as its everyday sibling in a warm theme.
enum World: String { case dev, chat }

struct Palette {
    var bg, side, surface, surface2, line, lineStrong, text, text2, text3, muted, accent: Color
    var accentBg: Color
}

/// The theme of the world on screen. Every `Hub.*` colour reads it, so a view built after the world
/// changes is drawn in the new colours.
enum Theme {
    nonisolated(unsafe) static var world: World = .dev

    static let dev = Palette(
        bg: Color(hex: 0x1a1918), side: Color(hex: 0x151413), surface: Color(hex: 0x211f1e), surface2: Color(hex: 0x242220),
        line: Color(hex: 0x2a2826), lineStrong: Color(hex: 0x3a3734), text: Color(hex: 0xece8e2), text2: Color(hex: 0xd3cfc8),
        text3: Color(hex: 0xbdb8b0), muted: Color(hex: 0x958f88), accent: Color(hex: 0xa5abee), accentBg: Color(hex: 0xa5abee, opacity: 0.15))

    /// "Warm": the Chat world.
    static let warm = Palette(
        bg: Color(hex: 0x201b16), side: Color(hex: 0x1a1612), surface: Color(hex: 0x2b251f), surface2: Color(hex: 0x30291f),
        line: Color(hex: 0x352e27), lineStrong: Color(hex: 0x473e34), text: Color(hex: 0xf1e9de), text2: Color(hex: 0xcbbfae),
        text3: Color(hex: 0xb8ac9b), muted: Color(hex: 0xa09484), accent: Color(hex: 0xe2b883), accentBg: Color(hex: 0xe2b883, opacity: 0.16))

    static var p: Palette { world == .dev ? dev : warm }
}

enum Hub {
    static var bg: Color { Theme.p.bg }
    static var side: Color { Theme.p.side }
    static var surface: Color { Theme.p.surface }
    static var surface2: Color { Theme.p.surface2 }
    static var line: Color { Theme.p.line }
    static var lineStrong: Color { Theme.p.lineStrong }
    static var text: Color { Theme.p.text }
    static var text2: Color { Theme.p.text2 }
    static var text3: Color { Theme.p.text3 }
    static var muted: Color { Theme.p.muted }
    static var accent: Color { Theme.p.accent }
    static var accentBg: Color { Theme.p.accentBg }
    static let green = Color(hex: 0x84c29a)
    static let greenBg = Color(hex: 0x84c29a, opacity: 0.1)
    static let amber = Color(hex: 0xddaa5e)
    static let amberBg = Color(hex: 0xddaa5e, opacity: 0.16)
    static let red = Color(hex: 0xe5806f)
    static let redBg = Color(hex: 0xe5806f, opacity: 0.1)

    /// Project folder colours.
    static let projectColors: [Color] = [
        Color(hex: 0xa99cf0), Color(hex: 0x7fa6ee), Color(hex: 0x6cc7b8),
        Color(hex: 0xec9a64), Color(hex: 0xee8a9a), Color(hex: 0xcdb48d),
    ]

    static func projectColor(_ name: String) -> Color {
        let sum = name.unicodeScalars.reduce(0) { $0 + Int($1.value) }
        return projectColors[sum % projectColors.count]
    }
}
