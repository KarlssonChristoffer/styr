# {{name}}

A command line tool in Rust (clap). Started from Hub's "Rust tool" template.

## Build, run and test
- Run: `cargo run -- --name Ada`
- Test: `cargo test`
- Lint: `cargo clippy --all-targets` and `cargo fmt`
- Release build: `cargo build --release`

## Layout
- `src/main.rs`: argument parsing (`Args`) and the logic (`greeting` to begin with) with its unit tests.
  Move logic into `src/lib.rs` and more modules as it grows, so tests and the binary share it.

## Rules
- Run `cargo test` and `cargo clippy` before calling work done.
- Write a test with every new behaviour.
- Commit `Cargo.lock` (this is a binary); add crates with `cargo add`.
