//! Formulas that give a block of values: a range (`=A1:A9`), operators applied
//! across ranges (`A1:A9*2`, `B2:B9>100`), and the functions UNIQUE, FILTER, SORT
//! and SEQUENCE, and those that reshape blocks (TAKE, DROP, CHOOSECOLS, TRANSPOSE,
//! VSTACK…). A block in a cell's formula spills into the cells below and to
//! the right (`Calc` does that); inside another function it's a range.

use super::formula::{self, Arg, Cells, Expr, Value};

/// Functions whose answer is a block.
pub const FUNCTIONS: [&str; 15] = [
    "UNIQUE", "FILTER", "SORT", "SEQUENCE", "SORTBY", "TEXTSPLIT", "CHOOSECOLS", "CHOOSEROWS", "TAKE", "DROP", "TRANSPOSE",
    "VSTACK", "HSTACK", "TOCOL", "TOROW",
];

/// The most cells a block may hold.
const MAX_CELLS: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub rows: usize,
    pub cols: usize,
    /// Row by row.
    pub values: Vec<Value>,
}

impl Block {
    pub fn one(v: Value) -> Block {
        Block { rows: 1, cols: 1, values: vec![v] }
    }

    fn err(e: &'static str) -> Block {
        Block::one(Value::Err(e))
    }

    /// The value at (r, c), a single row or column stretching to fit.
    fn at(&self, r: usize, c: usize) -> Value {
        let r = if self.rows == 1 { 0 } else { r };
        let c = if self.cols == 1 { 0 } else { c };
        if r >= self.rows || c >= self.cols {
            return Value::Err("#N/A");
        }
        self.values[r * self.cols + c].clone()
    }

    fn rows(&self) -> Vec<Vec<Value>> {
        self.values.chunks(self.cols.max(1)).map(<[Value]>::to_vec).collect()
    }

    fn from_rows(rows: Vec<Vec<Value>>) -> Block {
        let cols = rows.first().map_or(0, Vec::len);
        if rows.is_empty() || cols == 0 {
            return Block::err("#CALC!");
        }
        Block { rows: rows.len(), cols, values: rows.into_iter().flatten().collect() }
    }

    fn transposed(&self) -> Block {
        let mut values = Vec::with_capacity(self.values.len());
        for c in 0..self.cols {
            for r in 0..self.rows {
                values.push(self.values[r * self.cols + c].clone());
            }
        }
        Block { rows: self.cols, cols: self.rows, values }
    }

    /// Two blocks (or a block and a value) combined cell by cell.
    fn zip(a: Block, b: Block, f: impl Fn(Value, Value) -> Value) -> Block {
        let (rows, cols) = (a.rows.max(b.rows), a.cols.max(b.cols));
        if rows * cols > MAX_CELLS {
            return Block::err("#NUM!");
        }
        let mut values = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                values.push(f(a.at(r, c), b.at(r, c)));
            }
        }
        Block { rows, cols, values }
    }
}

/// Whether the expression can give a block (so its cell may spill).
pub fn may_spill(e: &Expr) -> bool {
    match e {
        Expr::Range { .. } => true,
        Expr::Call(name, _) => FUNCTIONS.contains(&name.as_str()),
        Expr::Neg(x) | Expr::Percent(x) => may_spill(x),
        Expr::Bin(_, a, b) | Expr::Cmp(_, a, b) => may_spill(a) || may_spill(b),
        _ => false,
    }
}

/// The expression as a block, or None when it gives a single value (without working it out).
pub fn block(e: &Expr, cells: &dyn Cells) -> Option<Block> {
    if !may_spill(e) {
        return None;
    }
    Some(match e {
        Expr::Range { .. } => match formula::arg(e, cells) {
            Arg::Grid { rows, cols, values } => Block { rows, cols, values },
            Arg::One(v) => Block::one(v),
        },
        Expr::Call(name, args) => call(name, args, cells),
        Expr::Neg(x) => {
            let b = whole(x, cells);
            Block::zip(b, Block::one(Value::Num(-1.0)), |a, k| formula::binary('*', a, k))
        }
        Expr::Percent(x) => {
            let b = whole(x, cells);
            Block::zip(b, Block::one(Value::Num(100.0)), |a, k| formula::binary('/', a, k))
        }
        Expr::Bin(op, a, b) => {
            let op = *op;
            Block::zip(whole(a, cells), whole(b, cells), move |x, y| formula::binary(op, x, y))
        }
        Expr::Cmp(op, a, b) => Block::zip(whole(a, cells), whole(b, cells), |x, y| formula::comparison(op, x, y)),
        _ => return None,
    })
}

/// The expression as a block, a single value as a block of one.
fn whole(e: &Expr, cells: &dyn Cells) -> Block {
    block(e, cells).unwrap_or_else(|| Block::one(formula::eval(e, cells)))
}

fn flag(args: &[Expr], i: usize, cells: &dyn Cells) -> Result<bool, &'static str> {
    match args.get(i) {
        Some(e) => formula::eval(e, cells).truthy(),
        None => Ok(false),
    }
}

fn number(args: &[Expr], i: usize, default: f64, cells: &dyn Cells) -> Result<f64, &'static str> {
    match args.get(i) {
        Some(e) => match formula::eval(e, cells) {
            Value::Empty => Ok(default),
            v => v.num(),
        },
        None => Ok(default),
    }
}

/// Two values the same for UNIQUE: numbers by value, words ignoring case.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => x == y,
        (Value::Text(x), Value::Text(y)) => x.to_lowercase() == y.to_lowercase(),
        _ => a == b,
    }
}

pub fn call(name: &str, args: &[Expr], cells: &dyn Cells) -> Block {
    match call_inner(name, args, cells) {
        Ok(b) => b,
        Err(e) => Block::err(e),
    }
}

fn call_inner(name: &str, args: &[Expr], cells: &dyn Cells) -> Result<Block, &'static str> {
    let n = args.len();
    match name {
        "UNIQUE" if (1..=3).contains(&n) => {
            let by_col = flag(args, 1, cells)?;
            let once = flag(args, 2, cells)?;
            let mut b = whole(&args[0], cells);
            if by_col {
                b = b.transposed();
            }
            let rows = b.rows();
            let mut kept: Vec<(Vec<Value>, usize)> = Vec::new();
            for row in rows {
                match kept.iter_mut().find(|(k, _)| k.len() == row.len() && k.iter().zip(&row).all(|(a, b)| same(a, b))) {
                    Some((_, count)) => *count += 1,
                    None => kept.push((row, 1)),
                }
            }
            let out = Block::from_rows(kept.into_iter().filter(|(_, k)| !once || *k == 1).map(|(r, _)| r).collect());
            Ok(if by_col && out.values.len() > 1 { out.transposed() } else { out })
        }
        "FILTER" if (2..=3).contains(&n) => {
            let b = whole(&args[0], cells);
            let include = whole(&args[1], cells);
            let keep = |v: Value| v.truthy();
            let out = if include.cols == 1 && include.rows == b.rows {
                let mut rows = Vec::new();
                for (i, row) in b.rows().into_iter().enumerate() {
                    if keep(include.values[i].clone())? {
                        rows.push(row);
                    }
                }
                rows
            } else if include.rows == 1 && include.cols == b.cols {
                let t = b.transposed();
                let mut cols = Vec::new();
                for (i, col) in t.rows().into_iter().enumerate() {
                    if keep(include.values[i].clone())? {
                        cols.push(col);
                    }
                }
                if cols.is_empty() {
                    Vec::new()
                } else {
                    Block::from_rows(cols).transposed().rows()
                }
            } else {
                return Err("#VALUE!");
            };
            if out.is_empty() {
                return Ok(match args.get(2) {
                    Some(e) => whole(e, cells),
                    None => Block::err("#CALC!"),
                });
            }
            Ok(Block::from_rows(out))
        }
        "SORT" if (1..=4).contains(&n) => {
            let index = number(args, 1, 1.0, cells)?.trunc();
            let order = number(args, 2, 1.0, cells)?;
            let by_col = flag(args, 3, cells)?;
            if order != 1.0 && order != -1.0 {
                return Err("#VALUE!");
            }
            let mut b = whole(&args[0], cells);
            if by_col {
                b = b.transposed();
            }
            if index < 1.0 || index as usize > b.cols {
                return Err("#VALUE!");
            }
            let k = index as usize - 1;
            let mut rows = b.rows();
            rows.sort_by(|x, y| {
                let o = sort_order(&x[k], &y[k]);
                if order < 0.0 { o.reverse() } else { o }
            });
            let out = Block::from_rows(rows);
            Ok(if by_col { out.transposed() } else { out })
        }
        "SEQUENCE" if (1..=4).contains(&n) => {
            let rows = number(args, 0, 1.0, cells)?.trunc();
            let cols = number(args, 1, 1.0, cells)?.trunc();
            let start = number(args, 2, 1.0, cells)?;
            let step = number(args, 3, 1.0, cells)?;
            if rows < 1.0 || cols < 1.0 {
                return Err("#CALC!");
            }
            if rows * cols > MAX_CELLS as f64 {
                return Err("#NUM!");
            }
            let (rows, cols) = (rows as usize, cols as usize);
            let values = (0..rows * cols).map(|i| Value::Num(start + step * i as f64)).collect();
            Ok(Block { rows, cols, values })
        }
        "SORTBY" if n >= 2 => {
            // SORTBY(array, by1, [order1], by2, [order2]…): each key one value per row.
            let b = whole(&args[0], cells);
            let mut keys: Vec<(Block, f64)> = Vec::new();
            let mut i = 1;
            while i < n {
                let key = whole(&args[i], cells);
                let order = if i + 1 < n { number(args, i + 1, 1.0, cells)? } else { 1.0 };
                if key.values.len() != b.rows || (order != 1.0 && order != -1.0) {
                    return Err("#VALUE!");
                }
                keys.push((key, order));
                i += 2;
            }
            let rows = b.rows();
            let mut idx: Vec<usize> = (0..rows.len()).collect();
            idx.sort_by(|x, y| {
                for (k, order) in &keys {
                    let o = sort_order(&k.values[*x], &k.values[*y]);
                    let o = if *order < 0.0 { o.reverse() } else { o };
                    if o.is_ne() {
                        return o;
                    }
                }
                std::cmp::Ordering::Equal
            });
            Ok(Block::from_rows(idx.into_iter().map(|i| rows[i].clone()).collect()))
        }
        "TEXTSPLIT" if (2..=6).contains(&n) => {
            let text = formula::eval(&args[0], cells);
            if let Value::Err(e) = text {
                return Err(e);
            }
            let text = text.text();
            let col_delim = formula::eval(&args[1], cells).text();
            let row_delim = args.get(2).map(|a| formula::eval(a, cells).text()).filter(|d| !d.is_empty());
            let skip_empty = flag(args, 3, cells)?;
            let pad = args.get(5).map(|a| formula::eval(a, cells)).unwrap_or(Value::Err("#N/A"));
            let split = |s: &str, d: &str| -> Vec<String> {
                if d.is_empty() { vec![s.to_string()] } else { s.split(d).map(String::from).collect() }
            };
            let lines = match &row_delim {
                Some(d) => split(&text, d),
                None => vec![text],
            };
            let mut rows: Vec<Vec<Value>> = lines
                .iter()
                .filter(|l| !(skip_empty && l.is_empty()))
                .map(|l| split(l, &col_delim).into_iter().filter(|p| !(skip_empty && p.is_empty())).map(Value::Text).collect())
                .collect();
            let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
            for r in &mut rows {
                r.resize(cols, pad.clone());
            }
            Ok(Block::from_rows(rows))
        }
        "CHOOSECOLS" | "CHOOSEROWS" if n >= 2 => {
            let mut b = whole(&args[0], cells);
            if name == "CHOOSECOLS" {
                b = b.transposed();
            }
            let rows = b.rows();
            let mut out = Vec::new();
            for a in &args[1..] {
                for k in whole(a, cells).values {
                    let k = k.num()?.trunc() as i64;
                    let at = if k > 0 { k - 1 } else { rows.len() as i64 + k };
                    if k == 0 || at < 0 || at as usize >= rows.len() {
                        return Err("#VALUE!");
                    }
                    out.push(rows[at as usize].clone());
                }
            }
            let out = Block::from_rows(out);
            Ok(if name == "CHOOSECOLS" { out.transposed() } else { out })
        }
        "TAKE" | "DROP" if (2..=3).contains(&n) => {
            let b = whole(&args[0], cells);
            let rows_n = match args.get(1) {
                Some(e) if !matches!(formula::eval(e, cells), Value::Empty) => Some(number(args, 1, 0.0, cells)?.trunc() as i64),
                _ => None,
            };
            let cols_n = if n == 3 { Some(number(args, 2, 0.0, cells)?.trunc() as i64) } else { None };
            // Which of `len` items stay: the first k (or last, for a negative k), or all but them.
            let keep = |len: usize, k: Option<i64>| -> std::ops::Range<usize> {
                let Some(k) = k else { return 0..len };
                let m = (k.unsigned_abs() as usize).min(len);
                match (name == "TAKE", k >= 0) {
                    (true, true) => 0..m,
                    (true, false) => len - m..len,
                    (false, true) => m..len,
                    (false, false) => 0..len - m,
                }
            };
            let (rr, cr) = (keep(b.rows, rows_n), keep(b.cols, cols_n));
            let rows: Vec<Vec<Value>> = b.rows()[rr].iter().map(|r| r[cr.clone()].to_vec()).collect();
            Ok(Block::from_rows(rows))
        }
        "TRANSPOSE" if n == 1 => Ok(whole(&args[0], cells).transposed()),
        "VSTACK" | "HSTACK" if n >= 1 => {
            let across = name == "HSTACK";
            let parts: Vec<Block> = args.iter().map(|a| {
                let b = whole(a, cells);
                if across { b.transposed() } else { b }
            }).collect();
            // Stacked down; shorter rows padded with #N/A, as in Excel.
            let width = parts.iter().map(|b| b.cols).max().unwrap_or(1);
            let mut rows = Vec::new();
            for b in parts {
                for mut r in b.rows() {
                    r.resize(width, Value::Err("#N/A"));
                    rows.push(r);
                }
            }
            let out = Block::from_rows(rows);
            Ok(if across { out.transposed() } else { out })
        }
        "TOCOL" | "TOROW" if (1..=3).contains(&n) => {
            let b = whole(&args[0], cells);
            let ignore = number(args, 1, 0.0, cells)? as i64;
            let by_col = flag(args, 2, cells)?;
            let b = if by_col { b.transposed() } else { b };
            let values: Vec<Value> = b
                .values
                .into_iter()
                .filter(|v| match ignore {
                    1 => !matches!(v, Value::Empty),
                    2 => !v.is_err(),
                    3 => !matches!(v, Value::Empty) && !v.is_err(),
                    _ => true,
                })
                .collect();
            if values.is_empty() {
                return Err("#CALC!");
            }
            let k = values.len();
            Ok(if name == "TOCOL" { Block { rows: k, cols: 1, values } } else { Block { rows: 1, cols: k, values } })
        }
        _ => Err("#ERROR!"),
    }
}

/// Spreadsheet sort order: numbers, then words, then TRUE/FALSE, then errors; empty last.
fn sort_order(a: &Value, b: &Value) -> std::cmp::Ordering {
    let rank = |v: &Value| match v {
        Value::Num(_) => 0,
        Value::Text(_) => 1,
        Value::Bool(_) => 2,
        Value::Err(_) => 3,
        Value::Empty => 4,
    };
    rank(a).cmp(&rank(b)).then_with(|| formula::compare(a, b))
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
            0.0
        }
    }

    fn grid() -> Grid {
        // A: fruit, B: amount.
        let mut m = HashMap::new();
        for (r, (f, n)) in [("pear", 3.0), ("Apple", 10.0), ("pear", 7.0), ("fig", 1.0), ("apple", 2.0)].into_iter().enumerate() {
            m.insert((r as u32, 0), Value::Text(f.into()));
            m.insert((r as u32, 1), Value::Num(n));
        }
        Grid(m)
    }

    fn texts(b: &Block) -> Vec<String> {
        b.values.iter().map(Value::text).collect()
    }

    fn run(src: &str) -> Block {
        let g = grid();
        block(&formula::parse(src), &g).expect("a block")
    }

    #[test]
    fn unique_filter_sort_sequence() {
        assert_eq!(texts(&run("UNIQUE(A1:A5)")), ["pear", "Apple", "fig"]);
        assert_eq!(texts(&run("UNIQUE(A1:A5,FALSE,TRUE)")), ["fig"]);
        let f = run("FILTER(A1:B5,B1:B5>2)");
        assert_eq!((f.rows, f.cols), (3, 2));
        assert_eq!(texts(&f), ["pear", "3", "Apple", "10", "pear", "7"]);
        assert_eq!(texts(&run("FILTER(A1:A5,B1:B5>100,\"none\")")), ["none"]);
        assert_eq!(run("FILTER(A1:A5,B1:B5>100)").values, [Value::Err("#CALC!")]);
        assert_eq!(texts(&run("SORT(A1:B5,2,-1)"))[..4], ["Apple", "10", "pear", "7"]);
        assert_eq!(texts(&run("SORT(UNIQUE(A1:A5))")), ["Apple", "fig", "pear"]);
        let s = run("SEQUENCE(2,3,10,5)");
        assert_eq!((s.rows, s.cols), (2, 3));
        assert_eq!(texts(&s), ["10", "15", "20", "25", "30", "35"]);
        assert_eq!(texts(&run("B1:B3*2")), ["6", "20", "14"]);
    }

    #[test]
    fn reshaping() {
        assert_eq!(texts(&run("TAKE(A1:B5,2,1)")), ["pear", "Apple"]);
        assert_eq!(texts(&run("TAKE(B1:B5,-2)")), ["1", "2"]);
        assert_eq!(texts(&run("DROP(B1:B5,3)")), ["1", "2"]);
        assert_eq!(texts(&run("CHOOSECOLS(A1:B2,2,1)")), ["3", "pear", "10", "Apple"]);
        assert_eq!(texts(&run("CHOOSEROWS(A1:A5,-1)")), ["apple"]);
        let t = run("TRANSPOSE(A1:B2)");
        assert_eq!((t.rows, t.cols), (2, 2));
        assert_eq!(texts(&t), ["pear", "Apple", "3", "10"]);
        assert_eq!(texts(&run("VSTACK(B1:B2,B4)")), ["3", "10", "1"]);
        assert_eq!(texts(&run("HSTACK(B1:B2,A1:A2)")), ["3", "pear", "10", "Apple"]);
        assert_eq!(texts(&run("TOROW(A1:B2)")), ["pear", "3", "Apple", "10"]);
        assert_eq!(texts(&run("SORTBY(A1:A5,B1:B5,-1)")), ["Apple", "pear", "pear", "apple", "fig"]);
        let s = run("TEXTSPLIT(\"a,b;c\",\",\",\";\")");
        assert_eq!((s.rows, s.cols), (2, 2));
        assert_eq!(texts(&s), ["a", "b", "c", "#N/A"]);
        let g = grid();
        let one = |src: &str| formula::eval(&formula::parse(src), &g).text();
        assert_eq!(one("TEXTBEFORE(\"one-two-three\",\"-\")"), "one");
        assert_eq!(one("TEXTAFTER(\"one-two-three\",\"-\",-1)"), "three");
        assert_eq!(one("TEXTAFTER(\"abc\",\"x\",1,0,0,\"none\")"), "none");
        assert_eq!(one("AVERAGEA(B1:B2,TRUE,\"x\")"), "3.5");
        assert_eq!(one("XMATCH(\"fig\",A1:A5)"), "4");
        assert_eq!(one("XMATCH(4,B1:B5,1)"), "3");
    }

    #[test]
    fn blocks_inside_other_functions() {
        let g = grid();
        let one = |src: &str| formula::eval(&formula::parse(src), &g).text();
        assert_eq!(one("COUNTA(UNIQUE(A1:A5))"), "3");
        assert_eq!(one("SUM(FILTER(B1:B5,A1:A5=\"pear\"))"), "10");
        assert_eq!(one("SUMPRODUCT((B1:B5>2)*B1:B5)"), "20");
        assert_eq!(one("INDEX(SORT(B1:B5),2)"), "2");
        // Where one value is wanted, the block's first.
        assert_eq!(one("UNIQUE(A1:A5)"), "pear");
    }
}
