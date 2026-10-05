//! The larger part of the function library: text, dates, maths, statistics,
//! conditional totals and lookups (money functions are in `finance.rs`). `formula::call` asks here first; `None`
//! means the name is one of the core functions it handles itself.

use super::formula::{
    Arg, Cells, Expr, Value, arg, compare, date_serial, eval, matches_criterion, parse_date, parse_number, serial_date,
};

const BAD: Value = Value::Err("#ERROR!");

/// What one argument works out to, its first cell for a range.
fn one(e: &Expr, cells: &dyn Cells) -> Value {
    arg(e, cells).one()
}

fn num(e: &Expr, cells: &dyn Cells) -> Result<f64, &'static str> {
    one(e, cells).num()
}

/// A number or a typed date ("2026-10-05").
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

/// The numbers among all the arguments: from ranges only the number cells,
/// typed values as given.
fn numbers(args: &[Expr], cells: &dyn Cells) -> Result<Vec<f64>, &'static str> {
    let mut out = Vec::new();
    for a in args {
        match arg(a, cells) {
            Arg::One(Value::Empty) => {}
            Arg::One(v) => out.push(v.num()?),
            Arg::Grid { values, .. } => {
                for v in values {
                    match v {
                        Value::Num(n) => out.push(n),
                        Value::Err(e) => return Err(e),
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(out)
}

fn res(r: Result<f64, &'static str>) -> Value {
    match r {
        Ok(x) if x.is_finite() => Value::Num(x),
        Ok(_) => Value::Err("#NUM!"),
        Err(e) => Value::Err(e),
    }
}

fn text_res(r: Result<String, &'static str>) -> Value {
    match r {
        Ok(s) => Value::Text(s),
        Err(e) => Value::Err(e),
    }
}

/// Characters, so positions count letters rather than bytes.
fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn days_in_month(y: i64, m: i64) -> i64 {
    (date_serial(y, m + 1, 1) - date_serial(y, m, 1)) as i64
}

/// The same day `months` later, kept inside the month (Jan 31 + 1 month = Feb 28).
fn add_months(s: f64, months: i64) -> f64 {
    let (y, m, d) = serial_date(s);
    let total = y * 12 + (m - 1) + months;
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    date_serial(y, m, d.min(days_in_month(y, m)))
}

/// 1 for Sunday through 7 for Saturday.
fn weekday(s: f64) -> i64 {
    // Serial 1 (1899-12-31) was a Sunday.
    (s.floor() as i64 - 1).rem_euclid(7) + 1
}

fn sample_variance(v: &[f64]) -> Result<f64, &'static str> {
    if v.len() < 2 {
        return Err("#DIV/0!");
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    Ok(v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (v.len() - 1) as f64)
}

/// A number from the system clock, for RAND.
fn random() -> f64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(0);
    let mut x = STATE.load(Ordering::Relaxed);
    if x == 0 {
        x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    STATE.store(x, Ordering::Relaxed);
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// Which cells pass every (range, criterion) pair of COUNTIFS and friends.
fn passing(pairs: &[Expr], cells: &dyn Cells) -> Result<Vec<bool>, Value> {
    if pairs.is_empty() || !pairs.len().is_multiple_of(2) {
        return Err(BAD);
    }
    let mut pass: Option<Vec<bool>> = None;
    for p in pairs.chunks(2) {
        let values = arg(&p[0], cells).values();
        let crit = one(&p[1], cells);
        if let Value::Err(e) = crit {
            return Err(Value::Err(e));
        }
        let hits: Vec<bool> = values.iter().map(|v| matches_criterion(v, &crit)).collect();
        pass = Some(match pass {
            None => hits,
            Some(prev) if prev.len() == hits.len() => prev.iter().zip(&hits).map(|(a, b)| *a && *b).collect(),
            Some(_) => return Err(Value::Err("#VALUE!")),
        });
    }
    Ok(pass.unwrap_or_default())
}

pub fn call(name: &str, args: &[Expr], cells: &dyn Cells) -> Option<Value> {
    if let Some(v) = super::finance::call(name, args, cells) {
        return Some(v);
    }
    let n = args.len();
    let arity = |range: std::ops::RangeInclusive<usize>| range.contains(&n);
    let v = match name {
        // ----- Text -----
        "LEFT" | "RIGHT" if arity(1..=2) => {
            let count = if n == 2 { num(&args[1], cells) } else { Ok(1.0) };
            match (text(&args[0], cells), count) {
                (Ok(_), Ok(k)) if k < 0.0 => Value::Err("#VALUE!"),
                (Ok(t), Ok(k)) => {
                    let c = chars(&t);
                    let k = (k as usize).min(c.len());
                    let part = if name == "LEFT" { &c[..k] } else { &c[c.len() - k..] };
                    Value::Text(part.iter().collect())
                }
                (Err(e), _) | (_, Err(e)) => Value::Err(e),
            }
        }
        "MID" if n == 3 => match (text(&args[0], cells), num(&args[1], cells), num(&args[2], cells)) {
            (Ok(_), Ok(start), Ok(k)) if start < 1.0 || k < 0.0 => Value::Err("#VALUE!"),
            (Ok(t), Ok(start), Ok(k)) => {
                Value::Text(t.chars().skip(start as usize - 1).take(k as usize).collect())
            }
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => Value::Err(e),
        },
        "FIND" | "SEARCH" if arity(2..=3) => {
            let start = if n == 3 { num(&args[2], cells) } else { Ok(1.0) };
            match (text(&args[0], cells), text(&args[1], cells), start) {
                (Ok(needle), Ok(hay), Ok(start)) => {
                    let (needle, hay) = if name == "SEARCH" {
                        (needle.to_lowercase(), hay.to_lowercase())
                    } else {
                        (needle, hay)
                    };
                    let (h, nd) = (chars(&hay), chars(&needle));
                    let from = (start as usize).max(1) - 1;
                    if start < 1.0 || from > h.len() {
                        Value::Err("#VALUE!")
                    } else {
                        (from..=h.len().saturating_sub(nd.len()))
                            .find(|&i| h.len() >= nd.len() && h[i..i + nd.len()] == nd[..])
                            .map(|i| Value::Num(i as f64 + 1.0))
                            .unwrap_or(Value::Err("#VALUE!"))
                    }
                }
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => Value::Err(e),
            }
        }
        "SUBSTITUTE" if arity(3..=4) => {
            let which = if n == 4 { num(&args[3], cells).map(Some) } else { Ok(None) };
            match (text(&args[0], cells), text(&args[1], cells), text(&args[2], cells), which) {
                (Ok(t), Ok(old), Ok(_), _) if old.is_empty() => Value::Text(t),
                (Ok(t), Ok(old), Ok(new), Ok(None)) => Value::Text(t.replace(&old, &new)),
                (Ok(t), Ok(old), Ok(new), Ok(Some(k))) => {
                    let k = k as usize;
                    match t.match_indices(&old).nth(k.max(1) - 1) {
                        Some((i, _)) if k >= 1 => Value::Text(format!("{}{}{}", &t[..i], new, &t[i + old.len()..])),
                        _ => Value::Text(t),
                    }
                }
                (Err(e), ..) | (_, Err(e), ..) | (_, _, Err(e), _) | (.., Err(e)) => Value::Err(e),
            }
        }
        "REPLACE" if n == 4 => {
            match (text(&args[0], cells), num(&args[1], cells), num(&args[2], cells), text(&args[3], cells)) {
                (Ok(_), Ok(start), Ok(k), Ok(_)) if start < 1.0 || k < 0.0 => Value::Err("#VALUE!"),
                (Ok(t), Ok(start), Ok(k), Ok(new)) => {
                    let c = chars(&t);
                    let a = (start as usize - 1).min(c.len());
                    let b = (a + k as usize).min(c.len());
                    Value::Text(c[..a].iter().collect::<String>() + &new + &c[b..].iter().collect::<String>())
                }
                (Err(e), ..) | (_, Err(e), ..) | (_, _, Err(e), _) | (.., Err(e)) => Value::Err(e),
            }
        }
        "REPT" if n == 2 => match (text(&args[0], cells), num(&args[1], cells)) {
            (Ok(t), Ok(k)) if k >= 0.0 && t.len().checked_mul(k as usize).is_some_and(|n| n <= 32_767) => {
                Value::Text(t.repeat(k as usize))
            }
            (Ok(_), Ok(_)) => Value::Err("#VALUE!"),
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        },
        "PROPER" if n == 1 => text_res(text(&args[0], cells).map(|t| {
            let mut out = String::with_capacity(t.len());
            let mut start = true;
            for ch in t.chars() {
                if start {
                    out.extend(ch.to_uppercase());
                } else {
                    out.extend(ch.to_lowercase());
                }
                start = !ch.is_alphabetic();
            }
            out
        })),
        "VALUE" if n == 1 => match one(&args[0], cells) {
            Value::Num(x) => Value::Num(x),
            Value::Err(e) => Value::Err(e),
            v => {
                let t = v.text();
                parse_number(&t).or_else(|| parse_date(&t)).map(Value::Num).unwrap_or(Value::Err("#VALUE!"))
            }
        },
        "TEXTJOIN" if n >= 3 => {
            let (sep, skip) = match (text(&args[0], cells), one(&args[1], cells).truthy()) {
                (Ok(s), Ok(k)) => (s, k),
                (Err(e), _) | (_, Err(e)) => return Some(Value::Err(e)),
            };
            let mut parts = Vec::new();
            for a in &args[2..] {
                for v in arg(a, cells).values() {
                    if let Value::Err(e) = v {
                        return Some(Value::Err(e));
                    }
                    let t = v.text();
                    if !(skip && t.is_empty()) {
                        parts.push(t);
                    }
                }
            }
            Value::Text(parts.join(&sep))
        }
        "EXACT" if n == 2 => match (text(&args[0], cells), text(&args[1], cells)) {
            (Ok(a), Ok(b)) => Value::Bool(a == b),
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        },

        // ----- Dates and times -----
        "NOW" if n == 0 => Value::Num(cells.today()),
        "DAYS" if n == 2 => res(serial(&args[0], cells).and_then(|e| serial(&args[1], cells).map(|s| e.floor() - s.floor()))),
        "EDATE" | "EOMONTH" if n == 2 => match (serial(&args[0], cells), num(&args[1], cells)) {
            (Ok(s), Ok(m)) => {
                let moved = add_months(s.floor(), m.trunc() as i64);
                if name == "EDATE" {
                    Value::Num(moved)
                } else {
                    let (y, mo, _) = serial_date(moved);
                    Value::Num(date_serial(y, mo, days_in_month(y, mo)))
                }
            }
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        },
        "WEEKDAY" if arity(1..=2) => {
            let kind = if n == 2 { num(&args[1], cells) } else { Ok(1.0) };
            match (serial(&args[0], cells), kind) {
                (Ok(s), Ok(k)) => {
                    let d = weekday(s);
                    match k as i64 {
                        1 => Value::Num(d as f64),
                        // Monday 1 … Sunday 7.
                        2 => Value::Num(((d + 5) % 7 + 1) as f64),
                        // Monday 0 … Sunday 6.
                        3 => Value::Num(((d + 5) % 7) as f64),
                        _ => Value::Err("#NUM!"),
                    }
                }
                (Err(e), _) | (_, Err(e)) => Value::Err(e),
            }
        }
        "WEEKNUM" if arity(1..=2) => {
            let kind = if n == 2 { num(&args[1], cells) } else { Ok(1.0) };
            match (serial(&args[0], cells), kind) {
                (Ok(s), Ok(k)) if k == 1.0 || k == 2.0 => {
                    let (y, _, _) = serial_date(s);
                    let jan1 = date_serial(y, 1, 1);
                    // Days before Jan 1 in its week: weeks start Sunday (1) or Monday (2).
                    let lead = if k == 1.0 { weekday(jan1) - 1 } else { (weekday(jan1) + 5) % 7 };
                    Value::Num((((s.floor() - jan1) as i64 + lead) / 7 + 1) as f64)
                }
                (Ok(_), Ok(_)) => Value::Err("#NUM!"),
                (Err(e), _) | (_, Err(e)) => Value::Err(e),
            }
        }
        "HOUR" | "MINUTE" | "SECOND" if n == 1 => match one(&args[0], cells) {
            Value::Text(t) => match parse_time(&t) {
                Some(f) => time_part(name, f),
                None => Value::Err("#VALUE!"),
            },
            v => match v.num() {
                Ok(x) => time_part(name, x),
                Err(e) => Value::Err(e),
            },
        },
        "NETWORKDAYS" if arity(2..=3) => match (serial(&args[0], cells), serial(&args[1], cells)) {
            (Ok(a), Ok(b)) => {
                let holidays: Vec<f64> = if n == 3 {
                    arg(&args[2], cells)
                        .values()
                        .into_iter()
                        .filter_map(|v| match v {
                            Value::Num(x) => Some(x.floor()),
                            Value::Text(t) => parse_date(&t),
                            _ => None,
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                let (lo, hi, sign) = if a <= b { (a.floor(), b.floor(), 1.0) } else { (b.floor(), a.floor(), -1.0) };
                if hi - lo > 400_000.0 {
                    return Some(Value::Err("#NUM!"));
                }
                let mut count = 0.0;
                let mut d = lo;
                while d <= hi {
                    if (2..=6).contains(&weekday(d)) && !holidays.contains(&d) {
                        count += 1.0;
                    }
                    d += 1.0;
                }
                Value::Num(sign * count)
            }
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        },

        // ----- Maths -----
        "SIGN" if n == 1 => res(num(&args[0], cells).map(|x| if x == 0.0 { 0.0 } else { x.signum() })),
        "CEILING" | "FLOOR" if arity(1..=2) => {
            let sig = if n == 2 { num(&args[1], cells) } else { Ok(1.0) };
            match (num(&args[0], cells), sig) {
                (Ok(_), Ok(0.0)) => Value::Num(0.0),
                (Ok(x), Ok(s)) if x > 0.0 && s < 0.0 => Value::Err("#NUM!"),
                (Ok(x), Ok(s)) => {
                    let q = x / s;
                    // Snap away float noise (2.1/0.1 = 21.000000000000004).
                    let q = if (q - q.round()).abs() < 1e-9 { q.round() } else { q };
                    Value::Num(if name == "CEILING" { q.ceil() } else { q.floor() } * s)
                }
                (Err(e), _) | (_, Err(e)) => Value::Err(e),
            }
        }
        "TRUNC" if arity(1..=2) => {
            let d = if n == 2 { num(&args[1], cells) } else { Ok(0.0) };
            res(num(&args[0], cells).and_then(|x| {
                d.map(|d| {
                    let scale = 10f64.powi(d.trunc().clamp(-15.0, 15.0) as i32);
                    (x * scale).trunc() / scale
                })
            }))
        }
        "EXP" if n == 1 => res(num(&args[0], cells).map(f64::exp)),
        "LN" | "LOG10" if n == 1 => match num(&args[0], cells) {
            Ok(x) if x <= 0.0 => Value::Err("#NUM!"),
            r => res(r.map(if name == "LN" { f64::ln } else { f64::log10 })),
        },
        "LOG" if arity(1..=2) => {
            let base = if n == 2 { num(&args[1], cells) } else { Ok(10.0) };
            match (num(&args[0], cells), base) {
                (Ok(x), Ok(b)) if x <= 0.0 || b <= 0.0 || b == 1.0 => Value::Err("#NUM!"),
                (Ok(x), Ok(b)) => res(Ok(x.log(b))),
                (Err(e), _) | (_, Err(e)) => Value::Err(e),
            }
        }
        "PI" if n == 0 => Value::Num(std::f64::consts::PI),
        "RAND" if n == 0 => Value::Num(random()),
        "RANDBETWEEN" if n == 2 => match (num(&args[0], cells), num(&args[1], cells)) {
            (Ok(a), Ok(b)) if a.ceil() <= b.floor() => {
                let (a, b) = (a.ceil(), b.floor());
                Value::Num((a + (random() * (b - a + 1.0)).floor()).min(b))
            }
            (Ok(_), Ok(_)) => Value::Err("#NUM!"),
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        },
        "SUMPRODUCT" if n >= 1 => {
            let lists: Vec<Vec<Value>> = args.iter().map(|a| arg(a, cells).values()).collect();
            let len = lists[0].len();
            if lists.iter().any(|l| l.len() != len) {
                return Some(Value::Err("#VALUE!"));
            }
            let mut total = 0.0;
            for i in 0..len {
                let mut p = 1.0;
                for l in &lists {
                    match &l[i] {
                        Value::Num(x) => p *= x,
                        Value::Err(e) => return Some(Value::Err(e)),
                        _ => p = 0.0,
                    }
                }
                total += p;
            }
            Value::Num(total)
        }

        // ----- Statistics -----
        "STDEV" | "VAR" if n >= 1 => match numbers(args, cells).and_then(|v| sample_variance(&v)) {
            Ok(var) => Value::Num(if name == "STDEV" { var.sqrt() } else { var }),
            Err(e) => Value::Err(e),
        },
        "MODE" if n >= 1 => match numbers(args, cells) {
            Ok(v) => {
                // The most frequent value; the first one seen wins a tie.
                let mut best: Option<(f64, usize)> = None;
                for (i, x) in v.iter().enumerate() {
                    let count = v.iter().filter(|y| *y == x).count();
                    if count > 1 && best.is_none_or(|(_, c)| count > c) && !v[..i].contains(x) {
                        best = Some((*x, count));
                    }
                }
                best.map(|(x, _)| Value::Num(x)).unwrap_or(Value::Err("#N/A"))
            }
            Err(e) => Value::Err(e),
        },
        "LARGE" | "SMALL" if n == 2 => match (numbers(&args[..1], cells), num(&args[1], cells)) {
            (Ok(mut v), Ok(k)) => {
                let k = k.ceil() as usize;
                if k == 0 || k > v.len() {
                    Value::Err("#NUM!")
                } else {
                    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    Value::Num(if name == "SMALL" { v[k - 1] } else { v[v.len() - k] })
                }
            }
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        },
        "RANK" if arity(2..=3) => {
            let ascending = if n == 3 { num(&args[2], cells).map(|o| o != 0.0) } else { Ok(false) };
            match (num(&args[0], cells), numbers(&args[1..2], cells), ascending) {
                (Ok(x), Ok(v), Ok(asc)) => {
                    if !v.contains(&x) {
                        Value::Err("#N/A")
                    } else {
                        let better = v.iter().filter(|y| if asc { **y < x } else { **y > x }).count();
                        Value::Num(better as f64 + 1.0)
                    }
                }
                (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => Value::Err(e),
            }
        }
        "COUNTBLANK" if n == 1 => Value::Num(
            arg(&args[0], cells).values().iter().filter(|v| matches!(v, Value::Empty) || v.text().is_empty()).count()
                as f64,
        ),
        "COUNTIFS" if n >= 2 => match passing(args, cells) {
            Ok(p) => Value::Num(p.iter().filter(|x| **x).count() as f64),
            Err(v) => v,
        },
        "SUMIFS" | "AVERAGEIFS" | "MAXIFS" | "MINIFS" if n >= 3 => {
            let target = arg(&args[0], cells).values();
            let pass = match passing(&args[1..], cells) {
                Ok(p) if p.len() == target.len() => p,
                Ok(_) => return Some(Value::Err("#VALUE!")),
                Err(v) => return Some(v),
            };
            let picked: Vec<f64> = target
                .iter()
                .zip(&pass)
                .filter_map(|(v, ok)| if *ok { if let Value::Num(x) = v { Some(*x) } else { None } } else { None })
                .collect();
            match name {
                "SUMIFS" => Value::Num(picked.iter().sum()),
                "AVERAGEIFS" if picked.is_empty() => Value::Err("#DIV/0!"),
                "AVERAGEIFS" => Value::Num(picked.iter().sum::<f64>() / picked.len() as f64),
                _ if picked.is_empty() => Value::Num(0.0),
                "MAXIFS" => Value::Num(picked.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
                _ => Value::Num(picked.iter().cloned().fold(f64::INFINITY, f64::min)),
            }
        }

        // ----- Lookup and logic -----
        "INDEX" if arity(2..=3) => {
            let Arg::Grid { rows, cols, values } = arg(&args[0], cells) else {
                return Some(Value::Err("#REF!"));
            };
            let r = num(&args[1], cells);
            let c = if n == 3 { num(&args[2], cells) } else { Ok(if rows == 1 { 0.0 } else { 1.0 }) };
            match (r, c) {
                (Ok(r), Ok(c)) => {
                    // INDEX(row of cells, k) counts along the row.
                    let (r, c) = if rows == 1 && n == 2 { (1.0, r) } else { (r, c) };
                    let (r, c) = (r as usize, c.max(1.0) as usize);
                    if r == 0 || r > rows || c > cols {
                        Value::Err("#REF!")
                    } else {
                        match values[(r - 1) * cols + c - 1].clone() {
                            Value::Empty => Value::Num(0.0),
                            v => v,
                        }
                    }
                }
                (Err(e), _) | (_, Err(e)) => Value::Err(e),
            }
        }
        "MATCH" if arity(2..=3) => {
            let key = one(&args[0], cells);
            let list = arg(&args[1], cells).values();
            let kind = if n == 3 { num(&args[2], cells) } else { Ok(1.0) };
            let kind = match kind {
                Ok(k) => (k > 0.0) as i64 - (k < 0.0) as i64,
                Err(e) => return Some(Value::Err(e)),
            };
            let pos = match kind {
                0 => list.iter().position(|v| !matches!(v, Value::Empty) && match_eq(v, &key)),
                // Sorted ascending: the last value not above the key.
                1 => list
                    .iter()
                    .enumerate()
                    .take_while(|(_, v)| matches!(v, Value::Empty) || compare(v, &key).is_le())
                    .filter(|(_, v)| !matches!(v, Value::Empty))
                    .map(|(i, _)| i)
                    .last(),
                // Sorted descending: the last value not below the key.
                _ => list
                    .iter()
                    .enumerate()
                    .take_while(|(_, v)| matches!(v, Value::Empty) || compare(v, &key).is_ge())
                    .filter(|(_, v)| !matches!(v, Value::Empty))
                    .map(|(i, _)| i)
                    .last(),
            };
            pos.map(|i| Value::Num(i as f64 + 1.0)).unwrap_or(Value::Err("#N/A"))
        }
        "CHOOSE" if n >= 2 => match num(&args[0], cells) {
            Ok(k) if k >= 1.0 && (k as usize) < n => eval(&args[k as usize], cells),
            Ok(_) => Value::Err("#VALUE!"),
            Err(e) => Value::Err(e),
        },
        "HLOOKUP" if arity(3..=4) => {
            let key = one(&args[0], cells);
            let Arg::Grid { rows, cols, values } = arg(&args[1], cells) else {
                return Some(Value::Err("#REF!"));
            };
            let row = match num(&args[2], cells) {
                Ok(r) if r >= 1.0 && (r as usize) <= rows => r as usize - 1,
                Ok(_) => return Some(Value::Err("#REF!")),
                Err(e) => return Some(Value::Err(e)),
            };
            let approx = if n == 4 { one(&args[3], cells).truthy().unwrap_or(false) } else { false };
            let mut found = None;
            for (c, v) in values.iter().take(cols).enumerate() {
                if matches!(v, Value::Empty) {
                    continue;
                }
                if approx {
                    if compare(v, &key).is_le() {
                        found = Some(c);
                    } else {
                        break;
                    }
                } else if compare(v, &key).is_eq() {
                    found = Some(c);
                    break;
                }
            }
            found.map(|c| values[row * cols + c].clone()).unwrap_or(Value::Err("#N/A"))
        }
        "IFS" if n >= 2 && n.is_multiple_of(2) => {
            for p in args.chunks(2) {
                match eval(&p[0], cells).truthy() {
                    Ok(true) => return Some(eval(&p[1], cells)),
                    Ok(false) => {}
                    Err(e) => return Some(Value::Err(e)),
                }
            }
            Value::Err("#N/A")
        }
        "SWITCH" if n >= 3 => {
            let v = eval(&args[0], cells);
            if let Value::Err(e) = v {
                return Some(Value::Err(e));
            }
            let pairs = &args[1..];
            for p in pairs.chunks(2) {
                if p.len() == 1 {
                    return Some(eval(&p[0], cells));
                }
                if match_eq(&eval(&p[0], cells), &v) {
                    return Some(eval(&p[1], cells));
                }
            }
            Value::Err("#N/A")
        }
        "XOR" if n >= 1 => {
            let mut odd = false;
            for a in args {
                for v in arg(a, cells).values() {
                    if matches!(v, Value::Empty) {
                        continue;
                    }
                    match v.truthy() {
                        Ok(t) => odd ^= t,
                        Err(e) => return Some(Value::Err(e)),
                    }
                }
            }
            Value::Bool(odd)
        }
        "ISBLANK" if n == 1 => Value::Bool(matches!(one(&args[0], cells), Value::Empty)),
        "ISNUMBER" if n == 1 => Value::Bool(matches!(one(&args[0], cells), Value::Num(_))),
        "ISTEXT" if n == 1 => Value::Bool(matches!(one(&args[0], cells), Value::Text(_))),
        "ISERROR" if n == 1 => Value::Bool(one(&args[0], cells).is_err()),
        "ISNA" if n == 1 => Value::Bool(one(&args[0], cells) == Value::Err("#N/A")),
        "IFNA" if n == 2 => match eval(&args[0], cells) {
            Value::Err("#N/A") => eval(&args[1], cells),
            v => v,
        },

        // Known names with the wrong number of arguments.
        _ if NAMES.contains(&name) => BAD,
        _ => return None,
    };
    Some(v)
}

/// Equal for lookups: numbers by value, text ignoring case.
fn match_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Num(_), Value::Text(_)) | (Value::Text(_), Value::Num(_)) => false,
        _ => compare(a, b).is_eq(),
    }
}

/// "14:30" or "2:30:15".
fn parse_time(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    let n: Vec<f64> = parts.iter().map(|p| p.trim().parse::<f64>().ok()).collect::<Option<_>>()?;
    let secs = n[0] * 3600.0 + n[1] * 60.0 + n.get(2).copied().unwrap_or(0.0);
    Some(secs / 86400.0)
}

fn time_part(name: &str, x: f64) -> Value {
    if x < 0.0 {
        return Value::Err("#NUM!");
    }
    // Round to the second so 0.5 days is exactly 12:00:00.
    let secs = ((x - x.floor()) * 86400.0).round() as i64 % 86400;
    Value::Num(match name {
        "HOUR" => secs / 3600,
        "MINUTE" => secs / 60 % 60,
        _ => secs % 60,
    } as f64)
}

/// Every name handled here, so a wrong argument count reads #ERROR! rather than #NAME?.
pub const NAMES: &[&str] = &[
    "LEFT", "RIGHT", "MID", "FIND", "SEARCH", "SUBSTITUTE", "REPLACE", "REPT", "PROPER", "VALUE", "TEXTJOIN", "EXACT",
    "NOW", "DAYS", "EDATE", "EOMONTH", "WEEKDAY", "WEEKNUM", "HOUR", "MINUTE", "SECOND", "NETWORKDAYS", "SIGN",
    "CEILING", "FLOOR", "TRUNC", "EXP", "LN", "LOG", "LOG10", "PI", "RAND", "RANDBETWEEN", "SUMPRODUCT", "STDEV", "VAR",
    "MODE", "LARGE", "SMALL", "RANK", "COUNTBLANK", "COUNTIFS", "SUMIFS", "AVERAGEIFS", "MAXIFS", "MINIFS", "INDEX",
    "MATCH", "CHOOSE", "HLOOKUP", "IFS", "SWITCH", "XOR", "ISBLANK", "ISNUMBER", "ISTEXT", "ISERROR", "ISNA", "IFNA",
];

#[cfg(test)]
mod tests {
    use super::super::formula::{Cells, Value, date_serial, parse};
    use std::collections::HashMap;

    struct Grid(HashMap<(u32, u32), Value>);

    impl Cells for Grid {
        fn get(&self, _sheet: Option<&str>, r: u32, c: u32) -> Value {
            self.0.get(&(r, c)).cloned().unwrap_or(Value::Empty)
        }
        fn today(&self) -> f64 {
            date_serial(2026, 10, 5) + 0.6
        }
    }

    /// A1:A5 = 3, 1, 4, 1, 5; B1:B5 = a, b, c, d, e; C1:C3 = x, y, x.
    fn grid() -> Grid {
        let mut m = HashMap::new();
        for (i, x) in [3.0, 1.0, 4.0, 1.0, 5.0].iter().enumerate() {
            m.insert((i as u32, 0), Value::Num(*x));
        }
        for (i, t) in ["a", "b", "c", "d", "e"].iter().enumerate() {
            m.insert((i as u32, 1), Value::Text(t.to_string()));
        }
        for (i, t) in ["x", "y", "x"].iter().enumerate() {
            m.insert((i as u32, 2), Value::Text(t.to_string()));
        }
        Grid(m)
    }

    fn ev(src: &str) -> Value {
        super::super::formula::eval(&parse(src.trim_start_matches('=')), &grid())
    }

    fn n(x: f64) -> Value {
        Value::Num(x)
    }

    fn t(s: &str) -> Value {
        Value::Text(s.into())
    }

    #[test]
    fn text_functions() {
        assert_eq!(ev("=LEFT(\"héllo\", 2)"), t("hé"));
        assert_eq!(ev("=RIGHT(\"hello\")"), t("o"));
        assert_eq!(ev("=MID(\"spreadsheet\", 7, 5)"), t("sheet"));
        assert_eq!(ev("=FIND(\"l\", \"hello\")"), n(3.0));
        assert_eq!(ev("=FIND(\"L\", \"hello\")"), Value::Err("#VALUE!"));
        assert_eq!(ev("=SEARCH(\"L\", \"hello\", 4)"), n(4.0));
        assert_eq!(ev("=SUBSTITUTE(\"a-b-c\", \"-\", \"+\")"), t("a+b+c"));
        assert_eq!(ev("=SUBSTITUTE(\"a-b-c\", \"-\", \"+\", 2)"), t("a-b+c"));
        assert_eq!(ev("=REPLACE(\"abcdef\", 2, 3, \"X\")"), t("aXef"));
        assert_eq!(ev("=REPT(\"ab\", 3)"), t("ababab"));
        assert_eq!(ev("=REPT(\"abcd\", 4611686018427387904)"), Value::Err("#VALUE!"));
        assert_eq!(ev("=PROPER(\"hello wORLD-wide\")"), t("Hello World-Wide"));
        assert_eq!(ev("=VALUE(\"1,250.5\")"), n(1250.5));
        assert_eq!(ev("=TEXTJOIN(\", \", TRUE, B1:B3, \"\", \"z\")"), t("a, b, c, z"));
        assert_eq!(ev("=EXACT(\"a\", \"A\")"), Value::Bool(false));
    }

    #[test]
    fn date_functions() {
        let d = |y, m, dd| n(date_serial(y, m, dd));
        assert_eq!(ev("=DAYS(\"2026-12-25\", \"2026-10-05\")"), n(81.0));
        assert_eq!(ev("=EDATE(\"2026-01-31\", 1)"), d(2026, 2, 28));
        assert_eq!(ev("=EOMONTH(\"2024-01-15\", 1)"), d(2024, 2, 29));
        assert_eq!(ev("=EOMONTH(\"2026-03-10\", -3)"), d(2025, 12, 31));
        // 2026-10-05 is a Monday.
        assert_eq!(ev("=WEEKDAY(\"2026-10-05\")"), n(2.0));
        assert_eq!(ev("=WEEKDAY(\"2026-10-05\", 2)"), n(1.0));
        assert_eq!(ev("=WEEKNUM(\"2026-01-01\")"), n(1.0));
        assert_eq!(ev("=WEEKNUM(\"2026-01-04\")"), n(2.0));
        assert_eq!(ev("=HOUR(NOW())"), n(14.0));
        assert_eq!(ev("=MINUTE(\"14:35\")"), n(35.0));
        assert_eq!(ev("=NETWORKDAYS(\"2026-10-05\", \"2026-10-18\")"), n(10.0));
        assert_eq!(ev("=NETWORKDAYS(\"2026-10-05\", \"2026-10-09\", \"2026-10-07\")"), n(4.0));
    }

    #[test]
    fn maths_functions() {
        assert_eq!(ev("=SIGN(-4)"), n(-1.0));
        assert_eq!(ev("=CEILING(2.1, 0.5)"), n(2.5));
        assert_eq!(ev("=FLOOR(7, 3)"), n(6.0));
        assert_eq!(ev("=TRUNC(-3.789, 1)"), n(-3.7));
        assert_eq!(ev("=LOG(8, 2)"), n(3.0));
        assert_eq!(ev("=LN(0)"), Value::Err("#NUM!"));
        assert_eq!(ev("=LOG10(1000)"), n(3.0));
        assert_eq!(ev("=SUMPRODUCT(A1:A3, A3:A5)"), n(3.0 * 4.0 + 1.0 + 4.0 * 5.0));
        let Value::Num(r) = ev("=RANDBETWEEN(1, 6)") else { panic!() };
        assert!((1.0..=6.0).contains(&r) && r == r.trunc());
        let Value::Num(r) = ev("=RAND()") else { panic!() };
        assert!((0.0..1.0).contains(&r));
    }

    #[test]
    fn statistics() {
        assert_eq!(ev("=LARGE(A1:A5, 2)"), n(4.0));
        assert_eq!(ev("=SMALL(A1:A5, 2)"), n(1.0));
        assert_eq!(ev("=MODE(A1:A5)"), n(1.0));
        assert_eq!(ev("=RANK(4, A1:A5)"), n(2.0));
        assert_eq!(ev("=RANK(4, A1:A5, 1)"), n(4.0));
        assert_eq!(ev("=VAR(2, 4, 4, 4, 5, 5, 7, 9)"), n(32.0 / 7.0));
        assert_eq!(ev("=STDEV(1)"), Value::Err("#DIV/0!"));
        assert_eq!(ev("=COUNTBLANK(C1:C5)"), n(2.0));
        assert_eq!(ev("=COUNTIFS(C1:C3, \"x\", A1:A3, \">2\")"), n(2.0));
        assert_eq!(ev("=SUMIFS(A1:A3, C1:C3, \"x\")"), n(7.0));
        assert_eq!(ev("=AVERAGEIFS(A1:A5, A1:A5, \"<4\")"), n(5.0 / 3.0));
        assert_eq!(ev("=MAXIFS(A1:A3, C1:C3, \"x\")"), n(4.0));
        assert_eq!(ev("=MINIFS(A1:A3, C1:C3, \"zz\")"), n(0.0));
    }

    #[test]
    fn lookups_and_logic() {
        assert_eq!(ev("=INDEX(A1:B5, 3, 2)"), t("c"));
        assert_eq!(ev("=INDEX(B1:B5, 4)"), t("d"));
        assert_eq!(ev("=INDEX(A1:B5, 9, 1)"), Value::Err("#REF!"));
        assert_eq!(ev("=MATCH(\"C\", B1:B5, 0)"), n(3.0));
        assert_eq!(ev("=INDEX(A1:A5, MATCH(\"e\", B1:B5, 0))"), n(5.0));
        assert_eq!(ev("=CHOOSE(2, \"a\", \"b\", \"c\")"), t("b"));
        assert_eq!(ev("=CHOOSE(4, \"a\", \"b\", \"c\")"), Value::Err("#VALUE!"));
        assert_eq!(ev("=IFS(A1>5, \"big\", A1>2, \"mid\", TRUE, \"small\")"), t("mid"));
        assert_eq!(ev("=SWITCH(B2, \"a\", 1, \"b\", 2, 0)"), n(2.0));
        assert_eq!(ev("=SWITCH(B3, \"a\", 1, \"b\", 2, 0)"), n(0.0));
        assert_eq!(ev("=XOR(TRUE, TRUE, TRUE)"), Value::Bool(true));
        assert_eq!(ev("=ISBLANK(D1)"), Value::Bool(true));
        assert_eq!(ev("=ISNUMBER(A1)"), Value::Bool(true));
        assert_eq!(ev("=ISTEXT(A1)"), Value::Bool(false));
        assert_eq!(ev("=ISERROR(1/0)"), Value::Bool(true));
        assert_eq!(ev("=IFNA(MATCH(\"q\", B1:B5, 0), \"none\")"), t("none"));
        assert_eq!(ev("=LEFT()"), Value::Err("#ERROR!"));
    }
}
