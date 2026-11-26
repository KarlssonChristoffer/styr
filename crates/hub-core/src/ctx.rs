//! `Ctx`: the borrowed view of the core that new code takes in place of `&Hub`. A function that only
//! reads or writes the database and reads the settings asks for `&Ctx` (`hub.ctx()`), so it can sit
//! below `hub` in the layers (`tests/it/layers.rs`) and be called without a running core.
use crate::config::Config;
use crate::store::Store;

/// The database and the settings, borrowed. Copy it freely; it owns nothing.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub store: &'a Store,
    pub config: &'a Config,
}

impl<'a> Ctx<'a> {
    pub fn new(store: &'a Store, config: &'a Config) -> Self {
        Self { store, config }
    }
}
