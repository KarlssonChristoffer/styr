// The agent's text set as Markdown blocks (headings, lists, tables, quotes).
import SwiftUI
import WebKit

/// The agent's text, set as blocks: code, headings, lists, quotes, tables and pictures, with the
/// rest as Markdown paragraphs. Hairlines and grey type, colour only where it means something.
struct Prose: View {
    let text: String

    var body: some View {
        let _ = Perf.count("Prose.body")
        let blocks = MarkdownBlock.cached(text)
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                BlockView(block: block)
            }
        }
    }
}

/// Parsed texts kept by their words: a page rebuilds its messages whenever anything on it changes
/// (a panel opens, a pull ends), and parsing every reply again each time was a stutter in a long thread.
private final class ParsedBlocks { let list: [MarkdownBlock]; init(_ list: [MarkdownBlock]) { self.list = list } }
private final class ParsedInline { let text: AttributedString; init(_ text: AttributedString) { self.text = text } }
private let blockCache: NSCache<NSString, ParsedBlocks> = {
    let c = NSCache<NSString, ParsedBlocks>()
    c.countLimit = 300
    return c
}()
private let inlineCache: NSCache<NSString, ParsedInline> = {
    let c = NSCache<NSString, ParsedInline>()
    c.countLimit = 1000
    return c
}()

enum MarkdownBlock: Hashable {
    case code(String)
    case heading(Int, String)
    case bullets([String])
    case numbers([String])
    case quote(String)
    case table([String], [[String]])
    case image(String, String)
    case rule
    case paragraph(String)

    /// `parse`, remembered by the text.
    static func cached(_ text: String) -> [MarkdownBlock] {
        let key = text as NSString
        if let hit = blockCache.object(forKey: key) { return hit.list }
        let list = parse(text)
        blockCache.setObject(ParsedBlocks(list), forKey: key)
        return list
    }

    static func parse(_ text: String) -> [MarkdownBlock] {
        var out: [MarkdownBlock] = []
        let lines = text.components(separatedBy: "\n")
        var i = 0
        var para: [String] = []
        func flush() {
            let joined = para.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
            if !joined.isEmpty { out.append(.paragraph(joined)) }
            para = []
        }
        func isTableRule(_ s: String) -> Bool {
            let t = s.trimmingCharacters(in: .whitespaces)
            return t.contains("-") && t.contains("|") && t.allSatisfy { "|-: ".contains($0) }
        }
        func cells(_ s: String) -> [String] {
            var t = s.trimmingCharacters(in: .whitespaces)
            if t.hasPrefix("|") { t.removeFirst() }
            if t.hasSuffix("|") { t.removeLast() }
            return t.components(separatedBy: "|").map { $0.trimmingCharacters(in: .whitespaces) }
        }
        while i < lines.count {
            let line = lines[i]
            let t = line.trimmingCharacters(in: .whitespaces)
            if t.hasPrefix("```") {
                flush()
                var body: [String] = []
                i += 1
                while i < lines.count, !lines[i].trimmingCharacters(in: .whitespaces).hasPrefix("```") { body.append(lines[i]); i += 1 }
                i += 1
                out.append(.code(body.joined(separator: "\n")))
                continue
            }
            if t.isEmpty { flush(); i += 1; continue }
            if let m = t.range(of: #"^#{1,4}\s+"#, options: .regularExpression) {
                flush()
                let level = t[m].filter { $0 == "#" }.count
                out.append(.heading(level, String(t[m.upperBound...])))
                i += 1
                continue
            }
            if t.range(of: #"^(-{3,}|\*{3,}|_{3,})$"#, options: .regularExpression) != nil {
                flush(); out.append(.rule); i += 1; continue
            }
            if let m = t.range(of: #"^!\[([^\]]*)\]\(([^)\s]+)[^)]*\)$"#, options: .regularExpression) {
                flush()
                let inner = String(t[m])
                let alt = inner.drop { $0 != "[" }.dropFirst().prefix { $0 != "]" }
                let url = inner.components(separatedBy: "](").last?.dropLast().split(separator: " ").first.map { String($0) } ?? ""
                out.append(.image(String(alt), url))
                i += 1
                continue
            }
            if t.contains("|"), i + 1 < lines.count, isTableRule(lines[i + 1]) {
                flush()
                let head = cells(t)
                var rows: [[String]] = []
                i += 2
                while i < lines.count, lines[i].contains("|"), !lines[i].trimmingCharacters(in: .whitespaces).isEmpty {
                    var row = cells(lines[i])
                    while row.count < head.count { row.append("") }
                    rows.append(Array(row.prefix(head.count)))
                    i += 1
                }
                out.append(.table(head, rows))
                continue
            }
            if t.hasPrefix("- ") || t.hasPrefix("* ") || t.hasPrefix("• ") {
                flush()
                var items: [String] = []
                while i < lines.count {
                    let s = lines[i].trimmingCharacters(in: .whitespaces)
                    guard s.hasPrefix("- ") || s.hasPrefix("* ") || s.hasPrefix("• ") else { break }
                    items.append(String(s.dropFirst(2)))
                    i += 1
                }
                out.append(.bullets(items))
                continue
            }
            if t.range(of: #"^\d+[.)]\s"#, options: .regularExpression) != nil {
                flush()
                var items: [String] = []
                while i < lines.count {
                    let s = lines[i].trimmingCharacters(in: .whitespaces)
                    guard let m = s.range(of: #"^\d+[.)]\s+"#, options: .regularExpression) else { break }
                    items.append(String(s[m.upperBound...]))
                    i += 1
                }
                out.append(.numbers(items))
                continue
            }
            if t.hasPrefix(">") {
                flush()
                var quoted: [String] = []
                while i < lines.count {
                    let s = lines[i].trimmingCharacters(in: .whitespaces)
                    guard s.hasPrefix(">") else { break }
                    quoted.append(String(s.dropFirst()).trimmingCharacters(in: .whitespaces))
                    i += 1
                }
                out.append(.quote(quoted.joined(separator: "\n")))
                continue
            }
            para.append(line)
            i += 1
        }
        flush()
        return out
    }
}

private func inline(_ s: String) -> AttributedString {
    let key = s as NSString
    if let hit = inlineCache.object(forKey: key) { return hit.text }
    let parsed = (try? AttributedString(markdown: s, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(s)
    inlineCache.setObject(ParsedInline(parsed), forKey: key)
    return parsed
}

struct BlockView: View {
    let block: MarkdownBlock

    var body: some View {
        switch block {
        case let .code(body):
            ScrollView(.horizontal, showsIndicators: false) {
                Text(body).font(.system(size: 12, design: .monospaced)).foregroundStyle(Hub.text2)
            }
            .swipeZone()
            .padding(10).frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 8).fill(Hub.side))
            .textSelection(.enabled)
        case let .heading(level, text):
            Text(inline(text))
                .font(.system(size: level == 1 ? 20 : level == 2 ? 17.5 : 15.5, weight: .semibold))
                .foregroundStyle(Hub.text).padding(.top, level <= 2 ? 6 : 2)
                .textSelection(.enabled)
        case let .bullets(items):
            VStack(alignment: .leading, spacing: 5) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .firstTextBaseline, spacing: 9) {
                        Text("•").foregroundStyle(Hub.muted)
                        Text(inline(item)).tint(Hub.accent)
                    }
                }
            }
            .font(.system(size: 15)).foregroundStyle(Hub.text2).lineSpacing(3).textSelection(.enabled)
        case let .numbers(items):
            VStack(alignment: .leading, spacing: 5) {
                ForEach(Array(items.enumerated()), id: \.offset) { n, item in
                    HStack(alignment: .firstTextBaseline, spacing: 9) {
                        Text("\(n + 1).").foregroundStyle(Hub.muted).monospacedDigit().frame(minWidth: 18, alignment: .trailing)
                        Text(inline(item)).tint(Hub.accent)
                    }
                }
            }
            .font(.system(size: 15)).foregroundStyle(Hub.text2).lineSpacing(3).textSelection(.enabled)
        case let .quote(text):
            HStack(alignment: .top, spacing: 10) {
                Rectangle().fill(Hub.lineStrong).frame(width: 2)
                Text(inline(text)).font(.system(size: 15)).foregroundStyle(Hub.text3).lineSpacing(3).tint(Hub.accent)
            }
            .fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
        case let .table(head, rows):
            TableBlock(head: head, rows: rows)
        case let .image(alt, url):
            if let u = URL(string: url), ["http", "https"].contains(u.scheme ?? "") {
                VStack(alignment: .leading, spacing: 5) {
                    AsyncImage(url: u) { phase in
                        switch phase {
                        case let .success(image): image.resizable().scaledToFit().clipShape(RoundedRectangle(cornerRadius: 14))
                        case .failure: Label(alt.isEmpty ? "Picture" : alt, systemImage: "photo").font(.system(size: 12.5)).foregroundStyle(Hub.muted)
                        default: RoundedRectangle(cornerRadius: 14).fill(Hub.surface).frame(height: 160).overlay(ProgressView())
                        }
                    }
                    .contextMenu {
                        Button { UIPasteboard.general.string = url } label: { Label("Copy link", systemImage: "link") }
                        Button { UIApplication.shared.open(u) } label: { Label("Open in Safari", systemImage: "safari") }
                    }
                    if !alt.isEmpty { Text(alt).font(.system(size: 11.5)).foregroundStyle(Hub.muted) }
                }
            } else {
                // A picture on the agent's machine: the phone cannot read it from here yet.
                Label(alt.isEmpty ? url : alt, systemImage: "photo").font(.system(size: 12.5)).foregroundStyle(Hub.muted)
            }
        case .rule:
            Hub.line.frame(height: 1).padding(.vertical, 4)
        case let .paragraph(text):
            Text(inline(text))
                .font(.system(size: 15)).foregroundStyle(Hub.text2).lineSpacing(3)
                .tint(Hub.accent)
                .textSelection(.enabled)
        }
    }
}

/// A table with hairlines and no frame; a column of numbers lines up on the right.
struct TableBlock: View {
    let head: [String]
    let rows: [[String]]

    private func numeric(_ s: String) -> Bool {
        s.range(of: #"^[-+−]?\d[\d\s.,]*\s?(kr|%|h|min|g|kg|\$|€|£|ms|s)?$"#, options: .regularExpression) != nil
    }

    private func right(_ col: Int) -> Bool {
        let cells = rows.compactMap { $0.indices.contains(col) ? $0[col] : nil }.filter { !$0.isEmpty }
        return !cells.isEmpty && cells.allSatisfy(numeric)
    }

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 0) {
                GridRow {
                    ForEach(Array(head.enumerated()), id: \.offset) { c, h in
                        Text(inline(h)).font(.system(size: 11.5, weight: .semibold)).foregroundStyle(Hub.muted)
                            .frame(maxWidth: .infinity, alignment: right(c) ? .trailing : .leading)
                            .padding(.vertical, 6)
                    }
                }
                Hub.lineStrong.frame(height: 1).gridCellUnsizedAxes(.horizontal)
                ForEach(Array(rows.enumerated()), id: \.offset) { r, row in
                    GridRow {
                        ForEach(Array(row.enumerated()), id: \.offset) { c, cell in
                            Text(inline(cell)).font(.system(size: 13.5).monospacedDigit()).foregroundStyle(right(c) ? Hub.text : Hub.text2)
                                .frame(maxWidth: .infinity, alignment: right(c) ? .trailing : .leading)
                                .padding(.vertical, 8)
                        }
                    }
                    if r < rows.count - 1 {
                        Hub.line.frame(height: 1).gridCellUnsizedAxes(.horizontal)
                    }
                }
            }
            .padding(.trailing, 2)
        }
        .swipeZone()
        .textSelection(.enabled)
    }
}

// MARK: - Pictures
