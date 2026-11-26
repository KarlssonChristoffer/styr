//! All of hub-core's integration tests in ONE test program.
//!
//! Every `tests/*.rs` file is its own program, and each one links the whole core library, which
//! cost the update build about twenty links and, on Windows, a page file each. Add a test file here
//! as a module (and a `mod` line below), never as `tests/<name>.rs`. Run one of them with
//! `cargo test -p hub-core --test it <module>::` (e.g. `compat::`, `no_window::`).

mod agents_mirror;
mod api_routes;
mod bag;
mod bench;
mod bench_api;
mod bench_run;
mod clippy;
mod cloud;
mod compat;
mod customs;
mod desk;
mod engine;
mod golden_log;
mod golden_reads;
mod handoff;
mod hosts;
mod hub_items;
mod layers;
mod hub_tools;
mod ios_contract;
mod map;
mod no_window;
mod notices;
mod now;
mod outputs;
mod overview;
mod problems;
mod pty;
mod analysis;
mod project_parts;
mod ratchet;
mod status;
mod reactions;
mod replay;
mod rules;
mod sim;
mod support;
mod system;
mod test_code;
mod tools;
mod vakten_guard;
mod wire;
mod workers;
mod worker_runs;
