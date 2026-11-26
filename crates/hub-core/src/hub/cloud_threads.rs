//! Hub's side of cloud threads (`cloud_threads.rs`): the store, card, status and events a cloud
//! thread reads and writes, through `cloud_threads::Threads`, so that module never names Hub.

use anyhow::Result;
use futures::future::BoxFuture;
use serde_json::Value;

use crate::cloud_threads::Threads;
use crate::hub::Hub;
use crate::model::{Event, EventKind, Thread, ThreadStatus};
use crate::store::Store;

impl Threads for Hub {
    fn store(&self) -> &Store {
        &self.store
    }

    fn edit_thread<'a>(&'a self, thread_id: &'a str, f: Box<dyn FnOnce(&mut Thread) + Send>) -> BoxFuture<'a, Result<Thread>> {
        Box::pin(Hub::update_thread(self, thread_id, f))
    }

    fn set_status<'a>(&'a self, thread_id: &'a str, status: ThreadStatus) -> BoxFuture<'a, Result<Thread>> {
        Box::pin(Hub::set_status(self, thread_id, status))
    }

    fn set_status_because<'a>(&'a self, thread_id: &'a str, status: ThreadStatus, why: &'a str) -> BoxFuture<'a, Result<Thread>> {
        Box::pin(Hub::set_status_because(self, thread_id, status, why))
    }

    fn emit<'a>(&'a self, thread_id: &'a str, kind: EventKind, data: Value) -> BoxFuture<'a, Result<Event>> {
        Box::pin(Hub::emit(self, thread_id, kind, data))
    }
}
