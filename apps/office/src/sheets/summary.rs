//! Cell borders, wrapped text (rows grow to fit), and Summarize: totals by
//! group on a new sheet, kept live with SUMIFS-style formulas.

use super::Sheets;
use super::formula::{self, Value};
use super::model::*;
use crate::{widgets, window};
use gtk::prelude::*;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Summary {
    Sum,
    Count,
    Average,
    Max,
    Min,
}

impl Summary {
    pub const ALL: [Summary; 5] = [Summary::Sum, Summary::Count, Summary::Average, Summary::Max, Summary::Min];
    pub fn name(self) -> &'static str {
        match self {
            Summary::Sum => "Sum",
            Summary::Count => "Count",
            Summary::Average => "Average",
            Summary::Max => "Largest",
            Summary::Min => "Smallest",
        }
    }
    fn function(self) -> &'static str {
        match self {
            Summary::Sum => "SUMIFS",
            Summary::Count => "COUNTIFS",
            Summary::Average => "AVERAGEIFS",
            Summary::Max => "MAXIFS",
            Summary::Min => "MINIFS",
        }
    }
}

/// What to summarize: the data (header row first), and which columns.
pub struct Request {
    pub sheet: usize,
    pub data: Area,
    pub group: u32,
    pub across: Option<u32>,
    pub value: u32,
    pub how: Summary,
}

/// A sheet name as a formula writes it.
fn sheet_ref(name: &str) -> String {
    if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.starts_with(|c: char| c.is_ascii_digit()) {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// The distinct values in a column of the data (header left out), in order.
fn distinct(wb: &Workbook, calc: &Calc, si: usize, data: Area, col: u32) -> Vec<String> {
    let s = &wb.sheets[si];
    let mut seen: Vec<(Value, String)> = Vec::new();
    for r in data.top + 1..=data.bottom {
        let raw = s.raw(r, col);
        if raw.is_empty() {
            continue;
        }
        // A label as typed, so the criterion matches it (formulas give their value).
        let v = calc.value(wb, si, r, col);
        let text = if raw.starts_with('=') { v.text() } else { raw.to_string() };
        if text.is_empty() || seen.iter().any(|(_, t)| t.eq_ignore_ascii_case(&text)) {
            continue;
        }
        seen.push((v, text));
    }
    seen.sort_by(|(a, at), (b, bt)| match (a, b) {
        (Value::Num(x), Value::Num(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Num(_), _) => std::cmp::Ordering::Less,
        (_, Value::Num(_)) => std::cmp::Ordering::Greater,
        _ => at.to_lowercase().cmp(&bt.to_lowercase()),
    });
    seen.into_iter().map(|(_, t)| t).collect()
}

/// Add a summary sheet after the source and fill it in. Returns its index.
pub fn summarize(wb: &mut Workbook, req: &Request) -> Option<usize> {
    let calc = Calc::new(wb, 0.0);
    let src = &wb.sheets[req.sheet];
    let header = |c: u32| {
        let h = src.raw(req.data.top, c);
        if h.is_empty() { format!("Column {}", formula::col_name(c)) } else { h.to_string() }
    };
    let (group_name, value_name) = (header(req.group), header(req.value));
    let rows = distinct(wb, &calc, req.sheet, req.data, req.group);
    let cols: Vec<String> = match req.across {
        Some(a) => distinct(wb, &calc, req.sheet, req.data, a),
        None => vec![format!("{} of {}", req.how.name(), value_name)],
    };
    if rows.is_empty() || cols.is_empty() {
        return None;
    }
    let sref = sheet_ref(&src.name);
    let range = |c: u32| {
        format!("{sref}!${}${}:${}${}", formula::col_name(c), req.data.top + 2, formula::col_name(c), req.data.bottom + 1)
    };
    let (vr, gr) = (range(req.value), range(req.group));
    let ar = req.across.map(range);
    let base = format!("Summary of {}", src.name);
    let mut name = base.clone();
    let mut n = 2;
    while wb.sheets.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
        name = format!("{base} {n}");
        n += 1;
    }
    let name: String = name.chars().take(31).collect();
    if !wb.add_sheet() {
        return None;
    }
    let at = req.sheet + 1;
    let last = wb.sheets.len() - 1;
    wb.move_sheet_to(last, at);
    wb.active = at;
    let _ = wb.rename_sheet(at, &name);

    // Header row: the group's name, then a column per across value (or the measure).
    wb.set(0, 0, &group_name);
    for (j, c) in cols.iter().enumerate() {
        wb.set(0, j as u32 + 1, c);
    }
    let total_col = (req.across.is_some() && req.how == Summary::Sum).then_some(cols.len() as u32 + 1);
    if let Some(tc) = total_col {
        wb.set(0, tc, "Total");
    }
    for (i, label) in rows.iter().enumerate() {
        let r = i as u32 + 1;
        // A label that looks like a formula or a criterion is kept as text.
        let shown = if label.starts_with(['=', '<', '>']) { format!("'{label}") } else { label.clone() };
        wb.set(r, 0, &shown);
        for j in 0..cols.len() as u32 {
            let c = j + 1;
            let target = if req.how == Summary::Count { gr.clone() } else { format!("{vr}, {gr}") };
            let mut f = format!("={}({target}, $A{}", req.how.function(), r + 1);
            if let Some(ar) = &ar {
                let _ = std::fmt::Write::write_fmt(&mut f, format_args!(", {ar}, {}$1", formula::col_name(c)));
            }
            f.push(')');
            wb.set(r, c, &f);
        }
        if let Some(tc) = total_col {
            wb.set(r, tc, &format!("=SUM(B{0}:{1}{0})", r + 1, formula::col_name(tc - 1)));
        }
    }
    let width = total_col.unwrap_or(cols.len() as u32);
    if req.how == Summary::Sum || req.how == Summary::Count {
        let tr = rows.len() as u32 + 1;
        wb.set(tr, 0, "Total");
        for c in 1..=width {
            let col = formula::col_name(c);
            wb.set(tr, c, &format!("=SUM({col}2:{col}{tr})"));
        }
        wb.format(Area::new(tr, 0, tr, width), |f| {
            f.bold = true;
            f.borders |= TOP;
        });
    }
    wb.format(Area::new(0, 0, 0, width), |f| {
        f.bold = true;
        f.borders |= BOTTOM;
    });
    if req.how == Summary::Average {
        wb.format(Area::new(1, 1, rows.len() as u32, width), |f| f.number = NumFmt::Decimal);
    }
    wb.set_col_w(0, 160);
    let s = wb.sheet_mut();
    s.header = true;
    s.freeze = true;
    Some(at)
}

impl Sheets {
    /// Lines around the selected cells.
    pub(super) fn borders_button(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-grid-2x2-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Borders"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        pop.add_css_class("menu-popover");
        let list = widgets::vbox(0);
        for (label, kind) in [
            ("All borders", Borders::All),
            ("Outside border", Borders::Outside),
            ("Top border", Borders::Top),
            ("Bottom border", Borders::Bottom),
            ("No borders", Borders::None),
        ] {
            let b = gtk::Button::with_label(label);
            b.add_css_class("menu-item");
            if let Some(l) = b.child().and_downcast::<gtk::Label>() {
                l.set_xalign(0.0);
            }
            let me = Rc::downgrade(self);
            let p = pop.clone();
            b.connect_clicked(move |_| {
                p.popdown();
                if let Some(s) = me.upgrade() {
                    let a = s.selection_src();
                    s.change(|wb| wb.set_borders(a, kind));
                    s.area.grab_focus();
                }
            });
            list.append(&b);
        }
        pop.set_child(Some(&list));
        menu.set_popover(Some(&pop));
        menu
    }

    /// Wrap (or unwrap) the selection, and fit its rows to the wrapped text.
    pub(super) fn set_wrap(&self, on: bool) {
        let a = self.selection_src();
        let fits = self.fitted_heights(a, on);
        self.change(|wb| {
            wb.format(a, |f| f.wrap = on);
            for (r, h) in &fits {
                wb.set_row_h(*r, *h);
            }
        });
        self.area.grab_focus();
    }

    /// Row heights that show every wrapped cell in `a`'s rows whole.
    fn fitted_heights(&self, a: Area, wrapping: bool) -> Vec<(u32, u32)> {
        let wb = self.wb.borrow();
        let calc = self.calc.borrow();
        let s = wb.sheet();
        let Some((_, last_col)) = s.used() else { return Vec::new() };
        let layout = self.area.create_pango_layout(None);
        layout.set_wrap(gtk::pango::WrapMode::WordChar);
        let mut out = Vec::new();
        for r in a.top..=a.bottom.min(s.used().map(|u| u.0).unwrap_or(0)) {
            let mut need = ROW_H as i32;
            for c in 0..=last_col {
                let wrapped = if a.contains(r, c) { wrapping } else { s.format(r, c).wrap };
                let raw = s.raw(r, c);
                if !wrapped || raw.is_empty() {
                    continue;
                }
                let f = s.format(r, c);
                let mut font = gtk::pango::FontDescription::from_string("Noto Sans 10");
                if f.bold {
                    font.set_weight(gtk::pango::Weight::Bold);
                }
                layout.set_font_description(Some(&font));
                layout.set_width(((s.col_w(c) as f64 - 12.0).max(1.0) * gtk::pango::SCALE as f64) as i32);
                layout.set_text(&display(&calc.value(&wb, wb.active, r, c), raw, &f).0);
                need = need.max(layout.pixel_size().1 + 10);
            }
            out.push((r, need as u32));
        }
        out
    }

    /// Summarize: pick the group, the column to total and how, and get a new sheet.
    pub(super) fn summarize_dialog(&self) {
        self.end_edit(true);
        let wb = self.wb.borrow();
        let s = wb.sheet();
        let Some((last_row, last_col)) = s.used() else {
            window::toast("Put some data on the sheet to summarize it.");
            return;
        };
        let sel = self.selection_src();
        // The selection, if it's a block; otherwise all the data from the top.
        let data = if !sel.is_cell() { sel } else { Area::new(0, 0, last_row, last_col) };
        if data.rows() < 2 {
            window::toast("Select a header row and the rows of data under it.");
            return;
        }
        let names: Vec<String> = (data.left..=data.right)
            .map(|c| {
                let h = s.raw(data.top, c);
                if h.is_empty() { format!("Column {}", formula::col_name(c)) } else { format!("{h} ({})", formula::col_name(c)) }
            })
            .collect();
        drop(wb);
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let (dialog, card) = widgets::dialog("Summarize", 440);
        let note = widgets::label(
            &format!("From {} on this sheet, with its first row as headings. The totals stay up to date; new groups need summarizing again.", data.name()),
            "dim",
        );
        note.set_wrap(true);
        note.set_xalign(0.0);
        card.append(&note);
        let group = gtk::DropDown::from_strings(&refs);
        let mut across_names = vec!["Nothing"];
        across_names.extend(refs.iter());
        let across = gtk::DropDown::from_strings(&across_names);
        let value = gtk::DropDown::from_strings(&refs);
        value.set_selected((refs.len() as u32).saturating_sub(1));
        let how_names: Vec<&str> = Summary::ALL.iter().map(|s| s.name()).collect();
        let how = gtk::DropDown::from_strings(&how_names);
        let list = widgets::vbox(0);
        list.add_css_class("group-list");
        list.append(&widgets::row("Group rows by", "A row in the summary for each different value.", Some(group.upcast_ref())));
        list.append(&widgets::row("Split across", "Optional: a column in the summary for each value.", Some(across.upcast_ref())));
        list.append(&widgets::row("Total", "The column with the numbers.", Some(value.upcast_ref())));
        list.append(&widgets::row("As", "", Some(how.upcast_ref())));
        card.append(&list);
        let buttons = widgets::hbox(8);
        buttons.set_halign(gtk::Align::End);
        let cancel = gtk::Button::with_label("Cancel");
        let ok = gtk::Button::with_label("Summarize");
        ok.add_css_class("suggested-action");
        buttons.append(&cancel);
        buttons.append(&ok);
        card.append(&buttons);
        let d = dialog.clone();
        cancel.connect_clicked(move |_| d.close());
        let me = self.weak();
        let d = dialog.clone();
        ok.connect_clicked(move |_| {
            d.close();
            let Some(s) = me.upgrade() else { return };
            let a = across.selected();
            let req = Request {
                sheet: s.wb.borrow().active,
                data,
                group: data.left + group.selected(),
                across: (a > 0).then(|| data.left + a - 1),
                value: data.left + value.selected(),
                how: Summary::ALL[(how.selected() as usize).min(4)],
            };
            if s.wb.borrow().sheets.len() >= MAX_SHEETS {
                window::toast(&format!("A workbook holds up to {MAX_SHEETS} sheets."));
                return;
            }
            let mut done = None;
            s.change(|wb| done = summarize(wb, &req));
            if done.is_none() {
                window::toast("There's nothing to group in that column.");
                return;
            }
            s.cursor.set((0, 0));
            s.anchor.set((0, 0));
            s.after_change(false);
        });
        dialog.present();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_by_group_and_across() {
        let mut wb = Workbook::default();
        wb.rename_sheet(0, "Sales data").unwrap();
        for (r, row) in [
            ["Region", "Month", "Amount"],
            ["North", "Jan", "100"],
            ["South", "Jan", "50"],
            ["North", "Feb", "25"],
            ["north", "Feb", "5"],
        ]
        .iter()
        .enumerate()
        {
            for (c, v) in row.iter().enumerate() {
                wb.set(r as u32, c as u32, v);
            }
        }
        let req = Request { sheet: 0, data: Area::new(0, 0, 4, 2), group: 0, across: None, value: 2, how: Summary::Sum };
        let at = summarize(&mut wb, &req).unwrap();
        assert_eq!(at, 1);
        assert_eq!(wb.sheets[1].name, "Summary of Sales data");
        assert_eq!(wb.sheets[1].raw(1, 1), "=SUMIFS('Sales data'!$C$2:$C$5, 'Sales data'!$A$2:$A$5, $A2)");
        let calc = Calc::new(&wb, 0.0);
        let v = |r, c| calc.value(&wb, 1, r, c);
        assert_eq!((v(1, 0), v(1, 1)), (Value::Text("North".into()), Value::Num(130.0)));
        assert_eq!(v(2, 1), Value::Num(50.0));
        assert_eq!((v(3, 0), v(3, 1)), (Value::Text("Total".into()), Value::Num(180.0)));
        // The totals follow the data.
        wb.active = 0;
        wb.set(2, 2, "70");
        let calc = Calc::new(&wb, 0.0);
        assert_eq!(calc.value(&wb, 1, 2, 1), Value::Num(70.0));

        // Across months, counting.
        let req = Request { across: Some(1), how: Summary::Count, ..req };
        let at = summarize(&mut wb, &req).unwrap();
        assert_eq!(wb.sheets[at].name, "Summary of Sales data 2");
        let calc = Calc::new(&wb, 0.0);
        let v = |r, c| calc.value(&wb, at, r, c);
        assert_eq!((v(0, 1), v(0, 2)), (Value::Text("Feb".into()), Value::Text("Jan".into())));
        assert_eq!((v(1, 1), v(1, 2)), (Value::Num(2.0), Value::Num(1.0)));
    }
}
