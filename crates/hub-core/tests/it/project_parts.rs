//! The backlog and the board are markdown files in the repo: items are added, put on the board,
//! moved and dropped, and a change aimed at a line that no longer matches touches nothing.

use hub_core::model::PartEdit;
use hub_core::project_parts::{backlog, board, docs_card, edit_backlog, edit_board};

fn edit(op: &str, line: Option<u32>, title: Option<&str>, to: Option<&str>) -> PartEdit {
    PartEdit { path: String::new(), op: op.into(), line, title: title.map(Into::into), tag: Some("ui".into()), to: to.map(Into::into) }
}

#[test]
fn an_idea_goes_from_the_backlog_to_the_board_and_across_it() {
    let dir = hub_core::janitor::test_dir();
    let items = edit_backlog(&dir, &edit("add", None, Some("Heatmap flow"), None)).unwrap();
    assert_eq!((items[0].title.as_str(), items[0].tag.as_str()), ("Heatmap flow", "ui"));
    assert!(dir.join("docs/BACKLOG.md").exists());

    // Onto the board: gone from the backlog, first card in To do.
    let items = edit_backlog(&dir, &edit("board", Some(items[0].line), Some("Heatmap flow"), None)).unwrap();
    assert!(items.is_empty());
    let cols = board(&dir);
    assert_eq!(cols[0].cards[0].title, "Heatmap flow");

    // Across: Doing, and the file keeps the columns in order.
    let cols = edit_board(&dir, &edit("move", Some(cols[0].cards[0].line), Some("Heatmap flow"), Some("Doing"))).unwrap();
    assert!(cols[0].cards.is_empty());
    assert_eq!(cols[1].cards[0].title, "Heatmap flow");
    let text = std::fs::read_to_string(dir.join("docs/BOARD.md")).unwrap();
    assert!(text.find("## To do").unwrap() < text.find("## Doing").unwrap());

    // A line that no longer matches is refused and nothing changes.
    assert!(edit_board(&dir, &edit("drop", Some(cols[1].cards[0].line), Some("Another card"), None)).is_err());
    let cols = edit_board(&dir, &edit("drop", Some(cols[1].cards[0].line), Some("Heatmap flow"), None)).unwrap();
    assert!(cols.iter().all(|c| c.cards.is_empty()));
    assert!(backlog(&dir).is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn docs_list_the_pinned_files_first() {
    let dir = hub_core::janitor::test_dir();
    std::fs::create_dir_all(dir.join("docs")).unwrap();
    std::fs::write(dir.join("docs/ARCHITECTURE.md"), "# Architecture\n").unwrap();
    std::fs::write(dir.join("docs/notes.md"), "# Notes\n").unwrap();
    let card = docs_card(&dir);
    assert_eq!(card.pinned[0].rel, "docs/ARCHITECTURE.md");
    assert_eq!(card.recent[0].title, "Notes");
    assert_eq!(card.count, 2);
    std::fs::remove_dir_all(dir).unwrap();
}
