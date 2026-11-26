//! The inbox simulator (Labs): a handful of fake threads that start, work, ask for permission,
//! ask a question, fail and finish on timers, so the Activity list can be tried without an AI.
//! They run on the `sim` engine, which hubd provides itself (`hubd sim-agent`, the scripted agent
//! in `crates/mock-agent/src/sim.rs`). This names them; `hub/sim.rs` starts and removes them.

/// The engine the simulated threads run on.
pub const ENGINE: &str = "sim";

/// Title, what "you" wrote, the agent's scenario, and seconds after the start it is sent.
pub(crate) const THREADS: &[(&str, &str, &str, u64)] = &[
    ("Fix the flaky login test", "The login test fails every few runs on CI. Find out why and fix it.", "quick", 0),
    ("Dark mode toggle", "Add a dark mode toggle to Settings > General.", "ask", 3),
    ("Speed up search", "The search page is slow when typing. Make it faster.", "question", 6),
    ("Upgrade Tauri", "Upgrade Tauri to the latest version and make sure it still builds.", "fail", 9),
    ("Migrate the users table", "Move users to the new schema with separate profile rows.", "late_ask", 14),
    ("Docs for pairing", "Write docs for how pairing a second machine works.", "long", 20),
    ("Remove unused CSS", "Clean out CSS that nothing uses any more.", "quick", 40),
];
