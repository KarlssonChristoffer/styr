//! The context bag: files, pictures, links and snippets kept apart from any thread, so they can
//! be taken along into whichever thread needs them, in any project. Each item is kept under
//! `bag:<id>` with its content; the list leaves the content out and the app fetches one item
//! when it is used. Ids are time ordered, so the newest sorts last.

use std::sync::LazyLock;

use anyhow::{Result, bail};
use tokio::sync::Mutex;

use crate::hub::Hub;
use crate::model::{Attachment, BagAdd, BagItem, now_ms};

const PREFIX: &str = "bag:";
/// The same limit as a message's attachments; more would not go along anyway.
pub const MAX_BYTES: i64 = 20 * 1024 * 1024;
/// A bag, not a drawer: past this, the oldest are for clearing out.
pub const MAX_ITEMS: usize = 100;
/// A list picture bigger than this is left out rather than sent with every list.
const MAX_THUMB: usize = 64 * 1024;

/// One writer at a time, so two adds of the same thing make one item.
static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Newest first, without content.
pub async fn list(hub: &Hub) -> Result<Vec<BagItem>> {
    Ok(all(hub).await?.into_iter().rev().map(light).collect())
}

pub async fn get(hub: &Hub, id: &str) -> Result<Option<BagItem>> {
    Ok(match hub.store.get_value(&key(id)?).await? {
        Some(json) => Some(serde_json::from_str(&json)?),
        None => None,
    })
}

/// Puts `add` in the bag, or gives back the item already holding the same thing.
pub async fn add(hub: &Hub, add: BagAdd) -> Result<BagItem> {
    let BagAdd { mut item, thumb, from } = add;
    item.name = item.name.trim().to_owned();
    if let Some(url) = item.url.take() {
        let url = url.trim();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            bail!("only http and https links go in the bag");
        }
        item.url = Some(url.to_owned());
        if item.name.is_empty() {
            item.name = url.to_owned();
        }
    }
    let set = [item.data.is_some(), item.text.is_some(), item.path.is_some(), item.url.is_some()];
    if set.iter().filter(|s| **s).count() != 1 {
        bail!("an item holds exactly one of data, text, path or url");
    }
    if item.name.is_empty() {
        bail!("an item needs a name");
    }
    let size = size_of(&item);
    if size > MAX_BYTES {
        bail!("{} is larger than {} MB", item.name, MAX_BYTES / 1024 / 1024);
    }
    let _guard = LOCK.lock().await;
    let items = all(hub).await?;
    if let Some(same) = items.iter().find(|b| same(&b.item, &item)) {
        return Ok(light(same.clone()));
    }
    if items.len() >= MAX_ITEMS {
        bail!("the bag holds {MAX_ITEMS} things; take some out first");
    }
    let bag = BagItem {
        id: uuid::Uuid::now_v7().to_string(),
        item,
        thumb: thumb.filter(|t| t.starts_with("data:image/") && t.len() <= MAX_THUMB),
        size,
        from: from.map(|f| f.trim().to_owned()).filter(|f| !f.is_empty()),
        added_at: now_ms(),
    };
    hub.store.set_value(&key(&bag.id)?, &serde_json::to_string(&bag)?).await?;
    Ok(light(bag))
}

pub async fn remove(hub: &Hub, id: &str) -> Result<()> {
    hub.store.take_value(&key(id)?).await?;
    Ok(())
}

/// A picture on the web, fetched so it goes in the bag as the picture itself rather than a link.
/// A browser hands over only the address when one is dragged out of a page; the app cannot fetch
/// it across origins, the core can. Anything that is not a picture is refused.
pub async fn picture(url: &str) -> Result<Attachment> {
    use base64::Engine as _;
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        bail!("only http and https pictures can be fetched");
    }
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .user_agent(concat!("hub/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .unwrap_or_default()
    });
    let mut res = CLIENT.get(url).send().await?.error_for_status()?;
    let mime = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or(v).trim().to_lowercase())
        .unwrap_or_default();
    if !mime.starts_with("image/") {
        bail!("{url} is not a picture");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = res.chunk().await? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() as i64 > MAX_BYTES {
            bail!("the picture is larger than {} MB", MAX_BYTES / 1024 / 1024);
        }
    }
    let ext = mime.trim_start_matches("image/").split('+').next().unwrap_or("png").replace("jpeg", "jpg");
    let stem = url
        .split(['?', '#'])
        .next()
        .and_then(|u| u.rsplit('/').next())
        .filter(|n| !n.is_empty())
        .map(|n| n.rsplit_once('.').map_or(n, |(a, _)| a).chars().take(60).collect::<String>())
        .unwrap_or_else(|| "picture".into());
    Ok(Attachment {
        name: format!("{stem}.{ext}"),
        mime,
        data: Some(base64::engine::general_purpose::STANDARD.encode(&bytes)),
        text: None,
        path: None,
        url: None,
    })
}

/// Empties the bag; says how many went.
pub async fn clear(hub: &Hub) -> Result<usize> {
    let _guard = LOCK.lock().await;
    let items = all(hub).await?;
    for b in &items {
        hub.store.take_value(&key(&b.id)?).await?;
    }
    Ok(items.len())
}

/// Oldest first.
async fn all(hub: &Hub) -> Result<Vec<BagItem>> {
    let mut items: Vec<BagItem> = hub
        .store
        .values_with_prefix(PREFIX)
        .await?
        .into_iter()
        .filter_map(|(_, json)| serde_json::from_str(&json).ok())
        .collect();
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(items)
}

fn key(id: &str) -> Result<String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        bail!("no such item");
    }
    Ok(format!("{PREFIX}{id}"))
}

fn light(mut b: BagItem) -> BagItem {
    b.item.data = None;
    b.item.text = None;
    b
}

fn size_of(a: &Attachment) -> i64 {
    match (&a.data, &a.text) {
        (Some(data), _) => (data.len() as i64) * 3 / 4,
        (_, Some(text)) => text.len() as i64,
        _ => 0,
    }
}

fn same(a: &Attachment, b: &Attachment) -> bool {
    match (&a.url, &a.path) {
        (Some(url), _) => b.url.as_ref() == Some(url),
        (_, Some(path)) => b.path.as_ref() == Some(path),
        _ => a.name == b.name && a.data == b.data && a.text == b.text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, text: &str) -> Attachment {
        Attachment { name: name.into(), mime: "text/plain".into(), data: None, text: Some(text.into()), path: None, url: None }
    }

    #[test]
    fn the_same_thing_is_the_same_item() {
        assert!(same(&file("a.txt", "hi"), &file("a.txt", "hi")));
        assert!(!same(&file("a.txt", "hi"), &file("a.txt", "ho")));
        let link = |u: &str| Attachment { url: Some(u.into()), text: None, ..file("x", "") };
        assert!(same(&link("https://a"), &link("https://a")));
        assert!(!same(&link("https://a"), &file("https://a", "")));
    }

    #[test]
    fn ids_cannot_reach_other_keys() {
        assert!(key("0190-abc").is_ok());
        assert!(key("").is_err());
        assert!(key("x:routines").is_err());
        assert!(key("../a").is_err());
    }
}
