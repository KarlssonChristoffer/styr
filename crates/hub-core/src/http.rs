//! The one shared HTTP client for calls out of the core (API connections, balances), with Hub's
//! user agent. A plain helper: it knows nothing of Hub, so any module can use it.

use std::sync::LazyLock;

pub(crate) static CLIENT: LazyLock<reqwest::Client> =
    LazyLock::new(|| reqwest::Client::builder().user_agent(concat!("hub/", env!("CARGO_PKG_VERSION"))).build().unwrap_or_default());
