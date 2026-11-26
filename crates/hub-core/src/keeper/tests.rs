//! Guards engines that outlive the core: a turn one core started ends in the next, a turn that
//! ended while no core listened is kept for the next one, and keys in the environment stay out of
//! the log.

use super::*;

fn core(r: &mut Relay, v: Value) -> Vec<Out> {
    r.on_core(&v.to_string())
}
fn agent(r: &mut Relay, v: Value) -> Vec<Out> {
    r.on_agent(&v.to_string())
}
fn json_of(out: &Out) -> Value {
    match out {
        Out::Agent(l) | Out::Core(l) => serde_json::from_str(l).unwrap(),
    }
}

#[test]
fn a_turn_started_by_one_core_ends_in_the_next() {
    let mut r = Relay::default();
    r.attach();
    // The first core starts the engine and a session.
    let out = core(&mut r, json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    let to_agent = json_of(&out[0]);
    let out = agent(&mut r, json!({ "jsonrpc": "2.0", "id": to_agent["id"], "result": { "protocolVersion": 1 } }));
    assert_eq!(json_of(&out[0])["id"], 0, "answered under the core's own id");
    let out = core(&mut r, json!({ "jsonrpc": "2.0", "id": 1, "method": "session/new", "params": {} }));
    let n = json_of(&out[0])["id"].clone();
    agent(&mut r, json!({ "jsonrpc": "2.0", "id": n, "result": { "sessionId": "s1" } }));
    let out = core(&mut r, json!({ "jsonrpc": "2.0", "id": 2, "method": "session/prompt", "params": { "sessionId": "s1" } }));
    let prompt_id = json_of(&out[0])["id"].clone();

    // It hands over mid-turn; the agent talks on and asks something.
    r.detached();
    assert!(agent(&mut r, json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "n": 1 } })).is_empty());
    assert!(agent(&mut r, json!({ "jsonrpc": "2.0", "id": 7, "method": "session/request_permission", "params": {} })).is_empty());

    // The next core takes over and hears what it missed, the question last.
    let (attached, replay) = r.attach();
    assert_eq!(attached.init, Some(json!({ "protocolVersion": 1 })));
    assert_eq!(attached.session.as_deref(), Some("s1"));
    assert!(attached.busy);
    assert_eq!(replay.len(), 2);
    assert!(replay[1].contains("request_permission"));

    // Its own ids start over at 0 and do not mix with the old prompt.
    let out = core(&mut r, json!({ "jsonrpc": "2.0", "id": 2, "method": WAIT_TURN, "params": {} }));
    assert!(out.is_empty(), "waits for the turn");
    let out = core(&mut r, json!({ "jsonrpc": "2.0", "id": 7, "result": { "outcome": "ok" } }));
    assert_eq!(out, vec![Out::Agent(json!({ "jsonrpc": "2.0", "id": 7, "result": { "outcome": "ok" } }).to_string())]);
    let out = agent(&mut r, json!({ "jsonrpc": "2.0", "id": prompt_id, "result": { "stopReason": "end_turn" } }));
    assert_eq!(json_of(&out[0]), json!({ "jsonrpc": "2.0", "id": 2, "result": { "stopReason": "end_turn" } }));
    assert!(!r.attach().0.busy);
}

#[test]
fn a_turn_that_ended_while_nobody_listened_is_kept_for_the_next_core() {
    let mut r = Relay::default();
    r.attach();
    let out = core(&mut r, json!({ "jsonrpc": "2.0", "id": 5, "method": "session/prompt", "params": {} }));
    let n = json_of(&out[0])["id"].clone();
    r.detached();
    assert!(agent(&mut r, json!({ "jsonrpc": "2.0", "id": n, "error": { "code": -1, "message": "no" } })).is_empty());
    assert!(r.attach().0.busy);
    let out = core(&mut r, json!({ "jsonrpc": "2.0", "id": 0, "method": WAIT_TURN }));
    assert_eq!(json_of(&out[0]), json!({ "jsonrpc": "2.0", "id": 0, "error": { "code": -1, "message": "no" } }));
    // An answer to something else of the old core's is dropped.
    let out = core(&mut r, json!({ "jsonrpc": "2.0", "id": 1, "method": "session/set_config_option" }));
    let n = json_of(&out[0])["id"].clone();
    r.detached();
    r.attach();
    assert!(agent(&mut r, json!({ "jsonrpc": "2.0", "id": n, "result": {} })).is_empty());
}

#[test]
fn keys_in_the_environment_are_kept_out_of_the_log() {
    let env = BTreeMap::from([
        ("ANTHROPIC_API_KEY".to_owned(), "sk-ant-0123456789".to_owned()),
        ("PATH".to_owned(), "/usr/bin:/bin:/usr/local/bin".to_owned()),
        ("HUB_TOKEN".to_owned(), "short".to_owned()),
    ]);
    assert_eq!(secrets(&env), vec!["sk-ant-0123456789".to_owned()]);
}
