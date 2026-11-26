//! The edits of `focus.rs`: one function per operation on the focus file's lines.

use super::*;

/// The focus file changed under the person's edit.
pub(super) fn stale() -> anyhow::Error {
    anyhow::anyhow!("the focus file changed, reload it")
}

/// The text of an edit on one line, or an error when there is none.
pub(super) fn one_line(t: Option<&str>) -> Result<String> {
    let t = t.unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        bail!("nothing to add");
    }
    Ok(t)
}

/// Ticks or unticks the checkbox on a line.
pub(super) fn edit_tick(lines: &mut [String], edit: &FocusEdit) -> Result<()> {
        let done = edit.done.context("say whether it is done")?;
        let target = lines.get_mut(edit.line.context("which line")? as usize).ok_or_else(stale)?;
        if checkbox(target).is_none() {
            return Err(stale());
        }
        let (from, to) = if done { ("[ ]", "[x]") } else { ("[x]", "[ ]") };
        *target = target.replacen(from, to, 1).replacen(&from.to_uppercase(), to, 1);
    Ok(())
}

/// Makes a stage the active one.
pub(super) fn edit_activate(lines: &mut [String], edit: &FocusEdit) -> Result<()> {
        let at = edit.line.context("which stage")? as usize;
        if !lines.get(at).is_some_and(|l| l.starts_with("## ") && heading(&l[3..]).0 == Part::Stage) {
            return Err(stale());
        }
        for l in lines.iter_mut().filter(|l| l.starts_with("## ")) {
            if heading(&l[3..]).2 {
                let cut = l.rfind('(').unwrap_or(l.len());
                l.truncate(cut);
                l.truncate(l.trim_end().len());
            }
        }
        lines[at].push_str(ACTIVE);
    Ok(())
}

/// Adds a checkbox to a stage's list ("Delivered when" or "To test").
pub(super) fn edit_add(lines: &mut Vec<String>, edit: &FocusEdit) -> Result<()> {
        let text = one_line(edit.text.as_deref())?;
        let at = edit.line.context("which stage")? as usize;
        if !lines.get(at).is_some_and(|l| l.starts_with("## ")) {
            return Err(stale());
        }
        let want = match edit.list.as_deref() {
            Some("test") => List::Test,
            Some("done") => List::Done,
            other => bail!("unknown list {other:?}"),
        };
        let end = section_end(&*lines, at);
        let found = (at + 1..end).find(|&i| label(&lines[i]).is_some_and(|(l, _)| l == want));
        let item = format!("- [ ] {text}");
        match found {
            Some(l) => {
                // After the list's last item, which follow the label with no blank line.
                let mut after = l + 1;
                while after < end && checkbox(&lines[after]).is_some() {
                    after += 1;
                }
                lines.insert(after, item);
            }
            None => {
                let mut at_end = end;
                while at_end > at + 1 && lines[at_end - 1].trim().is_empty() {
                    at_end -= 1;
                }
                let name = match want {
                    List::Test => "To test:",
                    _ if heading(&lines[at][3..]).0 == Part::Direction => "Done when:",
                    _ => "Delivered when:",
                };
                lines.splice(at_end..at_end, [String::new(), name.to_owned(), item]);
            }
        }
    Ok(())
}

/// Adds an idea to a stage.
pub(super) fn edit_idea(lines: &mut Vec<String>, edit: &FocusEdit) -> Result<()> {
        let text = one_line(edit.text.as_deref())?;
        let at = edit.line.context("which stage")? as usize;
        if !lines.get(at).is_some_and(|l| l.starts_with("## ") && heading(&l[3..]).0 == Part::Stage) {
            return Err(stale());
        }
        let end = section_end(&*lines, at);
        let item = format!("- {text}");
        match (at + 1..end).find(|&i| label(&lines[i]).is_some_and(|(l, _)| l == List::Ideas)) {
            Some(l) => {
                let mut after = l + 1;
                while after < end && bullet(&lines[after]).is_some() && checkbox(&lines[after]).is_none() {
                    after += 1;
                }
                lines.insert(after, item);
            }
            None => {
                let mut at_end = end;
                while at_end > at + 1 && lines[at_end - 1].trim().is_empty() {
                    at_end -= 1;
                }
                lines.splice(at_end..at_end, [String::new(), "Ideas:".to_owned(), item]);
            }
        }
    Ok(())
}

/// Parks an idea at the end.
pub(super) fn edit_park(lines: &mut Vec<String>, edit: &FocusEdit) -> Result<()> {
        let text = one_line(edit.text.as_deref())?;
        match lines.iter().position(|l| l.starts_with("## ") && heading(&l[3..]).0 == Part::Parked) {
            Some(at) => {
                let mut after = section_end(&*lines, at);
                while after > at + 1 && lines[after - 1].trim().is_empty() {
                    after -= 1;
                }
                if after == at + 1 {
                    lines.insert(after, String::new());
                    after += 1;
                }
                lines.insert(after, format!("- {text}"));
            }
            None => {
                while lines.last().is_some_and(|l| l.trim().is_empty()) {
                    lines.pop();
                }
                lines.extend([String::new(), "## Parked".into(), String::new(), format!("- {text}")]);
            }
        }
    Ok(())
}
