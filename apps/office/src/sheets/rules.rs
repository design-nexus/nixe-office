//! Conditional highlighting: a rule colours the cells of a range that pass its test.

use super::formula::Value;
use super::model::{Area, Calc, Workbook};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "test", rename_all = "snake_case")]
pub enum Test {
    Greater { value: f64 },
    Less { value: f64 },
    Between { low: f64, high: f64 },
    Equal { value: f64 },
    /// Text containing these words, ignoring case.
    Contains { text: String },
    Blank,
    Error,
    /// The `count` largest numbers in the range.
    Top { count: u32 },
    AboveAverage,
}

impl Test {
    /// The tests offered in the dialog, by name.
    pub const NAMES: [&'static str; 9] = [
        "Greater than",
        "Less than",
        "Between",
        "Equal to",
        "Text contains",
        "Blank",
        "Error",
        "Top values",
        "Above average",
    ];

    pub fn index(&self) -> usize {
        match self {
            Test::Greater { .. } => 0,
            Test::Less { .. } => 1,
            Test::Between { .. } => 2,
            Test::Equal { .. } => 3,
            Test::Contains { .. } => 4,
            Test::Blank => 5,
            Test::Error => 6,
            Test::Top { .. } => 7,
            Test::AboveAverage => 8,
        }
    }

    /// Build test `index` from what was typed in the dialog's two boxes.
    pub fn from_input(index: usize, a: &str, b: &str) -> Result<Test, &'static str> {
        let num = |s: &str| super::formula::parse_number(s).ok_or("Type a number.");
        Ok(match index {
            0 => Test::Greater { value: num(a)? },
            1 => Test::Less { value: num(a)? },
            2 => {
                let (x, y) = (num(a)?, num(b)?);
                Test::Between { low: x.min(y), high: x.max(y) }
            }
            3 => Test::Equal { value: num(a)? },
            4 if a.trim().is_empty() => return Err("Type the text to look for."),
            4 => Test::Contains { text: a.trim().to_string() },
            5 => Test::Blank,
            6 => Test::Error,
            7 => match num(a)? {
                n if n >= 1.0 => Test::Top { count: n as u32 },
                _ => return Err("Type how many, 1 or more."),
            },
            _ => Test::AboveAverage,
        })
    }

    /// The values to fill the dialog's boxes with.
    pub fn inputs(&self) -> (String, String) {
        use super::formula::general;
        match self {
            Test::Greater { value } | Test::Less { value } | Test::Equal { value } => (general(*value), String::new()),
            Test::Between { low, high } => (general(*low), general(*high)),
            Test::Contains { text } => (text.clone(), String::new()),
            Test::Top { count } => (count.to_string(), String::new()),
            _ => (String::new(), String::new()),
        }
    }

    /// How many boxes the test needs: 0, 1 or 2.
    pub fn inputs_needed(index: usize) -> usize {
        match index {
            2 => 2,
            0 | 1 | 3 | 4 | 7 => 1,
            _ => 0,
        }
    }

    pub fn describe(&self) -> String {
        use super::formula::general;
        match self {
            Test::Greater { value } => format!("greater than {}", general(*value)),
            Test::Less { value } => format!("less than {}", general(*value)),
            Test::Between { low, high } => format!("between {} and {}", general(*low), general(*high)),
            Test::Equal { value } => format!("equal to {}", general(*value)),
            Test::Contains { text } => format!("text contains “{text}”"),
            Test::Blank => "blank".into(),
            Test::Error => "an error".into(),
            Test::Top { count } => format!("in the top {count}"),
            Test::AboveAverage => "above average".into(),
        }
    }

    /// Does a cell's value pass? `mark` gives the range's threshold for the
    /// tests that need one (worked out once per rule).
    pub fn matches(&self, v: &Value, mark: impl FnOnce() -> Option<f64>) -> bool {
        let n = match v {
            Value::Num(n) => Some(*n),
            _ => None,
        };
        match self {
            Test::Greater { value } => n.is_some_and(|n| n > *value),
            Test::Less { value } => n.is_some_and(|n| n < *value),
            Test::Between { low, high } => n.is_some_and(|n| n >= *low && n <= *high),
            Test::Equal { value } => n.is_some_and(|n| (n - value).abs() < 1e-9),
            Test::Contains { text } => {
                !matches!(v, Value::Empty) && v.text().to_lowercase().contains(&text.to_lowercase())
            }
            Test::Blank => matches!(v, Value::Empty) || v.text().is_empty(),
            Test::Error => v.is_err(),
            Test::Top { .. } => n.is_some_and(|n| mark().is_some_and(|m| n >= m)),
            Test::AboveAverage => n.is_some_and(|n| mark().is_some_and(|m| n > m)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// "B2:B40".
    pub range: String,
    #[serde(flatten)]
    pub test: Test,
    /// Background, `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    /// Text colour, `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
}

/// Ready-made looks for a rule: (name, fill, text colour).
pub const LOOKS: &[(&str, &str, &str)] = &[
    ("Red", "#fde2e1", "#9c1c1c"),
    ("Yellow", "#fff3c4", "#7a5a00"),
    ("Green", "#dcf3e0", "#1f6b31"),
    ("Blue", "#dde9fd", "#1d4a9c"),
    ("Grey", "#eceef1", "#3b414b"),
];

impl Rule {
    /// The number the test compares against for the whole range: the Nth
    /// largest for "top N", the mean for "above average".
    pub fn mark(&self, wb: &Workbook, si: usize, area: Area, calc: &Calc) -> Option<f64> {
        let s = wb.sheets.get(si)?;
        // Only cells that hold something, so a whole-column rule stays quick.
        let mut nums: Vec<f64> = s
            .cells
            .range((area.top, 0)..=(area.bottom, u32::MAX))
            .filter(|((r, c), cell)| area.contains(*r, *c) && !cell.raw.is_empty())
            .filter_map(|((r, c), _)| match calc.value(wb, si, *r, *c) {
                Value::Num(n) => Some(n),
                _ => None,
            })
            .collect();
        if nums.is_empty() {
            return None;
        }
        match self.test {
            Test::Top { count } => {
                nums.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
                Some(nums[(count.max(1) as usize).min(nums.len()) - 1])
            }
            Test::AboveAverage => Some(nums.iter().sum::<f64>() / nums.len() as f64),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::*;
    use super::*;

    #[test]
    fn rules_colour_matching_cells() {
        let mut wb = Workbook::default();
        for (i, v) in ["10", "50", "30", "error", "=1/0"].iter().enumerate() {
            wb.set(i as u32, 0, v);
        }
        let red = |test| Rule { range: "A1:A5".into(), test, fill: Some("#fde2e1".into()), color: None, bold: true };
        wb.sheet_mut().rules = vec![red(Test::Error), red(Test::Greater { value: 20.0 })];
        let calc = Calc::new(&wb, 0.0);
        let fill = |r| calc.look(&wb, 0, r, 0).fill;
        assert_eq!(fill(0), None);
        assert_eq!(fill(1).as_deref(), Some("#fde2e1"));
        assert!(calc.look(&wb, 0, 1, 0).bold);
        assert_eq!(fill(4).as_deref(), Some("#fde2e1"));
        // Outside the range: untouched.
        wb.set(5, 0, "99");
        let calc = Calc::new(&wb, 0.0);
        assert_eq!(calc.look(&wb, 0, 5, 0).fill, None);

        wb.sheet_mut().rules = vec![red(Test::Top { count: 2 })];
        let calc = Calc::new(&wb, 0.0);
        let hits: Vec<u32> = (0..5).filter(|r| calc.look(&wb, 0, *r, 0).fill.is_some()).collect();
        assert_eq!(hits, [1, 2]);

        wb.sheet_mut().rules = vec![red(Test::AboveAverage)];
        let calc = Calc::new(&wb, 0.0);
        let hits: Vec<u32> = (0..5).filter(|r| calc.look(&wb, 0, *r, 0).fill.is_some()).collect();
        assert_eq!(hits, [1]);

        assert!(Test::Contains { text: "ERR".into() }.matches(&Value::Text("an error".into()), || None));
        assert_eq!(Test::from_input(2, "9", "3"), Ok(Test::Between { low: 3.0, high: 9.0 }));
        assert!(Test::from_input(0, "x", "").is_err());
    }

    #[test]
    fn merges_comments_and_rules_survive_saving() {
        let mut wb = Workbook::default();
        wb.set(0, 0, "Title");
        wb.set(0, 1, "gone");
        assert!(wb.merge(Area::new(0, 0, 0, 3)));
        assert_eq!(wb.sheet().raw(0, 1), "");
        assert_eq!(wb.sheet().merge_at(0, 2), Some(Area::new(0, 0, 0, 3)));
        assert_eq!(wb.sheet().grow(Area::cell(1, 1)), Area::cell(1, 1));
        assert_eq!(wb.sheet().grow(Area::new(0, 3, 1, 3)), Area::new(0, 0, 1, 3));
        wb.set_comment(2, 2, " Check this ");
        wb.sheet_mut().rules.push(Rule {
            range: "B2:B9".into(),
            test: Test::Less { value: 0.0 },
            fill: None,
            color: Some("#9c1c1c".into()),
            bold: false,
        });
        let back = Workbook::from_json(&wb.to_json()).unwrap();
        assert_eq!(back, wb);
        assert_eq!(back.sheet().comment(2, 2), Some("Check this"));

        // Sorting refuses to tear merged cells apart.
        wb.set(3, 0, "b");
        wb.set(4, 0, "a");
        wb.sheet_mut().header = false;
        let calc = Calc::new(&wb, 0.0);
        assert!(!wb.sort_rows(0, 0, true, &calc));
        assert!(wb.unmerge(Area::cell(0, 2)));
        assert!(wb.sheet().merges.is_empty());
    }
}
