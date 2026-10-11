//! Formulas: parsing to an expression tree, evaluating, and moving references
//! when cells are copied or filled.

use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Empty,
    Num(f64),
    Text(String),
    Bool(bool),
    Err(&'static str),
}

impl Value {
    /// As a number: empty is 0, TRUE is 1, text that reads as a number counts.
    pub fn num(&self) -> Result<f64, &'static str> {
        match self {
            Value::Empty => Ok(0.0),
            Value::Num(n) => Ok(*n),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Value::Text(t) => parse_number(t).ok_or("#VALUE!"),
            Value::Err(e) => Err(e),
        }
    }

    pub fn truthy(&self) -> Result<bool, &'static str> {
        match self {
            Value::Empty => Ok(false),
            Value::Num(n) => Ok(*n != 0.0),
            Value::Bool(b) => Ok(*b),
            Value::Text(t) => match t.to_uppercase().as_str() {
                "TRUE" => Ok(true),
                "FALSE" => Ok(false),
                _ => Ok(!t.is_empty()),
            },
            Value::Err(e) => Err(e),
        }
    }

    /// As text, the way a cell shows it without a number format.
    pub fn text(&self) -> String {
        match self {
            Value::Empty => String::new(),
            Value::Num(n) => general(*n),
            Value::Text(t) => t.clone(),
            Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
            Value::Err(e) => (*e).into(),
        }
    }

    pub fn is_err(&self) -> bool {
        matches!(self, Value::Err(_))
    }
}

/// A number the way a cell shows it in General format.
pub fn general(n: f64) -> String {
    if !n.is_finite() {
        return "#NUM!".into();
    }
    if n == n.trunc() && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    if n.abs() >= 1e15 || n.abs() < 1e-9 {
        return format!("{n:e}");
    }
    let s = format!("{n:.10}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

/// "12", "-3.5", "1,234.5", "$12.50", "50%".
pub fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (t, pct) = match t.strip_suffix('%') {
        Some(r) => (r.trim_end(), true),
        None => (t, false),
    };
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t),
    };
    let t = t.strip_prefix('$').unwrap_or(t);
    // Thousands separators only between groups of three digits.
    let plain = if t.contains(',') {
        let int = t.split('.').next().unwrap_or("");
        let groups: Vec<&str> = int.split(',').collect();
        if groups.len() < 2 || groups[0].is_empty() || groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3) {
            return None;
        }
        t.replace(',', "")
    } else {
        t.to_string()
    };
    if plain.is_empty() || !plain.chars().all(|c| c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-')
    {
        return None;
    }
    if !plain.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let v: f64 = plain.parse().ok()?;
    let v = if neg { -v } else { v };
    Some(if pct { v / 100.0 } else { v })
}

// ---------- Dates ----------

/// Days since 1970-01-01 for a calendar date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The spreadsheet date number (days since 1899-12-30, as Excel counts).
pub fn date_serial(y: i64, m: i64, d: i64) -> f64 {
    // DATE(2026, 14, 1) is February 2027, as in other spreadsheets.
    let (y, m) = (y + (m - 1).div_euclid(12), (m - 1).rem_euclid(12) + 1);
    (days_from_civil(y, m, 1) + d - 1 - days_from_civil(1899, 12, 30)) as f64
}

pub fn serial_date(serial: f64) -> (i64, i64, i64) {
    civil_from_days(serial.floor() as i64 + days_from_civil(1899, 12, 30))
}

/// "2026-10-05" or "10/5/2026" (month first).
pub fn parse_date(s: &str) -> Option<f64> {
    let t = s.trim();
    let parts: Vec<&str> = if t.contains('-') { t.split('-').collect() } else { t.split('/').collect() };
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    let n: Vec<i64> = parts.iter().map(|p| p.parse().unwrap_or(0)).collect();
    let (y, m, d) = if t.contains('-') { (n[0], n[1], n[2]) } else { (n[2], n[0], n[1]) };
    if !(1900..=9999).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let s = date_serial(y, m, d);
    // Reject dates that roll over (April 31).
    (serial_date(s) == (y, m, d)).then_some(s)
}

pub fn fmt_date(serial: f64) -> String {
    let (y, m, d) = serial_date(serial);
    format!("{y:04}-{m:02}-{d:02}")
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// TEXT(value, format): a small subset of spreadsheet format codes.
pub fn format_text(v: f64, code: &str) -> String {
    let lower = code.to_lowercase();
    if lower.contains('y') || lower.contains('d') {
        let (y, m, d) = serial_date(v);
        let mut out = String::new();
        let mut rest = lower.as_str();
        while !rest.is_empty() {
            let take = |p: &str| rest.starts_with(p);
            if take("yyyy") {
                let _ = write!(out, "{y:04}");
                rest = &rest[4..];
            } else if take("yy") {
                let _ = write!(out, "{:02}", y % 100);
                rest = &rest[2..];
            } else if take("mmmm") || take("mmm") {
                let full = take("mmmm");
                out.push_str(MONTHS[(m - 1) as usize]);
                rest = &rest[if full { 4 } else { 3 }..];
            } else if take("mm") {
                let _ = write!(out, "{m:02}");
                rest = &rest[2..];
            } else if take("m") {
                let _ = write!(out, "{m}");
                rest = &rest[1..];
            } else if take("dd") {
                let _ = write!(out, "{d:02}");
                rest = &rest[2..];
            } else if take("d") {
                let _ = write!(out, "{d}");
                rest = &rest[1..];
            } else {
                let c = rest.chars().next().unwrap_or(' ');
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
        return out;
    }
    let pct = code.contains('%');
    let v = if pct { v * 100.0 } else { v };
    let decimals = code.split('.').nth(1).map(|d| d.chars().filter(|c| *c == '0' || *c == '#').count()).unwrap_or(0);
    let grouped = code.contains(',');
    let mut s = fixed(v, decimals, grouped);
    if code.contains('$') {
        s = if let Some(r) = s.strip_prefix('-') { format!("-${r}") } else { format!("${s}") };
    }
    if pct {
        s.push('%');
    }
    s
}

/// `v` with `decimals` places, optionally with thousands separators.
pub fn fixed(v: f64, decimals: usize, grouped: bool) -> String {
    let s = format!("{:.*}", decimals, v.abs());
    let (int, frac) = s.split_once('.').map(|(a, b)| (a.to_string(), Some(b.to_string()))).unwrap_or((s.clone(), None));
    let int = if grouped {
        let mut out = String::new();
        for (i, c) in int.chars().enumerate() {
            if i > 0 && (int.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        out
    } else {
        int
    };
    let neg = v < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0');
    format!("{}{}{}", if neg { "-" } else { "" }, int, frac.map(|f| format!(".{f}")).unwrap_or_default())
}

// ---------- References ----------

/// "A" for 0, "Z" for 25, "AA" for 26.
pub fn col_name(mut c: u32) -> String {
    let mut s = Vec::new();
    loop {
        s.push((b'A' + (c % 26) as u8) as char);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    s.iter().rev().collect()
}

pub fn cell_name(r: u32, c: u32) -> String {
    format!("{}{}", col_name(c), r + 1)
}

/// "B12" → (11, 1).
pub fn parse_cell(s: &str) -> Option<(u32, u32)> {
    let s = s.trim().replace('$', "");
    let letters: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits = &s[letters.len()..];
    if letters.is_empty() || letters.len() > 3 || digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut c = 0u32;
    for ch in letters.to_ascii_uppercase().chars() {
        c = c * 26 + (ch as u32 - 'A' as u32 + 1);
    }
    let r: u32 = digits.parse().ok()?;
    (r >= 1).then_some((r - 1, c - 1))
}

/// "A1:B8" → ((0,0),(7,1)), normalised so the first corner is the top-left.
pub fn parse_range(s: &str) -> Option<((u32, u32), (u32, u32))> {
    let (a, b) = s.split_once(':').unwrap_or((s, s));
    let (r1, c1) = parse_cell(a)?;
    let (r2, c2) = parse_cell(b)?;
    Some(((r1.min(r2), c1.min(c2)), (r1.max(r2), c1.max(c2))))
}

// ---------- Parsing ----------

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Ref { sheet: Option<String>, r: u32, c: u32 },
    Range { sheet: Option<String>, r1: u32, c1: u32, r2: u32, c2: u32 },
    Neg(Box<Expr>),
    Percent(Box<Expr>),
    Bin(char, Box<Expr>, Box<Expr>),
    Cmp(&'static str, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Error(&'static str),
}

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
    /// How deep in brackets, function calls and signs we are.
    depth: usize,
}

/// Deeper formulas are refused rather than risk running out of stack.
const MAX_DEPTH: usize = 200;

pub fn parse(src: &str) -> Expr {
    let mut p = Parser { s: src.as_bytes(), src, i: 0, depth: 0 };
    let e = p.comparison();
    p.ws();
    if p.i != p.s.len() {
        return Expr::Error("#ERROR!");
    }
    e
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn eat_str(&mut self, t: &str) -> bool {
        self.ws();
        if self.src[self.i..].starts_with(t) {
            self.i += t.len();
            true
        } else {
            false
        }
    }

    fn comparison(&mut self) -> Expr {
        let left = self.concat();
        for op in ["<=", ">=", "<>", "=", "<", ">"] {
            if self.eat_str(op) {
                let right = self.concat();
                return Expr::Cmp(op, Box::new(left), Box::new(right));
            }
        }
        left
    }

    fn concat(&mut self) -> Expr {
        let mut left = self.additive();
        while self.eat(b'&') {
            let right = self.additive();
            left = Expr::Bin('&', Box::new(left), Box::new(right));
        }
        left
    }

    fn additive(&mut self) -> Expr {
        let mut left = self.term();
        loop {
            let op = match self.peek() {
                Some(b'+') => '+',
                Some(b'-') => '-',
                _ => return left,
            };
            self.i += 1;
            let right = self.term();
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
    }

    fn term(&mut self) -> Expr {
        let mut left = self.power();
        loop {
            let op = match self.peek() {
                Some(b'*') => '*',
                Some(b'/') => '/',
                _ => return left,
            };
            self.i += 1;
            let right = self.power();
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
    }

    /// `^` works left to right, as in spreadsheets: 2^3^2 is 64.
    fn power(&mut self) -> Expr {
        let mut left = self.unary();
        while self.eat(b'^') {
            let exp = self.unary();
            left = Expr::Bin('^', Box::new(left), Box::new(exp));
        }
        left
    }

    fn unary(&mut self) -> Expr {
        if self.depth >= MAX_DEPTH {
            // Stop here; the rest of the text stays unread, so the formula is an error.
            return Expr::Error("#ERROR!");
        }
        self.depth += 1;
        let e = self.unary_inner();
        self.depth -= 1;
        e
    }

    fn unary_inner(&mut self) -> Expr {
        if self.eat(b'-') {
            return Expr::Neg(Box::new(self.unary()));
        }
        if self.eat(b'+') {
            return self.unary();
        }
        let mut e = self.atom();
        while self.eat(b'%') {
            e = Expr::Percent(Box::new(e));
        }
        e
    }

    /// `Sheet!` or `'Sheet name'!` before a reference.
    fn sheet_prefix(&mut self) -> Option<String> {
        self.ws();
        let start = self.i;
        if self.s.get(self.i) == Some(&b'\'') {
            let mut j = self.i + 1;
            let mut name = String::new();
            while j < self.s.len() {
                if self.s[j] == b'\'' {
                    if self.s.get(j + 1) == Some(&b'\'') {
                        name.push('\'');
                        j += 2;
                        continue;
                    }
                    break;
                }
                let ch = self.src[j..].chars().next().unwrap_or(' ');
                name.push(ch);
                j += ch.len_utf8();
            }
            if self.s.get(j + 1) == Some(&b'!') {
                self.i = j + 2;
                return Some(name);
            }
            self.i = start;
            return None;
        }
        let mut j = self.i;
        while j < self.s.len() && (self.s[j].is_ascii_alphanumeric() || self.s[j] == b'_') {
            j += 1;
        }
        if j > self.i && self.s.get(j) == Some(&b'!') {
            let name = self.src[self.i..j].to_string();
            self.i = j + 1;
            return Some(name);
        }
        None
    }

    /// A cell address at the cursor (`A1`, `$B$2`), or None leaving the cursor put.
    fn cell(&mut self) -> Option<(u32, u32)> {
        self.ws();
        let start = self.i;
        let mut j = self.i;
        if self.s.get(j) == Some(&b'$') {
            j += 1;
        }
        let ls = j;
        while j < self.s.len() && self.s[j].is_ascii_alphabetic() {
            j += 1;
        }
        if j == ls || j - ls > 3 {
            return None;
        }
        if self.s.get(j) == Some(&b'$') {
            j += 1;
        }
        let ds = j;
        while j < self.s.len() && self.s[j].is_ascii_digit() {
            j += 1;
        }
        if j == ds {
            return None;
        }
        // A1( would be a function name.
        if self.s.get(j).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'(' || *c == b'_') {
            return None;
        }
        let cell = parse_cell(&self.src[start..j])?;
        self.i = j;
        Some(cell)
    }

    fn reference(&mut self) -> Option<Expr> {
        let start = self.i;
        let sheet = self.sheet_prefix();
        let Some((r1, c1)) = self.cell() else {
            self.i = start;
            return None;
        };
        let save = self.i;
        if self.eat(b':')
            && let Some((r2, c2)) = self.cell()
        {
            return Some(Expr::Range { sheet, r1: r1.min(r2), c1: c1.min(c2), r2: r1.max(r2), c2: c1.max(c2) });
        }
        self.i = save;
        Some(Expr::Ref { sheet, r: r1, c: c1 })
    }

    fn atom(&mut self) -> Expr {
        match self.peek() {
            None => Expr::Error("#ERROR!"),
            Some(b'(') => {
                self.i += 1;
                let e = self.comparison();
                if !self.eat(b')') {
                    return Expr::Error("#ERROR!");
                }
                e
            }
            Some(b'"') => {
                self.i += 1;
                let mut out = String::new();
                loop {
                    let Some(ch) = self.src[self.i..].chars().next() else { return Expr::Error("#ERROR!") };
                    self.i += ch.len_utf8();
                    if ch == '"' {
                        if self.s.get(self.i) == Some(&b'"') {
                            out.push('"');
                            self.i += 1;
                            continue;
                        }
                        return Expr::Str(out);
                    }
                    out.push(ch);
                }
            }
            Some(c) if c.is_ascii_digit() || c == b'.' => {
                let start = self.i;
                while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
                    self.i += 1;
                }
                if self.i < self.s.len() && (self.s[self.i] == b'e' || self.s[self.i] == b'E') {
                    let save = self.i;
                    self.i += 1;
                    if self.i < self.s.len() && (self.s[self.i] == b'+' || self.s[self.i] == b'-') {
                        self.i += 1;
                    }
                    let ds = self.i;
                    while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                        self.i += 1;
                    }
                    if ds == self.i {
                        self.i = save;
                    }
                }
                self.src[start..self.i].parse().map(Expr::Num).unwrap_or(Expr::Error("#ERROR!"))
            }
            Some(_) => {
                if let Some(r) = self.reference() {
                    return r;
                }
                let start = self.i;
                while self.i < self.s.len()
                    && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_' || self.s[self.i] == b'.')
                {
                    self.i += 1;
                }
                if self.i == start {
                    return Expr::Error("#ERROR!");
                }
                let name = self.src[start..self.i].to_ascii_uppercase();
                if self.eat(b'(') {
                    let mut args = Vec::new();
                    if !self.eat(b')') {
                        loop {
                            args.push(self.comparison());
                            if self.eat(b')') {
                                break;
                            }
                            if !self.eat(b',') {
                                return Expr::Error("#ERROR!");
                            }
                        }
                    }
                    return Expr::Call(name, args);
                }
                match name.as_str() {
                    "TRUE" => Expr::Bool(true),
                    "FALSE" => Expr::Bool(false),
                    _ => Expr::Error("#NAME?"),
                }
            }
        }
    }
}

// ---------- Evaluating ----------

/// What formulas read cells through.
pub trait Cells {
    /// The value of a cell; `sheet` None is the formula's own sheet.
    fn get(&self, sheet: Option<&str>, r: u32, c: u32) -> Value;
    /// Now, as a date number: the fraction is the time of day.
    fn today(&self) -> f64;
}

/// An argument: one value, or a block of cells row by row.
pub(super) enum Arg {
    One(Value),
    Grid { rows: usize, cols: usize, values: Vec<Value> },
}

impl Arg {
    pub(super) fn values(self) -> Vec<Value> {
        match self {
            Arg::One(v) => vec![v],
            Arg::Grid { values, .. } => values,
        }
    }
    pub(super) fn one(self) -> Value {
        match self {
            Arg::One(v) => v,
            // A range where one value is wanted: its first cell.
            Arg::Grid { values, .. } => values.into_iter().next().unwrap_or(Value::Empty),
        }
    }
}

/// Cells a single range may cover.
const MAX_RANGE: u64 = 2_000_000;

pub(super) fn arg(e: &Expr, cells: &dyn Cells) -> Arg {
    match e {
        Expr::Range { sheet, r1, c1, r2, c2 } => {
            let (rows, cols) = ((r2 - r1 + 1) as usize, (c2 - c1 + 1) as usize);
            if rows as u64 * cols as u64 > MAX_RANGE {
                return Arg::One(Value::Err("#REF!"));
            }
            let mut values = Vec::with_capacity(rows * cols);
            for r in *r1..=*r2 {
                for c in *c1..=*c2 {
                    values.push(cells.get(sheet.as_deref(), r, c));
                }
            }
            Arg::Grid { rows, cols, values }
        }
        // A block worked out from ranges (A1:A9*2) or a function that gives one (UNIQUE).
        _ => match super::arrays::block(e, cells) {
            Some(b) => Arg::Grid { rows: b.rows, cols: b.cols, values: b.values },
            None => Arg::One(eval(e, cells)),
        },
    }
}

pub(super) fn compare(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Empty, Value::Num(y)) => 0.0f64.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Num(x), Value::Empty) => x.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.text().to_lowercase().cmp(&b.text().to_lowercase()),
    }
}

/// Does `v` meet a COUNTIF-style criterion (">5", "<>x", "apple", 3)?
pub(super) fn matches_criterion(v: &Value, crit: &Value) -> bool {
    let text = crit.text();
    let (op, rest) = ["<=", ">=", "<>", "<", ">", "="]
        .iter()
        .find_map(|op| text.strip_prefix(op).map(|r| (*op, r)))
        .unwrap_or(("=", text.as_str()));
    let target = match parse_number(rest) {
        Some(n) if !matches!(crit, Value::Text(_)) || rest.trim() == rest => Value::Num(n),
        _ => Value::Text(rest.to_string()),
    };
    let v = match (v, &target) {
        (Value::Text(t), Value::Num(_)) => parse_number(t).map(Value::Num).unwrap_or_else(|| v.clone()),
        _ => v.clone(),
    };
    // A number never equals a word, and an empty cell only matches "" or "<>x".
    let comparable = !matches!((&v, &target), (Value::Num(_), Value::Text(_)) | (Value::Text(_), Value::Num(_)));
    let ord = compare(&v, &target);
    match op {
        "=" => comparable && ord.is_eq(),
        "<>" => !comparable || ord.is_ne(),
        "<" => comparable && ord.is_lt(),
        ">" => comparable && ord.is_gt(),
        "<=" => comparable && ord.is_le(),
        _ => comparable && ord.is_ge(),
    }
}

fn round_to(n: f64, digits: f64, mode: fn(f64) -> f64) -> f64 {
    let d = digits.trunc().clamp(-15.0, 15.0) as i32;
    let scale = 10f64.powi(d);
    mode(n * scale) / scale
}

fn away_from_zero(x: f64) -> f64 {
    let r = x.abs().ceil();
    if x < 0.0 { -r } else { r }
}

pub fn eval(e: &Expr, cells: &dyn Cells) -> Value {
    match e {
        Expr::Num(n) => Value::Num(*n),
        Expr::Str(s) => Value::Text(s.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Error(code) => Value::Err(code),
        Expr::Ref { sheet, r, c } => cells.get(sheet.as_deref(), *r, *c),
        Expr::Range { .. } => arg(e, cells).one(),
        Expr::Neg(x) => match eval(x, cells).num() {
            Ok(n) => Value::Num(-n),
            Err(e) => Value::Err(e),
        },
        Expr::Percent(x) => match eval(x, cells).num() {
            Ok(n) => Value::Num(n / 100.0),
            Err(e) => Value::Err(e),
        },
        Expr::Bin(op, a, b) => binary(*op, eval(a, cells), eval(b, cells)),
        Expr::Cmp(op, a, b) => comparison(op, eval(a, cells), eval(b, cells)),
        Expr::Call(name, args) => call(name, args, cells),
    }
}

/// `a op b` for an arithmetic operator or `&`.
pub(super) fn binary(op: char, a: Value, b: Value) -> Value {
    if op == '&' {
        if let Value::Err(e) = a {
            return Value::Err(e);
        }
        if let Value::Err(e) = b {
            return Value::Err(e);
        }
        return Value::Text(a.text() + &b.text());
    }
    let (a, b) = match (a.num(), b.num()) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => return Value::Err(e),
    };
    let r = match op {
        '+' => a + b,
        '-' => a - b,
        '*' => a * b,
        '/' if b == 0.0 => return Value::Err("#DIV/0!"),
        '/' => a / b,
        _ => a.powf(b),
    };
    if r.is_finite() { Value::Num(r) } else { Value::Err("#NUM!") }
}

/// `a op b` for a comparison.
pub(super) fn comparison(op: &str, a: Value, b: Value) -> Value {
    if let Value::Err(e) = a {
        return Value::Err(e);
    }
    if let Value::Err(e) = b {
        return Value::Err(e);
    }
    let ord = compare(&a, &b);
    Value::Bool(match op {
        "=" => ord.is_eq(),
        "<>" => ord.is_ne(),
        "<" => ord.is_lt(),
        ">" => ord.is_gt(),
        "<=" => ord.is_le(),
        _ => ord.is_ge(),
    })
}

fn call(name: &str, args: &[Expr], cells: &dyn Cells) -> Value {
    let n = args.len();
    let bad = Value::Err("#ERROR!");
    if let Some(v) = super::functions::call(name, args, cells) {
        return v;
    }
    // Where one value is wanted, a block gives its first.
    if super::arrays::FUNCTIONS.contains(&name) {
        return super::arrays::call(name, args, cells).values.into_iter().next().unwrap_or(Value::Empty);
    }
    // Functions that look at their arguments lazily or as blocks come first.
    match name {
        "IF" => {
            if !(2..=3).contains(&n) {
                return bad;
            }
            return match eval(&args[0], cells).truthy() {
                Ok(true) => eval(&args[1], cells),
                Ok(false) if n == 3 => eval(&args[2], cells),
                Ok(false) => Value::Bool(false),
                Err(e) => Value::Err(e),
            };
        }
        "IFERROR" => {
            if n != 2 {
                return bad;
            }
            let v = eval(&args[0], cells);
            return if v.is_err() { eval(&args[1], cells) } else { v };
        }
        "COUNTIF" | "SUMIF" | "AVERAGEIF" => {
            if n < 2 || n > if name == "COUNTIF" { 2 } else { 3 } {
                return bad;
            }
            let candidates = arg(&args[0], cells).values();
            let crit = eval(&args[1], cells);
            if let Value::Err(e) = crit {
                return Value::Err(e);
            }
            let sums = if n == 3 { arg(&args[2], cells).values() } else { candidates.clone() };
            if sums.len() != candidates.len() {
                return bad;
            }
            let mut total = 0.0;
            let mut count = 0usize;
            for (v, s) in candidates.iter().zip(&sums) {
                if matches_criterion(v, &crit) {
                    count += 1;
                    if let Value::Num(x) = s {
                        total += x;
                    }
                }
            }
            return match name {
                "COUNTIF" => Value::Num(count as f64),
                "SUMIF" => Value::Num(total),
                _ if count == 0 => Value::Err("#DIV/0!"),
                _ => Value::Num(total / count as f64),
            };
        }
        "VLOOKUP" => {
            if !(3..=4).contains(&n) {
                return bad;
            }
            let key = eval(&args[0], cells);
            let Arg::Grid { rows, cols, values } = arg(&args[1], cells) else { return Value::Err("#REF!") };
            let col = match eval(&args[2], cells).num() {
                Ok(c) if c >= 1.0 && (c as usize) <= cols => c as usize - 1,
                Ok(_) => return Value::Err("#REF!"),
                Err(e) => return Value::Err(e),
            };
            // The fourth argument TRUE (approximate match) finds the last row not above the key.
            let approx = if n == 4 { eval(&args[3], cells).truthy().unwrap_or(false) } else { false };
            let mut found = None;
            for r in 0..rows {
                let v = &values[r * cols];
                if approx {
                    if compare(v, &key).is_le() && !matches!(v, Value::Empty) {
                        found = Some(r);
                    } else if !matches!(v, Value::Empty) {
                        break;
                    }
                } else if compare(v, &key).is_eq() && !matches!(v, Value::Empty) {
                    found = Some(r);
                    break;
                }
            }
            return found.map(|r| values[r * cols + col].clone()).unwrap_or(Value::Err("#N/A"));
        }
        "XLOOKUP" => {
            if !(3..=4).contains(&n) {
                return bad;
            }
            let key = eval(&args[0], cells);
            let look = arg(&args[1], cells).values();
            let Arg::Grid { rows, cols, values } = arg(&args[2], cells) else {
                return Value::Err("#VALUE!");
            };
            let pos = look.iter().position(|v| !matches!(v, Value::Empty) && compare(v, &key).is_eq());
            return match pos {
                // A result range shaped like the lookup range: same position.
                Some(i) if rows * cols == look.len() => values[i].clone(),
                Some(i) if cols == 1 || rows == 1 => values.get(i).cloned().unwrap_or(Value::Err("#REF!")),
                Some(i) => values.get(i * cols).cloned().unwrap_or(Value::Err("#REF!")),
                None if n == 4 => eval(&args[3], cells),
                None => Value::Err("#N/A"),
            };
        }
        "COUNTA" => {
            let c = args.iter().flat_map(|a| arg(a, cells).values()).filter(|v| !matches!(v, Value::Empty)).count();
            return Value::Num(c as f64);
        }
        "COUNT" => {
            let c = args.iter().flat_map(|a| arg(a, cells).values()).filter(|v| matches!(v, Value::Num(_))).count();
            return Value::Num(c as f64);
        }
        "TODAY" => return if n == 0 { Value::Num(cells.today().floor()) } else { bad },
        _ => {}
    }

    // Everything else takes its arguments as values.
    let blocks: Vec<Arg> = args.iter().map(|a| arg(a, cells)).collect();
    let mut flat: Vec<Value> = Vec::new();
    let mut singles: Vec<Value> = Vec::new();
    for b in blocks {
        match b {
            Arg::One(v) => {
                flat.push(v.clone());
                singles.push(v);
            }
            Arg::Grid { values, .. } => {
                singles.push(values.first().cloned().unwrap_or(Value::Empty));
                // Ranges contribute only their numbers to sums; typed values count as given.
                flat.extend(values.into_iter().filter(|v| matches!(v, Value::Num(_) | Value::Err(_))));
            }
        }
    }
    if let Some(e) = flat.iter().find_map(|v| if let Value::Err(e) = v { Some(*e) } else { None }) {
        return Value::Err(e);
    }
    let nums =
        || -> Result<Vec<f64>, &'static str> { flat.iter().filter(|v| !matches!(v, Value::Empty)).map(|v| v.num()).collect() };
    let one = |i: usize| -> Result<f64, &'static str> { singles.get(i).ok_or("#ERROR!")?.num() };
    let res = |r: Result<f64, &'static str>| match r {
        Ok(x) if x.is_finite() => Value::Num(x),
        Ok(_) => Value::Err("#NUM!"),
        Err(e) => Value::Err(e),
    };
    match name {
        "SUM" => res(nums().map(|v| v.iter().sum())),
        "PRODUCT" => res(nums().map(|v| if v.is_empty() { 0.0 } else { v.iter().product() })),
        "AVERAGE" => match nums() {
            Ok(v) if v.is_empty() => Value::Err("#DIV/0!"),
            Ok(v) => Value::Num(v.iter().sum::<f64>() / v.len() as f64),
            Err(e) => Value::Err(e),
        },
        "MIN" | "MAX" => match nums() {
            Ok(v) if v.is_empty() => Value::Num(0.0),
            Ok(v) => Value::Num(if name == "MIN" {
                v.iter().cloned().fold(f64::INFINITY, f64::min)
            } else {
                v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            }),
            Err(e) => Value::Err(e),
        },
        "MEDIAN" => match nums() {
            Ok(v) if v.is_empty() => Value::Err("#NUM!"),
            Ok(mut v) => {
                v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let m = v.len() / 2;
                Value::Num(if v.len() % 2 == 1 { v[m] } else { (v[m - 1] + v[m]) / 2.0 })
            }
            Err(e) => Value::Err(e),
        },
        "AND" | "OR" => {
            if flat.is_empty() {
                return bad;
            }
            let mut all = true;
            let mut any = false;
            for v in flat.iter().filter(|v| !matches!(v, Value::Empty)) {
                match v.truthy() {
                    Ok(t) => {
                        all &= t;
                        any |= t;
                    }
                    Err(e) => return Value::Err(e),
                }
            }
            Value::Bool(if name == "AND" { all } else { any })
        }
        "NOT" if n == 1 => match singles[0].truthy() {
            Ok(t) => Value::Bool(!t),
            Err(e) => Value::Err(e),
        },
        "ABS" if n == 1 => res(one(0).map(f64::abs)),
        "INT" if n == 1 => res(one(0).map(f64::floor)),
        "SQRT" if n == 1 => match one(0) {
            Ok(x) if x < 0.0 => Value::Err("#NUM!"),
            r => res(r.map(f64::sqrt)),
        },
        "POWER" if n == 2 => res(one(0).and_then(|a| one(1).map(|b| a.powf(b)))),
        "MOD" if n == 2 => match (one(0), one(1)) {
            (Ok(_), Ok(0.0)) => Value::Err("#DIV/0!"),
            // The result takes the divisor's sign, as in other spreadsheets.
            (Ok(a), Ok(b)) => Value::Num(a - b * (a / b).floor()),
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        },
        "ROUND" | "ROUNDUP" | "ROUNDDOWN" if (1..=2).contains(&n) => {
            let digits = if n == 2 { one(1) } else { Ok(0.0) };
            let mode: fn(f64) -> f64 = match name {
                "ROUND" => f64::round,
                "ROUNDUP" => away_from_zero,
                _ => f64::trunc,
            };
            res(one(0).and_then(|x| digits.map(|d| round_to(x, d, mode))))
        }
        "DATE" if n == 3 => match (one(0), one(1), one(2)) {
            (Ok(y), Ok(m), Ok(d)) => Value::Num(date_serial(y.trunc() as i64, m.trunc() as i64, d.trunc() as i64)),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => Value::Err(e),
        },
        "YEAR" | "MONTH" | "DAY" if n == 1 => {
            let serial = match &singles[0] {
                Value::Text(t) => parse_date(t).ok_or("#VALUE!"),
                v => v.num(),
            };
            match serial {
                Ok(s) => {
                    let (y, m, d) = serial_date(s);
                    Value::Num(match name {
                        "YEAR" => y,
                        "MONTH" => m,
                        _ => d,
                    } as f64)
                }
                Err(e) => Value::Err(e),
            }
        }
        "CONCAT" | "CONCATENATE" => {
            let mut s = String::new();
            for a in args {
                for v in arg(a, cells).values() {
                    if let Value::Err(e) = v {
                        return Value::Err(e);
                    }
                    s.push_str(&v.text());
                }
            }
            Value::Text(s)
        }
        "TEXT" if n == 2 => match one(0) {
            Ok(x) => Value::Text(format_text(x, &singles[1].text())),
            Err(_) => Value::Text(singles[0].text()),
        },
        "LEN" if n == 1 => Value::Num(singles[0].text().chars().count() as f64),
        "UPPER" if n == 1 => Value::Text(singles[0].text().to_uppercase()),
        "LOWER" if n == 1 => Value::Text(singles[0].text().to_lowercase()),
        "TRIM" if n == 1 => Value::Text(singles[0].text().split_whitespace().collect::<Vec<_>>().join(" ")),
        "NOT" | "ABS" | "INT" | "SQRT" | "POWER" | "MOD" | "ROUND" | "ROUNDUP" | "ROUNDDOWN" | "DATE" | "YEAR" | "MONTH"
        | "DAY" | "TEXT" | "LEN" | "UPPER" | "LOWER" | "TRIM" => bad,
        _ => Value::Err("#NAME?"),
    }
}

// ---------- Moving references ----------

/// Move every relative reference in a formula by (`dr`, `dc`), as when the cell
/// is copied that far. `$` parts stay put; text in quotes is left alone; a
/// reference pushed off the sheet becomes `#REF!`.
pub fn shift(raw: &str, dr: i64, dc: i64, max_rows: u32, max_cols: u32) -> String {
    if !raw.starts_with('=') || (dr == 0 && dc == 0) {
        return raw.to_string();
    }
    let b = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' {
            // Copy a string literal as is.
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
        if c == b'\'' {
            // A quoted sheet name.
            let start = i;
            i += 1;
            while i < b.len() && b[i] != b'\'' {
                i += 1;
            }
            i = (i + 1).min(b.len());
            out.push_str(&raw[start..i]);
            continue;
        }
        let prev_word = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b'.');
        if !prev_word && (c == b'$' || c.is_ascii_alphabetic()) {
            let start = i;
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
            if le > ls && le - ls <= 3 && j > ds && !follows_word {
                let (r, col) = parse_cell(&raw[ls..j].replace('$', "")).unwrap_or((0, 0));
                let nr = if row_abs { r as i64 } else { r as i64 + dr };
                let nc = if col_abs { col as i64 } else { col as i64 + dc };
                if nr < 0 || nc < 0 || nr >= max_rows as i64 || nc >= max_cols as i64 {
                    out.push_str("#REF!");
                } else {
                    let _ = write!(
                        out,
                        "{}{}{}{}",
                        if col_abs { "$" } else { "" },
                        col_name(nc as u32),
                        if row_abs { "$" } else { "" },
                        nr + 1
                    );
                }
                i = j;
                continue;
            }
            // A name (a function, a sheet): copy the whole word.
            let mut k = start;
            while k < b.len() && (b[k].is_ascii_alphanumeric() || b[k] == b'_' || b[k] == b'$' || b[k] == b'.') {
                k += 1;
            }
            out.push_str(&raw[start..k.max(start + 1)]);
            i = k.max(start + 1);
            continue;
        }
        let ch = raw[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Grid(HashMap<(u32, u32), Value>);
    impl Cells for Grid {
        fn get(&self, _sheet: Option<&str>, r: u32, c: u32) -> Value {
            self.0.get(&(r, c)).cloned().unwrap_or(Value::Empty)
        }
        fn today(&self) -> f64 {
            date_serial(2026, 10, 5)
        }
    }

    fn grid(cells: &[(&str, Value)]) -> Grid {
        Grid(cells.iter().map(|(a, v)| (parse_cell(a).unwrap(), v.clone())).collect())
    }

    fn ev(src: &str, g: &Grid) -> Value {
        eval(&parse(src), g)
    }

    fn n(x: f64) -> Value {
        Value::Num(x)
    }

    fn t(s: &str) -> Value {
        Value::Text(s.into())
    }

    #[test]
    fn arithmetic_and_precedence() {
        let g = grid(&[("A1", n(2.0)), ("A2", n(3.0))]);
        assert_eq!(ev("1+2*3", &g), n(7.0));
        assert_eq!(ev("(1+2)*3", &g), n(9.0));
        assert_eq!(ev("2^3^2", &g), n(64.0));
        assert_eq!(ev("-2^2", &g), n(4.0));
        assert_eq!(ev("2^-1", &g), n(0.5));
        let deep = format!("{}1{}", "(".repeat(100_000), ")".repeat(100_000));
        assert!(matches!(ev(&deep, &g), Value::Err(_)));
        assert!(matches!(ev(&"-".repeat(100_000), &g), Value::Err(_)));
        assert_eq!(ev("-A1+A2", &g), n(1.0));
        assert_eq!(ev("A1/0", &g), Value::Err("#DIV/0!"));
        assert_eq!(ev("50%*A1", &g), n(1.0));
        assert_eq!(ev("1+", &g), Value::Err("#ERROR!"));
        assert_eq!(ev("FOO(1)", &g), Value::Err("#NAME?"));
        assert_eq!(ev("\"a\"+1", &g), Value::Err("#VALUE!"));
        assert_eq!(ev("\"Total: \"&A1", &g), t("Total: 2"));
        assert_eq!(ev("A1>1", &g), Value::Bool(true));
        assert_eq!(ev("\"abc\"=\"ABC\"", &g), Value::Bool(true));
    }

    #[test]
    fn ranges_and_summaries() {
        let g = grid(&[
            ("A1", n(1.0)),
            ("A2", n(2.0)),
            ("A3", t("x")),
            ("A4", n(4.0)),
            ("B1", t("apple")),
            ("B2", t("pear")),
            ("B3", t("apple")),
        ]);
        assert_eq!(ev("SUM(A1:A5)", &g), n(7.0));
        assert_eq!(ev("AVERAGE(A1:A4)", &g), n(7.0 / 3.0));
        assert_eq!(ev("COUNT(A1:A4)", &g), n(3.0));
        assert_eq!(ev("COUNTA(A1:A5)", &g), n(4.0));
        assert_eq!(ev("MAX(A1:A4,10)", &g), n(10.0));
        assert_eq!(ev("MEDIAN(A1:A4)", &g), n(2.0));
        assert_eq!(ev("COUNTIF(B1:B3,\"apple\")", &g), n(2.0));
        assert_eq!(ev("COUNTIF(A1:A4,\">1\")", &g), n(2.0));
        assert_eq!(ev("SUMIF(B1:B3,\"apple\",A1:A3)", &g), n(1.0));
        assert_eq!(ev("AVERAGE(B1:B3)", &g), Value::Err("#DIV/0!"));
    }

    #[test]
    fn logic_and_errors() {
        let g = grid(&[("A1", n(5.0))]);
        assert_eq!(ev("IF(A1>3,\"big\",\"small\")", &g), t("big"));
        assert_eq!(ev("IFERROR(1/0,\"none\")", &g), t("none"));
        assert_eq!(ev("AND(A1>1,A1<10)", &g), Value::Bool(true));
        assert_eq!(ev("NOT(OR(FALSE,0))", &g), Value::Bool(true));
        assert_eq!(ev("SQRT(-1)", &g), Value::Err("#NUM!"));
        assert_eq!(ev("MOD(-7,3)", &g), n(2.0));
    }

    #[test]
    fn rounding() {
        let g = grid(&[]);
        assert_eq!(ev("ROUND(2.345,2)", &g), n(2.35));
        assert_eq!(ev("ROUNDUP(2.341,2)", &g), n(2.35));
        assert_eq!(ev("ROUNDDOWN(-2.349,2)", &g), n(-2.34));
        assert_eq!(ev("ROUNDUP(-1.2)", &g), n(-2.0));
        assert_eq!(ev("INT(-1.5)", &g), n(-2.0));
    }

    #[test]
    fn lookups() {
        let g = grid(&[
            ("A1", t("apple")),
            ("B1", n(1.5)),
            ("A2", t("pear")),
            ("B2", n(2.25)),
            ("D1", n(10.0)),
            ("E1", t("low")),
            ("D2", n(20.0)),
            ("E2", t("mid")),
            ("D3", n(30.0)),
            ("E3", t("high")),
        ]);
        assert_eq!(ev("VLOOKUP(\"PEAR\",A1:B2,2)", &g), n(2.25));
        assert_eq!(ev("VLOOKUP(\"plum\",A1:B2,2,FALSE)", &g), Value::Err("#N/A"));
        assert_eq!(ev("VLOOKUP(25,D1:E3,2,TRUE)", &g), t("mid"));
        assert_eq!(ev("VLOOKUP(\"pear\",A1:B2,3)", &g), Value::Err("#REF!"));
        assert_eq!(ev("XLOOKUP(\"apple\",A1:A2,B1:B2)", &g), n(1.5));
        assert_eq!(ev("XLOOKUP(\"plum\",A1:A2,B1:B2,\"none\")", &g), t("none"));
    }

    #[test]
    fn text_and_dates() {
        let g = grid(&[("A1", t("  a  b ")), ("A2", n(1234.5))]);
        assert_eq!(ev("TRIM(A1)", &g), t("a b"));
        assert_eq!(ev("UPPER(\"x\")&LEN(\"abc\")", &g), t("X3"));
        assert_eq!(ev("CONCAT(\"a\",1,\"b\")", &g), t("a1b"));
        assert_eq!(ev("TEXT(A2,\"$#,##0.00\")", &g), t("$1,234.50"));
        assert_eq!(ev("TEXT(0.256,\"0.0%\")", &g), t("25.6%"));
        assert_eq!(ev("DATE(2026,10,5)", &g), n(date_serial(2026, 10, 5)));
        assert_eq!(ev("TODAY()-DATE(2026,10,1)", &g), n(4.0));
        assert_eq!(ev("TEXT(DATE(2026,1,9),\"mmm d, yyyy\")", &g), t("Jan 9, 2026"));
        assert_eq!(ev("YEAR(DATE(2026,14,1))", &g), n(2027.0));
        assert_eq!(ev("MONTH(\"2026-03-04\")", &g), n(3.0));
        // Excel's serial for 2026-10-05.
        assert_eq!(date_serial(2026, 10, 5), 46300.0);
        assert_eq!(fmt_date(46300.0), "2026-10-05");
        assert_eq!(parse_date("10/5/2026"), Some(46300.0));
        assert_eq!(parse_date("2026-02-30"), None);
    }

    #[test]
    fn typed_numbers() {
        assert_eq!(parse_number("1,234.5"), Some(1234.5));
        assert_eq!(parse_number("$12.50"), Some(12.5));
        assert_eq!(parse_number("-$3"), Some(-3.0));
        assert_eq!(parse_number("50%"), Some(0.5));
        assert_eq!(parse_number("12,34"), None);
        assert_eq!(parse_number("e5"), None);
        assert_eq!(parse_number("abc"), None);
        assert_eq!(general(0.1 + 0.2), "0.3");
        assert_eq!(general(1e20), "1e20");
        assert_eq!(fixed(-1234.567, 2, true), "-1,234.57");
    }

    #[test]
    fn references_move_when_copied() {
        assert_eq!(shift("=A1+$B$1+C$2+$D3", 1, 1, 100, 100), "=B2+$B$1+D$2+$D4");
        assert_eq!(shift("=SUM(A1:A3)", 2, 0, 100, 100), "=SUM(A3:A5)");
        assert_eq!(shift("=\"A1\"&A1", 1, 0, 100, 100), "=\"A1\"&A2");
        assert_eq!(shift("=A1", -1, 0, 100, 100), "=#REF!");
        assert_eq!(shift("=LOG10(A1)", 1, 0, 100, 100), "=LOG10(A2)");
        assert_eq!(shift("='My sheet'!A1+Other!B2", 1, 0, 100, 100), "='My sheet'!A2+Other!B3");
        assert_eq!(shift("plain A1", 1, 0, 100, 100), "plain A1");
    }

    #[test]
    fn sheet_references_parse() {
        assert_eq!(parse("Sheet2!B3"), Expr::Ref { sheet: Some("Sheet2".into()), r: 2, c: 1 });
        assert_eq!(parse("'My sheet'!A1:B2"), Expr::Range { sheet: Some("My sheet".into()), r1: 0, c1: 0, r2: 1, c2: 1 });
        assert_eq!(col_name(26), "AA");
        assert_eq!(parse_cell("ab10"), Some((9, 27)));
    }
}
