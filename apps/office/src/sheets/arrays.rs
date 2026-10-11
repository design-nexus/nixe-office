//! Formulas that give a block of values: a range (`=A1:A9`), operators applied
//! across ranges (`A1:A9*2`, `B2:B9>100`), and the functions UNIQUE, FILTER, SORT
//! and SEQUENCE. A block in a cell's formula spills into the cells below and to
//! the right (`Calc` does that); inside another function it's a range.

use super::formula::{self, Arg, Cells, Expr, Value};

/// Functions whose answer is a block.
pub const FUNCTIONS: [&str; 4] = ["UNIQUE", "FILTER", "SORT", "SEQUENCE"];

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
