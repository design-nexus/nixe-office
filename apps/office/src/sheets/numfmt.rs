//! Number format codes, as other spreadsheets write them: `#,##0.00`,
//! `"$"#,##0;[Red]-"$"#,##0`, `0.0%`, `d mmm yyyy`, `h:mm AM/PM`. Up to four
//! sections (positive; negative; zero; text). The usual parts are understood:
//! digit placeholders `0 # ?`, thousands `,`, decimals `.`, percent, quoted and
//! escaped text, currency in `[$€-407]`, colour tags (ignored), and dates and
//! times. What isn't understood shows the number plainly.

use super::formula::{fixed, general, serial_date};

/// Ready-made formats: (name, code).
pub const PRESETS: &[(&str, &str)] = &[
    ("Whole number", "#,##0"),
    ("Number, 1 decimal", "#,##0.0"),
    ("Number, 3 decimals", "#,##0.000"),
    ("Dollars", "$#,##0.00"),
    ("Dollars, no cents", "$#,##0"),
    ("Dollars, negatives in brackets", "$#,##0.00;($#,##0.00)"),
    ("Euros", "€#,##0.00"),
    ("Pounds", "£#,##0.00"),
    ("Yen", "¥#,##0"),
    ("Percent, whole", "0%"),
    ("Percent, 2 decimals", "0.00%"),
    ("Date, 10/5/2026", "m/d/yyyy"),
    ("Date, 5/10/2026", "d/m/yyyy"),
    ("Date, 5 Oct 2026", "d mmm yyyy"),
    ("Date, October 5, 2026", "mmmm d, yyyy"),
    ("Date, Mon 5 Oct", "ddd d mmm"),
    ("Month and year", "mmmm yyyy"),
    ("Time, 2:30 PM", "h:mm AM/PM"),
    ("Time, 14:30", "hh:mm"),
    ("Date and time", "yyyy-mm-dd hh:mm"),
];

const MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// A piece of a section.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Text(String),
    /// A digit placeholder: '0', '#' or '?'.
    Digit(char),
    Comma,
    Point,
    Percent,
    /// Date and time parts, as written: "yyyy", "mm", "h", "ss", "AM/PM".
    Date(String),
    /// Where text goes (`@`).
    At,
}

/// Split at `;` outside quotes and brackets.
fn sections(code: &str) -> Vec<&str> {
    let (mut out, mut start, mut quoted, mut bracket) = (Vec::new(), 0, false, false);
    let b = code.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => quoted = !quoted,
            b'\\' if !quoted => i += 1,
            b'[' if !quoted => bracket = true,
            b']' if !quoted => bracket = false,
            b';' if !quoted && !bracket => {
                out.push(&code[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&code[start..]);
    out
}

fn tokens(section: &str) -> Vec<Tok> {
    let chars: Vec<char> = section.chars().collect();
    let mut out: Vec<Tok> = Vec::new();
    let text = |out: &mut Vec<Tok>, s: &str| match out.last_mut() {
        Some(Tok::Text(t)) => t.push_str(s),
        _ => out.push(Tok::Text(s.to_string())),
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let rest: String = chars[i..].iter().collect::<String>().to_ascii_lowercase();
        match c {
            '"' => {
                let end = chars[i + 1..].iter().position(|x| *x == '"').map_or(chars.len(), |p| i + 1 + p);
                text(&mut out, &chars[i + 1..end.min(chars.len())].iter().collect::<String>());
                i = end + 1;
                continue;
            }
            '\\' => {
                if let Some(n) = chars.get(i + 1) {
                    text(&mut out, &n.to_string());
                }
                i += 2;
                continue;
            }
            // Room for a character: a space.
            '_' => {
                text(&mut out, " ");
                i += 2;
                continue;
            }
            // Repeat to fill the cell: left out.
            '*' => {
                i += 2;
                continue;
            }
            '[' => {
                let end = chars[i..].iter().position(|x| *x == ']').map_or(chars.len(), |p| i + p);
                let inside: String = chars[i + 1..end].iter().collect();
                // [$€-407] is a currency symbol; colours and conditions are left out.
                if let Some(sym) = inside.strip_prefix('$') {
                    text(&mut out, sym.split('-').next().unwrap_or(""));
                }
                i = end + 1;
                continue;
            }
            '0' | '#' | '?' => out.push(Tok::Digit(c)),
            ',' => out.push(Tok::Comma),
            '.' => out.push(Tok::Point),
            '%' => out.push(Tok::Percent),
            '@' => out.push(Tok::At),
            _ if rest.starts_with("am/pm") => {
                out.push(Tok::Date("AM/PM".into()));
                i += 5;
                continue;
            }
            _ if rest.starts_with("a/p") => {
                out.push(Tok::Date("A/P".into()));
                i += 3;
                continue;
            }
            'y' | 'Y' | 'm' | 'M' | 'd' | 'D' | 'h' | 'H' | 's' | 'S' => {
                let lc = c.to_ascii_lowercase();
                let n = chars[i..].iter().take_while(|x| x.to_ascii_lowercase() == lc).count();
                out.push(Tok::Date(std::iter::repeat_n(lc, n).collect()));
                i += n;
                continue;
            }
            'E' | 'e' if matches!(chars.get(i + 1), Some('+') | Some('-')) => {
                // Scientific notation isn't supported: show the number plainly.
                return Vec::new();
            }
            _ => text(&mut out, &c.to_string()),
        }
        i += 1;
    }
    out
}

/// `n` in format `code`. None when the code isn't understood.
pub fn render(code: &str, n: f64) -> Option<String> {
    if code.trim().eq_ignore_ascii_case("general") {
        return Some(general(n));
    }
    let secs = sections(code);
    let (section, signed) = match secs.len() {
        1 => (secs[0], true),
        _ if n < 0.0 => (secs[1], false),
        3.. if n == 0.0 => (secs[2], false),
        _ => (secs[0], false),
    };
    let toks = tokens(section);
    if toks.is_empty() && !section.is_empty() {
        return None;
    }
    if toks.iter().any(|t| matches!(t, Tok::Date(_))) {
        return Some(date(&toks, n));
    }
    // With a negative section, its own text shows the sign.
    let v = if signed { n } else { n.abs() };
    Some(number(&toks, v))
}

/// Text in the format's text section (the fourth, or one with `@`).
pub fn render_text(code: &str, text: &str) -> String {
    let secs = sections(code);
    let section = secs.get(3).copied().or_else(|| secs.iter().copied().find(|s| s.contains('@')));
    match section {
        Some(s) => tokens(s)
            .iter()
            .map(|t| match t {
                Tok::Text(x) => x.clone(),
                Tok::At => text.to_string(),
                _ => String::new(),
            })
            .collect(),
        None => text.to_string(),
    }
}

fn number(toks: &[Tok], v: f64) -> String {
    let first = toks.iter().position(|t| matches!(t, Tok::Digit(_) | Tok::Point));
    let last = toks.iter().rposition(|t| matches!(t, Tok::Digit(_) | Tok::Point));
    let (Some(first), Some(last)) = (first, last) else {
        // Text only (and maybe %): the text, as written.
        return toks.iter().map(|t| if let Tok::Text(x) = t { x.as_str() } else if *t == Tok::Percent { "%" } else { "" }).collect();
    };
    let body = &toks[first..=last];
    let point = body.iter().position(|t| *t == Tok::Point);
    let (int, frac) = match point {
        Some(p) => (&body[..p], &body[p + 1..]),
        None => (body, &body[..0]),
    };
    let grouped = int.contains(&Tok::Comma);
    let min_int = int.iter().filter(|t| **t == Tok::Digit('0')).count();
    let places = frac.iter().filter(|t| matches!(t, Tok::Digit(_))).count();
    let min_places = frac.iter().filter(|t| matches!(t, Tok::Digit('0') | Tok::Digit('?'))).count();
    // Commas right after the digits divide by a thousand each.
    let scale = toks[last + 1..].iter().take_while(|t| **t == Tok::Comma).count();
    let percents = toks.iter().filter(|t| **t == Tok::Percent).count();
    let v = v * 100f64.powi(percents as i32) / 1000f64.powi(scale as i32);

    let s = fixed(v, places, grouped);
    let (sign, s) = match s.strip_prefix('-') {
        Some(r) => ("-", r.to_string()),
        None => ("", s),
    };
    let (mut i, mut f) = match s.split_once('.') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (s, String::new()),
    };
    while f.len() > min_places && f.ends_with('0') {
        f.pop();
    }
    if i == "0" && min_int == 0 {
        i.clear();
    }
    let digits = i.chars().filter(|c| c.is_ascii_digit()).count();
    if digits < min_int {
        i = "0".repeat(min_int - digits) + &i;
    }
    // A point in the code always shows ("0.##" shows 1 as "1."), as in other spreadsheets.
    let shown = if point.is_some() { format!("{i}.{f}") } else { i };
    let text_of = |ts: &[Tok]| -> String {
        ts.iter()
            .map(|t| match t {
                Tok::Text(x) => x.clone(),
                Tok::Percent => "%".into(),
                _ => String::new(),
            })
            .collect()
    };
    let (before, after) = (text_of(&toks[..first]), text_of(&toks[last + 1..]));
    // A minus sign goes before a currency symbol: -$1,234.00.
    format!("{sign}{before}{shown}{after}")
}

fn date(toks: &[Tok], n: f64) -> String {
    let (y, m, d) = serial_date(n);
    let secs = ((n - n.floor()) * 86400.0).round() as i64 % 86400;
    let (hour, min, sec) = (secs / 3600, secs / 60 % 60, secs % 60);
    let twelve = toks.iter().any(|t| matches!(t, Tok::Date(x) if x == "AM/PM" || x == "A/P"));
    let weekday = ((n.floor() as i64 - 1).rem_euclid(7)) as usize; // serial 1 (1900-01-01) is a Sunday
    let mut out = String::new();
    for (i, t) in toks.iter().enumerate() {
        match t {
            Tok::Text(x) => out.push_str(x),
            Tok::Digit(c) => out.push(*c),
            Tok::Comma => out.push(','),
            Tok::Point => out.push('.'),
            Tok::Percent => out.push('%'),
            Tok::At => {}
            Tok::Date(code) => {
                // "m" after an hour or before seconds is minutes.
                let prev = toks[..i].iter().rev().find_map(|t| if let Tok::Date(x) = t { Some(x.as_str()) } else { None });
                let next = toks[i + 1..].iter().find_map(|t| if let Tok::Date(x) = t { Some(x.as_str()) } else { None });
                let minutes = code.starts_with('m')
                    && code.len() <= 2
                    && (prev.is_some_and(|p| p.starts_with('h')) || next.is_some_and(|p| p.starts_with('s')));
                let h12 = if hour % 12 == 0 { 12 } else { hour % 12 };
                let hr = if twelve { h12 } else { hour };
                let part = match code.as_str() {
                    _ if minutes => format!("{:0w$}", min, w = code.len()),
                    "yyyy" | "yyy" => format!("{y:04}"),
                    "yy" | "y" => format!("{:02}", y.rem_euclid(100)),
                    "mmmmm" => MONTHS[(m - 1) as usize][..1].to_string(),
                    "mmmm" => MONTHS[(m - 1) as usize].to_string(),
                    "mmm" => MONTHS[(m - 1) as usize][..3].to_string(),
                    "mm" => format!("{m:02}"),
                    "m" => m.to_string(),
                    "dddd" => DAYS[weekday].to_string(),
                    "ddd" => DAYS[weekday][..3].to_string(),
                    "dd" => format!("{d:02}"),
                    "d" => d.to_string(),
                    "hh" => format!("{hr:02}"),
                    "h" => hr.to_string(),
                    "ss" => format!("{sec:02}"),
                    "s" => sec.to_string(),
                    "AM/PM" => if hour < 12 { "AM" } else { "PM" }.into(),
                    "A/P" => if hour < 12 { "A" } else { "P" }.into(),
                    other => other.to_string(),
                };
                out.push_str(&part);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::formula::date_serial;
    use super::*;

    fn r(code: &str, n: f64) -> String {
        render(code, n).unwrap_or_else(|| panic!("{code} not understood"))
    }

    #[test]
    fn numbers() {
        assert_eq!(r("#,##0", 1234567.891), "1,234,568");
        assert_eq!(r("#,##0.00", -1234.5), "-1,234.50");
        assert_eq!(r("0.000", 2.0), "2.000");
        assert_eq!(r("#.##", 0.5), ".5");
        assert_eq!(r("0.0#", 1.234), "1.23");
        assert_eq!(r("0.0#", 1.2), "1.2");
        assert_eq!(r("000", 7.0), "007");
        assert_eq!(r("$#,##0.00", -5.0), "-$5.00");
        assert_eq!(r("$#,##0.00;($#,##0.00)", -1234.5), "($1,234.50)");
        assert_eq!(r("0;-0;\"zero\"", 0.0), "zero");
        assert_eq!(r("[Red]0.00", 3.0), "3.00");
        assert_eq!(r("[$€-407]#,##0.00", 9.5), "€9.50");
        assert_eq!(r("\"€\"#,##0.00", 1000.0), "€1,000.00");
        assert_eq!(r("#,##0.00 \"kg\"", 12.0), "12.00 kg");
        assert_eq!(r("0%", 0.256), "26%");
        assert_eq!(r("0.00%", 0.256), "25.60%");
        assert_eq!(r("#,##0,\"K\"", 25300.0), "25K");
        assert_eq!(r("General", 1.5), "1.5");
        assert_eq!(render("0.00E+00", 1.0), None);
    }

    #[test]
    fn dates_and_times() {
        let d = date_serial(2026, 10, 5) + 14.5 / 24.0; // Monday, 2:30 PM
        assert_eq!(r("m/d/yyyy", d), "10/5/2026");
        assert_eq!(r("dd/mm/yy", d), "05/10/26");
        assert_eq!(r("d mmm yyyy", d), "5 Oct 2026");
        assert_eq!(r("mmmm d, yyyy", d), "October 5, 2026");
        assert_eq!(r("dddd", d), "Monday");
        assert_eq!(r("ddd d mmm", d), "Mon 5 Oct");
        assert_eq!(r("h:mm AM/PM", d), "2:30 PM");
        assert_eq!(r("hh:mm", d), "14:30");
        assert_eq!(r("yyyy-mm-dd hh:mm:ss", d), "2026-10-05 14:30:00");
    }

    #[test]
    fn text_section() {
        assert_eq!(render_text("0;-0;0;\"Note: \"@", "hi"), "Note: hi");
        assert_eq!(render_text("0.00", "hi"), "hi");
    }

    #[test]
    fn presets_all_render() {
        for (_, code) in PRESETS {
            assert!(render(code, 1234.5).is_some(), "{code}");
        }
    }
}
