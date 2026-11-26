import SwiftUI
import WebKit

// What an agent's answer can hold besides words, in the thread (sketch "K1, utan ram"): content sits
// straight on the chat with one small grey label above it, nothing boxed. This file has the
// breaks (the wavy line where the engine or the machine changed), the live pages an agent shows
// (`show_page`, in a web view that sleeps until tapped), and the sources line. The Markdown
// blocks are in RichProse.swift, the pictures and files in RichPictures.swift.

// MARK: - Breaks

/// One wavy line across the thread where something changed, as the desktop draws it (Break.svelte):
/// the colour says what sort of break it is (engine, machine, a wait, stuck), the words stay grey.
struct Brk: Hashable {
    /// "engine", "machine", "session", "waiting", "stuck", "time" or "new" (what came while you were away).
    var kind: String
    var text: String
    var note: String?
    var hint: String?
    var glyph: String?

    var color: Color {
        switch kind {
        case "engine": return Color(hex: 0xa99cf0)
        case "machine": return Color(hex: 0x6cc7b8)
        case "session": return Color(hex: 0x78c4e6)
        case "waiting": return Color(hex: 0xcdb48d)
        case "stuck": return Color(hex: 0xee8a9a)
        case "new": return Color(hex: 0xec9a64)
        default: return Hub.muted
        }
    }
}

struct Wave: Shape {
    func path(in rect: CGRect) -> Path {
        var p = Path()
        let mid = rect.midY
        let half: CGFloat = 12
        var x: CGFloat = 0
        var up = true
        p.move(to: CGPoint(x: 0, y: mid))
        while x < rect.width {
            let end = min(x + half, rect.width)
            p.addQuadCurve(to: CGPoint(x: end, y: mid), control: CGPoint(x: x + half / 2, y: mid + (up ? -4 : 4)))
            x += half
            up.toggle()
        }
        return p
    }
}

struct BreakView: View {
    let brk: Brk
    @Environment(Store.self) private var store

    var body: some View {
        VStack(spacing: 5) {
            HStack(spacing: 12) {
                Wave().stroke(brk.color.opacity(0.7), lineWidth: 1.3).frame(height: 8).frame(minWidth: 14)
                HStack(spacing: 6) {
                    if let glyph = brk.glyph { Image(systemName: glyph).font(.system(size: 11)).foregroundStyle(brk.color) }
                    Text(brk.text).font(.system(size: 12.5)).foregroundStyle(brk.kind == "new" ? brk.color : Hub.text3).lineLimit(2).multilineTextAlignment(.center)
                    if let note = brk.note { Text(note).font(.system(size: 11.5)).foregroundStyle(Hub.muted).lineLimit(1) }
                }
                .fixedSize(horizontal: false, vertical: true)
                .layoutPriority(1)
                Wave().stroke(brk.color.opacity(0.7), lineWidth: 1.3).frame(height: 8).frame(minWidth: 14)
            }
            if let hint = brk.hint {
                Text(hint).font(.system(size: 12)).foregroundStyle(Hub.muted).multilineTextAlignment(.center).padding(.horizontal, 12)
            }
        }
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .combine)
        // The jump button on the thread shows while the "New" break is out of sight.
        .onScrollVisibilityChange(threshold: 0.2) { visible in if brk.kind == "new" { store.newInView = visible } }
    }
}

// MARK: - Side jobs

/// A side job the agent handed to another thread, as the desktop draws it (ChildLine.svelte): a
/// bracket in the left margin, a branch that goes out from the agent's words and comes back. It is
/// never the wavy line, which says the thread itself changed; here the thread stays on its own
/// engine. Compact on the phone: one head line (who asked whom, and the state), the job, and once
/// it is done a "Result" row that opens the job's thread. A job that drew pictures shows them under
/// it, read from the job's own log as the desktop does (`ChildLine.svelte`), a tap opens one big.
/// A tap on the line opens the job's thread.
struct SideJobView: View {
    @Environment(Store.self) private var store
    let job: SideJob
    /// The engine of the thread this sits in: who handed the job over.
    var from: String = ""
    private let sky = Color(hex: 0x78c4e6)

    private var tint: Color { job.state == "failed" ? Hub.red : sky }
    private var verb: (String, String) {
        switch job.job {
        case "image": return ("drawing", "drew")
        case "research": return ("looking into", "looked into")
        case "review": return ("reviewing", "reviewed")
        default: return ("working on", "did")
        }
    }
    private var who: String {
        let to = Folded.provider(job.engine)
        return from.isEmpty ? to : "\(Folded.provider(from)) asked \(to)"
    }
    private var took: String {
        guard let end = job.finished, end > job.started else { return "" }
        return " · " + ThreadView.duration(end - job.started)
    }

    /// The pictures in the job's log, once it is done and drew any.
    private var pictures: [Picture] {
        guard job.state == "done", job.pictures > 0 else { return [] }
        return (store.folded[job.id]?.items ?? []).compactMap { if case let .picture(p) = $0 { return p }; return nil }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            line
            ForEach(pictures, id: \.id) { p in PictureView(picture: p).padding(.leading, 28) }
        }
        .task(id: job.state) {
            if job.state == "done", job.pictures > 0, store.folded[job.id]?.items.isEmpty ?? true { await store.loadEvents(job.id) }
        }
    }

    private var line: some View {
        NavigationLink(value: Route.thread(job.id)) {
            HStack(alignment: .top, spacing: 10) {
                Bracket(closed: job.state == "done").stroke(tint.opacity(0.55), lineWidth: 1.5).frame(width: 12)
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        Image(systemName: "arrow.triangle.branch").font(.system(size: 11, weight: .semibold)).foregroundStyle(tint)
                        Text("SIDE JOB").font(.system(size: 10.5, weight: .semibold)).tracking(0.6).foregroundStyle(tint)
                        Text(who).font(.system(size: 12.5)).foregroundStyle(Hub.text3).lineLimit(1)
                        Spacer(minLength: 4)
                        state
                    }
                    Text((job.state == "done" ? verb.1 + " " : job.state == "failed" ? "could not do " : "") + "“\(job.title)”" + (job.state == "failed" ? (job.error.map { " · \($0)" } ?? "") : ""))
                        .font(.system(size: 13)).foregroundStyle(Hub.text2).lineLimit(1)
                    if job.state == "done" {
                        HStack(spacing: 6) {
                            Text("RESULT").font(.system(size: 10.5, weight: .semibold)).tracking(0.6).foregroundStyle(Hub.muted)
                            Text("Open").font(.system(size: 12)).foregroundStyle(Hub.muted)
                            Image(systemName: "chevron.right").font(.system(size: 9, weight: .semibold)).foregroundStyle(Hub.muted)
                        }
                        .padding(.top, 2)
                    }
                }
            }
            .padding(.vertical, 8).padding(.leading, 6)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.press)
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder private var state: some View {
        switch job.state {
        case "done":
            Label("Done\(took)", systemImage: "checkmark").font(.system(size: 12)).foregroundStyle(Hub.green).lineLimit(1)
        case "failed":
            Label("Failed", systemImage: "xmark").font(.system(size: 12)).foregroundStyle(Hub.red).lineLimit(1)
        default:
            HStack(spacing: 5) {
                Working(size: 11)
                Text("\(verb.0)…").font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(1)
            }
        }
    }
}

/// The bracket's rail: a corner at each end, rounded, opening to the right. Closed (a finished
/// job) it ends on a small dot, the way a branch rejoins.
struct Bracket: Shape {
    var closed = false
    func path(in rect: CGRect) -> Path {
        var p = Path()
        let r: CGFloat = 9
        p.move(to: CGPoint(x: rect.maxX, y: rect.minY + 4))
        p.addLine(to: CGPoint(x: rect.minX + r, y: rect.minY + 4))
        p.addQuadCurve(to: CGPoint(x: rect.minX, y: rect.minY + 4 + r), control: CGPoint(x: rect.minX, y: rect.minY + 4))
        p.addLine(to: CGPoint(x: rect.minX, y: rect.maxY - 4 - r))
        p.addQuadCurve(to: CGPoint(x: rect.minX + r, y: rect.maxY - 4), control: CGPoint(x: rect.minX, y: rect.maxY - 4))
        p.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - 4))
        if closed { p.addEllipse(in: CGRect(x: rect.maxX - 3, y: rect.maxY - 6.5, width: 5, height: 5)) }
        return p
    }
}

// MARK: - Live pages

/// A page the agent showed with `show_page`. A page it changed is a new version of the first one.
struct PageItem: Hashable {
    var id: String
    var title: String
    var height: Double?
    var version = 1
    var of = 1
}

/// The page in the thread: a label above (glyph, name, version, a button to open it big), and the
/// page below on its own surface. It sleeps until tapped so a thumb scrolling the chat never drags
/// something in it; the lock button in the label puts it back to sleep.
struct PageCard: View {
    let page: PageItem
    let threadId: String
    @Environment(Store.self) private var store
    @State private var awake = false
    @State private var reported: CGFloat?
    @State private var big = false

    /// Where the page is served from when it is not the thread's own: "files" for an HTML file the
    /// agent wrote (`FilePages`).
    var servedFrom: String?

    private var url: URL? {
        store.client(for: threadId)?.base.appendingPathComponent("api/threads/\(servedFrom ?? threadId)/pages/\(page.id)")
    }
    private var shown: CGFloat { min(max(reported ?? CGFloat(page.height ?? 320), 60), 760) }

    var body: some View {
        if page.version < page.of { older } else { card }
    }

    /// An older version of a page is one grey line that opens it, as on the desktop: the page as it
    /// is now is further down.
    private var older: some View {
        Button { big = true; Haptics.tap() } label: {
            HStack(spacing: 6) {
                Image(systemName: "doc.richtext").font(.system(size: 11.5))
                Text(page.title).lineLimit(1)
                Text("v\(page.version) · v\(page.of) is further down").lineLimit(1).layoutPriority(1)
                Spacer(minLength: 0)
            }
            .font(.system(size: 12)).foregroundStyle(Hub.muted)
            .frame(minHeight: 28)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(url == nil)
        .sheet(isPresented: $big) {
            if let url { PageSheet(title: page.title, url: url) }
        }
    }

    private var card: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: "doc.richtext").font(.system(size: 11.5)).foregroundStyle(Hub.muted)
                Text(page.title).lineLimit(1)
                if page.of > 1 { Text(page.version < page.of ? "v\(page.version) of \(page.of)" : "v\(page.version)").foregroundStyle(Hub.muted) }
                Spacer(minLength: 8)
                if awake {
                    Button { awake = false; Haptics.tap() } label: {
                        Label("In use", systemImage: "lock.open").labelStyle(.titleAndIcon).foregroundStyle(Hub.green)
                    }.buttonStyle(.plain)
                }
                Button { big = true; Haptics.tap() } label: {
                    Image(systemName: "arrow.up.left.and.arrow.down.right").font(.system(size: 11.5)).foregroundStyle(Hub.muted).frame(width: 30, height: 24)
                }
                .buttonStyle(.plain).accessibilityLabel("Open big")
            }
            .font(.system(size: 12)).foregroundStyle(Hub.muted)

            if let url {
                ZStack {
                    PageWeb(url: url, interactive: awake, height: $reported)
                    if !awake {
                        Color.black.opacity(0.001)
                            .overlay {
                                Label("Tap to use", systemImage: "hand.tap").font(.system(size: 13)).foregroundStyle(Hub.text)
                                    .padding(.horizontal, 14).frame(height: 34)
                                    .background(Capsule().fill(Hub.surface2.opacity(0.95))).overlay(Capsule().strokeBorder(Hub.lineStrong))
                            }
                            .contentShape(Rectangle())
                            .safeTap { awake = true; Haptics.tap() }
                    }
                }
                .frame(height: shown)
                .swipeZone(awake)
                .background(RoundedRectangle(cornerRadius: 14).fill(Hub.surface))
                .clipShape(RoundedRectangle(cornerRadius: 14))
                .contextMenu {
                    Button { UIPasteboard.general.string = url.absoluteString } label: { Label("Copy link", systemImage: "link") }
                    Button { UIApplication.shared.open(url) } label: { Label("Open in Safari", systemImage: "safari") }
                    Button { big = true } label: { Label("Open big", systemImage: "arrow.up.left.and.arrow.down.right") }
                }
            } else {
                Text("This machine is not reachable, so the page cannot load.").font(.system(size: 12.5)).foregroundStyle(Hub.muted)
            }
        }
        .sheet(isPresented: $big) {
            if let url { PageSheet(title: page.title, url: url) }
        }
    }
}

/// The page big, in a sheet you pull down to close.
struct PageSheet: View {
    let title: String
    let url: URL
    @Environment(\.dismiss) private var dismiss
    @State private var reported: CGFloat?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Button { dismiss() } label: {
                    Image(systemName: "chevron.down").font(.system(size: 15, weight: .semibold)).foregroundStyle(Hub.text)
                        .frame(width: 36, height: 36).background(Circle().fill(Hub.surface))
                }
                .buttonStyle(.plain)
                Spacer()
                Text(title).font(.system(size: 15, weight: .semibold)).lineLimit(1)
                Spacer()
                Button { UIApplication.shared.open(url) } label: {
                    Image(systemName: "safari").font(.system(size: 15)).foregroundStyle(Hub.text2)
                        .frame(width: 36, height: 36).background(Circle().fill(Hub.surface))
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 14).padding(.top, 14).padding(.bottom, 8)
            PageWeb(url: url, interactive: true, height: $reported, scrolls: true)
        }
        .background(Hub.bg.ignoresSafeArea())
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
    }
}

/// The web view that holds a page. The core serves a page in a sandbox with an opaque origin
/// (`pages.rs`), which posts its height, that it is ready and links to its parent window. Here the
/// page is the top window, so a small script passes those messages on, and Hub's colours go back
/// the way the desktop frame gives them (`PageFrame.svelte`).
struct PageWeb: UIViewRepresentable {
    let url: URL
    let interactive: Bool
    @Binding var height: CGFloat?
    var scrolls = false
    /// Below 1 the page is drawn smaller, as in a thumbnail (Artifacts in Chat).
    var zoom: CGFloat = 1

    private static let bridge = """
    window.addEventListener('message', function (e) {
      var d = e.data;
      if (d && typeof d === 'object' && d.hub && e.source === window) {
        try { window.webkit.messageHandlers.hub.postMessage(d); } catch (_) {}
      }
    });
    """

    /// The height the page's content takes. The page's own report measures the document, which
    /// is never shorter than the view it sits in, so a card that starts taller than its page would
    /// keep the empty space under it. This measures what is in the body instead. A page sized by
    /// the view (vh) shrinks with it, so it may shrink only a few times.
    private static let fit = """
    (function () {
      var last = 0, shrinks = 0;
      function measure() {
        var b = document.body;
        if (!b) return;
        var bottom = 0;
        for (var i = 0; i < b.children.length; i++) {
          var el = b.children[i], cs = getComputedStyle(el);
          if (cs.display === 'none' || cs.position === 'fixed' || el.tagName === 'SCRIPT' || el.tagName === 'STYLE') continue;
          var r = el.getBoundingClientRect();
          bottom = Math.max(bottom, r.bottom + window.scrollY + (parseFloat(cs.marginBottom) || 0));
        }
        var bs = getComputedStyle(b);
        bottom += (parseFloat(bs.paddingBottom) || 0) + (parseFloat(bs.borderBottomWidth) || 0) + (parseFloat(bs.marginBottom) || 0);
        var n = Math.ceil(bottom);
        if (n <= 0 || Math.abs(n - last) < 2) return;
        if (n < last) { if (shrinks >= 3) return; shrinks++; }
        last = n;
        try { window.webkit.messageHandlers.hub.postMessage({ hub: 'fit', height: n }); } catch (_) {}
      }
      var ro = new ResizeObserver(measure);
      function watch() {
        if (!document.body) return;
        ro.observe(document.body);
        for (var i = 0; i < document.body.children.length; i++) ro.observe(document.body.children[i]);
        measure();
      }
      watch();
      new MutationObserver(watch).observe(document.body, { childList: true });
      window.addEventListener('load', measure);
      setTimeout(measure, 400); setTimeout(measure, 1500);
    })();
    """

    private static let theme: [String: String] = [
        "--bg": "#1a1918", "--surface": "#211f1e", "--surface-2": "#242220", "--surface-hover": "#2a2826",
        "--line": "#2a2826", "--line-strong": "#3a3734", "--text": "#ece8e2", "--text-2": "#d3cfc8",
        "--text-3": "#bdb8b0", "--muted": "#958f88", "--accent": "#a5abee", "--accent-bg": "rgba(165,171,238,0.15)",
        "--green": "#84c29a", "--green-bg": "rgba(132,194,154,0.1)", "--amber": "#ddaa5e", "--amber-bg": "rgba(221,170,94,0.16)",
        "--red": "#e5806f", "--red-bg": "rgba(229,128,111,0.1)", "--radius": "10px",
        "--font": "-apple-system, system-ui, sans-serif", "--mono": "ui-monospace, Menlo, monospace",
    ]

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> WKWebView {
        let conf = WKWebViewConfiguration()
        let content = WKUserContentController()
        content.addUserScript(WKUserScript(source: Self.bridge, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        content.addUserScript(WKUserScript(source: Self.fit, injectionTime: .atDocumentEnd, forMainFrameOnly: true))
        content.add(WeakHandler(context.coordinator), name: "hub")
        conf.userContentController = content
        let web = WKWebView(frame: .zero, configuration: conf)
        web.isOpaque = false
        web.backgroundColor = .clear
        web.scrollView.backgroundColor = .clear
        web.scrollView.isScrollEnabled = scrolls
        web.scrollView.bounces = scrolls
        web.pageZoom = zoom
        web.navigationDelegate = context.coordinator
        context.coordinator.web = web
        context.coordinator.onHeight = { h in DispatchQueue.main.async { height = h } }
        web.load(URLRequest(url: url))
        return web
    }

    func updateUIView(_ web: WKWebView, context: Context) {
        web.isUserInteractionEnabled = interactive
        context.coordinator.onHeight = { h in DispatchQueue.main.async { height = h } }
    }

    final class Coordinator: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
        weak var web: WKWebView?
        var onHeight: (CGFloat) -> Void = { _ in }

        func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
            guard let body = message.body as? [String: Any], let kind = body["hub"] as? String else { return }
            switch kind {
            case "fit":
                // The page's own "height" is left alone: see `fit`.
                if let h = body["height"] as? Double, h.isFinite { onHeight(CGFloat(h.rounded(.up))) }
            case "ready":
                let payload: [String: Any] = ["hub": "theme", "vars": PageWeb.theme]
                if let data = try? JSONSerialization.data(withJSONObject: payload),
                   let json = String(data: data, encoding: .utf8) {
                    web?.evaluateJavaScript("window.postMessage(\(json), '*')")
                }
            case "open":
                if let s = body["url"] as? String, let u = URL(string: s), ["http", "https"].contains(u.scheme ?? "") {
                    UIApplication.shared.open(u)
                }
            default:
                break
            }
        }

        func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
            if navigationAction.navigationType == .linkActivated, let u = navigationAction.request.url, ["http", "https"].contains(u.scheme ?? "") {
                UIApplication.shared.open(u)
                decisionHandler(.cancel)
            } else {
                decisionHandler(.allow)
            }
        }
    }
}

/// Keeps the web view's message handler from holding its owner alive.
final class WeakHandler: NSObject, WKScriptMessageHandler {
    weak var target: WKScriptMessageHandler?
    init(_ target: WKScriptMessageHandler) { self.target = target }
    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        target?.userContentController(userContentController, didReceive: message)
    }
}

// MARK: - Sources

/// Where an answer's facts came from: the pages the agent fetched during the turn, taken from its
/// tool calls, so the agent does not have to write them out.
enum Sources {
    static func of(_ items: [Item]) -> [URL] {
        var seen = Set<String>()
        var out: [URL] = []
        for item in items {
            guard case let .work(steps) = item else { continue }
            for step in steps {
                for text in [step.title, step.command ?? ""] {
                    for raw in links(in: text) {
                        guard let url = URL(string: raw), let host = url.host, !host.isEmpty else { continue }
                        // localhost and addresses are the agent's own dev servers, not sources.
                        if host == "localhost" || host.hasPrefix("127.") || host.hasPrefix("192.168.") || host.hasPrefix("100.") { continue }
                        if seen.insert(raw).inserted { out.append(url) }
                    }
                }
            }
        }
        return out
    }

    private static func links(in text: String) -> [String] {
        guard let re = try? NSRegularExpression(pattern: #"https?://[^\s"'`<>)\]]+"#) else { return [] }
        let range = NSRange(text.startIndex..., in: text)
        return re.matches(in: text, range: range).compactMap { m in Range(m.range, in: text).map { String(text[$0]) } }
    }

    static func name(_ url: URL) -> String {
        let host = url.host ?? url.absoluteString
        return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
    }
}

struct SourcesLine: View {
    let urls: [URL]
    @State private var open = false

    var body: some View {
        let names = Array(NSOrderedSet(array: urls.map(Sources.name))) as? [String] ?? []
        Button { open = true; Haptics.tap() } label: {
            HStack(spacing: 7) {
                Image(systemName: "globe").font(.system(size: 12))
                Text("\(urls.count) \(urls.count == 1 ? "source" : "sources") · \(names.prefix(3).joined(separator: ", "))\(names.count > 3 ? "…" : "")").lineLimit(1)
                Image(systemName: "chevron.right").font(.system(size: 9, weight: .semibold)).foregroundStyle(Hub.lineStrong)
            }
            .font(.system(size: 12.5)).foregroundStyle(Hub.muted)
        }
        .buttonStyle(.plain)
        .sheet(isPresented: $open) { SourcesSheet(urls: urls) }
    }
}

struct SourcesSheet: View {
    let urls: [URL]
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            ZStack {
                Text("Sources").font(.system(size: 15, weight: .semibold))
                HStack { Spacer(); Button("Done") { dismiss() }.foregroundStyle(Hub.accent).fontWeight(.semibold) }
            }
            .padding(.horizontal, 16).padding(.top, 16).padding(.bottom, 10)
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(urls, id: \.absoluteString) { url in
                        Button { UIApplication.shared.open(url) } label: {
                            HStack(spacing: 11) {
                                Text(String(Sources.name(url).prefix(2)).uppercased())
                                    .font(.system(size: 10, weight: .bold)).foregroundStyle(Hub.text2)
                                    .frame(width: 30, height: 30).background(RoundedRectangle(cornerRadius: 8).fill(Hub.surface2))
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(Sources.name(url)).font(.system(size: 14, weight: .medium)).foregroundStyle(Hub.text)
                                    Text(url.absoluteString).font(.system(size: 11.5)).foregroundStyle(Hub.muted).lineLimit(1)
                                }
                                Spacer()
                                Image(systemName: "arrow.up.right").font(.system(size: 11)).foregroundStyle(Hub.muted)
                            }
                            .padding(.horizontal, 16).padding(.vertical, 10)
                            .overlay(alignment: .bottom) { Hub.line.frame(height: 1) }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
        .background(Hub.surface.ignoresSafeArea())
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }
}

// MARK: - Markdown blocks
