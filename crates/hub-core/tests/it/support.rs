//! What the integration tests share: the mock agent and hubd, found (built only when missing or
//! out of date) once for the whole program, and the two waits every engine test uses.
//!
//! Each file used to build the mock agent with its own `cargo build` behind its own `OnceLock`:
//! eight cargos at once, and every test that started first waited for them (nine tests showed
//! 20 s each on tincan for one build). Here the binary is only built when it is not there.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use hub_core::Hub;
use hub_core::model::{EventKind, ThreadStatus};

/// `<target>/debug`: this test program sits in `<target>/debug/deps`, wherever cargo was told to
/// build.
fn debug_dir() -> PathBuf {
    std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_path_buf()
}

/// Whether `bin` is missing or older than a file of `src` (a crate's `src` folder).
fn stale(bin: &Path, src: &Path) -> bool {
    let Ok(built) = std::fs::metadata(bin).and_then(|m| m.modified()) else { return true };
    let Ok(files) = std::fs::read_dir(src) else { return false };
    files.flatten().any(|f| f.metadata().and_then(|m| m.modified()).is_ok_and(|t| t > built))
}

/// Builds `package` unless its binary in `debug` is there and newer than `src`. A run of
/// `cargo test --workspace` has built every binary already.
fn binary(package: &str, src: &str) -> PathBuf {
    let bin = debug_dir().join(if cfg!(windows) { format!("{package}.exe") } else { package.to_owned() });
    if stale(&bin, &hub_core::source_tree::root().join("crates").join(src).join("src")) {
        // Windows can hold a freshly linked exe for a moment (antivirus), so cargo fails to replace it.
        let built = (0..3).any(|attempt| {
            if attempt > 0 {
                std::thread::sleep(Duration::from_secs(2));
            }
            std::process::Command::new(env!("CARGO")).args(["build", "-q", "-p", package]).status().expect("running cargo").success()
        });
        assert!(built, "building {package} failed");
    }
    bin
}

/// The scripted ACP agent (`crates/mock-agent`).
pub fn mock_agent() -> &'static PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| binary("mock-agent", "mock-agent"))
}

/// `hubd`, for keeping engines (`keeper.rs`).
pub fn hubd() -> &'static PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| binary("hubd", "hubd"))
}

/// Waits until the thread has this status (20 s).
pub async fn wait_status(hub: &Hub, id: &str, status: ThreadStatus) {
    for _ in 0..800 {
        if hub.store.thread(id).await.unwrap().unwrap().status == status {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("thread never became {status:?}");
}

/// Waits until a turn of the thread has ended (10 s).
pub async fn turn_ended(hub: &Hub, id: &str) {
    for _ in 0..400 {
        if hub.store.events(id, 0, 1000).await.unwrap().iter().any(|e| e.kind == EventKind::TurnEnd) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no turn ended");
}
