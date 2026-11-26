//! A SQLite file an agent mentioned (`.db`, `.sqlite`, `.sqlite3`) as tables for the table viewer:
//! each table with its columns, its row count and its first rows. The file is opened read-only and
//! only `SELECT`s run; the size check, the header check and the caps on tables and rows keep a
//! huge or foreign file from filling the screen or the memory.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde::Serialize;
use ts_rs::TS;

/// Past this a database is too big to open here.
const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_TABLES: usize = 60;
const MAX_ROWS: usize = 100;
const MAX_COLS: usize = 60;
const MAX_CELL: usize = 500;
const MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// What a database holds, table by table.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Database {
    pub tables: Vec<DbTable>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DbTable {
    pub name: String,
    pub columns: Vec<String>,
    /// The first rows (at most 100) as text; a blob shows as its size, a missing value as empty.
    pub rows: Vec<Vec<String>>,
    /// How many rows the table has, before the cut.
    pub total_rows: u32,
}

/// Whether a file name is one of the database kinds this reads.
pub fn is_database(rel: &str) -> bool {
    let ext = rel.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "db" | "sqlite" | "sqlite3")
}

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn cell(v: ValueRef<'_>) -> String {
    let s = match v {
        ValueRef::Null => String::new(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => f.to_string(),
        // Cut the bytes first so a huge cell is never copied whole (4 bytes cover a character).
        ValueRef::Text(t) => String::from_utf8_lossy(&t[..t.len().min(MAX_CELL * 4 + 4)]).into_owned(),
        ValueRef::Blob(b) => return format!("<{} bytes>", b.len()),
    };
    if s.chars().count() > MAX_CELL { s.chars().take(MAX_CELL).collect::<String>() + "…" } else { s }
}

fn first_rows(db: &Connection, name: &str) -> rusqlite::Result<(Vec<String>, Vec<Vec<String>>)> {
    let mut q = db.prepare(&format!("SELECT * FROM {} LIMIT {MAX_ROWS}", quote(name)))?;
    let columns: Vec<String> = q.column_names().into_iter().take(MAX_COLS).map(String::from).collect();
    let width = columns.len();
    let rows = q
        .query_map([], |r| (0..width).map(|i| Ok(cell(r.get_ref(i)?))).collect::<rusqlite::Result<Vec<_>>>())?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok((columns, rows))
}

/// The database at `rel` under `root`, from its file as it is now.
pub fn read(root: &Path, rel: &str) -> Result<Database> {
    if !is_database(rel) {
        bail!("{rel} is not a database file");
    }
    let path = crate::project_folder::inside(root, rel)?;
    let size = std::fs::metadata(&path).with_context(|| format!("reading {rel}"))?.len();
    if size > MAX_FILE {
        bail!("{rel} is {} MB, too big to show here", size / (1024 * 1024));
    }
    // An empty file is a database with nothing in it; anything else must start with SQLite's header.
    if size == 0 {
        return Ok(Database { tables: Vec::new() });
    }
    let mut head = [0u8; 16];
    let n = std::fs::File::open(&path).and_then(|mut f| f.read(&mut head)).with_context(|| format!("reading {rel}"))?;
    if n < 16 || &head != MAGIC {
        bail!("{rel} is not a SQLite database (it does not start with \"SQLite format 3\")");
    }
    let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .with_context(|| format!("{rel} could not be opened as a database"))?;
    let names: Vec<String> = db
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name")
        .and_then(|mut q| q.query_map([], |r| r.get(0))?.collect())
        .with_context(|| format!("{rel} is damaged or encrypted: its tables could not be listed"))?;
    let mut tables = Vec::new();
    for name in names.into_iter().take(MAX_TABLES) {
        let total_rows: i64 = db.query_row(&format!("SELECT count(*) FROM {}", quote(&name)), [], |r| r.get(0)).unwrap_or(0);
        // A table that cannot be read (a virtual table whose module is missing) shows empty, not as a failure of the file.
        let (columns, rows) = first_rows(&db, &name).unwrap_or_default();
        tables.push(DbTable { name, columns, rows, total_rows: total_rows.max(0) as u32 });
    }
    Ok(Database { tables })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(dir: &Path, name: &str) -> Connection {
        Connection::open(dir.join(name)).unwrap()
    }

    #[test]
    fn tables_columns_rows_and_counts_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let db = make(dir.path(), "app.db");
        db.execute_batch(
            "CREATE TABLE thread (id INTEGER PRIMARY KEY, title TEXT, score REAL, data BLOB);
             CREATE TABLE \"odd \"\"name\" (x);
             CREATE TABLE empty (a, b);
             INSERT INTO thread VALUES (1, 'First', 1.5, x'0102'), (2, NULL, 2, NULL);
             INSERT INTO \"odd \"\"name\" VALUES (7);",
        )
        .unwrap();
        db.execute("INSERT INTO empty SELECT 1, 2 WHERE 0", []).unwrap();
        for i in 0..150 {
            db.execute("INSERT INTO thread (id, title) VALUES (?1, 'bulk')", [100 + i]).unwrap();
        }
        drop(db);
        let book = read(dir.path(), "app.db").unwrap();
        let names: Vec<_> = book.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["empty", "odd \"name", "thread"]);
        let thread = &book.tables[2];
        assert_eq!(thread.columns, ["id", "title", "score", "data"]);
        assert_eq!(thread.rows[0], ["1", "First", "1.5", "<2 bytes>"]);
        assert_eq!(thread.rows[1], ["2", "", "2", ""]);
        assert_eq!((thread.rows.len(), thread.total_rows), (MAX_ROWS, 152));
        // A table with no rows still shows its columns.
        assert_eq!((book.tables[0].columns.as_slice(), book.tables[0].rows.len(), book.tables[0].total_rows), (["a".to_string(), "b".to_string()].as_slice(), 0, 0));
        assert_eq!(book.tables[1].rows, [["7"]]);
    }

    #[test]
    fn an_empty_file_has_no_tables_and_a_foreign_one_says_so() {
        assert!(is_database("a/b.DB") && is_database("c.sqlite3") && !is_database("d.csv") && !is_database("db"));
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("empty.db"), b"").unwrap();
        assert!(read(dir.path(), "empty.db").unwrap().tables.is_empty());
        std::fs::write(dir.path().join("fake.db"), b"this is just text, long enough to have a header").unwrap();
        let e = read(dir.path(), "fake.db").unwrap_err().to_string();
        assert!(e.contains("not a SQLite database"), "{e}");
        std::fs::write(dir.path().join("short.db"), b"abc").unwrap();
        assert!(read(dir.path(), "short.db").is_err());
        assert!(read(dir.path(), "missing.db").is_err());
        assert!(read(dir.path(), "notes.md").is_err());
        assert!(read(dir.path(), "../outside.db").is_err());
        // A database without tables reads as an empty list.
        drop(make(dir.path(), "none.db"));
        assert!(read(dir.path(), "none.db").unwrap().tables.is_empty());
    }
}
