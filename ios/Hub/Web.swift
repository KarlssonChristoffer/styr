import SwiftUI

// A web search, shown as one: your message with a globe, "Searching the web…" with the pages the
// agent finds while it looks, and the sources as cards under the answer (sketch "Webbsökning i
// chatten"). A chat may always use the internet, so there is no web switch any more and nothing new
// is sent with the instruction; chats searched with it earlier still show as searches, and the
// instruction is never shown.
// The desktop has the same rules in app/src/lib/web.ts; the instruction must stay word for word
// the same in both, so a search asked on one shows right on the other. Older wordings stay in
// `earlier`, so searches sent with them still show as searches.

enum Web {
    static let prefix = "Search the web for this. Open and read the pages you rely on rather than guessing from search snippets, then answer briefly and give the sources as links: "
    static let earlier = ["Search the web for this, answer briefly and give the sources as links: "]

    /// Your own words of a web search, nil for any other message.
    static func asked(_ text: String) -> String? {
        for p in [prefix] + earlier where text.hasPrefix(p) { return String(text.dropFirst(p.count)) }
        return nil
    }

    /// A step that searched the web or read a page from it.
    static func isStep(_ step: Step) -> Bool {
        if step.kind == "fetch" { return true }
        let t = step.title.lowercased()
        return t.contains("websearch") || t.contains("web search") || t.contains("web_search") || t.hasPrefix("fetch http")
            || t.hasPrefix("searching for")
    }

    /// The pages a turn's web steps found or read, newest last, one per site.
    static func pages(_ items: [Item]) -> [URL] {
        var hosts = Set<String>()
        var out: [URL] = []
        for case let .work(steps) in items {
            for step in steps where isStep(step) {
                for raw in links(in: step.title) + links(in: step.command ?? "") + links(in: step.output ?? "") {
                    guard let url = URL(string: raw), let host = url.host, real(host) else { continue }
                    if hosts.insert(Sources.name(url)).inserted { out.append(url) }
                }
            }
        }
        return out
    }

    /// Whether a turn has searched the web so far.
    static func searched(_ items: [Item]) -> Bool {
        items.contains { if case let .work(steps) = $0 { return steps.contains(where: isStep) }; return false }
    }

    /// An answer to a web search: its text without the list of sources it ends on, and the links
    /// it gives (that list's and those in the text), which show as cards instead.
    static func answer(_ text: String) -> (body: String, links: [URL]) {
        // One card a site: a card shows the site's name.
        var seen = Set<String>()
        var urls: [URL] = []
        for raw in links(in: text) {
            guard let url = URL(string: raw), let host = url.host, real(host), seen.insert(Sources.name(url)).inserted else { continue }
            urls.append(url)
        }
        guard !urls.isEmpty else { return (text, []) }
        var lines = text.components(separatedBy: "\n")
        var cut = false
        while let last = lines.last {
            let line = last.trimmingCharacters(in: .whitespaces)
            if line.isEmpty || onlyLinks(line) { lines.removeLast(); if !line.isEmpty { cut = true }; continue }
            if cut && heading(line) { lines.removeLast(); break }
            break
        }
        let body = cut ? lines.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines) : text
        return (body.isEmpty ? text : body, urls)
    }

    /// A line that is nothing but a link or two: "- [SCB](https://scb.se)", "1. https://…", "Källa: …".
    private static func onlyLinks(_ line: String) -> Bool {
        guard !links(in: line).isEmpty else { return false }
        var rest = line
        for pattern in [#"\[[^\]]*\]\([^)]*\)"#, #"<?https?://[^\s"'`<>)\]]+>?"#, #"^\s*([-*•]|\d+[.)])\s*"#] {
            rest = rest.replacingOccurrences(of: pattern, with: "", options: .regularExpression)
        }
        rest = rest.replacingOccurrences(of: #"^\s*\*{0,2}(sources?|källor|källa|references?)\*{0,2}\s*:?\*{0,2}"#, with: "", options: [.regularExpression, .caseInsensitive])
        // What is left may be a short label or separators, not a sentence.
        return rest.trimmingCharacters(in: .whitespacesAndNewlines.union(.punctuationCharacters)).count <= 24
    }

    private static func heading(_ line: String) -> Bool {
        line.range(of: #"^(#+\s*)?\**\s*(sources?|källor|references?)\s*:?\s*\**\s*:?$"#, options: [.regularExpression, .caseInsensitive]) != nil
    }

    private static func real(_ host: String) -> Bool {
        !host.isEmpty && host != "localhost" && !host.hasPrefix("127.") && !host.hasPrefix("192.168.") && !host.hasPrefix("100.")
    }

    static func links(in text: String) -> [String] {
        guard let re = try? NSRegularExpression(pattern: #"https?://[^\s"'`<>)\]]+"#) else { return [] }
        let range = NSRange(text.startIndex..., in: text)
        return re.matches(in: text, range: range).compactMap { m in
            Range(m.range, in: text).map { String(text[$0]).trimmingCharacters(in: CharacterSet(charactersIn: ".,;:")) }
        }
    }

    /// A small square with the site's first letter, tinted by the site, in place of its icon.
    static func tint(_ url: URL) -> Color {
        let name = Sources.name(url)
        let hues: [Double] = [0.58, 0.28, 0.12, 0.83, 0.47, 0.03]
        let h = name.unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) & 0xffff }
        return Color(hue: hues[h % hues.count], saturation: 0.32, brightness: 0.88)
    }
}

/// The globe on your message of a web search.
struct WebTag: View {
    var body: some View {
        Label("Web", systemImage: "globe")
            .font(.system(size: 11.5, weight: .semibold))
            .foregroundStyle(Hub.accent)
            .labelStyle(TightLabel())
    }
}

private struct TightLabel: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 4) { configuration.icon.font(.system(size: 11, weight: .semibold)); configuration.title }
    }
}

/// What a web search looks like while the agent looks: the working grid, how long, and the sites it
/// has found so far, each one fading in as it comes.
struct WebSearching: View {
    let start: Double
    let pages: [URL]

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(spacing: 8) {
                Working(tint: Hub.accent, size: 15)
                Text("Searching the web…").font(.system(size: 14))
                if start > 0 {
                    TimelineView(.periodic(from: .now, by: 1)) { _ in
                        Text(ThreadView.duration(Date().timeIntervalSince1970 * 1000 - start))
                            .font(.system(size: 13).monospacedDigit()).foregroundStyle(Hub.muted)
                    }
                }
                Spacer(minLength: 0)
            }
            .foregroundStyle(Hub.accent)
            if !pages.isEmpty {
                FlowLayout(spacing: 6) {
                    ForEach(pages.suffix(6), id: \.absoluteString) { url in
                        Text(Sources.name(url))
                            .font(.system(size: 11.5)).foregroundStyle(Hub.text3)
                            .padding(.horizontal, 8).padding(.vertical, 2)
                            .background(Capsule().fill(Hub.surface))
                            .overlay(Capsule().strokeBorder(Hub.line))
                            .transition(.opacity.combined(with: .offset(y: 4)))
                    }
                }
                .padding(.leading, 23)
                .animation(.easeOut(duration: 0.35), value: pages)
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }
}

/// The sources of a web search as small cards under the answer; a tap opens the page.
struct SourceCards: View {
    let urls: [URL]

    var body: some View {
        FlowLayout(spacing: 6) {
            ForEach(urls.prefix(8), id: \.absoluteString) { url in
                Button { UIApplication.shared.open(url); Haptics.tap() } label: {
                    HStack(spacing: 6) {
                        Text(String(Sources.name(url).prefix(1)).uppercased())
                            .font(.system(size: 9, weight: .bold)).foregroundStyle(Hub.bg)
                            .frame(width: 16, height: 16)
                            .background(RoundedRectangle(cornerRadius: 4).fill(Web.tint(url)))
                        Text(Sources.name(url)).font(.system(size: 12.5)).foregroundStyle(Hub.text2).lineLimit(1)
                    }
                    .padding(.horizontal, 9).padding(.vertical, 5)
                    .background(RoundedRectangle(cornerRadius: 10).fill(Hub.surface))
                    .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Hub.line))
                }
                .buttonStyle(.pressFlat)
            }
        }
    }
}
