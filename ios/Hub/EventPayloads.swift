import Foundation

// What an event of each kind carries, for the kinds the app reads. hubd's Rust structs
// (crates/hub-core/src/model/event_data/) are the source of truth; each view below names the
// fields of one of them that the app uses, so the field names of the log are written in this file
// (a `user_message`'s mark names are the exception: `mark(_:)` takes them where they are read). A view reads the event's JSON as it came and checks nothing: a
// field an older or newer hubd left out is nil (or empty), as a missing key always was. What is
// still read as JSON elsewhere is the engine's own: the fields of an `update`, and what is inside
// `tool_call` and `options`.

/// `user_message`: what you (or Hub, on your behalf) said, and the marks Hub puts on its own.
struct UserMessagePayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var text: String? { d["text"].string }
    var attachments: [JSON] { d["attachments"].array }
    /// One of Hub's marks (`review`, `restart`, `watch`, ...), null when the message has none.
    func mark(_ key: String) -> JSON { d[key] }
}

/// `update`: the engine's own session update. Only which update it is has a name here.
struct UpdatePayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var kind: String? { d["sessionUpdate"].string }
}

/// `session_state`: what the engine offers. The options are the engine's own JSON.
struct SessionStatePayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var configOptions: [JSON] { d["config_options"].array }
}

/// `setting`: a pick of yours; `value` is a string or a bool.
struct SettingPayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var configId: String? { d["config_id"].string }
    var value: JSON { d["value"] }
}

/// `engine_switch`: the thread went from one engine to another.
struct EngineSwitchPayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var from: String? { d["from"].string }
    var to: String? { d["to"].string }
}

/// `moved`: the thread came here from another machine.
struct MovedPayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var from: String? { d["from"].string }
    var dirty: Double? { d["dirty"].number }
    var stopped: Bool? { d["stopped"].bool }
}

/// `moved_away`: the thread left for another machine.
struct MovedAwayPayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var to: String? { d["to"].string }
}

/// `quota_switch`: Hub changed engine ahead of a limit.
struct QuotaSwitchPayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var to: String? { d["to"].string }
    var limit: String? { d["limit"].string }
    var percent: Double? { d["percent"].number }
}

/// `quota_wait`: the thread waits for a limit, or what became of that wait (`wait` names it).
struct QuotaWaitPayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var wait: Double? { d["wait"].number }
    var state: String? { d["state"].string }
    var at: Double? { d["at"].number }
    var known: Bool? { d["known"].bool }
    var limit: String? { d["limit"].string }
    var tries: Double? { d["tries"].number }
    var by: String? { d["by"].string }
    var from: String? { d["from"].string }
    var to: String? { d["to"].string }
}

/// `child`: a side job, when it starts and when it ends.
struct ChildPayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var id: String? { d["id"].string }
    var title: String? { d["title"].string }
    var kind: String? { d["kind"].string }
    var engine: String? { d["engine"].string }
    var state: String? { d["state"].string }
    var images: [JSON] { d["images"].array }
    var error: String? { d["error"].string }
}

/// `page`: a page the agent showed.
struct PagePayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var id: String? { d["id"].string }
    var title: String? { d["title"].string }
    var height: Double? { d["height"].number }
    var replaces: String? { d["replaces"].string }
}

/// `permission_request`: the engine waits for an answer. `toolCall` and `options` are its own JSON.
struct PermissionRequestPayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var id: String? { d["id"].string }
    var toolCall: JSON { d["tool_call"] }
    var options: [JSON] { d["options"].array }
}

/// `permission_resolved`: the answer to the request `id`.
struct PermissionResolvedPayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var id: String? { d["id"].string }
    var optionId: String? { d["option_id"].string }
}

/// `replies`: the quick replies the agent offered; each option has a `label` and a `message`.
struct RepliesPayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var question: String? { d["question"].string }
    var options: [JSON] { d["options"].array }
}

/// `error`: the engine (or Hub) failed.
struct ErrorPayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var message: String? { d["message"].string }
}

/// `reaction`: an emoji put on, or taken off, the message `seq`.
struct ReactionPayload {
    let d: JSON
    init(_ d: JSON) { self.d = d }
    var seq: Double? { d["seq"].number }
    var emoji: String? { d["emoji"].string }
    var by: String? { d["by"].string }
    var on: Bool? { d["on"].bool }
}
