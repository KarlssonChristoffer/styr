//! The door for notices from outside Hub: a widget, a script, a server or a Windows tool posts one
//! to `POST /api/notices`, and it goes out on the stream (`StreamMsg::Notice`) to every window of
//! Hub, which decide what to do with it under Settings > Notifications. Nothing here shows
//! anything by itself, and nothing reaches the phone unless the sender asked for it and the
//! phone's own choices (topic, switches, quiet hours) allow it.
//!
//! The last few hundred are kept in memory for `GET /api/notices`. A sender that talks too much
//! is turned away (`TOO_MANY` a minute each), so a loop in somebody's script cannot flood anyone.

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};

use crate::hub::Hub;
use crate::model::{Notice, NewNotice, StreamMsg, now_ms};
use crate::ntfy::{self, Kind};

/// How many are kept.
const KEPT: usize = 300;
/// How many one source may send in a minute.
const TOO_MANY: usize = 20;
const SORTS: [&str; 4] = ["info", "waiting", "failed", "done"];

static LOG: LazyLock<Mutex<VecDeque<Notice>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));
static SENT: LazyLock<Mutex<HashMap<String, VecDeque<i64>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Why a notice was refused: the text goes back to the sender.
#[derive(Debug, PartialEq)]
pub enum Refused {
    Invalid(String),
    TooMany,
}

/// The notice as it will be kept, or why it is not allowed.
pub fn check(new: NewNotice) -> Result<NewNotice, Refused> {
    let invalid = |why: &str| Err(Refused::Invalid(why.into()));
    let source = new.source.trim().to_owned();
    let source = if source.is_empty() { "outside".to_owned() } else { source };
    if source.chars().count() > 60 {
        return invalid("source is longer than 60 characters");
    }
    let title = new.title.trim().to_owned();
    if title.is_empty() {
        return invalid("title is empty");
    }
    if title.chars().count() > 200 {
        return invalid("title is longer than 200 characters");
    }
    if new.body.chars().count() > 2000 {
        return invalid("body is longer than 2000 characters");
    }
    let sort = if new.sort.trim().is_empty() { "info".to_owned() } else { new.sort.trim().to_lowercase() };
    if !SORTS.contains(&sort.as_str()) {
        return invalid("sort is one of info, waiting, failed, done");
    }
    let link = new.link.map(|l| l.trim().to_owned()).filter(|l| !l.is_empty());
    if let Some(link) = &link
        && !["hub://", "https://", "http://"].iter().any(|p| link.starts_with(p))
    {
        return invalid("link starts with hub://, https:// or http://");
    }
    Ok(NewNotice { source, sort, title, body: new.body.trim().to_owned(), link, phone: new.phone })
}

/// Counts one more from `source` at `now_ms`; false when it already sent `TOO_MANY` in the last minute.
fn allowed(source: &str, now: i64) -> bool {
    let mut sent = SENT.lock().unwrap_or_else(|e| e.into_inner());
    let times = sent.entry(source.to_owned()).or_default();
    while times.front().is_some_and(|t| now - t > 60_000) {
        times.pop_front();
    }
    if times.len() >= TOO_MANY {
        return false;
    }
    times.push_back(now);
    // A source that said nothing for a long while is forgotten.
    sent.retain(|_, t| !t.is_empty());
    true
}

/// Which kind of phone notice a sort is. "info" is told like "done": it can wait for the morning.
fn phone_kind(sort: &str) -> Kind {
    match sort {
        "waiting" => Kind::Question,
        "failed" => Kind::Failed,
        _ => Kind::Done,
    }
}

/// The newest first.
pub fn recent() -> Vec<Notice> {
    LOG.lock().unwrap_or_else(|e| e.into_inner()).iter().rev().take(100).cloned().collect()
}

impl Hub {
    /// Takes a notice from outside in: keeps it, puts it on the stream, and sends it to the phone when
    /// asked and allowed.
    pub async fn notice(&self, new: NewNotice) -> Result<Notice, Refused> {
        let new = check(new)?;
        let now = now_ms();
        if !allowed(&new.source, now) {
            return Err(Refused::TooMany);
        }
        let mut notice = Notice {
            id: uuid::Uuid::now_v7().to_string(),
            at: now,
            source: new.source,
            sort: new.sort,
            title: new.title,
            body: new.body,
            link: new.link,
            phone: None,
        };
        if new.phone {
            notice.phone = Some(self.notice_to_phone(&notice).await);
        }
        {
            let mut log = LOG.lock().unwrap_or_else(|e| e.into_inner());
            log.push_back(notice.clone());
            while log.len() > KEPT {
                log.pop_front();
            }
        }
        self.publish(StreamMsg::Notice { notice: notice.clone() });
        Ok(notice)
    }

    async fn notice_to_phone(&self, notice: &Notice) -> String {
        let n = ntfy::load(&self.ctx()).await;
        let kind = phone_kind(&notice.sort);
        let hour = ntfy::phone_hour(notice.at, n.utc_offset_min);
        if n.topic.trim().is_empty() {
            return "held: the phone is not set up".into();
        }
        if !ntfy::wanted(&n, kind, hour) {
            return "held: the phone's choices or quiet hours".into();
        }
        let body = if notice.body.is_empty() { notice.source.clone() } else { format!("{} · {}", notice.source, notice.body) };
        match ntfy::send(&n, &notice.title, &body, notice.link.as_deref(), kind != Kind::Done).await {
            Ok(()) => "sent".into(),
            Err(err) => format!("failed: {err:#}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(title: &str) -> NewNotice {
        NewNotice { title: title.into(), ..Default::default() }
    }

    #[test]
    fn a_plain_title_is_enough_and_gets_the_defaults() {
        let n = check(new("  Backup done ")).unwrap();
        assert_eq!((n.source.as_str(), n.sort.as_str(), n.title.as_str()), ("outside", "info", "Backup done"));
        assert!(!n.phone, "the phone only gets what it is asked to");
    }

    #[test]
    fn what_is_wrong_is_said() {
        assert!(matches!(check(new("  ")), Err(Refused::Invalid(_))));
        assert!(matches!(check(NewNotice { sort: "loud".into(), ..new("x") }), Err(Refused::Invalid(_))));
        assert!(matches!(check(NewNotice { link: Some("file:///etc/passwd".into()), ..new("x") }), Err(Refused::Invalid(_))));
        assert!(matches!(check(NewNotice { link: Some("javascript:alert(1)".into()), ..new("x") }), Err(Refused::Invalid(_))));
        assert!(check(NewNotice { link: Some("hub://thread/abc".into()), sort: "Failed".into(), ..new("x") }).is_ok());
        assert!(matches!(check(NewNotice { body: "x".repeat(2001), ..new("x") }), Err(Refused::Invalid(_))));
    }

    #[test]
    fn a_source_that_talks_too_much_is_turned_away_for_a_minute() {
        let name = "test-chatty-source";
        for i in 0..TOO_MANY {
            assert!(allowed(name, 1_000 + i as i64), "the {i}th is allowed");
        }
        assert!(!allowed(name, 2_000));
        assert!(allowed("test-other-source", 2_000), "another source is not affected");
        assert!(allowed(name, 62_000), "a minute later it may speak again");
    }

    #[test]
    fn sorts_map_to_the_phones_kinds() {
        assert_eq!(phone_kind("waiting"), Kind::Question);
        assert_eq!(phone_kind("failed"), Kind::Failed);
        assert_eq!(phone_kind("info"), Kind::Done);
    }
}
