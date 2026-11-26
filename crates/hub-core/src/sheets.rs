//! A spreadsheet an agent made (`.xlsx`, `.xlsm`, `.xls`, `.ods`) as rows of text for the table
//! viewer, one table per sheet. The file is only read: formulas show the value the program last
//! saved, nothing is run, and the size, sheets, rows and columns are capped so a huge workbook
//! cannot fill the screen or the memory.

use std::path::Path;

use anyhow::{Context, Result, bail};
use calamine::{Data, Reader, open_workbook_auto};
use serde::Serialize;
use ts_rs::TS;

/// Past this a workbook is too big to read here.
const MAX_FILE: u64 = 20 * 1024 * 1024;
const MAX_SHEETS: usize = 30;
const MAX_ROWS: usize = 1000;
const MAX_COLS: usize = 60;

/// What a spreadsheet holds, sheet by sheet.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Sheet {
    pub name: String,
    /// The first row is the heading row; rows are cut at 1000 and columns at 60.
    pub rows: Vec<Vec<String>>,
    /// How many rows and columns the sheet has, before the cut.
    pub total_rows: u32,
    pub total_cols: u32,
}

/// Whether a file name is one of the spreadsheet kinds this reads.
pub fn is_sheet(rel: &str) -> bool {
    let ext = rel.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "xlsx" | "xlsm" | "xls" | "ods")
}

fn text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        Data::Float(f) => format!("{f}"),
        Data::DateTime(d) if d.is_datetime() => serial_date(d.as_f64()),
        other => other.to_string(),
    }
}

/// An Excel date serial (days since 1899-12-30, the time as a fraction) as `2026-05-01` or
/// `2026-05-01 14:30`.
fn serial_date(serial: f64) -> String {
    let days = serial.floor();
    let minutes = ((serial - days) * 1440.0).round() as i64;
    // Days since 1970-01-01, then the civil date (Howard Hinnant's algorithm).
    let z = days as i64 - 25569 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    if minutes == 0 {
        format!("{year:04}-{month:02}-{day:02}")
    } else {
        format!("{year:04}-{month:02}-{day:02} {:02}:{:02}", minutes / 60 % 24, minutes % 60)
    }
}

/// The workbook at `rel` under `root`, from its file as it is now.
pub fn read(root: &Path, rel: &str) -> Result<Workbook> {
    if !is_sheet(rel) {
        bail!("{rel} is not a spreadsheet");
    }
    let path = crate::project_folder::inside(root, rel)?;
    let size = std::fs::metadata(&path).with_context(|| format!("reading {rel}"))?.len();
    if size > MAX_FILE {
        bail!("{rel} is {} MB, too big to show here", size / (1024 * 1024));
    }
    let mut book = open_workbook_auto(&path).with_context(|| format!("{rel} could not be opened as a spreadsheet"))?;
    let mut sheets = Vec::new();
    for name in book.sheet_names().into_iter().take(MAX_SHEETS) {
        let Ok(range) = book.worksheet_range(&name) else { continue };
        let (total_rows, total_cols) = range.get_size();
        let rows = range
            .rows()
            .take(MAX_ROWS)
            .map(|row| row.iter().take(MAX_COLS).map(text).collect::<Vec<_>>())
            .collect();
        sheets.push(Sheet { name, rows, total_rows: total_rows as u32, total_cols: total_cols as u32 });
    }
    Ok(Workbook { sheets })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_spreadsheet_files_are_read_and_a_broken_one_says_so() {
        assert!(is_sheet("a/b.XLSX") && is_sheet("c.ods") && !is_sheet("d.csv") && !is_sheet("xlsx"));
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("broken.xlsx"), b"not a zip").unwrap();
        assert!(read(dir.path(), "broken.xlsx").is_err());
        assert!(read(dir.path(), "missing.xlsx").is_err());
        assert!(read(dir.path(), "notes.md").is_err());
        assert!(read(dir.path(), "../outside.xlsx").is_err());
    }

    #[test]
    fn excel_dates_read_as_dates() {
        assert_eq!(serial_date(25569.0), "1970-01-01");
        assert_eq!(serial_date(45292.0), "2024-01-01");
        assert_eq!(serial_date(46143.604166667), "2026-05-01 14:30");
    }

    #[test]
    fn a_workbook_reads_as_one_table_per_sheet() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::copy(crate::source_tree::hub_core().join("tests/fixtures/small.xlsx"), dir.path().join("small.xlsx")).unwrap();
        let book = read(dir.path(), "small.xlsx").unwrap();
        assert_eq!(book.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Inflation", "Notes"]);
        let rows = &book.sheets[0].rows;
        assert_eq!(rows[0], ["Month", "CPIF", "CPI", "Date"]);
        // Whole numbers lose their ".0", dates read as dates, an empty cell is empty.
        assert_eq!(rows[1], ["Oct", "2.4", "2.9", "2025-10-01"]);
        assert_eq!(rows[2], ["Nov", "2", "2.6", "2025-11-01 14:30"]);
        assert_eq!(book.sheets[0].total_rows, 4);
        assert_eq!(book.sheets[1].rows, [["Source", "SCB"]]);
    }
}
