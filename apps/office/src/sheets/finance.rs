//! Money functions: loan payments and their interest, savings, present and
//! future values, net present value and rates of return. They work as in other
//! spreadsheets: money paid out is negative, money received positive, and `type`
//! 1 means payments at the start of each period (0, the default, at the end).

use super::formula::{Arg, Cells, Expr, Value, arg};

fn num(e: &Expr, cells: &dyn Cells) -> Result<f64, &'static str> {
    match arg(e, cells) {
        Arg::One(v) => v.num(),
        Arg::Grid { values, .. } => values.first().cloned().unwrap_or(Value::Empty).num(),
    }
}

/// Argument `i` as a number, or `default` when it's left out.
fn opt(args: &[Expr], i: usize, default: f64, cells: &dyn Cells) -> Result<f64, &'static str> {
    args.get(i).map_or(Ok(default), |e| num(e, cells))
}

/// The number cells of the arguments, in order (blank and text cells skipped).
fn flows(args: &[Expr], cells: &dyn Cells) -> Result<Vec<f64>, &'static str> {
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

/// (1 + r)^n, and the factor that turns a payment into its total over n periods.
fn growth(r: f64, n: f64, kind: f64) -> (f64, f64) {
    let g = (1.0 + r).powf(n);
    let annuity = if r == 0.0 { n } else { (1.0 + r * kind) * (g - 1.0) / r };
    (g, annuity)
}

pub fn pmt(r: f64, n: f64, pv: f64, fv: f64, kind: f64) -> f64 {
    let (g, annuity) = growth(r, n, kind);
    -(pv * g + fv) / annuity
}

pub fn fv(r: f64, n: f64, pmt: f64, pv: f64, kind: f64) -> f64 {
    let (g, annuity) = growth(r, n, kind);
    -(pv * g + pmt * annuity)
}

pub fn pv(r: f64, n: f64, pmt: f64, fv: f64, kind: f64) -> f64 {
    let (g, annuity) = growth(r, n, kind);
    -(fv + pmt * annuity) / g
}

/// The interest in payment `per` (from 1).
fn ipmt(r: f64, per: f64, n: f64, pv: f64, fv: f64, kind: f64) -> f64 {
    let p = pmt(r, n, pv, fv, kind);
    if kind != 0.0 {
        if per == 1.0 {
            return 0.0;
        }
        return (self::fv(r, per - 2.0, p, pv, 1.0) - p) * r;
    }
    self::fv(r, per - 1.0, p, pv, 0.0) * r
}

/// Newton's method from `guess`; None when it doesn't settle.
fn solve(guess: f64, f: impl Fn(f64) -> f64) -> Option<f64> {
    let mut x = guess;
    for _ in 0..100 {
        let y = f(x);
        let h = 1e-7 * x.abs().max(1e-4);
        let slope = (f(x + h) - f(x - h)) / (2.0 * h);
        if !slope.is_finite() || slope == 0.0 {
            return None;
        }
        let next = x - y / slope;
        if !next.is_finite() || next <= -1.0 {
            return None;
        }
        if (next - x).abs() < 1e-10 {
            return Some(next);
        }
        x = next;
    }
    None
}

fn npv(r: f64, values: &[f64]) -> f64 {
    values.iter().enumerate().map(|(i, v)| v / (1.0 + r).powi(i as i32 + 1)).sum()
}

fn done(x: f64) -> Value {
    if x.is_finite() { Value::Num(x) } else { Value::Err("#NUM!") }
}

/// Every name handled here.
pub const NAMES: &[&str] =
    &["PMT", "IPMT", "PPMT", "PV", "FV", "NPER", "RATE", "NPV", "IRR", "EFFECT", "NOMINAL", "SLN"];

pub fn call(name: &str, args: &[Expr], cells: &dyn Cells) -> Option<Value> {
    if !NAMES.contains(&name) {
        return None;
    }
    let n = args.len();
    let arity = |lo: usize, hi: usize| (lo..=hi).contains(&n);
    let r = (|| -> Result<Value, &'static str> {
        Ok(match name {
            "PMT" if arity(3, 5) => done(pmt(
                num(&args[0], cells)?,
                num(&args[1], cells)?,
                num(&args[2], cells)?,
                opt(args, 3, 0.0, cells)?,
                opt(args, 4, 0.0, cells)?,
            )),
            "FV" if arity(3, 5) => done(fv(
                num(&args[0], cells)?,
                num(&args[1], cells)?,
                num(&args[2], cells)?,
                opt(args, 3, 0.0, cells)?,
                opt(args, 4, 0.0, cells)?,
            )),
            "PV" if arity(3, 5) => done(pv(
                num(&args[0], cells)?,
                num(&args[1], cells)?,
                num(&args[2], cells)?,
                opt(args, 3, 0.0, cells)?,
                opt(args, 4, 0.0, cells)?,
            )),
            "IPMT" | "PPMT" if arity(4, 6) => {
                let (r, per, nper, pv) =
                    (num(&args[0], cells)?, num(&args[1], cells)?, num(&args[2], cells)?, num(&args[3], cells)?);
                let (fv, kind) = (opt(args, 4, 0.0, cells)?, opt(args, 5, 0.0, cells)?);
                if per < 1.0 || per > nper {
                    return Ok(Value::Err("#NUM!"));
                }
                let interest = ipmt(r, per, nper, pv, fv, kind);
                done(if name == "IPMT" { interest } else { pmt(r, nper, pv, fv, kind) - interest })
            }
            "NPER" if arity(3, 5) => {
                let (r, p, pv) = (num(&args[0], cells)?, num(&args[1], cells)?, num(&args[2], cells)?);
                let (fv, kind) = (opt(args, 3, 0.0, cells)?, opt(args, 4, 0.0, cells)?);
                if r == 0.0 {
                    if p == 0.0 {
                        return Ok(Value::Err("#NUM!"));
                    }
                    done(-(pv + fv) / p)
                } else {
                    let a = p * (1.0 + r * kind);
                    done(((a - fv * r) / (a + pv * r)).ln() / (1.0 + r).ln())
                }
            }
            "RATE" if arity(3, 6) => {
                let (nper, p, pv) = (num(&args[0], cells)?, num(&args[1], cells)?, num(&args[2], cells)?);
                let (fv, kind, guess) = (opt(args, 3, 0.0, cells)?, opt(args, 4, 0.0, cells)?, opt(args, 5, 0.1, cells)?);
                let f = |r: f64| {
                    let (g, annuity) = growth(r, nper, kind);
                    pv * g + p * annuity + fv
                };
                solve(guess, f).map_or(Value::Err("#NUM!"), done)
            }
            "NPV" if n >= 2 => done(npv(num(&args[0], cells)?, &flows(&args[1..], cells)?)),
            "IRR" if arity(1, 2) => {
                let v = flows(&args[..1], cells)?;
                if !(v.iter().any(|x| *x > 0.0) && v.iter().any(|x| *x < 0.0)) {
                    return Ok(Value::Err("#NUM!"));
                }
                // NPV counts from the first period; IRR's first value is now.
                let f = |r: f64| npv(r, &v) * (1.0 + r);
                solve(opt(args, 1, 0.1, cells)?, f).map_or(Value::Err("#NUM!"), done)
            }
            "EFFECT" | "NOMINAL" if n == 2 => {
                let (rate, per) = (num(&args[0], cells)?, num(&args[1], cells)?.trunc());
                if rate <= 0.0 || per < 1.0 {
                    return Ok(Value::Err("#NUM!"));
                }
                done(if name == "EFFECT" {
                    (1.0 + rate / per).powf(per) - 1.0
                } else {
                    per * ((1.0 + rate).powf(1.0 / per) - 1.0)
                })
            }
            "SLN" if n == 3 => {
                let (cost, salvage, life) = (num(&args[0], cells)?, num(&args[1], cells)?, num(&args[2], cells)?);
                if life == 0.0 {
                    return Ok(Value::Err("#DIV/0!"));
                }
                done((cost - salvage) / life)
            }
            // A known name with the wrong number of arguments.
            _ => Value::Err("#ERROR!"),
        })
    })();
    Some(r.unwrap_or_else(Value::Err))
}

#[cfg(test)]
mod tests {
    use super::super::formula::{Cells, Value, eval, parse};
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

    /// A1:A6 = -70000, 12000, 15000, 18000, 21000, 26000.
    fn ev(src: &str) -> f64 {
        let flows = [-70000.0, 12000.0, 15000.0, 18000.0, 21000.0, 26000.0];
        let g = Grid(flows.iter().enumerate().map(|(i, v)| ((i as u32, 0), Value::Num(*v))).collect());
        match eval(&parse(src), &g) {
            Value::Num(x) => x,
            v => panic!("{src} gave {v:?}"),
        }
    }

    fn close(a: f64, b: f64, places: i32) {
        let tol = 0.5 * 10f64.powi(-places);
        assert!((a - b).abs() <= tol, "{a} isn't {b}");
    }

    // The worked examples from Excel's documentation.
    #[test]
    fn loans_and_savings() {
        close(ev("PMT(8%/12, 10, 10000)"), -1037.03, 2);
        close(ev("PMT(6%/12, 18*12, 0, 50000)"), -129.08, 2);
        close(ev("PMT(0, 10, 1000)"), -100.0, 2);
        close(ev("FV(0.06/12, 10, -200, -500, 1)"), 2581.40, 2);
        close(ev("FV(0.12/12, 12, -1000)"), 12682.50, 2);
        close(ev("PV(0.08/12, 12*20, 500)"), -59777.15, 2);
        close(ev("NPER(0.12/12, -100, -1000, 10000, 1)"), 59.6739, 4);
        close(ev("NPER(0.12/12, -100, -1000)"), -9.57859, 5);
        close(ev("RATE(4*12, -200, 8000)"), 0.007701, 6);
        close(ev("IPMT(0.1/12, 1, 3*12, 8000)"), -66.67, 2);
        close(ev("IPMT(0.1, 3, 3, 8000)"), -292.45, 2);
        close(ev("PPMT(0.1/12, 1, 2*12, 2000)"), -75.62, 2);
        close(ev("PPMT(0.08, 10, 10, 200000)"), -27598.05, 2);
    }

    #[test]
    fn returns_and_rates() {
        close(ev("NPV(0.1, -10000, 3000, 4200, 6800)"), 1188.44, 2);
        close(ev("IRR(A1:A5)"), -0.021245, 6);
        close(ev("IRR(A1:A6)"), 0.086631, 6);
        close(ev("EFFECT(0.0525, 4)"), 0.053543, 6);
        close(ev("NOMINAL(0.053543, 4)"), 0.052500, 6);
        close(ev("SLN(30000, 7500, 10)"), 2250.0, 2);
    }

    #[test]
    fn bad_input() {
        let g = Grid(HashMap::new());
        assert_eq!(eval(&parse("IPMT(0.1, 5, 3, 8000)"), &g), Value::Err("#NUM!"));
        assert_eq!(eval(&parse("IRR(5)"), &g), Value::Err("#NUM!"));
        assert_eq!(eval(&parse("PMT(1)"), &g), Value::Err("#ERROR!"));
        assert_eq!(eval(&parse("SLN(1, 0, 0)"), &g), Value::Err("#DIV/0!"));
    }
}
