//! Inserting and deleting whole rows and columns. The cells below or to the
//! right move, and so does everything that points at them: formulas on every
//! sheet (`$` references too, as in other spreadsheets), merged blocks,
//! highlight rules, charts, tables and the filter.

use super::formula::{self, col_name};
use super::model::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Rows,
    Cols,
}

/// `n` rows or columns, starting at `at`, put in or taken out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edit {
    pub axis: Axis,
    pub at: u32,
    pub n: u32,
    pub insert: bool,
}

/// Which end of a range a coordinate is, for when its row or column goes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    Single,
    Start,
    Last,
}

impl Edit {
    fn limit(&self) -> u32 {
        match self.axis {
            Axis::Rows => ROWS,
            Axis::Cols => COLS,
        }
    }

    /// Where a row or column index ends up, or None when it's gone.
    fn map(&self, v: u32, end: End) -> Option<u32> {
        if v < self.at {
            return Some(v);
        }
        if self.insert {
            return (v + self.n < self.limit()).then_some(v + self.n);
        }
        if v >= self.at + self.n {
            return Some(v - self.n);
        }
        match end {
            End::Single => None,
            // A range losing its first rows starts at the next one kept…
            End::Start => Some(self.at),
            // …and one losing its last rows ends at the one before.
            End::Last => self.at.checked_sub(1),
        }
    }

    /// A cell's new place, or None when it's gone.
    fn cell(&self, r: u32, c: u32) -> Option<(u32, u32)> {
        match self.axis {
            Axis::Rows => Some((self.map(r, End::Single)?, c)),
            Axis::Cols => Some((r, self.map(c, End::Single)?)),
        }
    }

    /// A block's new extent, or None when all of it is gone.
    pub fn area(&self, a: Area) -> Option<Area> {
        let ((t, l), (b, r)) = self.range((a.top, a.left), (a.bottom, a.right))?;
        Some(Area { top: t, left: l, bottom: b, right: r })
    }

    fn range(&self, (r1, c1): (u32, u32), (r2, c2): (u32, u32)) -> Option<((u32, u32), (u32, u32))> {
        let (r1, c1, r2, c2) = match self.axis {
            Axis::Rows => (self.map(r1, End::Start)?, c1, self.map(r2, End::Last)?, c2),
            Axis::Cols => (r1, self.map(c1, End::Start)?, r2, self.map(c2, End::Last)?),
        };
        (r1 <= r2 && c1 <= c2).then_some(((r1, c1), (r2, c2)))
    }

    /// An "A1:B9" range string after the edit, or None when it's gone.
    fn range_text(&self, s: &str) -> Option<String> {
        Some(self.area(Area::parse(s)?)?.name())
    }
}

/// Rewrite the references in formula `raw` for `edit`, made on the sheet named
/// `target`. `here` is true when the formula is on that sheet, so references
/// without a sheet name point at it too.
pub fn rewrite(raw: &str, edit: &Edit, target: &str, here: bool) -> String {
    if !raw.starts_with('=') {
        return raw.to_string();
    }
    let b = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' {
            let start = i;
            i += 1;
            while i < b.len() {
                if b[i] == b'"' {
                    if b.get(i + 1) == Some(&b'"') {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push_str(&raw[start..i]);
            continue;
        }
        let prev_word = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b'.');
        let starts_ref = c == b'\'' || (!prev_word && (c == b'$' || c.is_ascii_alphabetic()));
        if !starts_ref {
            let ch = raw[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        let start = i;
        // An optional sheet name, then a cell, then maybe `:` and a second cell.
        let (sheet, at) = sheet_prefix(raw, i);
        let Some(first) = cell_at(b, at) else {
            // A quoted name with no cell after it, a function or other word: copy it.
            let mut k = if sheet.is_some() { at } else { start + 1 };
            if c == b'\'' && sheet.is_none() {
                k = start + 1;
                while k < b.len() && b[k] != b'\'' {
                    k += 1;
                }
                k = (k + 1).min(b.len());
            }
            while k < b.len() && (b[k].is_ascii_alphanumeric() || b[k] == b'_' || b[k] == b'$' || b[k] == b'.') {
                k += 1;
            }
            out.push_str(&raw[start..k]);
            i = k;
            continue;
        };
        let second = (b.get(first.end) == Some(&b':')).then(|| cell_at(b, first.end + 1)).flatten();
        let end = second.as_ref().map_or(first.end, |s| s.end);
        let applies = match &sheet {
            Some(name) => name.eq_ignore_ascii_case(target),
            None => here,
        };
        if !applies {
            out.push_str(&raw[start..end]);
            i = end;
            continue;
        }
        out.push_str(&raw[start..first.start]);
        match second {
            None => match edit.cell(first.r, first.c) {
                Some((r, c)) => first.write(&mut out, r, c),
                None => out.push_str("#REF!"),
            },
            Some(second) => match edit.range((first.r, first.c), (second.r, second.c)) {
                Some(((r1, c1), (r2, c2))) => {
                    first.write(&mut out, r1, c1);
                    out.push(':');
                    second.write(&mut out, r2, c2);
                }
                None => out.push_str("#REF!"),
            },
        }
        i = end;
    }
    out
}

/// A sheet name and `!` at `i` (quoted or plain): the name and where the cell
/// starts. Without one, (None, i).
fn sheet_prefix(raw: &str, i: usize) -> (Option<String>, usize) {
    let b = raw.as_bytes();
    if b.get(i) == Some(&b'\'') {
        let mut j = i + 1;
        let mut name = String::new();
        while j < b.len() {
            if b[j] == b'\'' {
                if b.get(j + 1) == Some(&b'\'') {
                    name.push('\'');
                    j += 2;
                    continue;
                }
                break;
            }
            let ch = raw[j..].chars().next().unwrap_or(' ');
            name.push(ch);
            j += ch.len_utf8();
        }
        if b.get(j + 1) == Some(&b'!') {
            return (Some(name), j + 2);
        }
        return (None, i);
    }
    let mut j = i;
    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
        j += 1;
    }
    if j > i && b.get(j) == Some(&b'!') {
        return (Some(raw[i..j].to_string()), j + 1);
    }
    (None, i)
}

struct CellRef {
    start: usize,
    end: usize,
    r: u32,
    c: u32,
    row_abs: bool,
    col_abs: bool,
}

impl CellRef {
    fn write(&self, out: &mut String, r: u32, c: u32) {
        let _ = write!(
            out,
            "{}{}{}{}",
            if self.col_abs { "$" } else { "" },
            col_name(c),
            if self.row_abs { "$" } else { "" },
            r + 1
        );
    }
}

/// A cell address (`B4`, `$B$4`) at `i`, not part of a longer word.
fn cell_at(b: &[u8], i: usize) -> Option<CellRef> {
    let mut j = i;
    let col_abs = b.get(j) == Some(&b'$');
    if col_abs {
        j += 1;
    }
    let ls = j;
    while j < b.len() && b[j].is_ascii_alphabetic() {
        j += 1;
    }
    let le = j;
    let row_abs = b.get(j) == Some(&b'$');
    if row_abs {
        j += 1;
    }
    let ds = j;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    let follows_word = j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'(' || b[j] == b'_' || b[j] == b'!');
    if le == ls || le - ls > 3 || j == ds || follows_word {
        return None;
    }
    let text: String = std::str::from_utf8(&b[ls..j]).ok()?.replace('$', "");
    let (r, c) = formula::parse_cell(&text)?;
    Some(CellRef { start: i, end: j, r, c, row_abs, col_abs })
}

/// How far down (rows) or across (columns) index `at` starts, in pixels.
fn offset(s: &Sheet, axis: Axis, at: u32) -> f64 {
    match axis {
        Axis::Rows => (0..at).map(|r| s.row_h(r) as f64).sum(),
        Axis::Cols => (0..at).map(|c| s.col_w(c) as f64).sum(),
    }
}

impl Workbook {
    /// Put in or take out rows or columns on sheet `si`. False (and nothing
    /// changed) when inserting would push cells off the end of the sheet.
    pub fn restructure(&mut self, si: usize, edit: Edit) -> bool {
        let Some(sheet) = self.sheets.get(si) else { return false };
        if edit.n == 0 || edit.at >= edit.limit() {
            return false;
        }
        if edit.insert {
            let last = edit.limit() - edit.n;
            let spills = sheet.cells.keys().any(|&(r, c)| match edit.axis {
                Axis::Rows => r >= last,
                Axis::Cols => c >= last,
            });
            if spills {
                return false;
            }
        }
        let target = sheet.name.clone();

        // Formulas on every sheet, and lists that take their choices from cells.
        for (sj, s) in self.sheets.iter_mut().enumerate() {
            for cell in s.cells.values_mut() {
                if cell.raw.starts_with('=') {
                    cell.raw = rewrite(&cell.raw, &edit, &target, sj == si);
                }
            }
            for l in &mut s.lists {
                if let Some(src) = &l.source {
                    let moved = rewrite(&format!("={src}"), &edit, &target, sj == si);
                    l.source = Some(moved.trim_start_matches('=').to_string());
                }
            }
        }

        let s = &mut self.sheets[si];
        // Pixel extent of what's inserted or removed, for placed charts.
        let edge = offset(s, edit.axis, edit.at);
        let span = if edit.insert {
            edit.n as f64 * match edit.axis {
                Axis::Rows => ROW_H as f64,
                Axis::Cols => COL_W as f64,
            }
        } else {
            offset(s, edit.axis, (edit.at + edit.n).min(edit.limit())) - edge
        };

        s.cells = std::mem::take(&mut s.cells)
            .into_iter()
            .filter_map(|((r, c), cell)| edit.cell(r, c).map(|k| (k, cell)))
            .collect();
        let sizes = |m: &mut BTreeMap<u32, u32>| {
            *m = std::mem::take(m).into_iter().filter_map(|(k, v)| Some((edit.map(k, End::Single)?, v))).collect();
        };
        // Rows or columns put in or taken out among those that stay put change their number.
        let frozen = |n: &mut u32| {
            if edit.at < *n {
                *n = if edit.insert { (*n + edit.n).min(MAX_FROZEN) } else { *n - (edit.at + edit.n).min(*n) + edit.at };
            }
        };
        match edit.axis {
            Axis::Rows => frozen(&mut s.freeze_rows),
            Axis::Cols => frozen(&mut s.freeze_cols),
        }
        let hidden = |set: &mut BTreeSet<u32>| {
            *set = std::mem::take(set).into_iter().filter_map(|k| edit.map(k, End::Single)).collect();
        };
        match edit.axis {
            Axis::Rows => {
                sizes(&mut s.row_heights);
                hidden(&mut s.hidden_rows);
            }
            Axis::Cols => {
                sizes(&mut s.col_widths);
                hidden(&mut s.hidden_cols);
            }
        }
        s.merges = s.merges.iter().filter_map(|a| edit.area(*a)).filter(|a| !a.is_cell()).collect();
        s.print_area = s.print_area.and_then(|a| edit.area(a));
        s.rules.retain_mut(|rule| match edit.range_text(&rule.range) {
            Some(r) => {
                rule.range = r;
                true
            }
            None => false,
        });
        s.lists.retain_mut(|l| match edit.range_text(&l.range) {
            Some(r) => {
                l.range = r;
                true
            }
            None => false,
        });
        s.tables.retain_mut(|t| match edit.range_text(&t.range) {
            Some(r) => {
                t.range = r;
                true
            }
            None => false,
        });
        if let Some(chart) = &mut s.chart {
            match edit.range_text(&chart.range) {
                Some(r) => chart.range = r,
                None => s.chart = None,
            }
        }
        s.charts.retain_mut(|pc| {
            let Some(r) = edit.range_text(&pc.range) else { return false };
            pc.range = r;
            let pos = match edit.axis {
                Axis::Rows => &mut pc.y,
                Axis::Cols => &mut pc.x,
            };
            if edit.insert {
                if *pos >= edge {
                    *pos += span;
                }
            } else if *pos >= edge + span {
                *pos -= span;
            } else if *pos > edge {
                *pos = edge;
            }
            true
        });
        if edit.axis == Axis::Cols
            && let Some(f) = &mut s.filter
        {
            match edit.map(f.col, End::Single) {
                Some(c) => f.col = c,
                None => s.filter = None,
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(at: u32, n: u32, insert: bool) -> Edit {
        Edit { axis: Axis::Rows, at, n, insert }
    }

    fn cols(at: u32, n: u32, insert: bool) -> Edit {
        Edit { axis: Axis::Cols, at, n, insert }
    }

    #[test]
    fn formulas_follow_inserted_rows() {
        let e = rows(2, 2, true); // two rows above row 3
        assert_eq!(rewrite("=A1+A3+$B$5", &e, "S", true), "=A1+A5+$B$7");
        assert_eq!(rewrite("=SUM(A1:A10)", &e, "S", true), "=SUM(A1:A12)");
        assert_eq!(rewrite("=SUM(A3:A4)", &e, "S", true), "=SUM(A5:A6)");
        assert_eq!(rewrite("=\"A3\"&A3", &e, "S", true), "=\"A3\"&A5");
        // Other sheets: only references naming this one move.
        assert_eq!(rewrite("=A3+S!A3+'S'!A3+T!A3", &e, "S", false), "=A3+S!A5+'S'!A5+T!A3");
        assert_eq!(rewrite("=A3+'My sheet'!A3", &e, "My sheet", false), "=A3+'My sheet'!A5");
        assert_eq!(rewrite("=LOG10(A3)", &e, "S", true), "=LOG10(A5)");
    }

    #[test]
    fn formulas_follow_deleted_rows_and_columns() {
        let e = rows(2, 2, false); // rows 3 and 4 go
        assert_eq!(rewrite("=A1+A5", &e, "S", true), "=A1+A3");
        assert_eq!(rewrite("=A3", &e, "S", true), "=#REF!");
        assert_eq!(rewrite("=SUM(A1:A10)", &e, "S", true), "=SUM(A1:A8)");
        assert_eq!(rewrite("=SUM(A3:A10)", &e, "S", true), "=SUM(A3:A8)");
        assert_eq!(rewrite("=SUM(A1:A4)", &e, "S", true), "=SUM(A1:A2)");
        assert_eq!(rewrite("=SUM(A3:B4)", &e, "S", true), "=SUM(#REF!)");
        let e = cols(1, 1, false); // column B goes
        assert_eq!(rewrite("=A1+C1+$D$2", &e, "S", true), "=A1+B1+$C$2");
        assert_eq!(rewrite("=SUM(A1:C1)", &e, "S", true), "=SUM(A1:B1)");
        let e = cols(0, 1, false);
        assert_eq!(rewrite("=SUM(A1:A3)", &e, "S", true), "=SUM(#REF!)");
    }

    #[test]
    fn cells_and_extras_move() {
        let mut wb = Workbook::default();
        wb.set(0, 0, "1");
        wb.set(4, 0, "5");
        wb.set(5, 0, "=A1+A5");
        wb.sheets[0].merges.push(Area::new(3, 0, 6, 1));
        wb.sheets[0].row_heights.insert(4, 50);
        wb.sheets[0].hidden_rows.extend([2, 6]);
        wb.sheets[0].freeze_rows = 3;
        wb.add_sheet();
        wb.active = 1;
        wb.set(0, 0, "='Sheet 1'!A5");
        assert!(wb.restructure(0, rows(1, 3, false)));
        let s = &wb.sheets[0];
        assert_eq!(s.raw(1, 0), "5");
        assert_eq!(s.raw(2, 0), "=A1+A2");
        assert_eq!(s.row_heights.get(&1), Some(&50));
        // Row 3 (hidden) went; row 7 (hidden) is now row 4.
        assert_eq!(s.hidden_rows.iter().copied().collect::<Vec<_>>(), [3]);
        // Rows 2 and 3 of the 3 kept in view went.
        assert_eq!(s.freeze_rows, 1);
        assert_eq!(s.merges, vec![Area::new(1, 0, 3, 1)]);
        assert_eq!(wb.sheets[1].raw(0, 0), "='Sheet 1'!A2");
        // Inserting refuses to push cells off the sheet.
        wb.set(ROWS - 1, 0, "end");
        assert!(!wb.restructure(1, rows(0, 1, true)));
    }
}
