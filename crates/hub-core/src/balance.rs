//! What is left on a pay-per-token provider's account, for the usage strip at the foot of the
//! sidebar. A provider without a plan has no limits to show, so its balance stands in for them.
//! Asking costs nothing (it is the provider's account endpoint, not a model call) and is cached
//! for a few minutes. Only DeepSeek so far: https://api-docs.deepseek.com/api/get-user-balance

use crate::locked::LockExt;
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use crate::Hub;
use crate::usage::AccountUsage;

/// How long a balance is shown before asking again.
const KEEP_MS: i64 = 5 * 60 * 1000;

/// The last answer per engine: when, and what it said.
static LAST: LazyLock<Mutex<Vec<(String, i64, AccountUsage)>>> = LazyLock::new(Default::default);

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    balance_infos: Vec<Info>,
}

#[derive(Deserialize)]
struct Info {
    currency: String,
    total_balance: String,
}

/// "$4.20" or "¥110.00": the first currency the account holds, and the amount in it.
fn shown(answer: &Answer) -> Option<(String, f64)> {
    let i = answer.balance_infos.first()?;
    let n: f64 = i.total_balance.parse().ok()?;
    Some((money(&i.currency, n), n))
}

/// An amount the way the account's currency writes it.
fn money(currency: &str, n: f64) -> String {
    match currency {
        "USD" => format!("${n:.2}"),
        "CNY" => format!("¥{n:.2}"),
        other => format!("{n:.2} {other}"),
    }
}

/// What a day has taken off the balance so far, kept between readings in the store under
/// `balance.spend.<engine>`: the day (the app's time zone), the balance last seen, the sum of its
/// falls since the day began. A rise is a top-up and is not spend. The provider has no call for
/// what was spent, so this is what the balance fell by while Hub was asking, and a gap while Hub
/// was shut counts toward the day it is seen on. `days` keeps each day's sum for the last 90 days,
/// for Settings > Where it goes; a ledger from before it starts with the day it was on.
#[derive(Serialize, Deserialize, Default, PartialEq, Debug)]
struct Spend {
    day: String,
    last: f64,
    spent: f64,
    #[serde(default)]
    days: std::collections::BTreeMap<String, f64>,
}

/// Days kept in a ledger's `days`.
const KEEP_DAYS: usize = 90;

/// The ledger after a reading of `now` on `day`.
fn spend_after(before: Option<Spend>, day: &str, now: f64) -> Spend {
    let Some(mut b) = before else {
        return Spend { day: day.into(), last: now, spent: 0.0, days: [(day.to_owned(), 0.0)].into() };
    };
    let fell = (b.last - now).max(0.0);
    let spent = if b.day == day { b.spent + fell } else { fell };
    // A ledger from before `days` still knows its own day.
    b.days.entry(b.day.clone()).or_insert(b.spent);
    b.days.insert(day.to_owned(), spent);
    while b.days.len() > KEEP_DAYS {
        b.days.pop_first();
    }
    Spend { day: day.into(), last: now, spent, days: b.days }
}

/// What every pay-per-token account's balance fell by, per day (YYYY-MM-DD in the app's time
/// zone), with the balances read again first unless they were read a moment ago. Several engines
/// on one provider share its balance, so a day takes the largest of their ledgers, not the sum.
pub async fn spent_by_day(hub: &Hub) -> std::collections::BTreeMap<String, f64> {
    all(hub).await;
    let mut out = std::collections::BTreeMap::<String, f64>::new();
    for (name, engine) in &hub.config.engines {
        if engine.provider.as_deref() != Some("deepseek") {
            continue;
        }
        let Some(ledger) = hub.store.get_value(&format!("balance.spend.{name}")).await.ok().flatten().and_then(|j| serde_json::from_str::<Spend>(&j).ok()) else {
            continue;
        };
        let mut days = ledger.days;
        days.entry(ledger.day).or_insert(ledger.spent);
        for (day, spent) in days {
            let at = out.entry(day).or_default();
            *at = at.max(spent);
        }
    }
    out
}

/// Every engine on a provider whose balance Hub can read, once per provider.
pub async fn all(hub: &Hub) -> Vec<AccountUsage> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (name, engine) in &hub.config.engines {
        let Some(provider) = engine.provider.as_deref().filter(|p| *p == "deepseek") else { continue };
        if !seen.insert(provider.to_string()) {
            continue;
        }
        out.push(one(hub, name, engine).await);
    }
    out
}

async fn one(hub: &Hub, name: &str, engine: &crate::config::EngineConfig) -> AccountUsage {
    let now = crate::model::now_ms();
    if let Some((_, _, kept)) = LAST.locked().iter().find(|(e, at, _)| e == name && now - at < KEEP_MS) {
        return kept.clone();
    }
    let read = async {
        let env = hub.secrets.engine_env(engine).await?;
        let key = env.iter().find(|(k, _)| k == "DEEPSEEK_API_KEY").map(|(_, v)| v.clone()).context("no DeepSeek key in the vault")?;
        let res = crate::http::CLIENT
            .get("https://api.deepseek.com/user/balance")
            .bearer_auth(key)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?;
        if !res.status().is_success() {
            bail!("DeepSeek answered {}", res.status());
        }
        let answer: Answer = res.json().await?;
        let currency = answer.balance_infos.first().map(|i| i.currency.clone()).unwrap_or_default();
        shown(&answer).map(|(text, n)| (text, n, currency)).context("DeepSeek listed no balance")
    };
    let (balance, spent_today, note) = match read.await {
        Ok((text, n, currency)) => {
            let spent = spent_today(hub, name, &currency, n, now).await;
            (Some(text), spent, None)
        }
        Err(e) => (None, None, Some(format!("Balance not read: {e}"))),
    };
    let usage = AccountUsage { engine: name.to_string(), label: "DeepSeek".into(), plan: None, note, balance, spent_today };
    let mut last = LAST.locked();
    last.retain(|(e, _, _)| e != name);
    last.push((name.to_string(), now, usage.clone()));
    usage
}

/// Writes the reading into the day's ledger and says what it has spent so far today.
async fn spent_today(hub: &Hub, engine: &str, currency: &str, now_balance: f64, now: i64) -> Option<String> {
    let tz: i32 = hub.store.get_value(crate::devlog::TZ).await.ok()??.parse().unwrap_or(0);
    let day = crate::activity::date_of(now, tz);
    let key = format!("balance.spend.{engine}");
    let before = hub.store.get_value(&key).await.ok()?.and_then(|j| serde_json::from_str::<Spend>(&j).ok());
    let after = spend_after(before, &day, now_balance);
    hub.store.set_value(&key, &serde_json::to_string(&after).ok()?).await.ok()?;
    Some(money(currency, after.spent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_balance_reads_in_its_currency() {
        let a: Answer = serde_json::from_str(r#"{"is_available":true,"balance_infos":[{"currency":"USD","total_balance":"4.2","granted_balance":"0","topped_up_balance":"4.2"}]}"#).unwrap();
        assert_eq!(shown(&a), Some(("$4.20".into(), 4.2)));
        let a: Answer = serde_json::from_str(r#"{"balance_infos":[{"currency":"CNY","total_balance":"110.00"}]}"#).unwrap();
        assert_eq!(shown(&a), Some(("¥110.00".into(), 110.0)));
        let a: Answer = serde_json::from_str(r#"{"balance_infos":[]}"#).unwrap();
        assert_eq!(shown(&a), None);
    }

    #[test]
    fn a_day_adds_up_what_the_balance_fell_by() {
        let one = spend_after(None, "2026-11-01", 5.60);
        assert_eq!(one.spent, 0.0);
        let two = spend_after(Some(one), "2026-11-01", 5.18);
        assert!((two.spent - 0.42).abs() < 1e-9);
        // A top-up is not spend and not a negative.
        let three = spend_after(Some(two), "2026-11-01", 15.18);
        assert!((three.spent - 0.42).abs() < 1e-9);
        let four = spend_after(Some(three), "2026-11-01", 15.00);
        assert!((four.spent - 0.60).abs() < 1e-9);
        // A new day starts again from what fell since the last look.
        let next = spend_after(Some(four), "2026-11-02", 14.50);
        assert!((next.spent - 0.50).abs() < 1e-9);
        // Each day keeps its own sum.
        assert!((next.days["2026-11-01"] - 0.60).abs() < 1e-9);
        assert!((next.days["2026-11-02"] - 0.50).abs() < 1e-9);
    }

    #[test]
    fn a_ledger_from_before_days_keeps_its_day() {
        let old: Spend = serde_json::from_str(r#"{"day":"2026-11-03","last":5.16,"spent":0.4}"#).unwrap();
        let next = spend_after(Some(old), "2026-11-03", 5.06);
        assert!((next.days["2026-11-03"] - 0.5).abs() < 1e-9);
        assert_eq!(next.days.len(), 1);
    }
}
