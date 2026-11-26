import SwiftUI
import WebKit

// Viewkit on the phone for the files a reply wrote whole (sketch 40, as `Changes.svelte` on the
// desktop): a CSV or TSV as a table and an SVG as the drawing it is, each a card after the turn with
// the same label as a page card (glyph, name, open big), and big in a sheet you pull down to close.
// The text is the reply's own (`Folded.made`), so nothing is fetched and an old reply shows what it
// made then. HTML files are pages (`FilePages`).

struct MadeFilesView: View {
    let files: [MadeFile]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(files) { MadeFileCard(file: $0) }
        }
    }
}

struct MadeFileCard: View {
    let file: MadeFile
    @State private var big = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: file.kind == .svg ? "scribble.variable" : "tablecells").font(.system(size: 11.5))
                Text(file.kind == .svg ? "Drawing" : "Table")
                Text(file.name).foregroundStyle(Hub.text2).lineLimit(1)
                Spacer(minLength: 8)
                if file.kind != .svg { Text(DataTable.parse(file).rowsWord).monospacedDigit() }
                Button { big = true; Haptics.tap() } label: {
                    Image(systemName: "arrow.up.left.and.arrow.down.right").font(.system(size: 11.5)).frame(width: 30, height: 24)
                }
                .buttonStyle(.plain).accessibilityLabel("Open big")
            }
            .font(.system(size: 12)).foregroundStyle(Hub.muted)

            Group {
                if file.kind == .svg {
                    SVGView(svg: file.text, zoom: false)
                        .aspectRatio(SVGView.ratio(file.text), contentMode: .fit)
                        .frame(maxWidth: .infinity, minHeight: 80, maxHeight: 420)
                } else {
                    DataTableView(table: DataTable.parse(file), limit: 6)
                }
            }
            .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
            .clipShape(RoundedRectangle(cornerRadius: 14))
            .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(Hub.line))
            .contentShape(Rectangle())
            .safeTap { big = true; Haptics.tap() }
            .contextMenu {
                Button { UIPasteboard.general.string = file.text } label: { Label("Copy the text", systemImage: "doc.on.doc") }
                Button { UIPasteboard.general.string = file.path } label: { Label("Copy the path", systemImage: "link") }
                Button { big = true } label: { Label("Open big", systemImage: "arrow.up.left.and.arrow.down.right") }
            }
        }
        .sheet(isPresented: $big) { MadeFileSheet(file: file) }
    }
}

/// The file big: every row with the header row on top, or the drawing with pinch to zoom.
struct MadeFileSheet: View {
    let file: MadeFile
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let table = file.kind == .svg ? nil : DataTable.parse(file)
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Button { dismiss() } label: {
                    Image(systemName: "chevron.down").font(.system(size: 15, weight: .semibold)).foregroundStyle(Hub.text)
                        .frame(width: 36, height: 36).background(Circle().fill(Hub.surface))
                }
                .buttonStyle(.plain).accessibilityLabel("Close")
                VStack(spacing: 1) {
                    Text(file.name).font(.system(size: 15, weight: .semibold)).lineLimit(1)
                    if let table { Text("\(table.rowsWord) · \(table.columns) column\(table.columns == 1 ? "" : "s")").font(.system(size: 11.5)).foregroundStyle(Hub.muted) }
                }
                .frame(maxWidth: .infinity)
                ShareLink(item: file.text) {
                    Image(systemName: "square.and.arrow.up").font(.system(size: 14, weight: .medium)).foregroundStyle(Hub.text)
                        .frame(width: 36, height: 36).background(Circle().fill(Hub.surface))
                }
            }
            .padding(.horizontal, 14).padding(.top, 14).padding(.bottom, 10)
            if let table {
                DataTableView(table: table, limit: nil)
            } else {
                SVGView(svg: file.text, zoom: true).padding(.horizontal, 8)
            }
        }
        .background(Hub.bg.ignoresSafeArea())
        .presentationDragIndicator(.visible)
    }
}

// MARK: Tables

/// CSV or TSV as rows of cells (RFC 4180, as `parseDelimited` on the desktop): quoted cells may hold
/// the separator, quotes and line breaks; for CSV the separator is guessed from the first line
/// (comma, semicolon as Swedish Excel writes, or tab).
struct DataTable {
    var header: [String]
    var rows: [[String]]
    /// Columns that hold numbers, drawn to the right.
    var numeric: Set<Int>
    var columns: Int { header.count }
    var rowsWord: String { "\(rows.count) row\(rows.count == 1 ? "" : "s")" }

    nonisolated(unsafe) private static var cache: [String: DataTable] = [:]

    static func parse(_ file: MadeFile) -> DataTable {
        let key = "\(file.path)|\(file.text.count)|\(file.text.hashValue)"
        if let t = cache[key] { return t }
        var text = file.text
        if text.hasPrefix("\u{FEFF}") { text.removeFirst() }
        let sep: Character
        if file.kind == .tsv { sep = "\t" } else {
            let head = text.prefix { $0 != "\n" && $0 != "\r\n" }
            let candidates: [Character] = [",", ";", "\t"]
            sep = candidates.max { a, b in head.filter { $0 == a }.count < head.filter { $0 == b }.count } ?? ","
        }
        var all: [[String]] = []
        var row: [String] = []
        var cell = ""
        var quoted = false
        var chars = Array(text)
        chars.append("\n")
        var i = 0
        while i < chars.count {
            let c = chars[i]
            if quoted {
                if c == "\"" {
                    if i + 1 < chars.count, chars[i + 1] == "\"" { cell.append("\""); i += 1 } else { quoted = false }
                } else { cell.append(c) }
            } else if c == "\"" && cell.isEmpty {
                quoted = true
            } else if c == sep {
                row.append(cell); cell = ""
            } else if c == "\n" || c == "\r\n" || c == "\r" {
                row.append(cell); cell = ""
                if !(row.count == 1 && row[0].isEmpty) { all.append(row) }
                row = []
            } else {
                cell.append(c)
            }
            i += 1
        }
        let width = all.map(\.count).max() ?? 0
        let padded = all.map { $0 + Array(repeating: "", count: max(0, width - $0.count)) }
        let header = padded.first ?? []
        let rows = Array(padded.dropFirst())
        var numeric = Set<Int>()
        for col in 0..<width {
            let cells = rows.prefix(200).map { $0[col].trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
            if !cells.isEmpty, cells.allSatisfy(isNumber) { numeric.insert(col) }
        }
        let t = DataTable(header: header, rows: rows, numeric: numeric)
        if cache.count > 40 { cache.removeAll() }
        cache[key] = t
        return t
    }

    private static func isNumber(_ s: String) -> Bool {
        let t = s.replacingOccurrences(of: " ", with: "").replacingOccurrences(of: "\u{00A0}", with: "")
            .replacingOccurrences(of: ",", with: ".").trimmingCharacters(in: CharacterSet(charactersIn: "%$€£kr"))
        return Double(t) != nil
    }

    /// A column's width from its longest cell (of the first rows), within reason.
    func width(_ col: Int) -> CGFloat {
        let longest = ([header] + rows.prefix(200)).map { col < $0.count ? $0[col].count : 0 }.max() ?? 0
        return min(240, max(44, CGFloat(longest) * 7.6 + 20))
    }
}

/// The table: the header row in grey, numbers to the right, sideways for more columns. With a limit
/// it is the card (the first rows and how many more); without, every row with the header on top.
struct DataTableView: View {
    let table: DataTable
    let limit: Int?

    var body: some View {
        if let limit {
            VStack(alignment: .leading, spacing: 0) {
                ScrollView(.horizontal, showsIndicators: false) {
                    VStack(alignment: .leading, spacing: 0) {
                        line(table.header, head: true)
                        ForEach(Array(table.rows.prefix(limit).enumerated()), id: \.offset) { _, r in line(r, head: false) }
                    }
                }
                if table.rows.count > limit {
                    Text("\(table.rows.count - limit) more row\(table.rows.count - limit == 1 ? "" : "s")\(table.columns > 4 ? " · swipe sideways for more columns" : "")")
                        .font(.system(size: 12)).foregroundStyle(Hub.muted).padding(.horizontal, 10).padding(.vertical, 7)
                }
            }
        } else {
            ScrollView(.horizontal) {
                ScrollView(.vertical) {
                    LazyVStack(alignment: .leading, spacing: 0, pinnedViews: [.sectionHeaders]) {
                        Section {
                            ForEach(Array(table.rows.enumerated()), id: \.offset) { _, r in line(r, head: false) }
                        } header: {
                            line(table.header, head: true)
                        }
                    }
                    .padding(.bottom, 30)
                }
            }
        }
    }

    private func line(_ cells: [String], head: Bool) -> some View {
        HStack(spacing: 0) {
            ForEach(Array(cells.enumerated()), id: \.offset) { i, c in
                Text(c).lineLimit(1)
                    .font(.system(size: 12.5).monospacedDigit())
                    .foregroundStyle(head ? Hub.muted : Hub.text2)
                    .frame(width: table.width(i), alignment: table.numeric.contains(i) ? .trailing : .leading)
                    .padding(.horizontal, 5)
            }
        }
        .padding(.horizontal, 5).padding(.vertical, 6)
        .background(head ? Hub.surface2 : Color.clear)
        .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
    }
}

// MARK: Drawings

/// An SVG drawn as it is, with scripts off (an SVG can carry them), on a clear ground.
struct SVGView: UIViewRepresentable {
    let svg: String
    let zoom: Bool

    /// Width over height from the drawing's viewBox or its size, for the card's frame.
    static func ratio(_ svg: String) -> CGFloat {
        // Only the opening svg tag, so a child's stroke-width is never read as the size.
        guard let open = svg.range(of: "<svg", options: .caseInsensitive) else { return 1.6 }
        let head = String(svg[open.lowerBound...].prefix { $0 != ">" })
        func attr(_ name: String) -> String? {
            guard let r = head.range(of: #"\b\#(name)\s*=\s*["']([^"']+)["']"#, options: .regularExpression) else { return nil }
            let m = String(head[r])
            return m.split(whereSeparator: { $0 == "\"" || $0 == "'" }).dropFirst().first.map(String.init)
        }
        if let vb = attr("viewBox") {
            let n = vb.split(whereSeparator: { $0 == " " || $0 == "," }).compactMap { Double($0) }
            if n.count == 4, n[2] > 0, n[3] > 0 { return CGFloat(n[2] / n[3]) }
        }
        if let w = attr("width").flatMap({ Double($0.filter { $0.isNumber || $0 == "." }) }),
           let h = attr("height").flatMap({ Double($0.filter { $0.isNumber || $0 == "." }) }), w > 0, h > 0 {
            return CGFloat(w / h)
        }
        return 1.6
    }

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.defaultWebpagePreferences.allowsContentJavaScript = false
        let web = WKWebView(frame: .zero, configuration: config)
        web.isOpaque = false
        web.backgroundColor = .clear
        web.scrollView.backgroundColor = .clear
        web.scrollView.isScrollEnabled = zoom
        web.scrollView.bounces = zoom
        web.isUserInteractionEnabled = zoom
        load(web)
        return web
    }

    func updateUIView(_ web: WKWebView, context: Context) {}

    private func load(_ web: WKWebView) {
        let scale = zoom ? "width=device-width,initial-scale=1,maximum-scale=6,user-scalable=yes" : "width=device-width,initial-scale=1,user-scalable=no"
        let html = """
        <!doctype html><meta name="viewport" content="\(scale)">
        <style>html,body{margin:0;background:transparent;height:100%}body{display:flex;align-items:center;justify-content:center}svg{display:block;width:100%;height:auto;max-height:100%}</style>
        \(svg)
        """
        web.loadHTMLString(html, baseURL: nil)
    }
}

// MARK: - Files a reply only names

/// A CSV, TSV or SVG the answer names that no edit wrote (a script made it), shown as the same card
/// once the thread's folder is known to have it (`NamedFile.svelte` on the desktop). The file is read
/// through `phone.files`; a name that is no file shows nothing.
enum NamedFiles {
    /// The files read already, so a turn drawn again does not ask again.
    nonisolated(unsafe) static var read: [String: MadeFile] = [:]

    static func paths(in items: [Item], skipping made: [MadeFile]) -> [String] {
        var texts: [String] = []
        for item in items { if case let .reply(text) = item { texts.append(text) } }
        guard let re = try? NSRegularExpression(pattern: #"[^\s`"'<>()\[\]|*?]+\.(?:csv|tsv|tab|svg)\b"#, options: [.caseInsensitive]) else { return [] }
        let have = Set(made.map { $0.name.lowercased() })
        // The same file named twice (a link's words, then its address) is one, by its fullest path.
        var order: [String] = []
        var fullest: [String: String] = [:]
        for text in texts {
            for m in re.matches(in: text, range: NSRange(text.startIndex..., in: text)) {
                guard let r = Range(m.range, in: text) else { continue }
                var path = String(text[r])
                if path.lowercased().hasPrefix("file:///") { path = String(path.dropFirst(8)) } else if path.lowercased().hasPrefix("file://") { path = String(path.dropFirst(7)) }
                path = path.removingPercentEncoding ?? path
                let key = FilePages.name(path).lowercased()
                guard !path.contains("://"), !path.hasPrefix("www."), !have.contains(key) else { continue }
                if let had = fullest[key] { if path.count > had.count { fullest[key] = path } } else { fullest[key] = path; order.append(key) }
            }
        }
        let out = order.compactMap { fullest[$0] }
        return Array(out.prefix(4))
    }

    /// The path as the folder's own: relative to it, or nil for a file elsewhere.
    static func rel(_ path: String, cwd: String) -> String? {
        let p = path.replacingOccurrences(of: "\\", with: "/")
        var base = cwd.replacingOccurrences(of: "\\", with: "/")
        while base.hasSuffix("/") { base.removeLast() }
        if p.lowercased().hasPrefix(base.lowercased() + "/") { return String(p.dropFirst(base.count + 1)) }
        if p.hasPrefix("/") || (p.count > 2 && p[p.index(p.startIndex, offsetBy: 1)] == ":") { return nil }
        return p.hasPrefix("./") ? String(p.dropFirst(2)) : p
    }
}

struct NamedFilesView: View {
    let paths: [String]
    let threadId: String
    @Environment(Store.self) private var store
    @State private var files: [MadeFile] = []

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(files) { MadeFileCard(file: $0) }
        }
        .task(id: paths) { await load() }
    }

    private func load() async {
        guard !paths.isEmpty, let thread = store.thread(threadId),
              store.has("phone.files", on: store.url(of: thread.host)),
              let target = store.target(thread.host) else { return }
        var out: [MadeFile] = []
        for path in paths {
            let key = "\(threadId)|\(path)"
            if let done = NamedFiles.read[key] { out.append(done); continue }
            guard let rel = NamedFiles.rel(path, cwd: thread.cwd), let kind = MadeFile.kind(of: path),
                  let f = try? await target.client.file(path: thread.cwd, rel: rel, host: target.host), !f.truncated else { continue }
            let file = MadeFile(path: path, kind: kind, text: f.text)
            NamedFiles.read[key] = file
            out.append(file)
        }
        files = out
    }
}
