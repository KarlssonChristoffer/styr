//! The context bag keeps files, pictures and links apart from threads: the list leaves the
//! content out, one item has it all, the same thing is kept once, and odd links are refused.

use std::sync::Arc;

use hub_core::model::{Attachment, BagAdd};
use hub_core::{Config, Hub, Store};

fn hub() -> Arc<Hub> {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    Hub::new(Store::in_memory().unwrap(), config)
}

fn add(item: Attachment) -> BagAdd {
    BagAdd { item, thumb: None, from: Some("A thread".into()) }
}

fn text(name: &str, text: &str) -> Attachment {
    Attachment { name: name.into(), mime: "text/plain".into(), data: None, text: Some(text.into()), path: None, url: None }
}

fn link(url: &str) -> Attachment {
    Attachment { name: String::new(), mime: "text/uri-list".into(), data: None, text: None, path: None, url: Some(url.into()) }
}

#[tokio::test]
async fn keeps_things_apart_from_threads() {
    let hub = hub();
    let notes = hub_core::bag::add(&hub, add(text("notes.md", "# Plan"))).await.unwrap();
    assert_eq!(notes.size, 6);
    assert!(notes.item.text.is_none(), "the answer to an add is light");
    let docs = hub_core::bag::add(&hub, add(link("  https://example.com/docs "))).await.unwrap();
    assert_eq!(docs.item.name, "https://example.com/docs", "a link without a name is named by its address");

    let list = hub_core::bag::list(&hub).await.unwrap();
    assert_eq!(list.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), [docs.id.as_str(), notes.id.as_str()], "newest first");
    assert!(list.iter().all(|b| b.item.text.is_none() && b.item.data.is_none()));
    assert_eq!(list[1].from.as_deref(), Some("A thread"));

    let full = hub_core::bag::get(&hub, &notes.id).await.unwrap().unwrap();
    assert_eq!(full.item.text.as_deref(), Some("# Plan"));

    let again = hub_core::bag::add(&hub, add(text("notes.md", "# Plan"))).await.unwrap();
    assert_eq!(again.id, notes.id, "the same thing twice is one item");
    assert_eq!(hub_core::bag::list(&hub).await.unwrap().len(), 2);

    assert!(hub_core::bag::add(&hub, add(link("file:///etc/passwd"))).await.is_err());
    assert!(hub_core::bag::add(&hub, add(Attachment { text: None, ..text("empty", "") })).await.is_err());
    assert!(hub_core::bag::get(&hub, "../routines").await.is_err());

    hub_core::bag::remove(&hub, &docs.id).await.unwrap();
    assert!(hub_core::bag::get(&hub, &docs.id).await.unwrap().is_none());
    assert_eq!(hub_core::bag::clear(&hub).await.unwrap(), 1);
    assert!(hub_core::bag::list(&hub).await.unwrap().is_empty());
}
