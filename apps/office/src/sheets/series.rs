//! Filling a series: what comes next after 1, 2 or Jan or Monday or Item 1.

use super::formula::{parse_date, serial_date};

const MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// A name from `list` (in full or its first three letters): its place, and
/// whether it was short.
fn named(s: &str, list: &[&str]) -> Option<(i64, bool)> {
    let t = s.trim().to_lowercase();
    list.iter().enumerate().find_map(|(i, n)| {
        let n = n.to_lowercase();
        if t == n {
            Some((i as i64, false))
        } else if t.len() == 3 && n.starts_with(&t) {
            Some((i as i64, true))
        } else {
            None
        }
    })
}

/// Written in the way `like` was: "jan", "Jan", "JAN", "January".
fn styled(name: &str, short: bool, like: &str) -> String {
    let n = if short { &name[..3] } else { name };
    if like.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) && like.chars().any(|c| c.is_alphabetic()) && like.len() > 1 {
        n.to_uppercase()
    } else if like.starts_with(|c: char| c.is_lowercase()) {
        n.to_lowercase()
    } else {
        n.to_string()
    }
}

/// "Item 7" → ("Item ", 7): text ending in a whole number.
fn numbered(s: &str) -> Option<(&str, i64)> {
    let digits = s.len() - s.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 || digits == s.len() || digits > 9 {
        return None;
    }
    let (head, n) = s.split_at(s.len() - digits);
    Some((head, n.parse().ok()?))
}

/// The step between source values: how far the last is from the first, per place.
fn step(first: f64, last: f64, n: usize, single: f64) -> f64 {
    if n < 2 { single } else { (last - first) / (n - 1) as f64 }
}

fn number_text(v: f64) -> String {
    let r = (v * 1e9).round() / 1e9;
    if r == r.trunc() && r.abs() < 1e15 { format!("{}", r as i64) } else { format!("{r}") }
}

/// What goes `t` places from the first of the source values `vals` (t may be
/// before it, negative, or past the last): None to copy the source instead.
pub fn next(vals: &[&str], t: i64) -> Option<String> {
    let n = vals.len();
    if n == 0 || vals.iter().any(|v| v.starts_with('=') || v.trim().is_empty()) {
        return None;
    }
    let first = vals[0].trim();
    // Plain numbers: on by the step between them (one alone is copied).
    let nums: Option<Vec<f64>> = vals.iter().map(|v| v.trim().parse::<f64>().ok()).collect();
    if let Some(nums) = nums {
        if n < 2 {
            return None;
        }
        let d = step(nums[0], nums[n - 1], n, 0.0);
        return Some(number_text(nums[0] + d * t as f64));
    }
    // Dates: a day at a time, or by the gap between them.
    let dates: Option<Vec<f64>> = vals.iter().map(|v| parse_date(v)).collect();
    if let Some(dates) = dates {
        let d = step(dates[0], dates[n - 1], n, 1.0).round();
        let (y, m, day) = serial_date(dates[0] + d * t as f64);
        // Written as the source was: 2026-10-05 or 10/5/2026.
        return Some(if first.contains('-') { format!("{y:04}-{m:02}-{day:02}") } else { format!("{m}/{day}/{y}") });
    }
    for list in [&MONTHS[..], &DAYS[..]] {
        let places: Option<Vec<(i64, bool)>> = vals.iter().map(|v| named(v, list)).collect();
        if let Some(places) = places {
            let len = list.len() as i64;
            // The step goes forwards around the year or week: Nov, Dec, Jan.
            let d = if n < 2 { 1 } else { ((places[n - 1].0 - places[0].0).rem_euclid(len)) / (n as i64 - 1).max(1) };
            let i = (places[0].0 + d * t).rem_euclid(len) as usize;
            return Some(styled(list[i], places[0].1, first));
        }
    }
    // Text ending in a number, the rest the same: Item 1, Item 2.
    let parts: Option<Vec<(&str, i64)>> = vals.iter().map(|v| numbered(v.trim())).collect();
    if let Some(parts) = parts
        && parts.iter().all(|(h, _)| *h == parts[0].0)
    {
        let d = step(parts[0].1 as f64, parts[n - 1].1 as f64, n, 1.0).round() as i64;
        let v = parts[0].1 + d * t;
        return (v >= 0).then(|| format!("{}{v}", parts[0].0));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::next;

    fn run(vals: &[&str], from: i64, count: i64) -> Vec<String> {
        (from..from + count).map(|t| next(vals, t).unwrap_or_else(|| "·".into())).collect()
    }

    #[test]
    fn series() {
        assert_eq!(run(&["1", "2"], 2, 3), ["3", "4", "5"]);
        assert_eq!(run(&["5", "10"], 2, 2), ["15", "20"]);
        assert_eq!(run(&["0.5", "1"], 2, 1), ["1.5"]);
        assert_eq!(next(&["7"], 1), None);
        assert_eq!(run(&["Jan"], 1, 3), ["Feb", "Mar", "Apr"]);
        assert_eq!(run(&["November"], 1, 3), ["December", "January", "February"]);
        assert_eq!(run(&["JAN", "MAR"], 2, 2), ["MAY", "JUL"]);
        assert_eq!(run(&["mon"], 1, 2), ["tue", "wed"]);
        assert_eq!(run(&["Friday"], 1, 3), ["Saturday", "Sunday", "Monday"]);
        assert_eq!(run(&["2026-10-05"], 1, 2), ["2026-10-06", "2026-10-07"]);
        assert_eq!(run(&["10/30/2026", "11/6/2026"], 2, 1), ["11/13/2026"]);
        assert_eq!(run(&["Item 1"], 1, 2), ["Item 2", "Item 3"]);
        assert_eq!(run(&["Q1", "Q3"], 2, 1), ["Q5"]);
        // Backwards (filling up or left).
        assert_eq!(run(&["Mar"], -2, 2), ["Jan", "Feb"]);
        assert_eq!(next(&["Hello"], 1), None);
        assert_eq!(next(&["=A1"], 1), None);
    }

    #[test]
    fn filling_carries_series_on() {
        use super::super::model::{Area, Workbook};
        let mut wb = Workbook::default();
        wb.set(0, 0, "Jan");
        wb.set(0, 1, "1");
        wb.set(1, 1, "2");
        wb.set(0, 2, "=B1*2");
        wb.fill(Area::cell(0, 0), (3, 0));
        wb.fill(Area::new(0, 1, 1, 1), (3, 1));
        wb.fill(Area::cell(0, 2), (2, 2));
        let col = |c: u32| (0..4).map(|r| wb.sheet().raw(r, c).to_string()).collect::<Vec<_>>();
        assert_eq!(col(0), ["Jan", "Feb", "Mar", "Apr"]);
        assert_eq!(col(1), ["1", "2", "3", "4"]);
        assert_eq!(col(2)[..3], ["=B1*2", "=B2*2", "=B3*2"]);
    }
}
