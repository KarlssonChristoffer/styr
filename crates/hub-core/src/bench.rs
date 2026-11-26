//! The bench: a fixed suite of prompts run as real threads, checked with hard checks read from the
//! event log and the run folder, timed, and compared across Hub versions. This module is the
//! vocabulary only, pure code: the suite format (`suite`), the checks (`check`), the measurements
//! and validity (`measure`) and one case over its repeats with the regression flag (`summary`).
//! `run` runs a suite as real threads, `results` keeps and reports the runs and `catalog` lists
//! the suites with what earlier runs took.
//! Docs: docs/architecture/bench.md.

pub mod catalog;
pub mod check;
pub mod measure;
pub mod results;
pub mod run;
pub mod suite;
pub mod summary;

pub use check::{CheckResult, check};
pub use measure::{Measured, measure};
pub use suite::{Case, Check, ProofWant, Suite, builtin, builtin_names, lay_down, parse, suite_hash};
pub use summary::{Flag, Level, Range, Summary, flag, summarize};
