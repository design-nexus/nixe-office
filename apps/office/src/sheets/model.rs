//! A workbook: sheets of cells with their formats, plus each sheet's view
//! (column widths, sort, filter, frozen header) and chart. No GTK here.

use super::formula::{self, Cells, Value};
pub use super::rules::Rule;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const ROWS: u32 = 1_048_576;
pub const COLS: u32 = 104;
pub const MAX_SHEETS: usize = 50;
pub const ROW_H: u32 = 30;
pub const COL_W: u32 = 112;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NumFmt {
    #[default]
    General,
    /// Two decimal places, with thousands separators.
    Decimal,
    Currency,
    Percent,
    Date,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HAlign {
    Left,
    Center,
    Right,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Format {
    #[serde(skip_serializing_if = "is_default")]
    pub number: NumFmt,
    /// A format code (`#,##0.00`, `d mmm yyyy`) that takes the place of `number`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(skip_serializing_if = "is_default")]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub italic: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<HAlign>,
    /// Background, `#rrggbb`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    /// Text colour, `#rrggbb`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Lines on the cell's edges: `TOP | BOTTOM | LEFT | RIGHT`.
    #[serde(skip_serializing_if = "is_default")]
    pub borders: u8,
    /// Long text wraps onto more lines inside the cell.
    #[serde(skip_serializing_if = "is_default")]
    pub wrap: bool,
    /// Shown as a box to tick: the value is TRUE or FALSE.
    #[serde(skip_serializing_if = "is_default")]
    pub checkbox: bool,
}

/// Border edges, for `Format::borders`.
pub const TOP: u8 = 1;
pub const BOTTOM: u8 = 2;
pub const LEFT: u8 = 4;
pub const RIGHT: u8 = 8;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cell {
    /// What was typed: text, a number, or a formula starting with `=`.
    pub raw: String,
    pub format: Format,
    /// A note on the cell, shown when the pointer rests on it.
    pub comment: Option<String>,
}

impl Cell {
    fn is_blank(&self) -> bool {
        self.raw.is_empty() && self.format == Format::default() && self.comment.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Borders {
    All,
    Outside,
    Top,
    Bottom,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sort {
    pub col: u32,
    pub ascending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    pub col: u32,
    /// Show rows whose cell contains this text ("" for any).
    #[serde(default)]
    pub query: String,
    /// And, when set, only rows whose cell shows one of these (as shown; "" for empty).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ChartKind {
    #[default]
    Bar,
    Line,
    Area,
    Pie,
    /// Bars of each series on top of one another.
    StackedBar,
    StackedArea,
    /// Points: the first column holds the x values.
    Scatter,
}

impl ChartKind {
    pub const ALL: [ChartKind; 7] = [
        ChartKind::Bar,
        ChartKind::StackedBar,
        ChartKind::Line,
        ChartKind::Area,
        ChartKind::StackedArea,
        ChartKind::Pie,
        ChartKind::Scatter,
    ];
    pub fn name(self) -> &'static str {
        match self {
            ChartKind::Bar => "Bar",
            ChartKind::Line => "Line",
            ChartKind::Area => "Area",
            ChartKind::Pie => "Pie",
            ChartKind::StackedBar => "Stacked bar",
            ChartKind::StackedArea => "Stacked area",
            ChartKind::Scatter => "Scatter",
        }
    }
    pub fn stacked(self) -> bool {
        matches!(self, ChartKind::StackedBar | ChartKind::StackedArea)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chart {
    /// `A1:C12`: labels in the first column, one series per other column.
    pub range: String,
    pub kind: ChartKind,
    #[serde(flatten, default)]
    pub text: ChartText,
}

/// A chart's title, and what its axes measure.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChartText {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// Across the bottom.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub x_title: String,
    /// Up the side.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub y_title: String,
}

/// A chart's series names, labels, and values per series.
pub type ChartData = (Vec<String>, Vec<String>, Vec<Vec<f64>>);

/// Charts read at most this many rows, and this many series.
pub const CHART_ROWS: u32 = 500;
pub const CHART_SERIES: usize = 6;

#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub name: String,
    pub cells: BTreeMap<(u32, u32), Cell>,
    /// Pixels, only where they differ from the default.
    pub row_heights: BTreeMap<u32, u32>,
    pub col_widths: BTreeMap<u32, u32>,
    /// Rows and columns hidden from view (and from print); they keep their size.
    pub hidden_rows: BTreeSet<u32>,
    pub hidden_cols: BTreeSet<u32>,
    pub filter: Option<Filter>,
    /// The first row names the columns: sort and filter leave it at the top.
    pub header: bool,
    /// The first rows and columns stay in view while scrolling.
    pub freeze_rows: u32,
    pub freeze_cols: u32,
    pub chart: Option<Chart>,
    /// Blocks of cells shown as one; the top-left cell holds the content.
    pub merges: Vec<Area>,
    /// Conditional highlighting, first match wins.
    pub rules: Vec<Rule>,
    /// Charts placed on the sheet, over the cells.
    pub charts: Vec<PlacedChart>,
    /// Blocks of cells styled as tables: a heading row, banded rows, a total row.
    pub tables: Vec<TableRegion>,
    /// Cells whose value is picked from a list.
    pub lists: Vec<super::lists::ListRule>,
    /// Only these cells print.
    pub print_area: Option<Area>,
    /// Printing shrinks the sheet to fit.
    pub print_fit: Fit,
}

/// How a sheet fits on paper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Its own size, over as many pages as it takes.
    #[default]
    Actual,
    /// Every column on one page across.
    Width,
    /// The whole sheet on one page.
    Page,
}

/// A chart on the sheet: its data, and where it sits (pixels from the top
/// left of the cells, as drawn at normal size).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacedChart {
    pub range: String,
    pub kind: ChartKind,
    #[serde(flatten, default)]
    pub text: ChartText,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A table's colour scheme: (name, heading fill, band fill, Excel's style).
pub const TABLE_LOOKS: [(&str, &str, &str, &str); 5] = [
    ("Blue", "#3d6fc4", "#e8eef9", "TableStyleMedium2"),
    ("Orange", "#d9763a", "#fbeee4", "TableStyleMedium3"),
    ("Grey", "#6b7280", "#eef0f3", "TableStyleMedium4"),
    ("Gold", "#c9971c", "#fbf3dc", "TableStyleMedium5"),
    ("Green", "#3f8f5a", "#e6f3ea", "TableStyleMedium7"),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableRegion {
    pub name: String,
    /// "B2:E20", the heading row included.
    pub range: String,
    /// An index into `TABLE_LOOKS`.
    #[serde(default)]
    pub look: usize,
    #[serde(default = "yes")]
    pub header: bool,
    #[serde(default = "yes")]
    pub banded: bool,
    /// The last row totals the columns above it.
    #[serde(default)]
    pub total: bool,
}

impl TableRegion {
    pub fn area(&self) -> Option<Area> {
        Area::parse(&self.range)
    }
}

impl Sheet {
    /// The table covering (r, c), if any.
    pub fn table_at(&self, r: u32, c: u32) -> Option<&TableRegion> {
        self.tables.iter().find(|t| t.area().is_some_and(|a| a.contains(r, c)))
    }

    pub fn new(name: &str) -> Sheet {
        Sheet {
            name: name.into(),
            cells: BTreeMap::new(),
            row_heights: BTreeMap::new(),
            col_widths: BTreeMap::new(),
            hidden_rows: BTreeSet::new(),
            hidden_cols: BTreeSet::new(),
            filter: None,
            header: true,
            freeze_rows: 0,
            freeze_cols: 0,
            chart: None,
            merges: Vec::new(),
            rules: Vec::new(),
            charts: Vec::new(),
            tables: Vec::new(),
            lists: Vec::new(),
            print_area: None,
            print_fit: Fit::Actual,
        }
    }

    pub fn comment(&self, r: u32, c: u32) -> Option<&str> {
        self.cells.get(&(r, c)).and_then(|c| c.comment.as_deref())
    }

    /// The merged block covering (r, c), if any.
    pub fn merge_at(&self, r: u32, c: u32) -> Option<Area> {
        self.merges.iter().find(|m| m.contains(r, c)).copied()
    }

    /// `a` grown until it covers every merged block it touches.
    pub fn grow(&self, mut a: Area) -> Area {
        loop {
            let mut grown = a;
            for m in &self.merges {
                if m.overlaps(&grown) {
                    grown = Area {
                        top: grown.top.min(m.top),
                        left: grown.left.min(m.left),
                        bottom: grown.bottom.max(m.bottom),
                        right: grown.right.max(m.right),
                    };
                }
            }
            if grown == a {
                return a;
            }
            a = grown;
        }
    }

    pub fn raw(&self, r: u32, c: u32) -> &str {
        self.cells.get(&(r, c)).map(|c| c.raw.as_str()).unwrap_or("")
    }

    pub fn format(&self, r: u32, c: u32) -> Format {
        self.cells.get(&(r, c)).map(|c| c.format.clone()).unwrap_or_default()
    }

    /// A row's height as shown: nothing when it's hidden.
    pub fn row_h(&self, r: u32) -> u32 {
        if self.hidden_rows.contains(&r) { 0 } else { self.size_of_row(r) }
    }

    /// A column's width as shown: nothing when it's hidden.
    pub fn col_w(&self, c: u32) -> u32 {
        if self.hidden_cols.contains(&c) { 0 } else { self.size_of_col(c) }
    }

    /// A row's own height, hidden or not.
    pub fn size_of_row(&self, r: u32) -> u32 {
        self.row_heights.get(&r).copied().unwrap_or(ROW_H)
    }

    pub fn size_of_col(&self, c: u32) -> u32 {
        self.col_widths.get(&c).copied().unwrap_or(COL_W)
    }

    /// The last row and column holding anything, if any.
    pub fn used(&self) -> Option<(u32, u32)> {
        let mut last = None::<(u32, u32)>;
        for ((r, c), cell) in &self.cells {
            if cell.raw.is_empty() {
                continue;
            }
            last = Some(match last {
                Some((lr, lc)) => (lr.max(*r), lc.max(*c)),
                None => (*r, *c),
            });
        }
        last
    }

    fn put(&mut self, r: u32, c: u32, cell: Cell) {
        if cell.is_blank() {
            self.cells.remove(&(r, c));
        } else {
            self.cells.insert((r, c), cell);
        }
    }

    /// Anything only a native file can keep (CSV keeps values only).
    pub fn has_formatting(&self) -> bool {
        self.cells.values().any(|c| c.format != Format::default())
            || !self.row_heights.is_empty()
            || !self.col_widths.is_empty()
            || !self.hidden_rows.is_empty()
            || !self.hidden_cols.is_empty()
            || self.chart.is_some()
            || !self.merges.is_empty()
            || !self.rules.is_empty()
            || !self.charts.is_empty()
            || !self.tables.is_empty()
            || !self.lists.is_empty()
            || self.cells.values().any(|c| c.comment.is_some())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
    pub active: usize,
}

impl Default for Workbook {
    fn default() -> Self {
        Workbook { sheets: vec![Sheet::new("Sheet 1")], active: 0 }
    }
}

/// A rectangle of cells, inclusive: (top, left, bottom, right).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Area {
    pub top: u32,
    pub left: u32,
    pub bottom: u32,
    pub right: u32,
}

impl Area {
    pub fn new(r1: u32, c1: u32, r2: u32, c2: u32) -> Area {
        Area { top: r1.min(r2), left: c1.min(c2), bottom: r1.max(r2).min(ROWS - 1), right: c1.max(c2).min(COLS - 1) }
    }
    pub fn cell(r: u32, c: u32) -> Area {
        Area::new(r, c, r, c)
    }
    pub fn rows(&self) -> u32 {
        self.bottom - self.top + 1
    }
    pub fn cols(&self) -> u32 {
        self.right - self.left + 1
    }
    pub fn contains(&self, r: u32, c: u32) -> bool {
        (self.top..=self.bottom).contains(&r) && (self.left..=self.right).contains(&c)
    }
    pub fn is_cell(&self) -> bool {
        self.top == self.bottom && self.left == self.right
    }
    pub fn overlaps(&self, o: &Area) -> bool {
        self.top <= o.bottom && o.top <= self.bottom && self.left <= o.right && o.left <= self.right
    }
    /// "B2:D9" or "B2".
    pub fn parse(s: &str) -> Option<Area> {
        if let Some(((r1, c1), (r2, c2))) = formula::parse_range(s) {
            return Some(Area::new(r1, c1, r2, c2));
        }
        formula::parse_cell(s.trim()).map(|(r, c)| Area::new(r, c, r, c))
    }
    pub fn name(&self) -> String {
        if self.is_cell() {
            formula::cell_name(self.top, self.left)
        } else {
            format!("{}:{}", formula::cell_name(self.top, self.left), formula::cell_name(self.bottom, self.right))
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        (self.top..=self.bottom).flat_map(move |r| (self.left..=self.right).map(move |c| (r, c)))
    }
}

/// What a paste brings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paste {
    All,
    /// What the cells worked out to, without their formulas or formatting.
    Values,
    /// Only how the cells look.
    Formats,
}

/// Cells copied: their values and formats, and where they came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub from: (u32, u32),
    pub cells: Vec<Vec<Cell>>,
    /// What each cell worked out to when copied, for pasting values only.
    pub values: Vec<Vec<String>>,
    /// The text put on the system clipboard, to recognise our own copy.
    pub text: String,
}

// ---------- Values ----------

/// What a typed (non-formula) entry means.
pub fn literal(raw: &str) -> Value {
    if raw.is_empty() {
        return Value::Empty;
    }
    match raw.to_ascii_uppercase().as_str() {
        "TRUE" => return Value::Bool(true),
        "FALSE" => return Value::Bool(false),
        _ => {}
    }
    if let Some(n) = formula::parse_number(raw) {
        return Value::Num(n);
    }
    if let Some(d) = formula::parse_date(raw) {
        return Value::Num(d);
    }
    Value::Text(raw.to_string())
}

/// Every cell's value, worked out once per change.
pub struct Calc {
    values: RefCell<HashMap<(usize, u32, u32), Value>>,
    visiting: RefCell<HashSet<(usize, u32, u32)>>,
    today: f64,
    /// Per (sheet, rule): the number a "top N" or "above average" rule compares with.
    rule_marks: RefCell<HashMap<(usize, usize), Option<f64>>>,
    /// Per sheet, the cells whose formula may give a block of values.
    anchors: Vec<Vec<(u32, u32)>>,
    /// Values a block put in empty cells beside its formula.
    spilled: RefCell<HashMap<(usize, u32, u32), Value>>,
    /// Each spilling formula's block of cells.
    spills: RefCell<HashMap<(usize, u32, u32), Area>>,
    /// Every formula has been worked out, so every spill is known.
    ready: std::cell::Cell<bool>,
}

struct Ctx<'a> {
    wb: &'a Workbook,
    calc: &'a Calc,
    sheet: usize,
}

impl Cells for Ctx<'_> {
    fn get(&self, sheet: Option<&str>, r: u32, c: u32) -> Value {
        let si = match sheet {
            None => self.sheet,
            Some(name) => match self.wb.sheets.iter().position(|s| s.name.eq_ignore_ascii_case(name)) {
                Some(i) => i,
                None => return Value::Err("#REF!"),
            },
        };
        if r >= ROWS || c >= COLS {
            return Value::Err("#REF!");
        }
        self.calc.value(self.wb, si, r, c)
    }
    fn today(&self) -> f64 {
        self.calc.today
    }
}

/// Formulas nested deeper than this give up (a very long chain).
const MAX_DEPTH: usize = 400;

impl Calc {
    pub fn new(wb: &Workbook, today: f64) -> Calc {
        let anchors = wb
            .sheets
            .iter()
            .map(|s| {
                s.cells
                    .iter()
                    .filter(|(_, c)| c.raw.starts_with('=') && super::arrays::may_spill(&formula::parse(&c.raw[1..])))
                    .map(|(k, _)| *k)
                    .collect()
            })
            .collect();
        let calc = Calc {
            values: RefCell::new(HashMap::new()),
            visiting: RefCell::new(HashSet::new()),
            today,
            rule_marks: RefCell::new(HashMap::new()),
            anchors,
            spilled: RefCell::new(HashMap::new()),
            spills: RefCell::new(HashMap::new()),
            ready: std::cell::Cell::new(false),
        };
        // Work through formulas top to bottom, so a chain that refers upwards
        // (each row adding to the last) never recurses deeply.
        for (si, s) in wb.sheets.iter().enumerate() {
            for ((r, c), cell) in &s.cells {
                if cell.raw.starts_with('=') {
                    calc.value(wb, si, *r, *c);
                }
            }
        }
        calc.ready.set(true);
        calc
    }

    /// The value a block spilled into an empty cell, working out the formulas
    /// above and to the left that might reach it first.
    fn spilled_at(&self, wb: &Workbook, si: usize, r: u32, c: u32) -> Option<Value> {
        let key = (si, r, c);
        if !self.ready.get() {
            for &(ar, ac) in self.anchors.get(si).into_iter().flatten() {
                if ar <= r && ac <= c && (ar, ac) != (r, c) && !self.visiting.borrow().contains(&(si, ar, ac)) {
                    self.value(wb, si, ar, ac);
                }
            }
        }
        self.spilled.borrow().get(&key).cloned()
    }

    /// Puts a block's values in the cells below and right of (r, c), if they're
    /// all empty; returns the formula cell's own value.
    fn spill(&self, wb: &Workbook, si: usize, r: u32, c: u32, b: super::arrays::Block) -> Value {
        let s = &wb.sheets[si];
        let (bottom, right) = (r as u64 + b.rows as u64 - 1, c as u64 + b.cols as u64 - 1);
        if bottom >= ROWS as u64 || right >= COLS as u64 {
            return Value::Err("#SPILL!");
        }
        let area = Area::new(r, c, bottom as u32, right as u32);
        let blocked = (area.top..=area.bottom).flat_map(|rr| (area.left..=area.right).map(move |cc| (rr, cc))).any(|(rr, cc)| {
            (rr, cc) != (r, c)
                && (!s.raw(rr, cc).is_empty() || self.spilled.borrow().contains_key(&(si, rr, cc)) || s.merge_at(rr, cc).is_some())
        });
        if blocked {
            return Value::Err("#SPILL!");
        }
        let mut first = Value::Empty;
        let mut spilled = self.spilled.borrow_mut();
        for (i, v) in b.values.into_iter().enumerate() {
            let (rr, cc) = (r + (i / b.cols) as u32, c + (i % b.cols) as u32);
            if (rr, cc) == (r, c) {
                first = v;
            } else {
                spilled.insert((si, rr, cc), v);
            }
        }
        self.spills.borrow_mut().insert((si, r, c), area);
        first
    }

    /// The block of cells the formula at (r, c) spilled into.
    pub fn spill_area(&self, si: usize, r: u32, c: u32) -> Option<Area> {
        self.spills.borrow().get(&(si, r, c)).copied()
    }

    /// The formula whose block covers (r, c), and that block.
    pub fn spill_covering(&self, si: usize, r: u32, c: u32) -> Option<Area> {
        self.spills.borrow().iter().find(|((s, _, _), a)| *s == si && a.contains(r, c)).map(|(_, a)| *a)
    }

    /// Every spilled block on a sheet.
    pub fn spill_areas(&self, si: usize) -> Vec<Area> {
        self.spills.borrow().iter().filter(|((s, _, _), _)| *s == si).map(|(_, a)| *a).collect()
    }

    /// The last row and column any block reaches on a sheet.
    pub fn spill_extent(&self, si: usize) -> Option<(u32, u32)> {
        self.spills
            .borrow()
            .iter()
            .filter(|((s, _, _), _)| *s == si)
            .map(|(_, a)| (a.bottom, a.right))
            .reduce(|x, y| (x.0.max(y.0), x.1.max(y.1)))
    }

    pub fn value(&self, wb: &Workbook, si: usize, r: u32, c: u32) -> Value {
        let key = (si, r, c);
        if let Some(v) = self.values.borrow().get(&key) {
            return v.clone();
        }
        let Some(sheet) = wb.sheets.get(si) else { return Value::Err("#REF!") };
        let raw = sheet.raw(r, c);
        let Some(src) = raw.strip_prefix('=') else {
            if raw.is_empty()
                && let Some(v) = self.spilled_at(wb, si, r, c)
            {
                return v;
            }
            return literal(raw);
        };
        if self.visiting.borrow().contains(&key) {
            return Value::Err("#CYCLE!");
        }
        if self.visiting.borrow().len() > MAX_DEPTH {
            return Value::Err("#DEPTH!");
        }
        self.visiting.borrow_mut().insert(key);
        let expr = formula::parse(src);
        let ctx = Ctx { wb, calc: self, sheet: si };
        let v = match super::arrays::block(&expr, &ctx) {
            Some(b) if b.values.len() > 1 => self.spill(wb, si, r, c, b),
            Some(b) => b.values.into_iter().next().unwrap_or(Value::Empty),
            None => formula::eval(&expr, &ctx),
        };
        self.visiting.borrow_mut().remove(&key);
        // A value worked out inside a cycle isn't final; don't keep it.
        if self.visiting.borrow().is_empty() || !matches!(v, Value::Err("#CYCLE!")) {
            self.values.borrow_mut().insert(key, v.clone());
        }
        v
    }
}

impl Calc {
    /// The format a cell shows: its own, with the first matching highlight rule on top.
    pub fn look(&self, wb: &Workbook, si: usize, r: u32, c: u32) -> Format {
        let Some(s) = wb.sheets.get(si) else { return Format::default() };
        let mut f = s.format(r, c);
        // A table's look, under the cell's own fill and colour.
        if let Some(t) = s.table_at(r, c)
            && let Some(a) = t.area()
        {
            let (_, head, band, _) = TABLE_LOOKS[t.look.min(TABLE_LOOKS.len() - 1)];
            if t.header && r == a.top {
                f.bold = true;
                f.fill = f.fill.or(Some(head.into()));
                f.color = f.color.or(Some("#ffffff".into()));
            } else if t.total && r == a.bottom {
                f.bold = true;
                f.borders |= TOP;
            } else if t.banded && (r - a.top - t.header as u32) % 2 == 1 {
                f.fill = f.fill.or(Some(band.into()));
            }
        }
        for (i, rule) in s.rules.iter().enumerate() {
            let Some(area) = Area::parse(&rule.range) else { continue };
            if !area.contains(r, c) {
                continue;
            }
            let v = self.value(wb, si, r, c);
            let mark = || *self.rule_marks.borrow_mut().entry((si, i)).or_insert_with(|| rule.mark(wb, si, area, self));
            if rule.test.matches(&v, mark) {
                if rule.fill.is_some() {
                    f.fill = rule.fill.clone();
                }
                if rule.color.is_some() {
                    f.color = rule.color.clone();
                }
                f.bold |= rule.bold;
                break;
            }
        }
        f
    }
}

/// How a value shows in a cell with format `f`: the text, and whether it sits
/// at the right (numbers) unless the cell says otherwise.
pub fn display(v: &Value, raw: &str, f: &Format) -> (String, bool) {
    match v {
        Value::Num(n) => {
            if let Some(text) = f.pattern.as_deref().and_then(|p| super::numfmt::render(p, *n)) {
                return (text, true);
            }
            let text = match f.number {
                // A typed number or date shows as typed.
                NumFmt::General if !raw.starts_with('=') && !raw.is_empty() => raw.to_string(),
                NumFmt::General => formula::general(*n),
                NumFmt::Decimal => formula::fixed(*n, 2, true),
                NumFmt::Currency => {
                    let s = formula::fixed(*n, 2, true);
                    match s.strip_prefix('-') {
                        Some(r) => format!("-${r}"),
                        None => format!("${s}"),
                    }
                }
                NumFmt::Percent => format!("{}%", formula::fixed(n * 100.0, 1, true)),
                NumFmt::Date => formula::fmt_date(*n),
            };
            (text, true)
        }
        Value::Bool(_) => (v.text(), false),
        Value::Err(e) => ((*e).to_string(), false),
        Value::Text(t) if f.pattern.is_some() => (super::numfmt::render_text(f.pattern.as_deref().unwrap_or(""), t), false),
        _ => (v.text(), false),
    }
}

// ---------- Editing ----------

fn clean_color(c: &str) -> Option<String> {
    let c = c.trim().to_lowercase();
    (c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())).then_some(c)
}

impl Workbook {
    pub fn sheet(&self) -> &Sheet {
        &self.sheets[self.active]
    }

    pub fn sheet_mut(&mut self) -> &mut Sheet {
        &mut self.sheets[self.active]
    }

    pub fn set(&mut self, r: u32, c: u32, raw: &str) {
        if r >= ROWS || c >= COLS {
            return;
        }
        let s = self.sheet_mut();
        let mut cell = s.cells.get(&(r, c)).cloned().unwrap_or_default();
        cell.raw = raw.to_string();
        s.put(r, c, cell);
    }

    pub fn format(&mut self, area: Area, f: impl Fn(&mut Format)) {
        let s = self.sheet_mut();
        for (r, c) in area.iter() {
            let mut cell = s.cells.get(&(r, c)).cloned().unwrap_or_default();
            f(&mut cell.format);
            cell.format.fill = cell.format.fill.as_deref().and_then(clean_color);
            cell.format.color = cell.format.color.as_deref().and_then(clean_color);
            s.put(r, c, cell);
        }
    }

    /// Clear the values (formats stay, as in other spreadsheets).
    pub fn clear(&mut self, area: Area) {
        let s = self.sheet_mut();
        let keys: Vec<(u32, u32)> =
            s.cells.range((area.top, 0)..=(area.bottom, COLS)).map(|(k, _)| *k).filter(|(r, c)| area.contains(*r, *c)).collect();
        for (r, c) in keys {
            let mut cell = s.cells[&(r, c)].clone();
            cell.raw.clear();
            s.put(r, c, cell);
        }
    }

    pub fn copy(&self, area: Area) -> Clip {
        let s = self.sheet();
        let cells: Vec<Vec<Cell>> = (area.top..=area.bottom)
            .map(|r| (area.left..=area.right).map(|c| s.cells.get(&(r, c)).cloned().unwrap_or_default()).collect())
            .collect();
        let text = cells
            .iter()
            .map(|row| row.iter().map(|c| c.raw.replace(['\t', '\n'], " ")).collect::<Vec<_>>().join("\t"))
            .collect::<Vec<_>>()
            .join("\n");
        let values = cells.iter().map(|row| row.iter().map(|c| c.raw.clone()).collect()).collect();
        Clip { from: (area.top, area.left), cells, values, text }
    }

    /// A copy with what each cell works out to now, for pasting values only.
    pub fn copy_with_values(&self, area: Area, calc: &Calc) -> Clip {
        let mut clip = self.copy(area);
        clip.values = (area.top..=area.bottom)
            .map(|r| {
                (area.left..=area.right)
                    .map(|c| match calc.value(self, self.active, r, c) {
                        Value::Num(n) => format!("{n}"),
                        Value::Empty => String::new(),
                        v => v.text(),
                    })
                    .collect()
            })
            .collect();
        clip
    }

    /// Paste into `target`. A single cell takes the whole copy; a larger
    /// selection repeats the copy to fill it. Formulas move with the cells.
    pub fn paste(&mut self, target: Area, clip: &Clip) {
        self.paste_as(target, clip, Paste::All);
    }

    /// Paste all of the copy, or only its values or only its formatting.
    pub fn paste_as(&mut self, target: Area, clip: &Clip, what: Paste) {
        let (h, w) = (clip.cells.len() as u32, clip.cells.first().map(|r| r.len()).unwrap_or(0) as u32);
        if h == 0 || w == 0 {
            return;
        }
        let area = if target.is_cell() || target.rows() < h || target.cols() < w {
            Area::new(target.top, target.left, target.top + h - 1, target.left + w - 1)
        } else {
            target
        };
        let s = self.sheet_mut();
        for (r, c) in area.iter() {
            let (sr, sc) = ((r - area.top) % h, (c - area.left) % w);
            let copied = &clip.cells[sr as usize][sc as usize];
            let mut cell = s.cells.get(&(r, c)).cloned().unwrap_or_default();
            match what {
                Paste::All => {
                    let dr = r as i64 - (clip.from.0 + sr) as i64;
                    let dc = c as i64 - (clip.from.1 + sc) as i64;
                    cell = copied.clone();
                    cell.raw = formula::shift(&cell.raw, dr, dc, ROWS, COLS);
                }
                Paste::Values => {
                    cell.raw = clip.values.get(sr as usize).and_then(|v| v.get(sc as usize)).cloned().unwrap_or_default();
                }
                Paste::Formats => cell.format = copied.format.clone(),
            }
            s.put(r, c, cell);
        }
    }

    /// Paste plain text (tab-separated rows) from another app at `at`.
    pub fn paste_text(&mut self, at: (u32, u32), text: &str) -> Area {
        let rows: Vec<&str> = text.trim_end_matches(['\n', '\r']).split('\n').collect();
        let mut right = at.1;
        for (i, line) in rows.iter().enumerate() {
            for (j, field) in line.trim_end_matches('\r').split('\t').enumerate() {
                let (r, c) = (at.0 + i as u32, at.1 + j as u32);
                if r < ROWS && c < COLS {
                    self.set(r, c, field);
                    right = right.max(c);
                }
            }
        }
        Area::new(at.0, at.1, at.0 + rows.len().saturating_sub(1) as u32, right)
    }

    /// Drag-fill: repeat `src` towards `to` (down, up, left or right, whichever
    /// is further), moving formulas and copying formats.
    pub fn fill(&mut self, src: Area, to: (u32, u32)) -> Area {
        let (tr, tc) = to;
        let vd = if tr < src.top { src.top - tr } else { tr.saturating_sub(src.bottom) };
        let hd = if tc < src.left { src.left - tc } else { tc.saturating_sub(src.right) };
        if vd == 0 && hd == 0 {
            return src;
        }
        let area = if vd >= hd {
            Area::new(src.top.min(tr), src.left, src.bottom.max(tr), src.right)
        } else {
            Area::new(src.top, src.left.min(tc), src.bottom, src.right.max(tc))
        };
        let clip = self.copy(src);
        let (h, w) = (src.rows() as i64, src.cols() as i64);
        let down = vd >= hd;
        let s = self.sheet_mut();
        for (r, c) in area.iter() {
            if src.contains(r, c) {
                continue;
            }
            let sr = (r as i64 - src.top as i64).rem_euclid(h);
            let sc = (c as i64 - src.left as i64).rem_euclid(w);
            let mut cell = clip.cells[sr as usize][sc as usize].clone();
            // A series carries on (1, 2 → 3; Jan → Feb); anything else is copied.
            let (line, t): (Vec<&str>, i64) = if down {
                (clip.cells.iter().map(|row| row[sc as usize].raw.as_str()).collect(), r as i64 - src.top as i64)
            } else {
                (clip.cells[sr as usize].iter().map(|cl| cl.raw.as_str()).collect(), c as i64 - src.left as i64)
            };
            match super::series::next(&line, t) {
                Some(v) => cell.raw = v,
                None => {
                    cell.raw =
                        formula::shift(&cell.raw, r as i64 - (src.top as i64 + sr), c as i64 - (src.left as i64 + sc), ROWS, COLS)
                }
            }
            s.put(r, c, cell);
        }
        area
    }

    /// Cut and paste: move `src` so its top-left is at `to`. Formulas inside keep pointing where they did.
    pub fn move_area(&mut self, src: Area, to: (u32, u32)) -> Option<Area> {
        let dest = Area::new(to.0, to.1, to.0 + src.rows() - 1, to.1 + src.cols() - 1);
        if dest.bottom != to.0 + src.rows() - 1 || dest.right != to.1 + src.cols() - 1 || (to.0, to.1) == (src.top, src.left) {
            return None;
        }
        let clip = self.copy(src);
        let s = self.sheet_mut();
        for (r, c) in src.iter() {
            s.cells.remove(&(r, c));
        }
        for (r, c) in dest.iter() {
            let cell = clip.cells[(r - dest.top) as usize][(c - dest.left) as usize].clone();
            s.put(r, c, cell);
        }
        Some(dest)
    }

    /// Set a row's height; a hidden row dragged open shows again.
    pub fn set_row_h(&mut self, r: u32, h: u32) {
        let h = h.clamp(20, 400);
        let s = self.sheet_mut();
        s.hidden_rows.remove(&r);
        if h == ROW_H {
            s.row_heights.remove(&r);
        } else {
            s.row_heights.insert(r, h);
        }
    }

    pub fn set_col_w(&mut self, c: u32, w: u32) {
        let w = w.clamp(36, 800);
        let s = self.sheet_mut();
        s.hidden_cols.remove(&c);
        if w == COL_W {
            s.col_widths.remove(&c);
        } else {
            s.col_widths.insert(c, w);
        }
    }

    /// Hide or show rows `top..=bottom` of the active sheet.
    pub fn hide_rows(&mut self, top: u32, bottom: u32, hide: bool) {
        let s = self.sheet_mut();
        for r in top..=bottom.min(ROWS - 1) {
            if hide { s.hidden_rows.insert(r) } else { s.hidden_rows.remove(&r) };
        }
    }

    /// Hide or show columns `left..=right` of the active sheet.
    pub fn hide_cols(&mut self, left: u32, right: u32, hide: bool) {
        let s = self.sheet_mut();
        for c in left..=right.min(COLS - 1) {
            if hide { s.hidden_cols.insert(c) } else { s.hidden_cols.remove(&c) };
        }
    }

    /// Put lines around cells: `kind` is all, outside, top, bottom or none.
    pub fn set_borders(&mut self, area: Area, kind: Borders) {
        let s = self.sheet_mut();
        for (r, c) in area.iter() {
            let mut cell = s.cells.get(&(r, c)).cloned().unwrap_or_default();
            let edges = (if r == area.top { TOP } else { 0 })
                | (if r == area.bottom { BOTTOM } else { 0 })
                | (if c == area.left { LEFT } else { 0 })
                | (if c == area.right { RIGHT } else { 0 });
            cell.format.borders = match kind {
                Borders::All => TOP | BOTTOM | LEFT | RIGHT,
                Borders::Outside => edges,
                Borders::Top => cell.format.borders | (edges & TOP),
                Borders::Bottom => cell.format.borders | (edges & BOTTOM),
                Borders::None => 0,
            };
            s.put(r, c, cell);
        }
    }

    /// Tick or untick the checkbox cells in `area` (all the same way: ticked
    /// unless they all already are).
    pub fn toggle_checks(&mut self, area: Area, calc: &Calc) {
        let si = self.active;
        let cells: Vec<(u32, u32)> = area
            .iter()
            .filter(|(r, c)| self.sheet().format(*r, *c).checkbox && !self.sheet().raw(*r, *c).starts_with('='))
            .collect();
        let all_on = cells.iter().all(|(r, c)| calc.value(self, si, *r, *c).truthy().unwrap_or(false));
        for (r, c) in cells {
            self.set(r, c, if all_on { "FALSE" } else { "TRUE" });
        }
    }

    // ---------- Merges and comments ----------

    /// Merge `area` into one cell. Merges it overlaps are replaced; content
    /// outside the top-left cell is cleared. Returns whether any was.
    pub fn merge(&mut self, area: Area) -> bool {
        if area.is_cell() {
            return false;
        }
        let s = self.sheet_mut();
        s.merges.retain(|m| !m.overlaps(&area));
        let mut lost = false;
        for (r, c) in area.iter() {
            if (r, c) == (area.top, area.left) {
                continue;
            }
            if let Some(cell) = s.cells.get(&(r, c)).cloned() {
                lost |= !cell.raw.is_empty();
                s.put(r, c, Cell { raw: String::new(), comment: None, ..cell });
            }
        }
        s.merges.push(area);
        lost
    }

    /// Split every merged block that `area` touches.
    pub fn unmerge(&mut self, area: Area) -> bool {
        let s = self.sheet_mut();
        let before = s.merges.len();
        s.merges.retain(|m| !m.overlaps(&area));
        s.merges.len() != before
    }

    pub fn set_comment(&mut self, r: u32, c: u32, text: &str) {
        if r >= ROWS || c >= COLS {
            return;
        }
        let s = self.sheet_mut();
        let mut cell = s.cells.get(&(r, c)).cloned().unwrap_or_default();
        let t = text.trim();
        cell.comment = (!t.is_empty()).then(|| t.to_string());
        s.put(r, c, cell);
    }

    // ---------- Sheets ----------

    fn name_taken(&self, name: &str, except: Option<usize>) -> bool {
        self.sheets.iter().enumerate().any(|(i, s)| Some(i) != except && s.name.eq_ignore_ascii_case(name))
    }

    pub fn add_sheet(&mut self) -> bool {
        if self.sheets.len() >= MAX_SHEETS {
            return false;
        }
        let mut n = self.sheets.len() + 1;
        while self.name_taken(&format!("Sheet {n}"), None) {
            n += 1;
        }
        self.sheets.push(Sheet::new(&format!("Sheet {n}")));
        self.active = self.sheets.len() - 1;
        true
    }

    pub fn duplicate_sheet(&mut self, i: usize) -> bool {
        if self.sheets.len() >= MAX_SHEETS || i >= self.sheets.len() {
            return false;
        }
        let mut s = self.sheets[i].clone();
        let base = format!("{} copy", s.name);
        let mut name = base.clone();
        let mut n = 2;
        while self.name_taken(&name, None) {
            name = format!("{base} {n}");
            n += 1;
        }
        s.name = name;
        self.sheets.insert(i + 1, s);
        self.active = i + 1;
        true
    }

    /// Ok, or why the name can't be used.
    pub fn rename_sheet(&mut self, i: usize, name: &str) -> Result<(), &'static str> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 31 || name.contains(['[', ']', '*', '?', '/', '\\', ':']) {
            return Err("Use up to 31 characters, without : \\ / ? * [ ].");
        }
        if self.name_taken(name, Some(i)) {
            return Err("Another sheet already has that name.");
        }
        if let Some(s) = self.sheets.get_mut(i) {
            s.name = name.to_string();
        }
        Ok(())
    }

    pub fn remove_sheet(&mut self, i: usize) -> bool {
        if self.sheets.len() <= 1 || i >= self.sheets.len() {
            return false;
        }
        self.sheets.remove(i);
        if self.active > i || self.active >= self.sheets.len() {
            self.active = self.active.saturating_sub(1);
        }
        true
    }

    pub fn move_sheet(&mut self, i: usize, by: i64) -> bool {
        let j = i as i64 + by;
        if i >= self.sheets.len() || j < 0 || j as usize >= self.sheets.len() {
            return false;
        }
        self.sheets.swap(i, j as usize);
        if self.active == i {
            self.active = j as usize;
        } else if self.active == j as usize {
            self.active = i;
        }
        true
    }

    /// Take sheet `from` out and put it back at `to`; the active sheet stays the same sheet.
    pub fn move_sheet_to(&mut self, from: usize, to: usize) -> bool {
        let n = self.sheets.len();
        if from >= n || to >= n || from == to {
            return false;
        }
        let active = self.sheets[self.active].name.clone();
        let s = self.sheets.remove(from);
        self.sheets.insert(to, s);
        self.active = self.sheets.iter().position(|s| s.name == active).unwrap_or(0);
        true
    }

    // ---------- Sort and filter ----------

    /// The rows on screen, in order: all of them, or with a filter, the header
    /// row and the matching rows (then the empty rows after the data).
    pub fn view_rows(&self, si: usize, calc: &Calc) -> Vec<u32> {
        let s = &self.sheets[si];
        let Some(f) = s.filter.as_ref().filter(|f| f.active()) else {
            return (0..ROWS).collect();
        };
        let last = s.used().map(|(r, _)| r).unwrap_or(0);
        let first = if s.header { 1 } else { 0 };
        let mut out: Vec<u32> = (0..first).collect();
        out.extend((first..=last).filter(|r| f.keeps(&self.shown(si, *r, f.col, calc))));
        out.extend(last + 1..ROWS);
        out
    }

    /// Sort the rows of the data by column `col`. With a header row, row 1 stays
    /// on top. Empty cells go last; numbers before text. Formulas move with their rows.
    ///
    /// Merged cells can't be moved row by row, so a sheet with merges in its
    /// data isn't sorted: returns false.
    pub fn sort_rows(&mut self, si: usize, col: u32, ascending: bool, calc: &Calc) -> bool {
        self.sort_by(si, &[(col, ascending)], calc)
    }

    /// Sort by several columns: by the first, then the next where those are equal.
    pub fn sort_by(&mut self, si: usize, keys: &[(u32, bool)], calc: &Calc) -> bool {
        let s = &self.sheets[si];
        let Some((last, _)) = s.used() else { return true };
        let first = if s.header { 1 } else { 0 };
        if last <= first {
            return true;
        }
        if s.merges.iter().any(|m| m.bottom >= first && m.top <= last) {
            return false;
        }
        let mut keyed: Vec<(u32, Vec<Value>)> =
            (first..=last).map(|r| (r, keys.iter().map(|(c, _)| calc.value(self, si, r, *c)).collect())).collect();
        keyed.sort_by(|(_, a), (_, b)| {
            use std::cmp::Ordering;
            for (k, (x, y)) in a.iter().zip(b).enumerate() {
                let ascending = keys[k].1;
                let ord = match (x, y) {
                    (Value::Empty, Value::Empty) => Ordering::Equal,
                    // Empty cells go last either way.
                    (Value::Empty, _) => return Ordering::Greater,
                    (_, Value::Empty) => return Ordering::Less,
                    (Value::Num(p), Value::Num(q)) => p.partial_cmp(q).unwrap_or(Ordering::Equal),
                    (Value::Num(_), _) => Ordering::Less,
                    (_, Value::Num(_)) => Ordering::Greater,
                    _ => x.text().to_lowercase().cmp(&y.text().to_lowercase()),
                };
                let ord = if ascending { ord } else { ord.reverse() };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            Ordering::Equal
        });
        let s = &mut self.sheets[si];
        let old: BTreeMap<(u32, u32), Cell> = s.cells.range((first, 0)..(last + 1, 0)).map(|(k, v)| (*k, v.clone())).collect();
        let heights: BTreeMap<u32, u32> = s.row_heights.range(first..=last).map(|(k, v)| (*k, *v)).collect();
        let hidden: Vec<u32> = s.hidden_rows.range(first..=last).copied().collect();
        for k in old.keys() {
            s.cells.remove(k);
        }
        for r in first..=last {
            s.row_heights.remove(&r);
            s.hidden_rows.remove(&r);
        }
        for (new_r, (old_r, _)) in (first..).zip(keyed) {
            for ((_, c), cell) in old.range((old_r, 0)..(old_r + 1, 0)) {
                let mut cell = cell.clone();
                cell.raw = formula::shift(&cell.raw, new_r as i64 - old_r as i64, 0, ROWS, COLS);
                s.put(new_r, *c, cell);
            }
            if let Some(h) = heights.get(&old_r) {
                s.row_heights.insert(new_r, *h);
            }
            // A hidden row stays hidden where it goes.
            if hidden.contains(&old_r) {
                s.hidden_rows.insert(new_r);
            }
        }
        true
    }

    // ---------- Chart ----------

    /// The chart's labels and series: (series names, labels, values per series).
    pub fn chart_data(&self, si: usize, calc: &Calc) -> Option<ChartData> {
        let range = self.sheets[si].chart.as_ref()?.range.clone();
        self.chart_data_for(si, &range, calc)
    }

    /// A chart's data from any range of a sheet (the side chart's, or one placed on the sheet).
    pub fn chart_data_for(&self, si: usize, range: &str, calc: &Calc) -> Option<ChartData> {
        let ((r1, c1), (r2, c2)) = formula::parse_range(range)?;
        let r2 = r2.min(r1 + CHART_ROWS - 1).min(ROWS - 1);
        let label_col = (c2 > c1).then_some(c1);
        let first_value = if label_col.is_some() { c1 + 1 } else { c1 };
        let value_cols: Vec<u32> = (first_value..=c2.min(COLS - 1)).take(CHART_SERIES).collect();
        let value = |r: u32, c: u32| calc.value(self, si, r, c);
        // A first row of words names the series.
        let named = value_cols.iter().all(|c| matches!(value(r1, *c), Value::Text(_))) && r2 > r1;
        let start = if named { r1 + 1 } else { r1 };
        let names: Vec<String> =
            value_cols.iter().map(|c| if named { value(r1, *c).text() } else { formula::col_name(*c) }).collect();
        let mut labels = Vec::new();
        let mut series = vec![Vec::new(); value_cols.len()];
        for r in start..=r2 {
            let nums: Vec<Option<f64>> =
                value_cols.iter().map(|c| if let Value::Num(n) = value(r, *c) { Some(n) } else { None }).collect();
            if nums.iter().all(Option::is_none) {
                continue;
            }
            labels.push(label_col.map(|c| value(r, c).text()).filter(|t| !t.is_empty()).unwrap_or_else(|| (r + 1).to_string()));
            for (i, n) in nums.into_iter().enumerate() {
                series[i].push(n.unwrap_or(0.0));
            }
        }
        (!labels.is_empty()).then_some((names, labels, series))
    }
}

// ---------- Files ----------

#[derive(Serialize, Deserialize)]
struct CellFile {
    r: u32,
    c: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    raw: String,
    #[serde(flatten)]
    format: Format,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SheetFile {
    name: String,
    cells: Vec<CellFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    row_heights: Vec<(u32, u32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    col_widths: Vec<(u32, u32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hidden_rows: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hidden_cols: Vec<u32>,
    #[serde(default)]
    filter: Option<Filter>,
    #[serde(default = "yes")]
    header: bool,
    /// Files before frozen panes: the first row only.
    #[serde(default, skip_serializing)]
    freeze: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    freeze_rows: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    freeze_cols: u32,
    #[serde(default)]
    chart: Option<Chart>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    merges: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    charts: Vec<PlacedChart>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tables: Vec<TableRegion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lists: Vec<super::lists::ListRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    print_area: Option<String>,
    #[serde(default, skip_serializing_if = "is_actual")]
    print_fit: Fit,
}

fn is_actual(f: &Fit) -> bool {
    *f == Fit::Actual
}

fn yes() -> bool {
    true
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// At most this many rows or columns stay in view.
pub const MAX_FROZEN: u32 = 50;

#[derive(Serialize, Deserialize)]
struct WorkbookFile {
    active: usize,
    sheets: Vec<SheetFile>,
}

impl Workbook {
    pub fn to_json(&self) -> Vec<u8> {
        let file = WorkbookFile {
            active: self.active,
            sheets: self
                .sheets
                .iter()
                .map(|s| SheetFile {
                    name: s.name.clone(),
                    cells: s
                        .cells
                        .iter()
                        .map(|((r, c), cell)| CellFile {
                            r: *r,
                            c: *c,
                            raw: cell.raw.clone(),
                            format: cell.format.clone(),
                            comment: cell.comment.clone(),
                        })
                        .collect(),
                    row_heights: s.row_heights.iter().map(|(k, v)| (*k, *v)).collect(),
                    col_widths: s.col_widths.iter().map(|(k, v)| (*k, *v)).collect(),
                    hidden_rows: s.hidden_rows.iter().copied().collect(),
                    hidden_cols: s.hidden_cols.iter().copied().collect(),
                    filter: s.filter.clone(),
                    header: s.header,
                    freeze: false,
                    freeze_rows: s.freeze_rows,
                    freeze_cols: s.freeze_cols,
                    chart: s.chart.clone(),
                    merges: s.merges.iter().map(Area::name).collect(),
                    rules: s.rules.clone(),
                    charts: s.charts.clone(),
                    tables: s.tables.clone(),
                    lists: s.lists.clone(),
                    print_area: s.print_area.map(|a| a.name()),
                    print_fit: s.print_fit,
                })
                .collect(),
        };
        serde_json::to_vec(&file).expect("a workbook always serialises")
    }

    pub fn from_json(bytes: &[u8]) -> anyhow::Result<Workbook> {
        let f: WorkbookFile = serde_json::from_slice(bytes)?;
        let mut wb = Workbook { sheets: Vec::new(), active: 0 };
        for sf in f.sheets.into_iter().take(MAX_SHEETS) {
            let mut s = Sheet::new(&sf.name);
            for cf in sf.cells {
                if cf.r < ROWS && cf.c < COLS {
                    s.put(cf.r, cf.c, Cell { raw: cf.raw, format: cf.format, comment: cf.comment });
                }
            }
            s.row_heights = sf.row_heights.into_iter().filter(|(r, _)| *r < ROWS).collect();
            s.col_widths = sf.col_widths.into_iter().filter(|(c, _)| *c < COLS).collect();
            s.hidden_rows = sf.hidden_rows.into_iter().filter(|r| *r < ROWS).collect();
            s.hidden_cols = sf.hidden_cols.into_iter().filter(|c| *c < COLS).collect();
            s.filter = sf.filter.filter(|x| x.col < COLS);
            s.header = sf.header;
            s.freeze_rows = sf.freeze_rows.max(sf.freeze as u32).min(MAX_FROZEN);
            s.freeze_cols = sf.freeze_cols.min(MAX_FROZEN);
            s.chart = sf.chart;
            s.merges = sf.merges.iter().filter_map(|m| Area::parse(m)).filter(|a| !a.is_cell()).collect();
            s.rules = sf.rules;
            s.charts = sf.charts;
            s.tables = sf.tables;
            s.lists = sf.lists.into_iter().filter(|l| Area::parse(&l.range).is_some()).collect();
            s.print_area = sf.print_area.as_deref().and_then(Area::parse);
            s.print_fit = sf.print_fit;
            wb.sheets.push(s);
        }
        if wb.sheets.is_empty() {
            wb.sheets.push(Sheet::new("Sheet 1"));
        }
        wb.active = f.active.min(wb.sheets.len() - 1);
        Ok(wb)
    }

    /// A `.nsheets` file from the Qt version (JSON, format version 1 or 2).
    pub fn from_legacy_json(bytes: &[u8]) -> anyhow::Result<Workbook> {
        let v: serde_json::Value = serde_json::from_slice(bytes)?;
        if v.get("format").and_then(|f| f.as_str()) != Some("nexus-sheets") {
            anyhow::bail!("it isn't a Nexus Sheets file");
        }
        let one = |o: &serde_json::Value| -> (Sheet, Option<Sort>) {
            let get_i = |k: &str, d: i64| o.get(k).and_then(|x| x.as_i64()).unwrap_or(d);
            let mut s = Sheet::new(o.get("name").and_then(|x| x.as_str()).unwrap_or("Sheet 1"));
            for cell in o.get("cells").and_then(|c| c.as_array()).into_iter().flatten() {
                let gi = |k: &str| cell.get(k).and_then(|x| x.as_i64()).unwrap_or(-1);
                let gs = |k: &str| cell.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                let (r, c) = (gi("row"), gi("column"));
                if r < 0 || c < 0 || r >= ROWS as i64 || c >= COLS as i64 {
                    continue;
                }
                let format = Format {
                    number: match gi("numberFormat") {
                        1 => NumFmt::Decimal,
                        2 => NumFmt::Currency,
                        3 => NumFmt::Percent,
                        _ => NumFmt::General,
                    },
                    bold: cell.get("bold").and_then(|x| x.as_bool()).unwrap_or(false),
                    italic: false,
                    align: match gi("alignment") {
                        1 => Some(HAlign::Center),
                        2 => Some(HAlign::Right),
                        _ => None,
                    },
                    fill: clean_color(&gs("fillColor")),
                    color: clean_color(&gs("textColor")),
                    ..Default::default()
                };
                s.put(r as u32, c as u32, Cell { raw: gs("raw"), format, comment: None });
            }
            let pairs = |k: &str| -> BTreeMap<u32, u32> {
                o.get(k)
                    .and_then(|x| x.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|p| Some((p.get(0)?.as_u64()? as u32, p.get(1)?.as_u64()? as u32)))
                    .collect()
            };
            s.row_heights = pairs("rowHeights");
            s.col_widths = pairs("columnWidths");
            // The Qt version kept a sort as a view; it's applied once when opened.
            let sc = get_i("sortColumn", -1);
            let sort = (sc >= 0)
                .then(|| Sort { col: sc as u32, ascending: o.get("sortAscending").and_then(|x| x.as_bool()).unwrap_or(true) });
            let fc = get_i("filterColumn", -1);
            let fq = o.get("filterQuery").and_then(|x| x.as_str()).unwrap_or("");
            if fc >= 0 && !fq.is_empty() {
                s.filter = Some(Filter { col: fc as u32, query: fq.to_string(), values: None });
            }
            let range = o.get("chartRange").and_then(|x| x.as_str()).unwrap_or("");
            if !range.is_empty() {
                let kind = match o.get("chartType").and_then(|x| x.as_str()).unwrap_or("bar") {
                    "line" => ChartKind::Line,
                    "area" => ChartKind::Area,
                    "pie" => ChartKind::Pie,
                    _ => ChartKind::Bar,
                };
                s.chart = Some(Chart { range: range.to_uppercase(), kind, text: ChartText::default() });
            }
            (s, sort)
        };
        let (parts, active): (Vec<(Sheet, Option<Sort>)>, usize) = match v.get("version").and_then(|x| x.as_i64()) {
            Some(1) => (vec![one(&v)], 0),
            Some(2) => {
                let parts: Vec<(Sheet, Option<Sort>)> =
                    v.get("sheets").and_then(|s| s.as_array()).into_iter().flatten().map(one).collect();
                if parts.is_empty() {
                    anyhow::bail!("the workbook has no sheets");
                }
                let active = (v.get("activeSheet").and_then(|x| x.as_u64()).unwrap_or(0) as usize).min(parts.len() - 1);
                (parts, active)
            }
            _ => anyhow::bail!("it was made by a newer version of Nexus Sheets"),
        };
        let sorts: Vec<Option<Sort>> = parts.iter().map(|(_, s)| *s).collect();
        let mut wb = Workbook { sheets: parts.into_iter().map(|(s, _)| s).collect(), active };
        for (i, sort) in sorts.into_iter().enumerate() {
            if let Some(sort) = sort {
                let calc = Calc::new(&wb, 0.0);
                wb.sort_rows(i, sort.col, sort.ascending, &calc);
            }
        }
        Ok(wb)
    }
}

// ---------- CSV ----------

pub fn parse_csv(text: &str) -> Option<Vec<Vec<String>>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let (mut quoted, mut after_quote) = (false, false);
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                    after_quote = true;
                }
            } else {
                field.push(ch);
            }
        } else if ch == '"' && field.is_empty() && !after_quote {
            quoted = true;
        } else if ch == ',' || ch == '\n' || ch == '\r' {
            row.push(std::mem::take(&mut field));
            after_quote = false;
            if ch != ',' {
                rows.push(std::mem::take(&mut row));
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
        } else {
            if after_quote {
                return None;
            }
            field.push(ch);
        }
    }
    if quoted {
        return None;
    }
    if !field.is_empty() || !row.is_empty() || after_quote {
        row.push(field);
        rows.push(row);
    }
    Some(rows)
}

fn csv_field(f: &str) -> String {
    if f.contains([',', '"', '\n', '\r']) { format!("\"{}\"", f.replace('"', "\"\"")) } else { f.to_string() }
}

impl Workbook {
    pub fn from_csv(text: &str) -> anyhow::Result<Workbook> {
        let rows = parse_csv(text).ok_or_else(|| anyhow::anyhow!("the CSV is damaged (a quote is never closed)"))?;
        if rows.len() > ROWS as usize {
            anyhow::bail!("it has more than {} rows", crate::fmt::thousands(ROWS as usize));
        }
        let mut wb = Workbook::default();
        wb.sheets[0].header = true;
        for (r, row) in rows.iter().enumerate() {
            if row.len() > COLS as usize {
                anyhow::bail!("it has more than {COLS} columns");
            }
            for (c, v) in row.iter().enumerate() {
                if !v.is_empty() {
                    wb.set(r as u32, c as u32, v);
                }
            }
        }
        Ok(wb)
    }

    /// The active sheet as CSV: what was typed, formulas included.
    pub fn to_csv(&self) -> String {
        let s = self.sheet();
        let Some((lr, lc)) = s.used() else { return String::new() };
        let mut out = String::new();
        for r in 0..=lr {
            let line: Vec<String> = (0..=lc).map(|c| csv_field(s.raw(r, c))).collect();
            out.push_str(&line.join(","));
            out.push_str("\r\n");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calc(wb: &Workbook) -> Calc {
        Calc::new(wb, formula::date_serial(2026, 10, 5))
    }

    #[test]
    fn blocks_spill_into_empty_cells() {
        let mut wb = Workbook::default();
        for (r, v) in ["pear", "apple", "pear", "fig"].iter().enumerate() {
            wb.set(r as u32 + 1, 0, v);
        }
        // Read before its formula comes up (it's below), then the formula itself.
        wb.set(0, 3, "=COUNTA(C2:C9)&\" \"&C3");
        wb.set(1, 2, "=SORT(UNIQUE(A2:A5))");
        let c = calc(&wb);
        let v = |r, col| c.value(&wb, 0, r, col).text();
        assert_eq!((v(1, 2), v(2, 2), v(3, 2), v(4, 2)), ("apple".into(), "fig".into(), "pear".into(), String::new()));
        assert_eq!(v(0, 3), "3 fig");
        assert_eq!(c.spill_area(0, 1, 2), Some(Area::new(1, 2, 3, 2)));
        assert_eq!(c.spill_covering(0, 3, 2), Some(Area::new(1, 2, 3, 2)));
        // Something typed in the way stops it.
        wb.set(3, 2, "x");
        let c = calc(&wb);
        assert_eq!(c.value(&wb, 0, 1, 2), Value::Err("#SPILL!"));
        assert_eq!(c.value(&wb, 0, 2, 2), Value::Empty);
    }

    fn shown(wb: &Workbook, r: u32, c: u32) -> String {
        let s = wb.sheet();
        display(&calc(wb).value(wb, wb.active, r, c), s.raw(r, c), &s.format(r, c)).0
    }

    #[test]
    fn values_and_formats() {
        let mut wb = Workbook::default();
        wb.set(0, 0, "1,200");
        wb.set(1, 0, "=A1*2");
        wb.set(2, 0, "=A3");
        wb.set(3, 0, "2026-10-05");
        wb.set(4, 0, "=A4+1");
        assert_eq!(shown(&wb, 0, 0), "1,200");
        assert_eq!(shown(&wb, 1, 0), "2400");
        assert_eq!(shown(&wb, 2, 0), "#CYCLE!");
        wb.format(Area::cell(1, 0), |f| f.number = NumFmt::Currency);
        assert_eq!(shown(&wb, 1, 0), "$2,400.00");
        wb.format(Area::cell(4, 0), |f| f.number = NumFmt::Date);
        assert_eq!(shown(&wb, 4, 0), "2026-10-06");
        wb.format(Area::cell(0, 0), |f| f.number = NumFmt::Percent);
        assert_eq!(shown(&wb, 0, 0), "120,000.0%");
    }

    #[test]
    fn long_chains_dont_overflow() {
        let mut wb = Workbook::default();
        wb.set(0, 0, "1");
        for r in 1..5000 {
            wb.set(r, 0, &format!("=A{}+1", r));
        }
        assert_eq!(shown(&wb, 4999, 0), "5000");
    }

    #[test]
    fn cross_sheet_references() {
        let mut wb = Workbook::default();
        wb.set(0, 0, "7");
        wb.add_sheet();
        wb.set(0, 0, "='Sheet 1'!A1*2");
        assert_eq!(shown(&wb, 0, 0), "14");
        wb.rename_sheet(0, "Data").unwrap();
        assert_eq!(shown(&wb, 0, 0), "#REF!");
    }

    #[test]
    fn copy_paste_and_fill_move_formulas() {
        let mut wb = Workbook::default();
        wb.set(0, 0, "1");
        wb.set(0, 1, "=A1*10");
        wb.format(Area::cell(0, 1), |f| f.bold = true);
        let clip = wb.copy(Area::cell(0, 1));
        wb.paste(Area::cell(2, 1), &clip);
        assert_eq!(wb.sheet().raw(2, 1), "=A3*10");
        assert!(wb.sheet().format(2, 1).bold);
        // A larger selection repeats the copy.
        wb.paste(Area::new(5, 1, 7, 1), &clip);
        assert_eq!(wb.sheet().raw(7, 1), "=A8*10");
        // Values only: the number, not the formula, and the target's own look.
        let calc = Calc::new(&wb, 0.0);
        let clip = wb.copy_with_values(Area::cell(0, 1), &calc);
        wb.paste_as(Area::cell(20, 3), &clip, Paste::Values);
        assert_eq!(wb.sheet().raw(20, 3), "10");
        assert!(!wb.sheet().format(20, 3).bold);
        // Formatting only: the look, not the contents.
        wb.set(21, 3, "keep");
        wb.paste_as(Area::cell(21, 3), &clip, Paste::Formats);
        assert_eq!(wb.sheet().raw(21, 3), "keep");
        assert!(wb.sheet().format(21, 3).bold);
        // Fill down.
        wb.set(10, 0, "=B1");
        wb.fill(Area::cell(10, 0), (13, 0));
        assert_eq!(wb.sheet().raw(13, 0), "=B4");
        // Moving keeps the formula text.
        let dest = wb.move_area(Area::cell(10, 0), (20, 2)).unwrap();
        assert_eq!(dest, Area::cell(20, 2));
        assert_eq!(wb.sheet().raw(20, 2), "=B1");
        assert_eq!(wb.sheet().raw(10, 0), "");
        // Text from another app.
        let a = wb.paste_text((30, 0), "a\tb\nc\td\n");
        assert_eq!(a, Area::new(30, 0, 31, 1));
        assert_eq!(wb.sheet().raw(31, 1), "d");
    }

    #[test]
    fn sort_and_filter_keep_the_header() {
        let mut wb = Workbook::default();
        for (r, (name, n)) in [("Item", "Qty"), ("pear", "3"), ("apple", "10"), ("fig", "")].iter().enumerate() {
            wb.set(r as u32, 0, name);
            wb.set(r as u32, 1, n);
        }
        wb.set(1, 2, "=B2*2");
        let c = calc(&wb);
        wb.sort_rows(0, 1, false, &c);
        let col: Vec<&str> = (0..4).map(|r| wb.sheet().raw(r, 0)).collect();
        assert_eq!(col, vec!["Item", "apple", "pear", "fig"]);
        // The formula moved with its row and still points at it.
        assert_eq!(wb.sheet().raw(2, 2), "=B3*2");
        wb.sheet_mut().filter = Some(Filter { col: 0, query: "P".into(), values: None });
        let rows = wb.view_rows(0, &calc(&wb));
        assert_eq!(&rows[..3], &[0, 1, 2]);
        assert_eq!(rows[3], 4);
    }

    #[test]
    fn sheets_and_names() {
        let mut wb = Workbook::default();
        assert!(wb.add_sheet());
        assert_eq!(wb.sheets[1].name, "Sheet 2");
        assert!(wb.duplicate_sheet(1));
        assert_eq!(wb.sheets[2].name, "Sheet 2 copy");
        assert!(wb.rename_sheet(0, "sheet 2").is_err());
        assert!(wb.rename_sheet(0, "a/b").is_err());
        assert!(wb.remove_sheet(2));
        assert_eq!(wb.active, 1);
        assert!(wb.move_sheet(1, -1));
        assert_eq!(wb.sheets[0].name, "Sheet 2");
        assert_eq!(wb.active, 0);
        assert!(wb.add_sheet());
        wb.active = 0;
        assert!(wb.move_sheet_to(0, 2));
        let names: Vec<_> = wb.sheets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Sheet 1", "Sheet 3", "Sheet 2"]);
        assert_eq!(wb.active, 2);
        assert!(!wb.move_sheet_to(1, 1));
    }

    #[test]
    fn chart_reads_series() {
        let mut wb = Workbook::default();
        for (r, row) in [["Month", "Rent", "Food"], ["Jan", "900", "300"], ["Feb", "900", "350"]].iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                wb.set(r as u32, c as u32, v);
            }
        }
        wb.sheet_mut().chart = Some(Chart { range: "A1:C3".into(), kind: ChartKind::Line, text: ChartText::default() });
        let (names, labels, series) = wb.chart_data(0, &calc(&wb)).unwrap();
        assert_eq!(names, vec!["Rent", "Food"]);
        assert_eq!(labels, vec!["Jan", "Feb"]);
        assert_eq!(series[1], vec![300.0, 350.0]);
    }

    #[test]
    fn files_round_trip() {
        let mut wb = Workbook::default();
        wb.set(0, 0, "=SUM(B1:B3)");
        wb.format(Area::cell(0, 0), |f| {
            f.bold = true;
            f.fill = Some("#FFEEDD".into());
        });
        wb.set_col_w(2, 200);
        wb.sheet_mut().freeze_rows = 1;
        wb.add_sheet();
        let back = Workbook::from_json(&wb.to_json()).unwrap();
        assert_eq!(back, wb);
        assert_eq!(back.sheets[0].format(0, 0).fill.as_deref(), Some("#ffeedd"));

        let csv = "a,\"b, c\",\"say \"\"hi\"\"\"\r\n1,2,=A2+B2\r\n";
        let wb = Workbook::from_csv(csv).unwrap();
        assert_eq!(wb.sheet().raw(0, 1), "b, c");
        assert_eq!(wb.sheet().raw(0, 2), "say \"hi\"");
        assert_eq!(wb.to_csv(), csv);
        assert!(parse_csv("\"open").is_none());
    }

    #[test]
    fn reads_qt_files() {
        let v1 = br##"{"format":"nexus-sheets","version":1,"name":"Budget","cells":[{"row":0,"column":0,"raw":"Rent","bold":true,"numberFormat":0,"alignment":1,"fillColor":"#ff0000","textColor":""},{"row":1,"column":0,"raw":"=1+1","numberFormat":2,"alignment":0,"fillColor":"","textColor":"","bold":false}],"rowHeights":[[0,40]],"columnWidths":[[0,180]],"sortColumn":-1,"sortAscending":true,"filterColumn":-1,"filterQuery":"","chartRange":"A1:B4","chartType":"pie"}"##;
        let wb = Workbook::from_legacy_json(v1).unwrap();
        let s = wb.sheet();
        assert_eq!(s.name, "Budget");
        assert!(s.format(0, 0).bold);
        assert_eq!(s.format(0, 0).align, Some(HAlign::Center));
        assert_eq!(s.format(1, 0).number, NumFmt::Currency);
        assert_eq!(s.col_w(0), 180);
        assert_eq!(s.chart.as_ref().map(|c| c.kind), Some(ChartKind::Pie));
        let v2 = br#"{"format":"nexus-sheets","version":2,"activeSheet":1,"sheets":[{"name":"A","cells":[]},{"name":"B","cells":[]}]}"#;
        let wb = Workbook::from_legacy_json(v2).unwrap();
        assert_eq!(wb.sheets.len(), 2);
        assert_eq!(wb.active, 1);
    }
}

#[cfg(test)]
mod size_check {
    use super::*;
    use std::time::Instant;

    #[test]
    #[ignore]
    fn large_sheet_timings() {
        let mut wb = Workbook::default();
        let n = 200_000u32;
        let t = Instant::now();
        let s = wb.sheet_mut();
        for r in 0..n {
            s.cells.insert((r, 0), Cell { raw: format!("Item {r}"), ..Default::default() });
            s.cells.insert((r, 1), Cell { raw: format!("{}", r % 97), ..Default::default() });
            s.cells.insert((r, 2), Cell { raw: format!("=B{}*2", r + 1), ..Default::default() });
        }
        println!("build {:?}", t.elapsed());
        let t = Instant::now();
        let calc = Calc::new(&wb, 0.0);
        println!("calc {:?}", t.elapsed());
        let t = Instant::now();
        let view = wb.view_rows(0, &calc);
        let s = wb.sheet();
        let mut y = 0.0;
        for r in &view {
            y += s.row_h(*r) as f64;
        }
        println!("relayout {:?} ({y})", t.elapsed());
        let t = Instant::now();
        let json = wb.to_json();
        println!("to_json {:?} {} MB", t.elapsed(), json.len() / 1_000_000);
        let t = Instant::now();
        let back = Workbook::from_json(&json).unwrap();
        println!("from_json {:?}", t.elapsed());
        let t = Instant::now();
        let (bytes, _) = super::super::xlsx::write(&back, &calc).unwrap();
        println!("xlsx write {:?} {} MB", t.elapsed(), bytes.len() / 1_000_000);
        let t = Instant::now();
        let _ = super::super::xlsx::read(&bytes).unwrap();
        println!("xlsx read {:?}", t.elapsed());
        let t = Instant::now();
        let mut wb2 = back.clone();
        wb2.sort_rows(0, 1, true, &calc);
        println!("sort {:?}", t.elapsed());
    }
}
