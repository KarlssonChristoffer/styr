//! The words `hubd` takes after its name, and the markers in its binary that say it has them.
//! `hubd`'s `main` runs them (`signin.rs`, `terminal.rs`); `machine.rs` writes them into the
//! commands it opens a window with. Plain constants, so both sides use them without knowing each
//! other.

/// `hubd sign-in <engine>`.
pub const SIGN_IN_ARG: &str = "sign-in";
/// `hubd cli <engine>`: the engine's own CLI, signed in the way Hub's threads are.
pub const CLI_ARG: &str = "cli";
/// In hubd's binary, so a shell on the machine can tell that its hubd has `cli` before using it.
pub const CLI_MARKER: &str = "hubd-cli-signed-in-v1";
/// `hubd root`: a root shell (through sudo) that has Hub's sign-ins in its environment.
pub const ROOT_ARG: &str = "root";
/// In hubd's binary, so a shell on the machine can tell that its hubd has `root` before using it.
pub const ROOT_MARKER: &str = "hubd-root-signed-in-v1";
/// The `hubd` subcommand that follows a thread and takes it over.
pub const ATTACH_ARG: &str = "attach";
/// The `hubd` subcommand that lists threads with the short ids `attach` takes.
pub const THREADS_ARG: &str = "threads";
