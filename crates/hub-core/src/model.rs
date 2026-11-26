//! Types shared between the core and every surface. `cargo test -p hub-core export_bindings`
//! writes them as TypeScript to `app/src/lib/bindings`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub type ThreadId = String;

/// `#[serde(default = "yes")]` in every part of the model.
fn yes() -> bool {
    true
}

mod update_status;
pub use update_status::*;
mod bench;
pub use bench::*;

mod thread;
pub use thread::*;
mod event_data;
pub use event_data::*;
mod hosts;
pub use hosts::*;
mod projects;
pub use projects::*;
mod threads_io;
pub use threads_io::*;
mod machines;
pub use machines::*;
pub mod problems;
pub mod rows;
mod tools;
pub use tools::*;
mod system;
pub use system::*;
mod usage;
pub use usage::*;
mod settings;
pub use settings::*;
mod customs;
pub use customs::*;
mod outputs;
pub use outputs::*;
mod github;
pub use github::*;
mod routines;
pub use routines::*;
mod plugins;
pub use plugins::*;
mod timing;
pub use timing::*;
mod misc;
pub use misc::*;
