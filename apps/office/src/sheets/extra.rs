//! More of Excel's functions, the ones real workbooks use most beyond the core:
//! SUBTOTAL, trigonometry, LOOKUP, WORKDAY, conversions and the like.
//! `formula::call` asks here after `functions`; `None` means not one of these.

use super::formula::{Cells, Expr, Value, arg, compare, eval, parse_date, parse_number, serial_date};

const BAD: Value = Value::Err("#ERROR!");

fn one(e: &Expr, cells: &dyn Cells) -> Value {
    arg(e, cells).one()
}

fn num(e: &Expr, cells: &dyn Cells) -> Result<f64, &'static str> {
    one(e, cells).num()
}

/// A number or a typed date.
fn serial(e: &Expr, cells: &dyn Cells) -> Result<f64, &'static str> {
    match one(e, cells) {
        Value::Text(t) => parse_date(&t).or_else(|| parse_number(&t)).ok_or("#VALUE!"),
        v => v.num(),
    }
}

fn text(e: &Expr, cells: &dyn Cells) -> Result<String, &'static str> {
    match one(e, cells) {
        Value::Err(e) => Err(e),
        v => Ok(v.text()),
    }
}

fn res(r: Result<f64, &'static str>) -> Value {
    match r {
        Ok(x) if x.is_finite() => Value::Num(x),
        Ok(_) => Value::Err("#NUM!"),
        Err(e) => Value::Err(e),
    }
}

/// The numbers in the arguments: number cells from ranges, typed values as given.
fn numbers(args: &[Expr], cells: &dyn Cells) -> Result<Vec<f64>, &'static str> {
    let mut out = Vec::new();
    for a in args {
        for v in arg(a, cells).values() {
            match v {
                Value::Num(n) => out.push(n),
                Value::Err(e) => return Err(e),
                _ => {}
            }
        }
    }
    Ok(out)
}

/// Every value, words as 0 and TRUE as 1 (MINA, MAXA).
fn all_values(args: &[Expr], cells: &dyn Cells) -> Result<Vec<f64>, &'static str> {
    let mut out = Vec::new();
    for a in args {
        for v in arg(a, cells).values() {
            match v {
                Value::Empty => {}
                Value::Num(n) => out.push(n),
                Value::Bool(b) => out.push(if b { 1.0 } else { 0.0 }),
                Value::Text(_) => out.push(0.0),
                Value::Err(e) => return Err(e),
            }
        }
    }
    Ok(out)
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn variance(v: &[f64], sample: bool) -> Result<f64, &'static str> {
    let n = v.len();
    if n < if sample { 2 } else { 1 } {
        return Err("#DIV/0!");
    }
    let m = mean(v);
    Ok(v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (if sample { n - 1 } else { n }) as f64)
}

/// SUBTOTAL's values: the ranges' numbers, leaving out cells that are themselves subtotals.
fn subtotal_values(args: &[Expr], cells: &dyn Cells) -> Result<(Vec<f64>, usize), &'static str> {
    let mut nums = Vec::new();
    let mut nonblank = 0;
    for a in args {
        let (sheet, (r1, c1, r2, c2)) = match a {
            Expr::Ref { sheet, r, c } => (sheet, (*r, *c, *r, *c)),
            Expr::Range { sheet, r1, c1, r2, c2 } => (sheet, (*r1, *c1, *r2, *c2)),
            _ => return Err("#VALUE!"),
        };
        if (r2 - r1 + 1) as u64 * (c2 - c1 + 1) as u64 > 2_000_000 {
            return Err("#REF!");
        }
        for r in r1..=r2 {
            for c in c1..=c2 {
                if cells.raw(sheet.as_deref(), r, c).to_uppercase().contains("SUBTOTAL(") {
                    continue;
                }
                match cells.get(sheet.as_deref(), r, c) {
                    Value::Empty => {}
                    Value::Num(n) => {
                        nums.push(n);
                        nonblank += 1;
                    }
                    Value::Err(e) => return Err(e),
                    _ => nonblank += 1,
                }
            }
        }
    }
    Ok((nums, nonblank))
}

fn factorial(n: f64) -> Result<f64, &'static str> {
    if !(0.0..=170.0).contains(&n) {
        return Err("#NUM!");
    }
    Ok((1..=n.trunc() as u64).map(|k| k as f64).product())
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// 1 for Sunday through 7 for Saturday.
fn weekday(s: f64) -> i64 {
    (s.floor() as i64 - 1).rem_euclid(7) + 1
}

/// A number written in another base ("FF" in 16), as Excel's converters read it.
fn from_base(t: &str, base: u32, bits: u32) -> Result<f64, &'static str> {
    let t = t.trim();
    if t.len() > 10 || t.is_empty() {
        return Err("#NUM!");
    }
    let v = u64::from_str_radix(t, base).map_err(|_| "#NUM!")?;
    // Ten digits: the top one marks a negative number, in two's complement.
    let top = 1u64 << (bits - 1);
    Ok(if t.len() == 10 && v >= top { v as f64 - (1u64 << bits) as f64 } else { v as f64 })
}

fn to_base(v: f64, base: u32, bits: u32, places: Option<f64>) -> Result<String, &'static str> {
    let v = v.trunc() as i64;
    let lim = 1i64 << (bits - 1);
    if v < -lim || v >= lim {
        return Err("#NUM!");
    }
    let u = if v < 0 { ((1i64 << bits) + v) as u64 } else { v as u64 };
    let mut s = match base {
        2 => format!("{u:b}"),
        8 => format!("{u:o}"),
        _ => format!("{u:X}"),
    };
    if let Some(p) = places
        && v >= 0
    {
        let p = p as usize;
        if s.len() > p {
            return Err("#NUM!");
        }
        s = format!("{s:0>p$}");
    }
    Ok(s)
}

pub fn call(name: &str, args: &[Expr], cells: &dyn Cells) -> Option<Value> {
    let n = args.len();
    let arity = |r: std::ops::RangeInclusive<usize>| r.contains(&n);
    let unary = |f: fn(f64) -> f64| if n == 1 { res(num(&args[0], cells).map(f)) } else { BAD };
    let v = match name {
        "TRUE" if n == 0 => Value::Bool(true),
        "FALSE" if n == 0 => Value::Bool(false),
        "NA" if n == 0 => Value::Err("#N/A"),
        "SUBTOTAL" if n >= 2 => {
            let k = match num(&args[0], cells) {
                Ok(k) => k as i64 % 100,
                Err(e) => return Some(Value::Err(e)),
            };
            let (v, nonblank) = match subtotal_values(&args[1..], cells) {
                Ok(x) => x,
                Err(e) => return Some(Value::Err(e)),
            };
            match k {
                1 if v.is_empty() => Value::Err("#DIV/0!"),
                1 => Value::Num(mean(&v)),
                2 => Value::Num(v.len() as f64),
                3 => Value::Num(nonblank as f64),
                4 => Value::Num(if v.is_empty() { 0.0 } else { v.iter().cloned().fold(f64::MIN, f64::max) }),
                5 => Value::Num(if v.is_empty() { 0.0 } else { v.iter().cloned().fold(f64::MAX, f64::min) }),
                6 => Value::Num(v.iter().product()),
                7 => res(variance(&v, true).map(f64::sqrt)),
                8 => res(variance(&v, false).map(f64::sqrt)),
                9 => Value::Num(v.iter().sum()),
                10 => res(variance(&v, true)),
                11 => res(variance(&v, false)),
                _ => Value::Err("#VALUE!"),
            }
        }
        "ROWS" | "COLUMNS" if n == 1 => match &args[0] {
            Expr::Ref { .. } => Value::Num(1.0),
            Expr::Range { r1, c1, r2, c2, .. } => Value::Num(if name == "ROWS" { r2 - r1 + 1 } else { c2 - c1 + 1 } as f64),
            e => match super::arrays::block(e, cells) {
                Some(b) => Value::Num(if name == "ROWS" { b.rows } else { b.cols } as f64),
                None => Value::Num(1.0),
            },
        },
        "ISREF" if n == 1 => Value::Bool(matches!(args[0], Expr::Ref { .. } | Expr::Range { .. })),
        "SIN" => unary(f64::sin),
        "COS" => unary(f64::cos),
        "TAN" => unary(f64::tan),
        "ASIN" => unary(f64::asin),
        "ACOS" => unary(f64::acos),
        "ATAN" => unary(f64::atan),
        "SINH" => unary(f64::sinh),
        "COSH" => unary(f64::cosh),
        "TANH" => unary(f64::tanh),
        "ASINH" => unary(f64::asinh),
        "ACOSH" => unary(f64::acosh),
        "ATANH" => unary(f64::atanh),
        "RADIANS" => unary(f64::to_radians),
        "DEGREES" => unary(f64::to_degrees),
        "ATAN2" if n == 2 => match (num(&args[0], cells), num(&args[1], cells)) {
            (Ok(0.0), Ok(0.0)) => Value::Err("#DIV/0!"),
            // Excel's order: x first, then y.
            (Ok(x), Ok(y)) => res(Ok(y.atan2(x))),
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        },
        "FACT" if n == 1 => res(num(&args[0], cells).and_then(factorial)),
        "COMBIN" | "PERMUT" if n == 2 => match (num(&args[0], cells), num(&args[1], cells)) {
            (Ok(a), Ok(b)) if a >= b && b >= 0.0 => {
                let (a, b) = (a.trunc(), b.trunc());
                let p: f64 = (0..b as u64).map(|i| a - i as f64).product();
                res(if name == "PERMUT" { Ok(p) } else { factorial(b).map(|f| (p / f).round()) })
            }
            (Ok(_), Ok(_)) => Value::Err("#NUM!"),
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        },
        "GCD" | "LCM" if n >= 1 => match numbers(args, cells) {
            Ok(v) if v.iter().any(|x| *x < 0.0) => Value::Err("#NUM!"),
            Ok(v) => {
                let v: Vec<u64> = v.iter().map(|x| x.trunc() as u64).collect();
                let r = if name == "GCD" {
                    v.iter().fold(0, |a, b| gcd(a, *b))
                } else {
                    v.iter().fold(1u64, |a, b| if a == 0 || *b == 0 { 0 } else { a / gcd(a, *b) * b })
                };
                Value::Num(r as f64)
            }
            Err(e) => Value::Err(e),
        },
        "QUOTIENT" if n == 2 => match (num(&args[0], cells), num(&args[1], cells)) {
            (Ok(_), Ok(0.0)) => Value::Err("#DIV/0!"),
            (Ok(a), Ok(b)) => Value::Num((a / b).trunc()),
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        },
        "MROUND" if n == 2 => match (num(&args[0], cells), num(&args[1], cells)) {
            (Ok(_), Ok(0.0)) => Value::Num(0.0),
            (Ok(a), Ok(m)) if a * m < 0.0 => Value::Err("#NUM!"),
            (Ok(a), Ok(m)) => res(Ok((a / m).round() * m)),
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        },
        "ODD" | "EVEN" if n == 1 => match num(&args[0], cells) {
            Ok(x) => {
                let s = if x < 0.0 { -1.0 } else { 1.0 };
                let mut k = x.abs().ceil();
                let want_odd = name == "ODD";
                if (k as i64 % 2 == 1) != want_odd || (want_odd && k == 0.0) {
                    k += 1.0;
                }
                Value::Num(s * k)
            }
            Err(e) => Value::Err(e),
        },
        "ISODD" | "ISEVEN" if n == 1 => match num(&args[0], cells) {
            Ok(x) => Value::Bool(((x.trunc() as i64).rem_euclid(2) == 1) == (name == "ISODD")),
            Err(e) => Value::Err(e),
        },
        "ISLOGICAL" if n == 1 => Value::Bool(matches!(one(&args[0], cells), Value::Bool(_))),
        "ISNONTEXT" if n == 1 => Value::Bool(!matches!(one(&args[0], cells), Value::Text(_))),
        "SUMSQ" if n >= 1 => match numbers(args, cells) {
            Ok(v) => Value::Num(v.iter().map(|x| x * x).sum()),
            Err(e) => Value::Err(e),
        },
        "SUMX2MY2" | "SUMX2PY2" | "SUMXMY2" if n == 2 => {
            let (a, b) = (arg(&args[0], cells).values(), arg(&args[1], cells).values());
            if a.len() != b.len() {
                return Some(Value::Err("#N/A"));
            }
            let mut t = 0.0;
            for (x, y) in a.iter().zip(&b) {
                if let (Value::Num(x), Value::Num(y)) = (x, y) {
                    t += match name {
                        "SUMX2MY2" => x * x - y * y,
                        "SUMX2PY2" => x * x + y * y,
                        _ => (x - y).powi(2),
                    };
                }
            }
            Value::Num(t)
        }
        "VARP" | "VAR.P" | "VAR.S" | "STDEVP" | "STDEV.P" | "STDEV.S" | "DEVSQ" | "AVEDEV" if n >= 1 => match numbers(args, cells) {
            Ok(v) => match name {
                "DEVSQ" if !v.is_empty() => {
                    let m = mean(&v);
                    Value::Num(v.iter().map(|x| (x - m).powi(2)).sum())
                }
                "AVEDEV" if !v.is_empty() => {
                    let m = mean(&v);
                    Value::Num(v.iter().map(|x| (x - m).abs()).sum::<f64>() / v.len() as f64)
                }
                "DEVSQ" | "AVEDEV" => Value::Err("#NUM!"),
                "VAR.S" => res(variance(&v, true)),
                "STDEV.S" => res(variance(&v, true).map(f64::sqrt)),
                "VARP" | "VAR.P" => res(variance(&v, false)),
                _ => res(variance(&v, false).map(f64::sqrt)),
            },
            Err(e) => Value::Err(e),
        },
        "MINA" | "MAXA" if n >= 1 => match all_values(args, cells) {
            Ok(v) if v.is_empty() => Value::Num(0.0),
            Ok(v) => Value::Num(if name == "MINA" { v.iter().cloned().fold(f64::MAX, f64::min) } else { v.iter().cloned().fold(f64::MIN, f64::max) }),
            Err(e) => Value::Err(e),
        },
        "TIME" if n == 3 => match (num(&args[0], cells), num(&args[1], cells), num(&args[2], cells)) {
            (Ok(h), Ok(m), Ok(s)) => {
                let t = (h.trunc() * 3600.0 + m.trunc() * 60.0 + s.trunc()) / 86400.0;
                if t < 0.0 { Value::Err("#NUM!") } else { Value::Num(t.fract()) }
            }
            (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => Value::Err(e),
        },
        "T" if n == 1 => match one(&args[0], cells) {
            Value::Text(t) => Value::Text(t),
            Value::Err(e) => Value::Err(e),
            _ => Value::Text(String::new()),
        },
        "N" if n == 1 => match one(&args[0], cells) {
            Value::Num(x) => Value::Num(x),
            Value::Bool(b) => Value::Num(if b { 1.0 } else { 0.0 }),
            Value::Err(e) => Value::Err(e),
            _ => Value::Num(0.0),
        },
        "CLEAN" if n == 1 => match text(&args[0], cells) {
            Ok(t) => Value::Text(t.chars().filter(|c| (*c as u32) >= 32).collect()),
            Err(e) => Value::Err(e),
        },
        "CHAR" | "UNICHAR" if n == 1 => match num(&args[0], cells) {
            Ok(k) if k >= 1.0 && (name == "UNICHAR" || k <= 255.0) => match char::from_u32(k as u32) {
                Some(c) => Value::Text(c.to_string()),
                None => Value::Err("#VALUE!"),
            },
            Ok(_) => Value::Err("#VALUE!"),
            Err(e) => Value::Err(e),
        },
        "CODE" | "UNICODE" if n == 1 => match text(&args[0], cells) {
            Ok(t) => match t.chars().next() {
                Some(c) => Value::Num(c as u32 as f64),
                None => Value::Err("#VALUE!"),
            },
            Err(e) => Value::Err(e),
        },
        "DOLLAR" if arity(1..=2) => {
            let d = if n == 2 { num(&args[1], cells).unwrap_or(2.0) } else { 2.0 };
            match num(&args[0], cells) {
                Ok(x) => {
                    let s = super::formula::fixed(x.abs(), d.max(0.0) as usize, true);
                    Value::Text(if x < 0.0 { format!("(${s})") } else { format!("${s}") })
                }
                Err(e) => Value::Err(e),
            }
        }
        "HYPERLINK" if arity(1..=2) => one(&args[n - 1], cells),
        "ERROR.TYPE" if n == 1 => match one(&args[0], cells) {
            Value::Err(e) => {
                let k = ["#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A"].iter().position(|x| *x == e);
                k.map(|k| Value::Num(k as f64 + 1.0)).unwrap_or(Value::Err("#N/A"))
            }
            _ => Value::Err("#N/A"),
        },
        "DELTA" if arity(1..=2) => {
            let b = if n == 2 { num(&args[1], cells) } else { Ok(0.0) };
            match (num(&args[0], cells), b) {
                (Ok(a), Ok(b)) => Value::Num(if a == b { 1.0 } else { 0.0 }),
                (Err(e), _) | (_, Err(e)) => Value::Err(e),
            }
        }
        "HEX2DEC" | "BIN2DEC" | "OCT2DEC" if n == 1 => {
            let (base, bits) = match name {
                "HEX2DEC" => (16, 40),
                "BIN2DEC" => (2, 10),
                _ => (8, 30),
            };
            res(text(&args[0], cells).and_then(|t| from_base(&t, base, bits)))
        }
        "DEC2HEX" | "DEC2BIN" | "DEC2OCT" if arity(1..=2) => {
            let (base, bits) = match name {
                "DEC2HEX" => (16, 40),
                "DEC2BIN" => (2, 10),
                _ => (8, 30),
            };
            let places = if n == 2 { num(&args[1], cells).ok() } else { None };
            match num(&args[0], cells).and_then(|v| to_base(v, base, bits, places)) {
                Ok(s) => Value::Text(s),
                Err(e) => Value::Err(e),
            }
        }
        "ADDRESS" if arity(2..=5) => {
            let (r, c) = match (num(&args[0], cells), num(&args[1], cells)) {
                (Ok(r), Ok(c)) if r >= 1.0 && c >= 1.0 => (r as u32, c as u32),
                (Err(e), _) | (_, Err(e)) => return Some(Value::Err(e)),
                _ => return Some(Value::Err("#VALUE!")),
            };
            let abs = if n >= 3 { num(&args[2], cells).unwrap_or(1.0) as i64 } else { 1 };
            let (ar, ac) = match abs {
                2 => ("$", ""),
                3 => ("", "$"),
                4 => ("", ""),
                _ => ("$", "$"),
            };
            let cell = format!("{ac}{}{ar}{r}", super::formula::col_name(c - 1));
            match args.get(4).map(|a| text(a, cells)) {
                Some(Ok(sheet)) if !sheet.is_empty() => Value::Text(format!("{}!{cell}", super::summary::sheet_ref(&sheet))),
                Some(Err(e)) => Value::Err(e),
                _ => Value::Text(cell),
            }
        }
        "LOOKUP" if arity(2..=3) => {
            let want = one(&args[0], cells);
            let keys = arg(&args[1], cells).values();
            let results = if n == 3 { arg(&args[2], cells).values() } else { keys.clone() };
            // The last key not above the value (the keys run in order).
            let at = keys
                .iter()
                .enumerate()
                .filter(|(_, k)| !matches!(k, Value::Empty) && std::mem::discriminant(*k) == std::mem::discriminant(&want))
                .filter(|(_, k)| compare(k, &want).is_le())
                .map(|(i, _)| i)
                .next_back();
            match at.and_then(|i| results.get(i)) {
                Some(v) => v.clone(),
                None => Value::Err("#N/A"),
            }
        }
        "DAYS360" if arity(2..=3) => match (serial(&args[0], cells), serial(&args[1], cells)) {
            (Ok(a), Ok(b)) => {
                let european = n == 3 && eval(&args[2], cells).truthy().unwrap_or(false);
                let ((y1, m1, mut d1), (y2, m2, mut d2)) = (serial_date(a), serial_date(b));
                if european {
                    d1 = d1.min(30);
                    d2 = d2.min(30);
                } else {
                    // US: the last of February counts as the 30th, and so on.
                    let last_feb = |y: i64, m: i64, d: i64| m == 2 && serial_date(super::formula::date_serial(y, m, d) + 1.0).1 == 3;
                    if last_feb(y1, m1, d1) && last_feb(y2, m2, d2) {
                        d2 = 30;
                    }
                    if last_feb(y1, m1, d1) || d1 == 31 {
                        d1 = 30;
                    }
                    if d2 == 31 && d1 >= 30 {
                        d2 = 30;
                    }
                }
                Value::Num(((y2 - y1) * 360 + (m2 - m1) * 30 + (d2 - d1)) as f64)
            }
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        },
        "WORKDAY" if arity(2..=3) => {
            let (start, days) = match (serial(&args[0], cells), num(&args[1], cells)) {
                (Ok(s), Ok(d)) => (s.floor(), d.trunc() as i64),
                (Err(e), _) | (_, Err(e)) => return Some(Value::Err(e)),
            };
            let holidays: Vec<f64> = if n == 3 { arg(&args[2], cells).values().iter().filter_map(|v| v.num().ok()).map(f64::floor).collect() } else { Vec::new() };
            let step = if days < 0 { -1.0 } else { 1.0 };
            let mut d = start;
            let mut left = days.abs();
            if left > 100_000 {
                return Some(Value::Err("#NUM!"));
            }
            while left > 0 {
                d += step;
                let wd = weekday(d);
                if wd != 1 && wd != 7 && !holidays.contains(&d) {
                    left -= 1;
                }
            }
            Value::Num(d)
        }
        "MDETERM" if n == 1 => match super::arrays::block(&args[0], cells) {
            Some(b) if b.rows == b.cols => {
                let mut m: Vec<Vec<f64>> = Vec::new();
                for r in 0..b.rows {
                    let mut row = Vec::new();
                    for c in 0..b.cols {
                        match b.values[r * b.cols + c].num() {
                            Ok(x) => row.push(x),
                            Err(_) => return Some(Value::Err("#VALUE!")),
                        }
                    }
                    m.push(row);
                }
                Value::Num(super::arrays::determinant(m))
            }
            _ => Value::Err("#VALUE!"),
        },
        _ if NAMES.contains(&name) => BAD,
        _ => return None,
    };
    Some(v)
}

/// The names handled here, so a wrong argument count reads #ERROR! rather than #NAME?.
pub const NAMES: &[&str] = &[
    "TRUE", "FALSE", "NA", "SUBTOTAL", "ROWS", "COLUMNS", "ISREF", "SIN", "COS", "TAN", "ASIN", "ACOS", "ATAN", "SINH", "COSH",
    "TANH", "ASINH", "ACOSH", "ATANH", "RADIANS", "DEGREES", "ATAN2", "FACT", "COMBIN", "PERMUT", "GCD", "LCM", "QUOTIENT",
    "MROUND", "ODD", "EVEN", "ISODD", "ISEVEN", "ISLOGICAL", "ISNONTEXT", "SUMSQ", "SUMX2MY2", "SUMX2PY2", "SUMXMY2", "VARP",
    "VAR.P", "VAR.S", "STDEVP", "STDEV.P", "STDEV.S", "DEVSQ", "AVEDEV", "MINA", "MAXA", "TIME", "T", "N", "CLEAN", "CHAR",
    "UNICHAR", "CODE", "UNICODE", "DOLLAR", "HYPERLINK", "ERROR.TYPE", "DELTA", "HEX2DEC", "BIN2DEC", "OCT2DEC", "DEC2HEX",
    "DEC2BIN", "DEC2OCT", "ADDRESS", "LOOKUP", "DAYS360", "WORKDAY", "MDETERM",
];

#[cfg(test)]
mod tests {
    use super::super::formula::{Cells, Value, parse};
    use std::collections::HashMap;

    /// Cells with what was typed in them, for SUBTOTAL's look at formulas.
    struct Grid(HashMap<(u32, u32), (Value, &'static str)>);

    impl Cells for Grid {
        fn get(&self, _: Option<&str>, r: u32, c: u32) -> Value {
            self.0.get(&(r, c)).map(|x| x.0.clone()).unwrap_or(Value::Empty)
        }
        fn today(&self) -> f64 {
            0.0
        }
        fn here(&self) -> (u32, u32) {
            (6, 2)
        }
        fn raw(&self, _: Option<&str>, r: u32, c: u32) -> String {
            self.0.get(&(r, c)).map(|x| x.1.to_string()).unwrap_or_default()
        }
    }

    fn grid() -> Grid {
        // A1:A4 = 2, 3, a subtotal (5), 4; B1:B2 = "x", TRUE.
        Grid(HashMap::from([
            ((0, 0), (Value::Num(2.0), "2")),
            ((1, 0), (Value::Num(3.0), "3")),
            ((2, 0), (Value::Num(5.0), "=SUBTOTAL(9,A1:A2)")),
            ((3, 0), (Value::Num(4.0), "4")),
            ((0, 1), (Value::Text("x".into()), "x")),
            ((1, 1), (Value::Bool(true), "TRUE")),
        ]))
    }

    fn ev(src: &str) -> String {
        super::super::formula::eval(&parse(src), &grid()).text()
    }

    #[test]
    fn more_functions() {
        for (src, want) in [
            ("SUBTOTAL(9,A1:A4)", "9"),
            ("SUBTOTAL(109,A1:A4)", "9"),
            ("SUBTOTAL(2,A1:A4)", "3"),
            ("SUBTOTAL(1,A1:A4)", "3"),
            ("ROW()", "7"),
            ("COLUMN()", "3"),
            ("ROWS(A1:B4)", "4"),
            ("COLUMNS(A1:B4)", "2"),
            ("SUM(ROW(A1:A3))", "6"),
            ("SUM(OFFSET(A1,1,0,2,1))", "8"),
            ("INDIRECT(\"A4\")", "4"),
            ("SUM(INDIRECT(\"A1:A2\"))", "5"),
            ("ROUND(SIN(PI()/2),6)", "1"),
            ("ROUND(DEGREES(ATAN2(1,1)),6)", "45"),
            ("MROUND(17,5)", "15"),
            ("ODD(2)", "3"),
            ("EVEN(-1)", "-2"),
            ("FACT(5)", "120"),
            ("COMBIN(5,2)", "10"),
            ("GCD(12,18)", "6"),
            ("LCM(4,6)", "12"),
            ("SUMSQ(A1:A2)", "13"),
            ("MAXA(B1:B2)", "1"),
            ("TIME(12,30,0)", "0.5208333333"),
            ("CLEAN(\"a\"&CHAR(7)&\"b\")", "ab"),
            ("CODE(\"A\")", "65"),
            ("HEX2DEC(\"FF\")", "255"),
            ("DEC2BIN(5,4)", "0101"),
            ("ADDRESS(3,2)", "$B$3"),
            ("ADDRESS(3,2,4,TRUE,\"Data 2\")", "'Data 2'!B3"),
            ("LOOKUP(3.5,A1:A2,B1:B2)", "TRUE"),
            ("DAYS360(DATE(2026,1,30),DATE(2026,2,28))", "28"),
            // Friday the 9th: the next working day is Monday the 12th.
            ("WORKDAY(DATE(2026,10,9),1)", "46307"),
            ("MDETERM(MMULT(A1:A2,TRANSPOSE(A1:A2)))", "0"),
            ("ISODD(3)", "TRUE"),
            ("ISREF(A1)", "TRUE"),
            ("TRUE()", "TRUE"),
            ("ISNA(NA())", "TRUE"),
            ("ERROR.TYPE(1/0)", "2"),
        ] {
            assert_eq!(ev(src), want, "{src}");
        }
        // A matrix with no inverse.
        let inv = super::super::arrays::block(&parse("MINVERSE(MMULT(A1:A2,TRANSPOSE(A1:A2)))"), &grid()).unwrap();
        assert_eq!(inv.values, [Value::Err("#NUM!")]);
    }
}
