import SwiftUI

// Sheet "Ny tråd" (screen 6) and the model picker: every engine the machine can start (feature
// `phone.engines`, `GET /api/phone/engines`), the company as a tab at the top, its models as
// one-line rows, the thinking effort as a compact row, and the engine's logo in the reply box.
// The agent's own options (Claude's models, Codex's) come from `POST /api/phone/warm`, which
// starts the engine while the page is open.

struct PhoneEngine: Decodable, Hashable, Identifiable {
    struct Effort: Decodable, Hashable {
        var value: String
        var name: String
        var what: String?
        var `default`: Bool?
    }
    var name: String
    var label: String
    var `default`: Bool?
    var provider: String?
    var provider_label: String?
    var model: String?
    var route: JSON?
    var efforts: [String: [Effort]]?
    /// The model and thought_level options the engine reported on its latest thread (`phone.engines`),
    /// in the shape `EngineOption.parse` reads.
    var options: [JSON]?
    var id: String { name }
}

private struct Warmed: Decodable { var id: String? }

extension HubClient {
    func phoneEngines(host: String?) async throws -> [PhoneEngine] {
        try JSONDecoder().decode(Lossy<PhoneEngine>.self, from: try await call("GET", "api/phone/engines", query: host.map { ["host": $0] } ?? [:])).items
    }

    /// Starts the engine for a thread not created yet; answers its id, nil when nothing started.
    func warm(cwd: String, engine: String?, settings: [String: String], host: String?, chat: Bool = false) async throws -> String? {
        // A chat has no folder to name: the machine makes its own under chats.
        var b: [String: Any] = ["settings": settings]
        if chat { b["chat"] = true } else { b["cwd"] = cwd }
        if let engine { b["engine"] = engine }
        return try JSONDecoder().decode(Warmed.self, from: try await call("POST", "api/phone/warm", query: host.map { ["host": $0] } ?? [:], body: b)).id
    }

    func warmEvents(thread: String, host: String?) async throws -> [HubEvent] {
        var q = ["after": "0", "limit": "2000"]
        if let host { q["host"] = host }
        return try JSONDecoder().decode(Lossy<HubEvent>.self, from: try await call("GET", "api/threads/\(thread)/events", query: q)).items
    }
}

extension Store {
    /// Every engine this machine can start when it says so (`phone.engines`), else the ones seen on threads.
    var allEngines: [String] {
        phoneEngines.isEmpty ? engines : phoneEngines.map(\.name)
    }

    /// The company's name as a tab: "Claude", "Codex", "Hermes".
    func engineLabel(_ name: String) -> String {
        if let l = phoneEngines.first(where: { $0.name == name })?.label, !l.isEmpty { return l.prefix(1).uppercased() + l.dropFirst() }
        switch name.lowercased() {
        case "claude": return "Claude"
        case "codex": return "Codex"
        default: return name.split(separator: "-").map { $0.prefix(1).uppercased() + $0.dropFirst() }.joined(separator: " ")
        }
    }

    func loadPhoneEngines() async {
        guard !isDemo, has("phone.engines"), let t = target(nil) else { return }
        if let list = try? await t.client.phoneEngines(host: t.host), !list.isEmpty { phoneEngines = list }
    }
}

// MARK: Logos

/// A small mark for an engine's company: Claude's spark, OpenAI's knot, Hermes' wing, a letter for the rest.
struct EngineLogo: View {
    let engine: String
    var size: CGFloat = 16

    var body: some View {
        let l = engine.lowercased()
        Group {
            if l.contains("claude") || l.contains("anthropic") {
                Image(systemName: "asterisk").font(.system(size: size, weight: .bold)).foregroundStyle(Color(hex: 0xd97757))
            } else if l.contains("codex") || l.contains("openai") || l.contains("gpt") {
                OpenAIMark().fill(Hub.text).frame(width: size, height: size)
            } else if l.contains("hermes") || l.contains("deepseek") {
                HermesWing().stroke(Color(hex: 0x6e8cff), style: StrokeStyle(lineWidth: max(1.2, size / 10), lineCap: .round, lineJoin: .round)).frame(width: size, height: size)
            } else {
                Text(String(engine.prefix(1)).uppercased()).font(.system(size: size * 0.85, weight: .bold)).foregroundStyle(EnginePanel.color(engine))
            }
        }
        .frame(width: size + 4, height: size + 4)
    }
}

// MARK: What the picker shows for one engine

struct PickerData {
    var modelId: String?
    var groups: [(title: String?, choices: [EngineOption.Choice])] = []
    var modelValue: String?
    var effortId: String?
    var effortChoices: [EngineOption.Choice] = []
    var effortValue: String?
    /// The lists are Hub's own built-in ones, so a pick is only a hint kept on the phone (its id
    /// starts with `hint:`) and is never sent to the server.
    var builtIn = false

    /// The models and efforts known for an engine, so the sheet is never empty before a thread has
    /// run. A pick from them is a hint only.
    static func known(engine: String) -> (models: [EngineOption.Choice], efforts: [EngineOption.Choice]) {
        let l = engine.lowercased()
        func c(_ names: [String], first: String? = nil) -> [EngineOption.Choice] {
            names.map { .init(value: $0.lowercased(), name: $0, description: $0 == first ? "default" : nil) }
        }
        if l.contains("claude") {
            return (c(["Fable", "Opus", "Sonnet", "Haiku"], first: "Fable"), c(["Low", "Medium", "High", "Max"]))
        }
        if l.contains("codex") || l.contains("openai") || l.contains("gpt") {
            return (c(["Astra", "Sol", "Terra", "Luna"], first: "Astra"), c(["Low", "Medium", "High"]))
        }
        return ([.init(value: "default", name: "Default", description: nil)], [])
    }

    /// What goes to the server. Picks from Hub's built-in lists (`hint:`) go as names (`~terra`,
    /// `~medium`) that the engine resolves when it starts (`phone.pickbyname`); without that
    /// feature, or when the real option is already picked, they stay on the phone.
    static func sendable(_ settings: [String: String], names: Bool) -> [String: String] {
        var out = settings.filter { !$0.key.hasPrefix("hint:") }
        guard names else { return out }
        if out["model"] == nil, let m = settings["hint:model"], m != "default" { out["model"] = "~" + m }
        let efforts = ["thought_level", "reasoning_effort", "effort"]
        if !efforts.contains(where: { out[$0] != nil }), let e = settings["hint:effort"] { out["thought_level"] = "~" + e }
        return out
    }

    /// The real choice a built-in pick names, once the engine has told its own list.
    static func hinted(_ word: String?, in choices: [EngineOption.Choice]) -> String? {
        guard let w = word?.lowercased(), !w.isEmpty else { return nil }
        return choices.first { $0.value.lowercased() == w || $0.name.lowercased() == w }?.value
            ?? choices.first { $0.value.lowercased().contains(w) || $0.name.lowercased().contains(w) }?.value
    }

    /// What an engine offers: the agent's own options when it has reported them (the warmed thread,
    /// an old thread), else what the machine's engine list carries (`options` from its latest thread,
    /// a harness's provider-and-model route, a fixed model), else the built-in list, with the choices
    /// already made in `values`.
    static func make(engine: String, agent: [EngineOption], phone: PhoneEngine?, values: [String: String], builtIn allow: Bool = true) -> PickerData {
        var d = PickerData()
        let own = EngineOption.parse(phone?.options ?? [])
        let opts = agent.isEmpty ? own : agent
        if let m = opts.first(where: { $0.category == "model" }) {
            d.modelId = m.id; d.groups = [(nil, m.choices)]; d.modelValue = values[m.id] ?? Self.hinted(values["hint:model"], in: m.choices) ?? m.value
        } else if let route = phone?.route, !route.isNull {
            d.modelId = route["id"].string ?? "hub_route"
            for c in route["options"].array {
                let inner = c["options"].array
                if inner.isEmpty {
                    if let v = EngineOption.text(c["value"]) { d.groups.append((nil, [.init(value: v, name: c["name"].string ?? v, description: c["description"].string)])) }
                } else {
                    let title = c["name"].string ?? c["group"].string
                    let choices = inner.compactMap { x in EngineOption.text(x["value"]).map { EngineOption.Choice(value: $0, name: x["name"].string ?? $0, description: x["description"].string) } }
                    d.groups.append((title, choices))
                }
            }
            d.modelValue = values[d.modelId ?? ""] ?? EngineOption.text(route["currentValue"])
        } else if let model = phone?.model {
            d.groups = [(nil, [.init(value: model, name: model, description: "starts on it")])]; d.modelValue = model
        }
        if let e = opts.first(where: { $0.category == "thought_level" || $0.id.contains("effort") }) {
            d.effortId = e.id; d.effortChoices = e.choices; d.effortValue = values[e.id] ?? Self.hinted(values["hint:effort"], in: e.choices) ?? e.value
        } else if let efforts = phone?.efforts {
            let provider = d.modelValue?.split(separator: ":").first.map(String.init) ?? phone?.provider
            if let provider, let list = efforts[provider], !list.isEmpty {
                d.effortId = "reasoning_effort"
                d.effortChoices = list.map { .init(value: $0.value, name: $0.name, description: $0.what) }
                d.effortValue = values["reasoning_effort"] ?? list.first(where: { $0.default == true })?.value
            }
        }
        if d.groups.isEmpty {
            let k = known(engine: engine)
            d.groups = [(nil, k.models)]
            if allow {
                d.builtIn = true; d.modelId = "hint:model"
                d.modelValue = values["hint:model"] ?? k.models.first?.value
                if d.effortChoices.isEmpty, !k.efforts.isEmpty {
                    d.effortId = "hint:effort"; d.effortChoices = k.efforts
                    d.effortValue = values["hint:effort"] ?? k.efforts.first { $0.name == "Medium" }?.value
                }
            } else {
                // A thread that has not run a turn yet: nothing to pick, only its default.
                d.groups = [(nil, [.init(value: "default", name: "Default", description: nil)])]
                d.modelValue = "default"
            }
        }
        return d
    }
}

// MARK: Which models are shown

/// Only the newest models and the favourites are shown, the rest one tap away (the desktop does
/// the same with favourites): the latest of each family, and anything from Claude 5.5 or GPT 6 up.
/// The chosen one and the favourites always show.
enum ModelFilter {
    static func version(_ name: String) -> Double? {
        guard let r = name.range(of: #"\d+(\.\d+)?"#, options: .regularExpression) else { return nil }
        return Double(name[r])
    }

    static func family(_ name: String) -> String {
        var n = name
        if let r = n.range(of: #"\d+(\.\d+)?"#, options: .regularExpression) { n.removeSubrange(r) }
        return n.trimmingCharacters(in: CharacterSet(charactersIn: " -·")).lowercased()
    }

    static func threshold(_ engine: String) -> Double? {
        let l = engine.lowercased()
        if l.contains("claude") { return 5.5 }
        if l.contains("codex") || l.contains("openai") || l.contains("gpt") { return 6 }
        return nil
    }

    private static let key = "hub.favoriteModels"
    static var favorites: Set<String> {
        get { Set(UserDefaults.standard.stringArray(forKey: key) ?? []) }
        set { UserDefaults.standard.set(Array(newValue), forKey: key) }
    }
    static func key(_ engine: String, _ value: String) -> String { "\(engine):\(value)" }

    /// The choices to show, or all of them when the engine has no rule or `all` is on.
    static func visible(engine: String, choices: [EngineOption.Choice], selected: String?, all: Bool) -> [EngineOption.Choice] {
        guard !all, let min = threshold(engine) else { return choices }
        let fav = favorites
        var best: [String: Double] = [:]
        for c in choices { if let v = version(c.name) { best[family(c.name), default: 0] = max(best[family(c.name)] ?? 0, v) } }
        return choices.filter { c in
            guard let v = version(c.name) else { return true }
            return v >= min || v == best[family(c.name)] || c.value == selected || fav.contains(key(engine, c.value))
        }
    }
}

// MARK: The picker body

/// The company tabs at the top, models as one-line rows, effort as a compact row. Used by the new
/// thread's model sheet and by a thread's engine panel.
struct ModelPickerBody: View {
    @Environment(Store.self) private var store
    let engines: [String]
    @Binding var selected: String
    var blocked: (String) -> String? = { _ in nil }
    let data: PickerData
    var loading = false
    let onModel: (String) -> Void
    let onEffort: (String) -> Void
    /// With these, a star tab comes first once there are favourites: the models you have starred, on any engine.
    var engineData: ((String) -> PickerData)?
    var onPickFavorite: ((String, String) -> Void)?
    @State private var showAll = false
    @State private var favTick = 0
    @State private var favTab: Bool?

    /// The starred models of the engines shown, as (engine, value).
    private var favourites: [(engine: String, value: String)] {
        guard engineData != nil else { return [] }
        let keys = ModelFilter.favorites
        return engines.flatMap { e in
            (engineData?(e).groups.flatMap(\.choices) ?? []).filter { keys.contains(ModelFilter.key(e, $0.value)) }.map { (e, $0.value) }
        }
    }

    var body: some View {
        let _ = favTick
        let favs = favourites
        let onFavs = (favTab ?? !favs.isEmpty) && !favs.isEmpty
        let total = data.groups.reduce(0) { $0 + $1.choices.count }
        let newest = data.groups.map { (title: $0.title, choices: ModelFilter.visible(engine: selected, choices: $0.choices, selected: data.modelValue, all: false)) }
        let groups = showAll ? data.groups.map { (title: $0.title, choices: $0.choices) } : newest
        let older = total - newest.reduce(0) { $0 + $1.choices.count }
        VStack(alignment: .leading, spacing: 10) {
            tabs(favs: favs, onFavs: onFavs)
            if !onFavs, let why = blocked(selected) {
                Text(why).font(.system(size: 12)).foregroundStyle(Hub.amber).padding(.horizontal, 4)
            }
            if onFavs {
                VStack(spacing: 0) {
                    ForEach(Array(favs.enumerated()), id: \.offset) { i, f in
                        let choice = engineData?(f.engine).groups.flatMap(\.choices).first { $0.value == f.value }
                        row(name: choice?.name ?? f.value, detail: store.engineLabel(f.engine), logo: f.engine, engine: f.engine, value: f.value,
                            on: f.engine == selected && f.value == data.modelValue, last: i == favs.count - 1) { onPickFavorite?(f.engine, f.value) }
                    }
                }
                .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
            } else {
                VStack(spacing: 0) {
                    ForEach(Array(groups.enumerated()), id: \.offset) { gi, g in
                        if let title = g.title {
                            Text(title).font(.system(size: 11.5, weight: .semibold)).foregroundStyle(Hub.muted)
                                .padding(.horizontal, 13).padding(.top, 8).padding(.bottom, 2).frame(maxWidth: .infinity, alignment: .leading)
                        }
                        ForEach(Array(g.choices.enumerated()), id: \.offset) { i, c in
                            row(name: c.name, detail: c.description, logo: nil, engine: selected, value: c.value, on: c.value == data.modelValue,
                                last: gi == groups.count - 1 && i == g.choices.count - 1 && older == 0 && data.effortChoices.isEmpty) { onModel(c.value) }
                        }
                    }
                    if !data.effortChoices.isEmpty { reasoning(last: older == 0) }
                    if older > 0 {
                        Button { withAnimation(.snappy) { showAll.toggle() } } label: {
                            HStack(spacing: 8) {
                                Image(systemName: showAll ? "chevron.up" : "chevron.down").font(.system(size: 10, weight: .semibold))
                                Text(showAll ? "Only the newest" : "Older models · \(older)").font(.system(size: 13.5))
                                Spacer()
                            }
                            .foregroundStyle(Hub.muted).padding(.horizontal, 13).frame(height: 40).contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
                .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
            }
        }
    }

    // MARK: Pieces

    private func tabs(favs: [(engine: String, value: String)], onFavs: Bool) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 4) {
                if !favs.isEmpty {
                    Button { withAnimation(.snappy) { favTab = true }; Haptics.tap() } label: {
                        Image(systemName: "star.fill").font(.system(size: 13)).foregroundStyle(onFavs ? Hub.accent : Hub.muted)
                            .frame(width: 42, height: 32)
                            .background(Capsule().fill(onFavs ? Hub.lineStrong.opacity(0.8) : .clear))
                            .contentShape(Capsule())
                    }
                    .buttonStyle(.plain).accessibilityLabel("Favourites")
                }
                ForEach(engines, id: \.self) { e in
                    let on = !onFavs && selected == e
                    Button { withAnimation(.snappy) { selected = e; favTab = false; showAll = false }; Haptics.tap() } label: {
                        HStack(spacing: 6) {
                            EngineLogo(engine: e, size: 13)
                            Text(store.engineLabel(e)).font(.system(size: 13.5, weight: on ? .semibold : .regular)).lineLimit(1)
                        }
                        .foregroundStyle(blocked(e) != nil ? Hub.muted : on ? Hub.text : Hub.text3)
                        .padding(.horizontal, 12).frame(height: 32)
                        .background(Capsule().fill(on ? Hub.lineStrong.opacity(0.8) : .clear))
                        .contentShape(Capsule())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(3)
        }
        .background(Capsule().fill(Hub.surface))
    }

    /// A model: its name, what it is for, a star to keep it, a check when it is the one in use.
    private func row(name: String, detail: String?, logo: String?, engine: String, value: String, on: Bool, last: Bool, pick: @escaping () -> Void) -> some View {
        let key = ModelFilter.key(engine, value)
        let fav = ModelFilter.favorites.contains(key)
        return HStack(spacing: 0) {
            Button { if TouchGuard.ok { pick() } } label: {
                HStack(spacing: 8) {
                    if let logo { EngineLogo(engine: logo, size: 12) }
                    Text(name).font(.system(size: 14.5)).lineLimit(1)
                    if let d = detail, !d.isEmpty { Text(d).font(.system(size: 12)).foregroundStyle(Hub.muted).lineLimit(1) }
                    Spacer(minLength: 4)
                }
                .padding(.leading, 13).frame(maxWidth: .infinity, minHeight: 40, alignment: .leading).contentShape(Rectangle())
            }
            .buttonStyle(.pressFlat)
            Button {
                var f = ModelFilter.favorites
                if fav { f.remove(key) } else { f.insert(key) }
                ModelFilter.favorites = f; favTick += 1; Haptics.tap()
            } label: {
                Image(systemName: fav ? "star.fill" : "star").font(.system(size: 13)).foregroundStyle(fav ? Hub.accent : Hub.lineStrong)
                    .frame(width: 38, height: 40).contentShape(Rectangle())
            }
            .buttonStyle(.plain).accessibilityLabel(fav ? "Remove from favourites" : "Keep as a favourite")
            Image(systemName: "checkmark").font(.system(size: 12, weight: .bold)).foregroundStyle(Hub.accent).opacity(on ? 1 : 0).frame(width: 26)
        }
        .background(on ? Hub.accentBg : .clear)
        .overlay(alignment: .bottom) { if !last { Hub.line.frame(height: 1).padding(.leading, 13) } }
    }

    /// Reasoning as one row: the level in use, a tap lists the levels.
    private func reasoning(last: Bool) -> some View {
        HubMenu {
            ForEach(data.effortChoices, id: \.value) { c in
                Button { onEffort(c.value) } label: { Label(c.name, systemImage: c.value == data.effortValue ? "checkmark" : "") }
            }
        } label: {
            HStack(spacing: 6) {
                Text("Reasoning").font(.system(size: 14.5)).foregroundStyle(Hub.text)
                Spacer()
                Text(data.effortChoices.first { $0.value == data.effortValue }?.name ?? "").font(.system(size: 13)).foregroundStyle(Hub.muted)
                Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(Hub.muted)
            }
            .padding(.horizontal, 13).frame(height: 42).contentShape(Rectangle())
            .overlay(alignment: .top) { Hub.line.frame(height: 1).padding(.leading, 13) }
        }
    }
}

// MARK: The new thread's model sheet

/// "The logo opens the models": the picker on a new thread, with Safety as a button.
struct ModelSheet: View {
    @Environment(Store.self) private var store
    @Binding var engine: String?
    @Binding var settings: [String: String]
    let warmOptions: [EngineOption]
    var loading: Bool
    @Binding var level: String?
    let onEngine: () -> Void
    let onSetting: () -> Void
    let setLevel: (String) -> Void
    let done: () -> Void
    /// A chat always asks before it acts, so its sheet has no Safety button.
    var allowSafety = true
    @State private var showSafety = false
    @State private var atTop = true

    /// What the agent reported for this page (warm), else what an engine offered on an old thread.
    private var agentOptions: [EngineOption] { warmOptions.isEmpty ? store.knownOptions(current) : warmOptions }

    private var current: String { engine ?? store.phoneEngines.first(where: { $0.default == true })?.name ?? store.allEngines.first ?? "claude" }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(showSafety ? "Safety" : "Model").font(.system(size: 14, weight: .semibold))
                Spacer()
                if allowSafety {
                    Button(showSafety ? "Model" : "Safety") { withAnimation(.snappy) { showSafety.toggle() } }
                        .font(.system(size: 14, weight: .semibold)).foregroundStyle(Hub.accent)
                }
            }
            .padding(.horizontal, 14).padding(.top, 9).padding(.bottom, 6)
            ScrollView {
                Group {
                    if showSafety {
                        VStack(spacing: 0) {
                            ForEach([("read", "Read only", "looks and reports"), ("ask", "Ask first", "asks before commands and edits"), ("edit", "Edits", "edits without asking"), ("everything", "Full", "never asks")], id: \.0) { l in
                                Button { setLevel(l.0) } label: {
                                    HStack { VStack(alignment: .leading, spacing: 1) { Text(l.1).font(.system(size: 14.5)); Text(l.2).font(.system(size: 11.5)).foregroundStyle(Hub.muted) }; Spacer()
                                        if (level ?? "ask") == l.0 { Image(systemName: "checkmark").foregroundStyle(Hub.accent) } }
                                    .padding(.horizontal, 13).frame(height: 46).contentShape(Rectangle())
                                    .overlay(alignment: .bottom) { Hub.line.frame(height: 1).padding(.leading, 13) }
                                }
                                .buttonStyle(.plain)
                            }
                        }
                        .background(RoundedRectangle(cornerRadius: 12).fill(Hub.surface))
                    } else {
                        picker
                    }
                }
                .padding(.horizontal, 14).padding(.bottom, 10)
                .background(NoBounce())
            }
            .trackTop($atTop)
            .frame(maxHeight: 380)
            .fixedSize(horizontal: false, vertical: true)
        }
        .background(RoundedRectangle(cornerRadius: 18).fill(Hub.side))
        .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(Hub.lineStrong))
        .bowDismiss(atTop: atTop, close: done)
    }

    /// What an engine offers here: the page's own options for the one in use, else what an old thread showed.
    private func data(for e: String) -> PickerData {
        let agent: [EngineOption] = e == current ? agentOptions : store.knownOptions(e)
        let phone = store.phoneEngines.first { $0.name == e }
        return PickerData.make(engine: e, agent: agent, phone: phone, values: e == current ? settings : [:])
    }

    private var picker: some View {
        let engineBinding = Binding<String>(get: { current }, set: { engine = $0; onEngine() })
        return ModelPickerBody(
            engines: store.allEngines, selected: engineBinding, data: data(for: current), loading: loading,
            onModel: { v in set(data(for: current).modelId, v); closeSoon() },
            onEffort: { v in set(data(for: current).effortId, v) },
            engineData: { e in data(for: e) },
            onPickFavorite: { e, v in
                engine = e
                onEngine()
                set(data(for: e).modelId ?? "hint:model", v)
                closeSoon()
            })
    }

    /// A model picked is the choice made: the sheet goes once the check has shown on the row.
    private func closeSoon() {
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(260))
            done()
        }
    }

    private func set(_ id: String?, _ value: String) {
        guard let id else { return }
        settings[id] = value
        Haptics.tap()
        if !id.hasPrefix("hint:") { onSetting() }
    }
}
