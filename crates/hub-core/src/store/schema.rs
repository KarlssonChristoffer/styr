//! The database schema: every migration step in order (one per version), the version this build
//! knows, and the SQL of the first version and of code analysis.

use super::*;

/// The schema, one step per version. `PRAGMA user_version` says how many steps a database has
/// taken. Steps are only ever added: an old step is never changed once it shipped.
pub(super) fn steps() -> Vec<M<'static>> {
    vec![
        // 1: the tables as they were before versions were counted. Databases from then may lack
        // the columns added since, so the hook adds what is missing.
        M::up_with_hook(V1, |tx: &rusqlite::Transaction| {
            add_missing_columns(tx)?;
            Ok(())
        }),
        // 2: requests already carried out, by the id the client gave them, so a retry does not
        // send a message twice.
        M::up(
            "CREATE TABLE requests (
                client_id TEXT PRIMARY KEY,
                result TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );",
        ),
        // 3: prompts and tokens copied from Claude Code's session files for the activity map,
        // because Claude Code deletes them after 30 days.
        M::up(
            "CREATE TABLE IF NOT EXISTS cc_marks (
                key TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                ts INTEGER NOT NULL,
                prompt TEXT,
                tokens INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS cc_marks_ts ON cc_marks (ts);
            CREATE TABLE IF NOT EXISTS cc_sessions (
                id TEXT PRIMARY KEY,
                cwd TEXT,
                title TEXT
            );",
        ),
        // 4: which model answered and its tokens by kind, for the stats. Claude Code's files are
        // read again from the start so answers already copied get them too.
        M::up(
            "ALTER TABLE cc_marks ADD COLUMN model TEXT;
            ALTER TABLE cc_marks ADD COLUMN input INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE cc_marks ADD COLUMN output INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE cc_marks ADD COLUMN cache_write INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE cc_marks ADD COLUMN cache_read INTEGER NOT NULL DEFAULT 0;
            DELETE FROM kv WHERE key LIKE 'cc.offset.%';",
        ),
        // 5: the activity list: when a thread came back to the user, when they last saw it, and
        // when they marked it done.
        M::up(
            "ALTER TABLE threads ADD COLUMN returned_at INTEGER;
            ALTER TABLE threads ADD COLUMN seen_at INTEGER;
            ALTER TABLE threads ADD COLUMN settled_at INTEGER;",
        ),
        // 6: when the user last sent a thread work, which orders the working list.
        M::up("ALTER TABLE threads ADD COLUMN sent_at INTEGER;"),
        // 7: parking in the activity list: pins, snoozes, and whether Hub may settle a thread
        // by itself.
        M::up(
            "ALTER TABLE threads ADD COLUMN pinned_at INTEGER;
            ALTER TABLE threads ADD COLUMN snoozed_until INTEGER;
            ALTER TABLE threads ADD COLUMN auto_settle INTEGER NOT NULL DEFAULT 1;",
        ),
        // 8: side jobs: the thread that started a thread to do one thing for it.
        M::up("ALTER TABLE threads ADD COLUMN parent_id TEXT;"),
        // 9: where the last turn left the work, as read by a decision model (`reading.rs`).
        M::up("ALTER TABLE threads ADD COLUMN reading TEXT;"),
        // 10: problems (`problems.rs`): errors and warnings grouped by fingerprint, a few hits of
        // each kept with their stack, and a count per day.
        M::up(
            "CREATE TABLE problems (
                id INTEGER PRIMARY KEY,
                fingerprint TEXT NOT NULL UNIQUE,
                project TEXT NOT NULL,
                source TEXT NOT NULL,
                level TEXT NOT NULL,
                level_rank INTEGER NOT NULL,
                title TEXT NOT NULL,
                culprit TEXT,
                folder TEXT,
                machine TEXT NOT NULL,
                first_seen INTEGER NOT NULL,
                last_seen INTEGER NOT NULL,
                count INTEGER NOT NULL,
                status TEXT NOT NULL,
                regressed INTEGER NOT NULL DEFAULT 0,
                thread_id TEXT
            );
            CREATE INDEX problems_by_seen ON problems(status, last_seen);
            CREATE TABLE problem_hits (problem_id INTEGER NOT NULL, ts INTEGER NOT NULL, data TEXT NOT NULL);
            CREATE INDEX problem_hits_by ON problem_hits(problem_id, ts);
            CREATE TABLE problem_days (
                problem_id INTEGER NOT NULL,
                day INTEGER NOT NULL,
                count INTEGER NOT NULL,
                PRIMARY KEY (problem_id, day)
            ) WITHOUT ROWID;",
        ),
        // 11: problems v2 (`problems/`): stamps (prio, lane, category), Vakten's note, archives
        // with a condition, resolving for the next release, merging, and what happened to each.
        // "muted" problems become archived for good.
        M::up(
            "ALTER TABLE problems ADD COLUMN prio TEXT;
            ALTER TABLE problems ADD COLUMN lane TEXT;
            ALTER TABLE problems ADD COLUMN category TEXT;
            ALTER TABLE problems ADD COLUMN confidence TEXT;
            ALTER TABLE problems ADD COLUMN fixable TEXT;
            ALTER TABLE problems ADD COLUMN summary TEXT;
            ALTER TABLE problems ADD COLUMN stamped_by TEXT;
            ALTER TABLE problems ADD COLUMN archive TEXT;
            ALTER TABLE problems ADD COLUMN resolved_at INTEGER;
            ALTER TABLE problems ADD COLUMN resolved_by TEXT;
            ALTER TABLE problems ADD COLUMN resolved_releases TEXT;
            ALTER TABLE problems ADD COLUMN pr TEXT;
            ALTER TABLE problems ADD COLUMN merged_into INTEGER;
            ALTER TABLE problems ADD COLUMN escalating INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE problems ADD COLUMN reviewed INTEGER NOT NULL DEFAULT 0;
            UPDATE problems SET status = 'archived', archive = '{\"until\":\"forever\"}' WHERE status = 'muted';
            UPDATE problems SET resolved_at = last_seen WHERE status = 'resolved';
            CREATE TABLE problem_activity (
                problem_id INTEGER NOT NULL,
                ts INTEGER NOT NULL,
                by TEXT NOT NULL,
                kind TEXT NOT NULL,
                text TEXT NOT NULL
            );
            CREATE INDEX problem_activity_by ON problem_activity(problem_id, ts);
            CREATE TABLE problem_aliases (fingerprint TEXT PRIMARY KEY, problem_id INTEGER NOT NULL) WITHOUT ROWID;
            CREATE TABLE problem_metrics (
                name TEXT NOT NULL,
                release TEXT NOT NULL,
                machine TEXT NOT NULL,
                ts INTEGER NOT NULL,
                value REAL NOT NULL
            );
            CREATE INDEX problem_metrics_by ON problem_metrics(name, ts);",
        ),
        // 12: agents' tool calls (`problems/tools.rs`), counted per day, engine and tool, with how
        // many failed.
        M::up(
            "CREATE TABLE tool_calls (
                day INTEGER NOT NULL,
                engine TEXT NOT NULL,
                tool TEXT NOT NULL,
                calls INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (day, engine, tool)
            ) WITHOUT ROWID;",
        ),
        // 13: what failed tool calls said, per day, engine and tool (`problems/tools.rs`), so the
        // Tool calls tab can show the most common errors, also of calls from before step 12.
        M::up(
            "CREATE TABLE tool_errors (
                day INTEGER NOT NULL,
                engine TEXT NOT NULL,
                tool TEXT NOT NULL,
                gist TEXT NOT NULL,
                count INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (day, engine, tool, gist)
            ) WITHOUT ROWID;",
        ),
        // 14: who in Hub started a thread on its own (`system.rs`), as JSON `{ kind, name, id? }`.
        M::up("ALTER TABLE threads ADD COLUMN owner TEXT;"),
        // 15: what happens at Customs (`customs_log.rs`), so its page can show it. Append-only.
        M::up(
            "CREATE TABLE customs_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                what TEXT NOT NULL,
                kind TEXT NOT NULL DEFAULT '',
                name TEXT NOT NULL DEFAULT '',
                by TEXT NOT NULL DEFAULT '',
                detail TEXT NOT NULL DEFAULT '',
                thread_id TEXT,
                took_ms INTEGER
            );
            CREATE INDEX customs_log_ts ON customs_log(ts);
            CREATE INDEX customs_log_item ON customs_log(kind, name, ts);",
        ),
        // 16: what was sent to an AI (`package/ledger.rs`): one row per package, with the size of
        // each part as JSON. No text: it is rebuilt from the thread's log.
        M::up(
            "CREATE TABLE slips (
                id TEXT PRIMARY KEY,
                at INTEGER NOT NULL,
                purpose TEXT NOT NULL,
                thread_id TEXT,
                seq INTEGER,
                engine TEXT NOT NULL,
                model TEXT,
                tokens INTEGER NOT NULL,
                estimated INTEGER NOT NULL DEFAULT 1,
                attachments INTEGER NOT NULL DEFAULT 0,
                recipe TEXT NOT NULL DEFAULT 'legacy',
                fallback INTEGER NOT NULL DEFAULT 0,
                head TEXT NOT NULL DEFAULT '',
                sections TEXT NOT NULL DEFAULT '[]',
                session TEXT
            ) WITHOUT ROWID;
            CREATE INDEX slips_at ON slips(at);
            CREATE INDEX slips_thread ON slips(thread_id, at);
            CREATE INDEX slips_session ON slips(session);",
        ),
        // 17: Claude Code sessions in Anthropic's cloud (`cloud_usage.rs`): each one's running
        // total when last looked at, and what grew, by moment.
        M::up(
            "CREATE TABLE cloud_sessions (
                id TEXT PRIMARY KEY,
                updated_at INTEGER NOT NULL,
                input INTEGER NOT NULL,
                output INTEGER NOT NULL,
                cache_write INTEGER NOT NULL,
                cache_read INTEGER NOT NULL,
                cost REAL NOT NULL
            ) WITHOUT ROWID;
            CREATE TABLE cloud_marks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                ts INTEGER NOT NULL,
                input INTEGER NOT NULL,
                output INTEGER NOT NULL,
                cache_write INTEGER NOT NULL,
                cache_read INTEGER NOT NULL,
                cost REAL NOT NULL
            );
            CREATE INDEX cloud_marks_ts ON cloud_marks(ts);",
        ),
        // 18: Claude's weekly plan meter as it was read (`plan_meter.rs`).
        M::up(
            "CREATE TABLE plan_samples (
                ts INTEGER NOT NULL,
                percent REAL NOT NULL,
                window TEXT NOT NULL
            );
            CREATE INDEX plan_samples_ts ON plan_samples(ts);",
        ),
        // 19: threads that run in the cloud (`cloud_threads.rs`): the run as JSON.
        M::up("ALTER TABLE threads ADD COLUMN cloud TEXT;"),
        // 20: which parts of Hub you use (`uselog.rs`): one row per use, the feature's name, the
        // area it sits in and the feature used just before. Names only, never anything typed.
        M::up(
            "CREATE TABLE use_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                feature TEXT NOT NULL,
                area TEXT NOT NULL,
                prev TEXT NOT NULL DEFAULT '',
                took_ms INTEGER
            );
            CREATE INDEX use_log_ts ON use_log(ts);
            CREATE INDEX use_log_feature ON use_log(feature, ts);",
        ),
        // 21: immutable code-analysis runs, their observations, and current findings in Problems.
        M::up(ANALYSIS_SCHEMA),
    ]
}

/// The highest schema version this build knows.
pub fn schema_version() -> usize {
    steps().len()
}

pub(super) const V1: &str = "
CREATE TABLE IF NOT EXISTS threads (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    cwd TEXT NOT NULL,
    engine TEXT NOT NULL,
    status TEXT NOT NULL,
    engine_session_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    title_locked INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS events (
    thread_id TEXT NOT NULL REFERENCES threads(id),
    seq INTEGER NOT NULL,
    ts INTEGER NOT NULL,
    kind TEXT NOT NULL,
    data TEXT NOT NULL,
    PRIMARY KEY (thread_id, seq)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS kv (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS projects (
    path TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
";

/// Step 21: the code-analysis tables of Problems.
const ANALYSIS_SCHEMA: &str = "
CREATE TABLE analysis_runs (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 stream TEXT NOT NULL, profile TEXT NOT NULL, data TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'running', started_at INTEGER NOT NULL,
 finished_at INTEGER, diagnostic TEXT
);
CREATE INDEX analysis_runs_stream ON analysis_runs(stream, id);
CREATE TABLE analysis_findings (
 problem_id INTEGER PRIMARY KEY REFERENCES problems(id),
 stream TEXT NOT NULL, identity TEXT NOT NULL, profile TEXT NOT NULL,
 first_run INTEGER NOT NULL REFERENCES analysis_runs(id),
 last_run INTEGER NOT NULL REFERENCES analysis_runs(id),
 data TEXT NOT NULL, UNIQUE(stream, identity)
);
CREATE TABLE analysis_observations (
 run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
 identity TEXT NOT NULL, data TEXT NOT NULL,
 PRIMARY KEY(run_id, identity)
);";
