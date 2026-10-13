//! Starter workbooks.

use super::model::*;
use crate::doc::Template;

pub const TEMPLATES: &[Template] = &[
    Template {
        id: "monthly-budget",
        title: "Monthly budget",
        summary: "Planned and actual spending by category.",
        icon: "nxo-piggy-bank-symbolic",
    },
    Template {
        id: "home-inventory",
        title: "Home inventory",
        summary: "Belongings by room, with their value.",
        icon: "nxo-package-symbolic",
    },
    Template {
        id: "trip-budget",
        title: "Trip budget",
        summary: "What a trip costs and what's left.",
        icon: "nxo-plane-symbolic",
    },
];

/// Headers in row 1 (bold), and totals in `total_row` (bold).
fn layout(wb: &mut Workbook, headers: &[&str], widths: &[(u32, u32)]) {
    for (c, h) in headers.iter().enumerate() {
        wb.set(0, c as u32, h);
    }
    wb.format(Area::new(0, 0, 0, headers.len() as u32 - 1), |f| {
        f.bold = true;
        f.fill = Some("#eef1f5".into());
    });
    for (c, w) in widths {
        wb.set_col_w(*c, *w);
    }
    wb.sheet_mut().freeze_rows = 1;
}

pub fn build(id: &str) -> Option<Workbook> {
    let mut wb = Workbook::default();
    match id {
        "monthly-budget" | "trip-budget" => {
            let monthly = id == "monthly-budget";
            let headers: &[&str] = if monthly {
                &["Category", "Planned", "Actual", "Difference"]
            } else {
                &["Category", "Budget", "Spent", "Remaining"]
            };
            let cats: &[&str] = if monthly {
                &["Housing", "Utilities", "Groceries", "Transport", "Health", "Savings", "Fun", "Other"]
            } else {
                &["Travel", "Stay", "Food", "Activities", "Shopping", "Other"]
            };
            layout(&mut wb, headers, &[(0, 180), (1, 132), (2, 132), (3, 132)]);
            for (i, c) in cats.iter().enumerate() {
                let r = i as u32 + 1;
                wb.set(r, 0, c);
                wb.set(r, 3, &format!("=B{0}-C{0}", r + 1));
            }
            let total = cats.len() as u32 + 1;
            wb.set(total, 0, "Total");
            for c in 1..=3 {
                let col = super::formula::col_name(c);
                wb.set(total, c, &format!("=SUM({col}2:{col}{total})"));
            }
            wb.format(Area::new(1, 1, total, 3), |f| f.number = NumFmt::Currency);
            wb.format(Area::new(total, 0, total, 3), |f| f.bold = true);
            wb.sheet_mut().chart = Some(Chart { range: format!("A1:C{total}"), kind: ChartKind::Bar, text: ChartText::default() });
        }
        "home-inventory" => {
            layout(
                &mut wb,
                &["Item", "Room", "Quantity", "Value each", "Total value", "Notes"],
                &[(0, 180), (1, 140), (3, 132), (4, 132), (5, 220)],
            );
            for (i, room) in ["Living room", "Kitchen", "Bedroom", "Garage"].iter().enumerate() {
                let r = i as u32 + 1;
                wb.set(r, 1, room);
                wb.set(r, 4, &format!("=C{0}*D{0}", r + 1));
            }
            wb.set(5, 0, "Total");
            wb.set(5, 4, "=SUM(E2:E5)");
            wb.format(Area::new(1, 3, 5, 4), |f| f.number = NumFmt::Currency);
            wb.format(Area::new(5, 0, 5, 4), |f| f.bold = true);
        }
        _ => return None,
    }
    Some(wb)
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_template_builds() {
        for t in super::TEMPLATES {
            assert!(super::build(t.id).is_some(), "{}", t.id);
        }
    }
}
