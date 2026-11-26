import Foundation

/// The same evening the sketches show (iphone-app/putsad), so the app has something to show
/// before it is paired.
enum Demo {
    static let now = Date().timeIntervalSince1970 * 1000
    static func ago(_ minutes: Double) -> Double { now - minutes * 60_000 }

    static func thread(_ id: String, _ title: String, _ project: String, _ host: String,
                       _ status: HubThread.Status, _ minutes: Double, seen: Bool = false) -> HubThread {
        HubThread(id: id, title: title, cwd: "/home/c/dev/\(project.lowercased())", engine: "claude",
                  status: status, archived: false, host: host, updated_at: ago(minutes),
                  returned_at: status == .running ? nil : ago(minutes),
                  seen_at: seen ? now : nil, settled_at: nil, snoozed_until: nil,
                  sent_at: status == .running ? ago(minutes) : nil)
    }

    static let threads: [HubThread] = [
        thread("t1", "Machines page as cards", "Hub", "PC", .waiting_on_you, 2),
        thread("t2", "Weekly report as bars", "Forman", "tincan", .waiting_on_you, 6),
        thread("t3", "Desk cleanup", "Desk", "tincan", .error, 20),
        thread("t4", "Mac install fixes", "Hub", "MacBook", .running, 4),
        thread("t5", "Export to CSV", "Forman", "tincan", .running, 9),
        thread("t6", "Read the mandate inbox", "Knegarn", "PC", .running, 12),
        thread("t7", "Tool failures in Problems", "Hub", "PC", .idle, 40),
        thread("t8", "Chat signals said once", "Hub", "PC", .idle, 120, seen: true),
    ]

    static var folded: [String: Folded] {
        func f(_ items: [Item], ask: Ask? = nil, replies: [String] = [], question: String = "") -> Folded {
            var x = Folded()
            x.items = items
            x.openAsk = ask
            if !replies.isEmpty { x.replies = Peek.Replies(question: question, options: replies.map { Peek.Reply(label: $0, message: $0) }) }
            return x
        }
        let allow = Ask(id: "p1", title: "Run cargo test --workspace", command: "cargo test --workspace",
                        options: [.init(id: "once", name: "Allow", kind: "allow_once"),
                                  .init(id: "always", name: "Allow for thread", kind: "allow_always"),
                                  .init(id: "no", name: "Deny", kind: "reject_once")])
        var claude = Folded()
        claude.options = [
            EngineOption(id: "model", name: "Model", category: "model", isToggle: false, value: "sonnet",
                         choices: [.init(value: "opus", name: "Opus", description: "Hardest work, slower"),
                                   .init(value: "sonnet", name: "Sonnet", description: "Most work"),
                                   .init(value: "haiku", name: "Haiku", description: "Quick answers")]),
            EngineOption(id: "effort", name: "Effort", category: "thought_level", isToggle: false, value: "medium",
                         choices: [.init(value: "low", name: "Low", description: "Answers right away"),
                                   .init(value: "medium", name: "Medium", description: "Thinks a little first"),
                                   .init(value: "high", name: "High", description: "Thinks it through"),
                                   .init(value: "max", name: "Max", description: "For the hardest problems")]),
        ]
        var all: [String: Folded] = [
            "t1": f([.user("Make the machines page cards, with rings for CPU, RAM, disk and temperature"),
                     .reply("Moving on to the machines page. I'll run the whole test suite before I push."),
                     .work([Step(title: "Edited MachineCard.svelte and machineHealth.ts", status: "completed"),
                            Step(title: "Ran npm run check · 0 errors", status: "completed")]),
                     .ask(allow)], ask: allow),
            "t2": f([.user("The weekly report in Forman should show hours per project as bars, not a table"),
                     .reply("I'll swap the table in the weekly report for horizontal bars, one per project, longest first."),
                     .work([Step(title: "Read 4 files", status: "completed"),
                            Step(title: "Edited report.py and week.html", status: "completed"),
                            Step(title: "Ran pytest · 12 passed", status: "completed")]),
                     .reply("The bars work and the tests pass. One thing to settle: should the totals sit above the bars or in a row under them?"),
                     .files(["report.py", "week.html"])],
                    replies: ["Above", "Under"], question: "Should the totals sit above the bars or in a row under them?"),
            "t3": f([.user("Clean up the desk views and drop the old templates"),
                     .work([Step(title: "Ran pytest · 2 failed", status: "failed")]),
                     .error("pytest failed twice in test_views.py.")]),
            "t4": f([.user("Fix the Mac install"), .work([Step(title: "Editing install.sh", status: "in_progress")])]),
            "t7": f([.reply("PR #273 is open and every check is green.")]),
            "t8": f([.reply("Done. One signal per turn now."),
                    .brk(Brk(kind: "engine", text: "Claude ran out → continued on Codex", note: "14:05")),
                    .reply("Here is how the runs compare:\n\n| Run | Time | Cost |\n| --- | --- | --- |\n| Before | 4 min | 12 kr |\n| After | 2 min | 7 kr |\n\n- Fewer calls\n- Same result\n\n> One signal per turn is enough.")]),
        ]
        for (k, v) in all { var x = v; x.options = claude.options; all[k] = x }
        all["t4"]?.plan = [PlanStep(text: "Read install.sh and the Mac notes", status: "completed"),
                           PlanStep(text: "Make the Electron download retry", status: "completed"),
                           PlanStep(text: "Run the installer on the MacBook", status: "in_progress"),
                           PlanStep(text: "Say what to test by hand", status: "pending")]
        return all
    }

    static let changes: [ThreadChange] = [
        ThreadChange(path: "/home/c/dev/forman/report.py", diff: "@@ -84,3 +84,5 @@ def week_summary(rows):\n-    return table(rows, cols=HOURS)\n+    per = hours_by_project(rows)\n+    return bars(per, unit=\"h\",\n+                sort=\"desc\")", added: 38, removed: 21),
        ThreadChange(path: "/home/c/dev/forman/templates/week.html", diff: "@@ -12,4 +12,6 @@\n <section class=\"week\">\n-  {{ table }}\n+  {{ bars }}\n+  <p class=\"total\">{{ total }}</p>", added: 12, removed: 30),
    ]
}
