import Foundation

// The app's model and the pure parts of the store: what a hubd of any version sends, the event
// log folded into items, the inbox sections and dots, pairing links. Runs in the iOS simulator
// against every source file of the app but `HubApp.swift`; `ios/scripts/test.sh` does it.
nonisolated(unsafe) var failures = 0

func check(_ ok: Bool, _ what: @autoclosure () -> String, line: Int = #line) {
    if !ok { failures += 1; print("FAIL line \(line): \(what())") }
}

func thread(_ json: String) -> HubThread {
    try! JSONDecoder().decode(HubThread.self, from: Data(json.utf8))
}

func events(_ lines: [String]) -> [HubEvent] {
    let list = "[" + lines.joined(separator: ",") + "]"
    return try! JSONDecoder().decode(Lossy<HubEvent>.self, from: Data(list.utf8)).items
}

/// One event of the log: `ev(1, 1000, "update", #"{"sessionUpdate":"agent_message_chunk",...}"#)`.
func ev(_ seq: Int, _ ts: Double, _ kind: String, _ data: String = "{}") -> String {
    #"{"thread_id":"t","seq":\#(seq),"ts":\#(ts),"kind":"\#(kind)","data":\#(data)}"#
}

func chunk(_ seq: Int, _ ts: Double, _ text: String) -> String {
    ev(seq, ts, "update", #"{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"\#(text)"}}"#)
}

func tool(_ seq: Int, _ ts: Double, _ id: String, _ status: String, title: String = "Read file", kind: String = "read", extra: String = "") -> String {
    ev(seq, ts, "update", #"{"sessionUpdate":"tool_call","toolCallId":"\#(id)","title":"\#(title)","status":"\#(status)","kind":"\#(kind)"\#(extra)}"#)
}

func userMsg(_ seq: Int, _ ts: Double, _ text: String, extra: String = "") -> String {
    ev(seq, ts, "user_message", #"{"text":"\#(text)"\#(extra)}"#)
}

@main @MainActor struct ModelTests {
    static func main() {
        decoding()
        sections()
        lists()
        asks()
        fold()
        folded2()
        payloads()
        store()
        print(failures == 0 ? "Model tests passed" : "\(failures) model checks failed")
        exit(failures == 0 ? 0 : 1)
    }

    // MARK: A hubd of any version

    static func decoding() {
        let bare = thread(#"{"id":"a"}"#)
        check(bare.title == "" && bare.cwd == "" && bare.status == .idle && !bare.archived && bare.updated_at == 0, "only the id is needed")
        let odd = thread(#"{"id":"a","title":5,"status":"from-the-future","archived":"yes","updated_at":"x","unknown":{"a":1},"returned_at":7}"#)
        check(odd.title == "" && odd.status == .idle && !odd.archived && odd.updated_at == 0, "wrong types and unknown values fall back")
        check(odd.returned_at == 7, "a good field next to bad ones is kept")
        let full = thread(#"{"id":"b","title":"T","cwd":"/x/hub","engine":"claude","status":"waiting_on_you","host":"mac","pinned_at":3}"#)
        check(full.status == .waiting_on_you && full.host == "mac" && full.pinned_at == 3, "a full thread reads as it is")
        var threw = false
        do { _ = try JSONDecoder().decode(HubThread.self, from: Data(#"{"title":"no id"}"#.utf8)) } catch { threw = true }
        check(threw, "no id is not a thread")

        let list = try! JSONDecoder().decode(Lossy<HubThread>.self, from: Data(#"[{"id":"a"},{"title":"no id"},7,{"id":"c","status":"running"}]"#.utf8))
        check(list.items.map(\.id) == ["a", "c"], "Lossy skips what it cannot read, keeps the rest in order: \(list.items.map(\.id))")

        let es = events([
            #"{"thread_id":"t","seq":1}"#,
            #"{"thread_id":"t","seq":2,"ts":5,"kind":"never-heard-of","data":{"x":[1,2]}}"#,
            #"{"seq":3}"#,
        ])
        check(es.count == 2, "an event with no thread id is skipped")
        check(es[0].kind == "" && es[0].ts == 0 && es[0].data.isNull, "an event with only seq still reads")
        check(es[1].kind == "never-heard-of" && es[1].data["x"].array.count == 2, "an unknown kind is kept")

        let j = try! JSONDecoder().decode(JSON.self, from: Data(#"{"a":{"b":["x",null,true,1.5]}}"#.utf8))
        check(j["a"]["b"].array.count == 4 && j["a"]["b"].array[0].string == "x", "JSON subscripts")
        check(j["a"]["b"].array[1].isNull && j["missing"]["deeper"].isNull && j["a"]["b"].array[3].number == 1.5, "JSON null, missing and number")
        check(j["a"]["b"].array[2].bool == true && j["a"].string == nil, "JSON bool, wrong type is nil")
    }

    // MARK: Inbox sections, dots, names

    static func sections() {
        let now = Date().timeIntervalSince1970 * 1000
        func th(_ status: String, _ extra: String = "") -> HubThread { thread(#"{"id":"a","status":"\#(status)"\#(extra)}"#) }

        check(th("waiting_on_you").section == .needsYou && th("error").section == .needsYou, "asks and errors need you")
        check(th("running").section == .working && th("in_terminal").section == .working, "running is working")
        check(th("idle").section == nil, "an idle thread that never answered is in no group")
        check(th("idle", #","returned_at":10"#).section == .done, "an idle thread that answered is done")
        check(th("running", #","archived":true"#).section == nil, "archived is in no group")
        check(th("idle", #","returned_at":10,"settled_at":20"#).section == nil, "settled is in no group")
        check(th("idle", #","returned_at":10,"snoozed_until":\#(now + 3_600_000)"#).section == nil, "snoozed is hidden")
        check(th("waiting_on_you", #","snoozed_until":\#(now + 3_600_000)"#).section == .needsYou, "a snooze never hides an ask")
        check(th("error", #","snoozed_until":\#(now + 3_600_000)"#).section == .needsYou, "a snooze never hides an error")
        check(th("idle", #","returned_at":10,"snoozed_until":\#(now - 1000)"#).section == .done, "a snooze that ended is over")
        var side = th("idle", #","returned_at":10"#)
        side.sideBusy = 5
        check(side.section == .working && side.dot == .running, "a side job at work keeps it working")

        check(th("waiting_on_you").dot == .waiting && th("error").dot == .error && th("running").dot == .running, "dots follow status")
        check(th("idle", #","returned_at":10"#).dot == .unread, "unseen answer: unread dot")
        check(th("idle", #","returned_at":10,"seen_at":10"#).dot == .idle, "seen at the same time: no dot")
        check(th("idle", #","returned_at":10,"seen_at":9"#).unread && !th("idle").unread, "unread is returned after seen")

        check(th("idle", #","settled_at":1"#).statusWord == "Settled" && th("idle", #","archived":true"#).statusWord == "Archived", "status words")
        check(th("waiting_on_you").statusWord == "Needs you" && th("running").statusWord == "Working", "status words from the section")
        check(th("idle").statusWord == "Done", "status word of a thread in no group")

        func proj(_ cwd: String) -> String { thread(#"{"id":"a","cwd":\#(String(data: try! JSONEncoder().encode(cwd), encoding: .utf8)!)}"#).project }
        check(proj("/home/x/dev/hub") == "Hub", "project: last folder, capitalised")
        check(proj(#"D:\Dev\hub\.hub-threads\abc"#) == "Hub", "project: Windows path and worktree")
        check(proj("/r/repo/.hub-worktrees/x") == "Repo", "project: worktree folder")
        check(proj("") == "—" && proj("/") == "/", "project: empty is a dash: \(proj("/"))")

        func cwd(_ c: String) -> HubThread { thread(#"{"id":"a","cwd":"\#(c)"}"#) }
        check(cwd("/d/scratch/2026-11-01-130344").isQuick && !cwd("/d/hub").isQuick, "a dated folder is a quick thread")
        check(cwd("/d/chats/2026-11-01-130344").inChatsFolder && !cwd("/d/chats/2026-11-01-130344").isQuick, "a chat is not a quick thread")
        check(thread(#"{"id":"a","cwd":"C:\\Hub\\chats\\x"}"#).inChatsFolder, "a chat on Windows")
    }

    static func lists() {
        let all = (try! JSONDecoder().decode(Lossy<HubThread>.self, from: Data("""
        [{"id":"a","cwd":"/r/hub","updated_at":1,"returned_at":50},
         {"id":"b","cwd":"/r/hub/.hub-threads/b","updated_at":2,"sent_at":70},
         {"id":"c","cwd":"/r/hub","updated_at":3,"archived":true},
         {"id":"d","cwd":"/r/hub","host":"mac","updated_at":4},
         {"id":"e","cwd":"/r/other","updated_at":5}]
        """.utf8))).items
        let ix = ThreadIndex(all)
        check(ix.of(ProjectRef(path: "/r/hub", name: "Hub", host: nil)).map(\.id) == ["b", "a"], "a worktree joins its repo, newest first, archived left out: \(ix.of(ProjectRef(path: "/r/hub", name: "Hub", host: nil)).map(\.id))")
        check(ix.of(ProjectRef(path: "/r/hub", name: "Hub", host: "mac")).map(\.id) == ["d"], "another machine's folder is its own project")
        check(ix.lastActive(ProjectRef(path: "/r/hub", name: "Hub", host: nil)) == 70, "last active is the newest answer or send")
        check(ix.of(ProjectRef(path: "/none", name: "x", host: nil)).isEmpty && ix.lastActive(ProjectRef(path: "/none", name: "x", host: nil)) == nil, "unknown project")
    }

    // MARK: Asks

    static func asks() {
        func risky(_ c: String?) -> Bool { Ask(id: "a", title: "t", command: c, options: []).risky }
        check(risky("rm -rf /tmp/x") && risky("rm -fr x") && risky("git push origin main --force") && risky("git reset --hard HEAD~1"), "deleting commands are risky")
        check(risky("git clean -fd") && risky("DROP TABLE users") && risky("dd if=/dev/zero of=/dev/sda") && risky("mkfs.ext4 /dev/sda1"), "more risky commands")
        check(risky("Remove-Item C:\\x -Recurse -Force"), "PowerShell recursive delete")
        check(!risky("git status") && !risky("ls -la") && !risky("rm file.txt") && !risky("git push origin main") && !risky(nil), "ordinary commands are not risky")
        let a = Ask(id: "a", title: "t", command: nil, options: [
            .init(id: "1", name: "Allow once", kind: "allow_once"), .init(id: "2", name: "Always", kind: "allow_always"), .init(id: "3", name: "No", kind: "reject_once")])
        check(a.allowOptions.map(\.id) == ["1", "2"] && a.hasReject, "allow options and reject")
        check(!Ask(id: "a", title: "t", command: nil, options: []).hasReject, "no reject when none is offered")
    }

    // MARK: The event log, folded

    static func fold() {
        let f = Folded(events([
            userMsg(1, 1000, "Hello"),
            chunk(2, 1100, "Hi "), chunk(3, 1200, "there"),
            tool(4, 1300, "c1", "pending"), tool(5, 1400, "c2", "pending", title: "Edit", kind: "edit"),
            ev(6, 1500, "update", #"{"sessionUpdate":"tool_call_update","toolCallId":"c1","status":"completed","rawInput":{"command":"ls"},"content":[{"content":{"text":"out"}}]}"#),
            chunk(7, 1600, "Done."),
            ev(8, 1700, "turn_end"),
        ]))
        check(f.items.count == 4, "user, reply, work, reply: \(f.items.count)")
        if case let .user(t) = f.items[0] { check(t == "Hello", "user text") } else { check(false, "first is the user") }
        if case let .reply(t) = f.items[1] { check(t == "Hi there", "chunks join into one reply: \(t)") } else { check(false, "second is a reply") }
        if case let .work(steps) = f.items[2] {
            check(steps.count == 2 && steps[0].status == "completed" && steps[0].command == "ls" && steps[0].output == "out" && steps[1].kind == "edit", "two tool calls in one group, the update changes the first in place")
        } else { check(false, "third is work") }
        check(f.lastReply == "Done." && f.turns == [1, 1, 1, 1] && f.times.count == 4, "last reply, one turn: \(f.turns)")
        check(f.userSeq[1] == 1 && f.endSeq[1] == 8 && f.turnEnd[1] == 1700, "message seqs and the turn's end")
        check(f.activity == "Writing…", "activity is the latest live event: \(String(describing: f.activity))")

        // Two messages from you: two turns, the reply of the first stays apart.
        let g = Folded(events([userMsg(1, 1, "a"), chunk(2, 2, "r1"), userMsg(3, 3, "b"), chunk(4, 4, "r2")]))
        check(g.turns == [1, 1, 2, 2] && g.activity == "Writing…" && g.replies == nil, "two turns: \(g.turns)")
        let g2 = Folded(events([userMsg(1, 1, "a"), chunk(2, 2, "r1"), userMsg(3, 3, "b")]))
        check(g2.activity == nil, "your message clears what the agent was doing")

        // Notes Hub wrote to the agent are quiet lines, not your words.
        let h = Folded(events([
            userMsg(1, 1, "", extra: #","proof":{}"#),
            userMsg(2, 2, "", extra: #","watch":{"number":42}"#),
            userMsg(3, 3, "", extra: #","child":{"id":"x"}"#),
            userMsg(4, 4, "real"),
        ]))
        check(h.items.count == 3, "a side job's result adds no line: \(h.items)")
        if case let .hub(t) = h.items[1] { check(t == "Hub told the agent about pull request #42", "watch note: \(t)") } else { check(false, "second is a hub note") }
        check(h.userSeq[1] == nil && h.userSeq[4] == 4, "a note from Hub is not your message")

        // Attachments: a picture shows small, a link by its name; they come before your words.
        let s = Folded(events([userMsg(1, 1, "look", extra: #","attachments":[{"mime":"image/png","name":"a.png","data":"AAAA"},{"name":"doc","url":"https://x"},{"mime":"inode/directory","name":"dir"}]"#)]))
        if case let .sent(files) = s.items[0] {
            check(files.count == 3 && files[0].picture != nil && files[1].link == "https://x" && files[2].folder, "sent files")
        } else { check(false, "attachments come first") }
        check(s.items.count == 2, "attachments plus words")
        check(SentFile.found(in: [], seq: 1).isEmpty, "no attachments")
        let empty = SentFile.found(in: try! JSONDecoder().decode(JSON.self, from: Data(#"[{"mime":"image/png","name":"a","data":""}]"#.utf8)).array, seq: 1)
        check(empty[0].picture == nil, "an image with no bytes is a name only")

        // Asks.
        let a = Folded(events([
            tool(1, 1, "c1", "pending", title: "Run", kind: "execute", extra: #","rawInput":{"command":"rm -rf x"}"#),
            ev(2, 2, "permission_request", #"{"id":"p1","tool_call":{"toolCallId":"c1"},"options":[{"optionId":"o1","name":"Allow","kind":"allow_once"},{"optionId":"o2","name":"No","kind":"reject_once"}]}"#),
        ]))
        check(a.openAsk?.id == "p1" && a.openAsk?.command == "rm -rf x" && a.openAsk?.title == "Run" && a.openAsk?.risky == true, "an ask takes the command of its tool call")
        let a2 = Folded(events([
            ev(1, 1, "permission_request", #"{"id":"p1","options":[]}"#),
            ev(2, 2, "permission_request", #"{"id":"p2","options":[]}"#),
            ev(3, 3, "permission_resolved", #"{"id":"p2","option_id":"o1"}"#),
        ]))
        check(a2.openAsk?.id == "p1", "the ask still waiting, not the answered one")
        let a3 = Folded(events([ev(1, 1, "permission_request", #"{"id":"p1","options":[]}"#), ev(2, 2, "permission_resolved", #"{"id":"p1","option_id":"o1"}"#)]))
        check(a3.openAsk == nil, "answered: nothing open")
        if case let .ask(x) = a3.items[0] { check(x.resolved && x.answer == "o1" && x.title == "The agent wants to continue", "resolved ask keeps the answer and a default title") } else { check(false, "ask item") }

        // Quick replies need two options.
        let r = Folded(events([ev(1, 1, "replies", #"{"question":"Which?","options":[{"label":"A"},{"label":" ","message":"x"},{"label":"B","message":"go B"}]}"#)]))
        check(r.replies?.options.map(\.label) == ["A", "B"] && r.replies?.options[0].message == "A" && r.replies?.options[1].message == "go B", "replies: blank labels dropped, message defaults to label")
        let r1 = Folded(events([ev(1, 1, "replies", #"{"options":[{"label":"A"}]}"#)]))
        check(r1.replies == nil, "one option is no quick reply")
        let r2 = Folded(events([ev(1, 1, "replies", #"{"options":[{"label":"A"},{"label":"B"}]}"#), userMsg(2, 2, "x")]))
        check(r2.replies == nil, "your message drops the quick replies")
    }

    static func folded2() {
        // Reactions: the latest event per emoji decides, and chips keep first-seen order.
        let r = Folded(events([
            userMsg(1, 1, "a"), chunk(2, 2, "r"), ev(3, 3, "turn_end"),
            ev(4, 4, "reaction", #"{"seq":3,"emoji":"👍","by":"you"}"#),
            ev(5, 5, "reaction", #"{"seq":3,"emoji":"🎉","by":"agent"}"#),
            ev(6, 6, "reaction", #"{"seq":3,"emoji":"👍","by":"you","on":false}"#),
            ev(7, 7, "reaction", #"{"seq":3,"emoji":"","by":"you"}"#),
        ]))
        check(r.reactions[3]?.map(\.emoji) == ["🎉"] && r.reactions[3]?.first?.agent == true && r.reactions[3]?.first?.you == false, "reactions: removed ones go, empty emoji ignored: \(String(describing: r.reactions[3]))")
        check(r.turnEnd[1] == 3, "a reaction does not move when the turn ended")

        // Engine switches, moves and waits are lines on their own.
        let b = Folded(events([
            userMsg(1, 1, "a"),
            ev(2, 2, "engine_switch", #"{"from":"claude","to":"codex"}"#),
            ev(3, 3, "moved", #"{"from":"pc","dirty":2,"stopped":true}"#),
            ev(4, 4, "moved_away", #"{"to":"mac"}"#),
            ev(5, 5, "quota_switch", #"{"to":"codex","limit":"Weekly","percent":91.4}"#),
            ev(6, 6, "error", #"{"message":"boom"}"#),
            ev(7, 7, "error", "{}"),
            ev(8, 8, "restart"),
            ev(9, 9, "something_new", #"{"x":1}"#),
        ]))
        let kinds: [String] = b.items.map {
            switch $0 {
            case let .brk(x): return "brk:" + x.kind
            case let .hub(t): return "hub:" + t
            case let .error(t): return "error:" + t
            case .user: return "user"
            default: return "other"
            }
        }
        check(kinds[1] == "brk:engine" && kinds[2] == "brk:machine" && kinds[3] == "brk:machine", "engine and machine lines: \(kinds)")
        check(kinds[4] == "hub:Switched to codex: weekly at 91%", "quota switch line: \(kinds[4])")
        check(kinds[5] == "error:boom" && kinds[6] == "error:The engine failed" && kinds[7].hasPrefix("hub:Hub restarted"), "errors and restart: \(kinds)")
        check(kinds.count == 8, "an unknown kind adds nothing: \(kinds.count)")
        if case let .brk(x) = b.items[2] { check(x.text == "Moved here from pc" && x.note?.contains("with 2 uncommitted files") == true && x.hint == "The turn that was running carries on.", "moved line: \(x)") }
        if case let .brk(x) = b.items[1] { check(x.text == "Claude → switched to Codex", "switch line: \(x.text)") }

        // Side jobs: later events of one job change its line in place and keep when it started.
        let c = Folded(events([
            ev(1, 100, "child", #"{"id":"j1","title":"Image","kind":"image","engine":"codex"}"#),
            ev(2, 200, "child", #"{"id":"j1","state":"done","images":["a","b"]}"#),
            ev(3, 300, "child", #"{"title":"no id"}"#),
        ]))
        check(c.items.count == 1, "one line per side job")
        if case let .side(j) = c.items[0] { check(j.state == "done" && j.started == 100 && j.finished == 200 && j.pictures == 2, "side job: \(j)") }

        // Pages: a page that replaces another is a version of it.
        let p = Folded(events([
            ev(1, 1, "page", #"{"id":"p1","title":"Chart","height":300}"#),
            ev(2, 2, "page", #"{"id":"p2","replaces":"p1"}"#),
            ev(3, 3, "page", #"{"title":"no id"}"#),
        ]))
        check(p.items.count == 2, "a page with no id is skipped")
        if case let .page(first) = p.items[0], case let .page(second) = p.items[1] {
            check(first.of == 2 && second.of == 2 && second.version == 2 && second.title == "Page", "page versions: \(first.of) \(second.version)")
        }

        // The plan keeps when each step started and finished.
        let pl = Folded(events([
            userMsg(1, 1000, "go"),
            ev(2, 2000, "update", #"{"sessionUpdate":"plan","entries":[{"content":"A","status":"in_progress"},{"content":"B","status":"pending"}]}"#),
            ev(3, 5000, "update", #"{"sessionUpdate":"plan","entries":[{"content":"A","status":"completed"},{"content":"B","status":"in_progress"}]}"#),
        ]))
        check(pl.plan.count == 2 && pl.plan[0].took == 3000 && pl.plan[1].started == 5000 && pl.plan[1].finished == nil, "plan timings: \(pl.plan)")

        // "New" line: placed before the first thing the agent did after you last looked.
        let n = Folded(events([userMsg(1, 100, "a"), chunk(2, 150, "old"), chunk(3, 300, "new1"), ev(4, 310, "turn_end"), userMsg(5, 320, "b"), chunk(6, 330, "new2")]), since: 200)
        let newAt = n.items.firstIndex { if case let .brk(x) = $0 { return x.kind == "new" }; return false }
        check(newAt == 2, "the New line goes before what came after you looked: \(String(describing: newAt))")
        if let newAt, case let .brk(x) = n.items[newAt] { check(x.note?.hasPrefix("1 reply") == true || x.note?.hasPrefix("2 repl") == true, "New note counts replies: \(String(describing: x.note))") }
        let own = Folded(events([userMsg(1, 100, "a"), userMsg(2, 300, "mine"), chunk(3, 310, "r")]), since: 200)
        check(!own.items.contains { if case .brk = $0 { return true }; return false }, "your own message after you came back places no line")
        let none = Folded(events([userMsg(1, 100, "a"), chunk(2, 150, "r")]), since: 200)
        check(none.items.count == 2, "nothing new, no line")

        // A long pause before your message is a break too.
        let t = Folded(events([userMsg(1, 0, "a"), chunk(2, 1000, "r"), userMsg(3, 1000 + 3_600_000, "later")]))
        check(t.items.contains { if case let .brk(x) = $0 { return x.kind == "time" }; return false }, "an hour's pause draws a line")

        // The thread you just wrote to: shown at once as the next turn.
        var own2 = Folded(events([userMsg(1, 1, "a"), chunk(2, 2, "r")]))
        own2.addOwn("fast", at: 50)
        check(own2.turns.last == 2 && own2.turnEnd[2] == 50 && own2.activity == nil && own2.times.count == own2.items.count, "addOwn starts the next turn")

        // Words for what a live event shows.
        func words(_ json: String) -> String? { Folded.words(for: try! JSONDecoder().decode(JSON.self, from: Data(json.utf8))) }
        check(words(#"{"sessionUpdate":"agent_thought_chunk"}"#) == "Thinking…" && words(#"{"sessionUpdate":"plan"}"#) == nil, "live words")
        check(words(#"{"sessionUpdate":"tool_call","kind":"read"}"#) == "Reading…" && words(#"{"sessionUpdate":"tool_call","kind":"edit"}"#) == "Editing…", "live words for tools")
        check(words(#"{"sessionUpdate":"tool_call","title":"mcp show_page","kind":"other"}"#) == "Building the page…" && words(#"{"sessionUpdate":"tool_call","kind":"zzz"}"#) == "Working…", "live words by title and default")
        check(words(#"{"sessionUpdate":"tool_call","title":"WebSearch","kind":"fetch"}"#) == "Searching the web…", "web search")

        // Tool output: ACP content, else rawOutput; long output keeps its end.
        func out(_ json: String) -> String? { Folded.output(try! JSONDecoder().decode(JSON.self, from: Data(json.utf8))) }
        check(out(#"{"content":[{"content":{"text":"a"}},{"text":"b"}]}"#) == "a\nb", "output joins blocks")
        check(out(#"{"rawOutput":{"output":"raw"}}"#) == "raw" && out(#"{"rawOutput":"plain"}"#) == "plain" && out("{}") == nil, "output from rawOutput")
        let long = String(repeating: "x", count: 4500) + "END"
        check(out(#"{"rawOutput":"\#(long)"}"#)?.count == 4001 && out(#"{"rawOutput":"\#(long)"}"#)?.hasSuffix("END") == true, "long output keeps its end")

        // A failed tool call's edit never happened.
        let fe = Folded(events([
            userMsg(1, 1, "go"),
            tool(2, 2, "c1", "failed", title: "Write", kind: "edit", extra: #","content":[{"type":"diff","path":"a.csv","oldText":"","newText":"x,y\n1,2\n"}]"#),
            ev(3, 3, "turn_end"),
        ]))
        check(fe.made.isEmpty, "a failed edit makes no file")
    }

    // MARK: Payloads by kind

    static func payloads() {
        func json(_ text: String) -> JSON { try! JSONDecoder().decode(JSON.self, from: Data(text.utf8)) }
        // Lines as hubd writes them (crates/hub-core/tests/it/frozen/golden_events.jsonl has the shapes).
        let said = UserMessagePayload(json(#"{"text":"hi","attachments":[{"name":"a.png"}],"restart":4}"#))
        check(said.text == "hi" && said.attachments.count == 1 && !said.mark("restart").isNull && said.mark("review").isNull, "user_message")
        let wait = QuotaWaitPayload(json(#"{"state":"waiting","at":5000,"known":true,"limit":"Session","tries":2,"seq":7}"#))
        check(wait.state == "waiting" && wait.at == 5000 && wait.known == true && wait.limit == "Session" && wait.tries == 2 && wait.wait == nil && wait.to == nil, "quota_wait")
        let switched = QuotaWaitPayload(json(#"{"state":"switched","wait":44,"to":"codex","from":"claude","by":"you"}"#))
        check(switched.state == "switched" && switched.wait == 44 && switched.to == "codex" && switched.from == "claude" && switched.by == "you", "quota_wait, switched")
        let child = ChildPayload(json(#"{"id":"c1","title":"Draw","kind":"image","context":"","engine":"codex","model":null,"state":"done","by":"agent","images":["a.png","b.png"]}"#))
        check(child.id == "c1" && child.title == "Draw" && child.kind == "image" && child.engine == "codex" && child.state == "done" && child.images.count == 2 && child.error == nil, "child")
        check(ChildPayload(json(#"{"id":"c2","title":"Draw","kind":"image","state":"failed","by":"agent","error":"it was stopped"}"#)).error == "it was stopped", "child, failed")
        let asked = PermissionRequestPayload(json(#"{"tool_call":{"title":"Run","toolCallId":"t1"},"options":[{"optionId":"a","name":"Allow","kind":"allow_once"}],"id":"p1"}"#))
        check(asked.id == "p1" && asked.toolCall["title"].string == "Run" && asked.options.count == 1, "permission_request")
        let answered = PermissionResolvedPayload(json(#"{"id":"p1","option_id":"a"}"#))
        check(answered.id == "p1" && answered.optionId == "a", "permission_resolved")
        let page = PagePayload(json(#"{"id":"pg","title":"Board","height":320,"replaces":"old"}"#))
        check(page.id == "pg" && page.title == "Board" && page.height == 320 && page.replaces == "old", "page")
        let reaction = ReactionPayload(json(#"{"seq":3,"emoji":"👍","by":"you","on":false}"#))
        check(reaction.seq == 3 && reaction.emoji == "👍" && reaction.by == "you" && reaction.on == false, "reaction")
        let moved = MovedPayload(json(#"{"from":"pc","folder":"/r","via":"bundle","files":[],"dirty":2,"stopped":true}"#))
        check(moved.from == "pc" && moved.dirty == 2 && moved.stopped == true, "moved")
        let quota = QuotaSwitchPayload(json(#"{"limit":"Weekly","percent":92,"to":"codex"}"#))
        check(quota.limit == "Weekly" && quota.percent == 92 && quota.to == "codex", "quota_switch")
        let setting = SettingPayload(json(#"{"config_id":"model","value":"opus"}"#))
        check(setting.configId == "model" && setting.value.string == "opus", "setting")
        check(SettingPayload(json(#"{"config_id":"fast","value":true}"#)).value.bool == true, "setting, a switch")
        let engines = EngineSwitchPayload(json(#"{"from":"claude","to":"codex"}"#))
        check(engines.from == "claude" && engines.to == "codex" && MovedAwayPayload(json(#"{"to":"mac","folder":"/r"}"#)).to == "mac", "engine_switch, moved_away")
        let replies = RepliesPayload(json(#"{"question":"Which?","options":[{"label":"A","message":"a"}]}"#))
        check(replies.question == "Which?" && replies.options.count == 1 && ErrorPayload(json(#"{"message":"no"}"#)).message == "no", "replies, error")
        check(UpdatePayload(json(#"{"sessionUpdate":"plan","entries":[]}"#)).kind == "plan" && SessionStatePayload(json(#"{"config_options":[{"id":"model"}]}"#)).configOptions.count == 1, "update, session_state")
        // A payload that is not an object, or lacks the field, says nothing.
        let bare = ChildPayload(.null)
        check(bare.id == nil && bare.images.isEmpty && UserMessagePayload(.null).mark("restart").isNull && QuotaWaitPayload(json("{}")).state == nil, "an empty payload")
    }

    // MARK: Store helpers and pairing

    static func store() {
        check(Store.tail("First.\n\nSecond with [a link](https://x) and **bold** and `code`.") == "Second with a link and bold and code.", "tail: last paragraph, marks dropped: \(Store.tail("First.\n\nSecond with [a link](https://x) and **bold** and `code`."))")
        check(Store.tail("Text\n\n```\ncode\n```") == "Text" && Store.tail("![pic](http://x/y.png) caption") == "caption", "tail: code fences and pictures dropped")
        check(Store.tail("one\ntwo") == "one two", "tail: lines join")
        check(Store.scratch("2026-11-01-130344") && !Store.scratch("hub") && !Store.scratch("x2026-11-01"), "scratch folder names")
        check(Store.projectRoot("/r/hub/.hub-threads/abc") == "/r/hub" && Store.projectRoot(#"C:\r\hub\.hub-threads\abc"#) == #"C:\r\hub"# && Store.projectRoot("/r/hub") == "/r/hub", "project root of a worktree")
        check(Store.projectRoot("/r/hub/.hub-worktrees/x") == "/r/hub", "project root of a hub worktree")
        let times = Store.snoozeTimes()
        let now = Date().timeIntervalSince1970 * 1000
        check(times.count == 2 && times[0].until > now + 3_500_000 && times[0].until < now + 3_700_000 && times[1].until > now, "snooze times are in the future")

        let ok = HubClient.parse(link: "  https://tincan.tail1.ts.net/phone#pair=ABCD-1234 \n")
        check(ok?.0.absoluteString == "https://tincan.tail1.ts.net" && ok?.1 == "ABCD-1234", "pairing link: \(String(describing: ok))")
        let port = HubClient.parse(link: "http://100.1.2.3:7777/phone#pair=ZZ&x=1")
        check(port?.0.absoluteString == "http://100.1.2.3:7777" && port?.1 == "ZZ", "pairing link with a port and more: \(String(describing: port))")
        check(HubClient.parse(link: "https://x/phone") == nil && HubClient.parse(link: "https://x/phone#pair=") == nil && HubClient.parse(link: "not a link") == nil && HubClient.parse(link: "") == nil, "not a pairing link")
    }
}
