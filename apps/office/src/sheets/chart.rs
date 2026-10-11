//! Drawing a chart with Cairo, for the chart pane and the PDF alike. Charts are
//! part of the document, so they keep their own colours on a white page.

use super::model::{ChartKind, ChartText};
use gtk::pango;

/// Series colours, distinct in hue and readable on white.
pub const SERIES: [(f64, f64, f64); 6] = [
    (0.239, 0.482, 0.878),
    (0.890, 0.420, 0.333),
    (0.298, 0.659, 0.471),
    (0.839, 0.616, 0.231),
    (0.580, 0.443, 0.835),
    (0.231, 0.678, 0.749),
];
const INK: (f64, f64, f64) = (0.125, 0.137, 0.161);
const GRID: (f64, f64, f64) = (0.86, 0.87, 0.89);

pub struct Data<'a> {
    pub names: &'a [String],
    pub labels: &'a [String],
    pub series: &'a [Vec<f64>],
}

/// Text at (x, y), at most `max_w` wide.
fn text(
    cr: &cairo::Context,
    ctx: &pango::Context,
    s: &str,
    size: f64,
    (x, y, max_w): (f64, f64, f64),
    align: pango::Alignment,
) -> f64 {
    let layout = pango::Layout::new(ctx);
    let mut font = pango::FontDescription::from_string("Noto Sans");
    font.set_size((size * pango::SCALE as f64) as i32);
    layout.set_font_description(Some(&font));
    layout.set_width((max_w.max(1.0) * pango::SCALE as f64) as i32);
    layout.set_ellipsize(pango::EllipsizeMode::End);
    layout.set_alignment(align);
    layout.set_text(s);
    cr.move_to(x, y);
    pangocairo::functions::show_layout(cr, &layout);
    layout.pixel_size().1 as f64
}

/// A tidy step for the value axis: 1, 2 or 5 times a power of ten.
fn nice_step(span: f64) -> f64 {
    let raw = span / 5.0;
    let mag = 10f64.powf(raw.abs().max(1e-9).log10().floor());
    let f = raw / mag;
    mag * if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    }
}

fn short(v: f64) -> String {
    let a = v.abs();
    if a >= 1e6 {
        format!("{}M", super::formula::general((v / 1e5).round() / 10.0))
    } else if a >= 1e4 {
        format!("{}k", super::formula::general((v / 100.0).round() / 10.0))
    } else {
        super::formula::general((v * 100.0).round() / 100.0)
    }
}

/// Draw the chart in the box (x, y, w, h) on white. `ctx` is a 72-dpi Pango context
/// for the PDF, or the widget's for the screen.
/// Draw the chart in the box `(x, y, w, h)`.
/// A chart with its title along the top and its axes' titles beside them.
pub fn draw_titled(
    cr: &cairo::Context,
    ctx: &pango::Context,
    kind: ChartKind,
    d: &Data,
    t: &ChartText,
    (x, y, w, h): (f64, f64, f64, f64),
) {
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.rectangle(x, y, w, h);
    let _ = cr.fill();
    let (mut ix, mut iy, mut iw, mut ih) = (x, y, w, h);
    let fs = 9.0;
    if !t.title.trim().is_empty() {
        let layout = pango::Layout::new(ctx);
        let mut font = pango::FontDescription::from_string("Noto Sans Bold");
        font.set_size((fs * 1.35 * pango::SCALE as f64) as i32);
        layout.set_font_description(Some(&font));
        layout.set_width(((w - 24.0).max(1.0) * pango::SCALE as f64) as i32);
        layout.set_alignment(pango::Alignment::Center);
        layout.set_ellipsize(pango::EllipsizeMode::End);
        layout.set_text(t.title.trim());
        let (_, th) = layout.pixel_size();
        cr.set_source_rgb(INK.0, INK.1, INK.2);
        cr.move_to(x + 12.0, y + 10.0);
        pangocairo::functions::show_layout(cr, &layout);
        iy += th as f64 + 6.0;
        ih -= th as f64 + 6.0;
    }
    // Pie charts have no axes to name.
    if kind != ChartKind::Pie {
        let axis = |s: &str| {
            let layout = pango::Layout::new(ctx);
            let mut font = pango::FontDescription::from_string("Noto Sans");
            font.set_size((fs * pango::SCALE as f64) as i32);
            layout.set_font_description(Some(&font));
            layout.set_ellipsize(pango::EllipsizeMode::End);
            layout.set_alignment(pango::Alignment::Center);
            layout.set_text(s.trim());
            layout
        };
        if !t.x_title.trim().is_empty() {
            let layout = axis(&t.x_title);
            layout.set_width(((iw - 24.0).max(1.0) * pango::SCALE as f64) as i32);
            let (_, th) = layout.pixel_size();
            cr.set_source_rgb(0.32, 0.35, 0.4);
            cr.move_to(ix + 12.0, iy + ih - th as f64 - 6.0);
            pangocairo::functions::show_layout(cr, &layout);
            ih -= th as f64 + 4.0;
        }
        if !t.y_title.trim().is_empty() {
            let layout = axis(&t.y_title);
            layout.set_width(((ih - 24.0).max(1.0) * pango::SCALE as f64) as i32);
            let (_, th) = layout.pixel_size();
            // Turned to read upwards, along the left edge.
            cr.save().ok();
            cr.set_source_rgb(0.32, 0.35, 0.4);
            cr.translate(ix + 8.0, iy + ih - 12.0);
            cr.rotate(-std::f64::consts::FRAC_PI_2);
            cr.move_to(0.0, 0.0);
            pangocairo::functions::show_layout(cr, &layout);
            cr.restore().ok();
            ix += th as f64 + 6.0;
            iw -= th as f64 + 6.0;
        }
    }
    draw(cr, ctx, kind, d, (ix, iy, iw.max(1.0), ih.max(1.0)));
}

pub fn draw(cr: &cairo::Context, ctx: &pango::Context, kind: ChartKind, d: &Data, (x, y, w, h): (f64, f64, f64, f64)) {
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.rectangle(x, y, w, h);
    let _ = cr.fill();
    let pad = 14.0;
    let fs = 9.0;
    // Legend along the top when there's more than one series.
    let mut top = y + pad;
    if d.series.len() > 1 || kind == ChartKind::Pie {
        let names: Vec<&String> = if kind == ChartKind::Pie { d.labels.iter().collect() } else { d.names.iter().collect() };
        let mut lx = x + pad;
        let total_pie: f64 = d.series.first().map(|s| s.iter().filter(|v| **v > 0.0).sum()).unwrap_or(0.0);
        for (i, n) in names.iter().enumerate().take(12) {
            let (r, g, b) = SERIES[i % SERIES.len()];
            let label = if kind == ChartKind::Pie && total_pie > 0.0 {
                let v = d.series[0].get(i).copied().unwrap_or(0.0).max(0.0);
                format!("{n} {}%", (v / total_pie * 100.0).round())
            } else {
                n.to_string()
            };
            let item_w = (label.chars().count() as f64 * fs * 0.62 + 22.0).min(w / 2.0);
            if lx + item_w > x + w - pad {
                lx = x + pad;
                top += fs * 1.8;
            }
            cr.set_source_rgb(r, g, b);
            cr.rectangle(lx, top + 2.0, 9.0, 9.0);
            let _ = cr.fill();
            cr.set_source_rgb(INK.0, INK.1, INK.2);
            text(cr, ctx, &label, fs, (lx + 13.0, top - 1.0, item_w - 16.0), pango::Alignment::Left);
            lx += item_w;
        }
        top += fs * 2.2;
    }
    let (px, py, pw, ph) = (x + pad, top, w - 2.0 * pad, y + h - pad - top);
    if pw < 40.0 || ph < 40.0 || d.labels.is_empty() {
        return;
    }

    if kind == ChartKind::Pie {
        let values = d.series.first().cloned().unwrap_or_default();
        let total: f64 = values.iter().filter(|v| **v > 0.0).sum();
        if total <= 0.0 {
            return;
        }
        let rad = (pw.min(ph) / 2.0) - 4.0;
        let (cx, cy) = (px + pw / 2.0, py + ph / 2.0);
        let mut a = -std::f64::consts::FRAC_PI_2;
        for (i, v) in values.iter().enumerate() {
            if *v <= 0.0 {
                continue;
            }
            let sweep = v / total * std::f64::consts::TAU;
            let (r, g, b) = SERIES[i % SERIES.len()];
            cr.set_source_rgb(r, g, b);
            cr.move_to(cx, cy);
            cr.arc(cx, cy, rad, a, a + sweep);
            cr.close_path();
            let _ = cr.fill_preserve();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.set_line_width(1.5);
            let _ = cr.stroke();
            a += sweep;
        }
        return;
    }

    // Value axis: stacked charts reach the sums of each point's values.
    let n = d.labels.len();
    let mut all: Vec<f64> = d.series.iter().flatten().copied().collect();
    if kind.stacked() {
        for i in 0..n {
            let col = d.series.iter().filter_map(|s| s.get(i));
            all.push(col.clone().filter(|v| **v > 0.0).sum());
            all.push(col.filter(|v| **v < 0.0).sum());
        }
    }
    let (mut lo, mut hi) = (all.iter().cloned().fold(0.0, f64::min), all.iter().cloned().fold(0.0, f64::max));
    if hi == lo {
        hi = lo + 1.0;
    }
    let step = nice_step(hi - lo);
    lo = (lo / step).floor() * step;
    hi = (hi / step).ceil() * step;
    let axis_w = 44.0;
    let label_h = fs * 2.0;
    let (gx, gy, gw, gh) = (px + axis_w, py, pw - axis_w, ph - label_h);
    let ypos = |v: f64| gy + gh - (v - lo) / (hi - lo) * gh;
    cr.set_line_width(0.6);
    let mut v = lo;
    while v <= hi + step * 0.01 {
        let yy = ypos(v);
        cr.set_source_rgb(GRID.0, GRID.1, GRID.2);
        cr.move_to(gx, yy);
        cr.line_to(gx + gw, yy);
        let _ = cr.stroke();
        cr.set_source_rgb(0.42, 0.45, 0.5);
        text(cr, ctx, &short(v), fs * 0.9, (px, yy - fs * 0.75, axis_w - 6.0), pango::Alignment::Right);
        v += step;
    }
    let slot = gw / n as f64;
    let zero = ypos(0.0f64.clamp(lo, hi));
    if kind == ChartKind::Scatter {
        scatter(cr, ctx, d, (gx, gy, gw, gh), &ypos);
        return;
    }
    // Category labels, thinned out when they'd crowd.
    let every = ((fs * 5.0) / slot).ceil().max(1.0) as usize;
    cr.set_source_rgb(0.42, 0.45, 0.5);
    for (i, l) in d.labels.iter().enumerate() {
        if i % every == 0 {
            text(cr, ctx, l, fs * 0.9, (gx + slot * i as f64, gy + gh + 4.0, slot * every as f64), pango::Alignment::Center);
        }
    }
    if kind.stacked() {
        stacked(cr, kind, d, (gx, slot), &ypos);
        axis_line(cr, gx, gw, zero);
        return;
    }
    let k = d.series.len().max(1);
    for (si, s) in d.series.iter().enumerate() {
        let (r, g, b) = SERIES[si % SERIES.len()];
        match kind {
            ChartKind::Bar => {
                let bw = slot * 0.7 / k as f64;
                for (i, v) in s.iter().enumerate() {
                    let bx = gx + slot * i as f64 + slot * 0.15 + bw * si as f64;
                    let yv = ypos(*v);
                    cr.set_source_rgb(r, g, b);
                    cr.rectangle(bx, yv.min(zero), bw.max(1.0), (yv - zero).abs());
                    let _ = cr.fill();
                }
            }
            ChartKind::Line | ChartKind::Area => {
                let pts: Vec<(f64, f64)> = s.iter().enumerate().map(|(i, v)| (gx + slot * (i as f64 + 0.5), ypos(*v))).collect();
                if kind == ChartKind::Area && pts.len() > 1 {
                    cr.move_to(pts[0].0, zero);
                    for (xx, yy) in &pts {
                        cr.line_to(*xx, *yy);
                    }
                    cr.line_to(pts[pts.len() - 1].0, zero);
                    cr.close_path();
                    cr.set_source_rgba(r, g, b, 0.18);
                    let _ = cr.fill();
                }
                cr.set_source_rgb(r, g, b);
                cr.set_line_width(2.0);
                for (i, (xx, yy)) in pts.iter().enumerate() {
                    if i == 0 {
                        cr.move_to(*xx, *yy);
                    } else {
                        cr.line_to(*xx, *yy);
                    }
                }
                let _ = cr.stroke();
                if pts.len() <= 40 {
                    for (xx, yy) in &pts {
                        cr.arc(*xx, *yy, 2.5, 0.0, std::f64::consts::TAU);
                        let _ = cr.fill();
                    }
                }
            }
            _ => {}
        }
    }
    axis_line(cr, gx, gw, zero);
}

fn axis_line(cr: &cairo::Context, gx: f64, gw: f64, zero: f64) {
    cr.set_source_rgb(0.6, 0.63, 0.67);
    cr.set_line_width(0.8);
    cr.move_to(gx, zero);
    cr.line_to(gx + gw, zero);
    let _ = cr.stroke();
}

/// Bars or bands of each series piled on the ones before, up from zero (and
/// down from it for negative values).
fn stacked(cr: &cairo::Context, kind: ChartKind, d: &Data, (gx, slot): (f64, f64), ypos: &dyn Fn(f64) -> f64) {
    let n = d.labels.len();
    let (mut up, mut down) = (vec![0.0; n], vec![0.0; n]);
    for (si, s) in d.series.iter().enumerate() {
        let (r, g, b) = SERIES[si % SERIES.len()];
        let (start_up, start_down) = (up.clone(), down.clone());
        for (i, v) in s.iter().enumerate().take(n) {
            if *v >= 0.0 {
                up[i] += v;
            } else {
                down[i] += v;
            }
        }
        if kind == ChartKind::StackedBar {
            cr.set_source_rgb(r, g, b);
            for (i, v) in s.iter().enumerate().take(n) {
                let (from, to) = if *v >= 0.0 { (start_up[i], up[i]) } else { (start_down[i], down[i]) };
                let (y1, y2) = (ypos(from), ypos(to));
                cr.rectangle(gx + slot * i as f64 + slot * 0.18, y1.min(y2), slot * 0.64, (y1 - y2).abs());
            }
            let _ = cr.fill();
        } else if n > 0 {
            // A band between this series' top and the one below.
            let x = |i: usize| gx + slot * (i as f64 + 0.5);
            cr.move_to(x(0), ypos(start_up[0]));
            for (i, top) in up.iter().enumerate() {
                cr.line_to(x(i), ypos(*top));
            }
            for i in (0..n).rev() {
                cr.line_to(x(i), ypos(start_up[i]));
            }
            cr.close_path();
            cr.set_source_rgba(r, g, b, 0.75);
            let _ = cr.fill_preserve();
            cr.set_source_rgb(r, g, b);
            cr.set_line_width(1.2);
            let _ = cr.stroke();
        }
    }
}

/// Points at (x, y): x from the first column, one colour per series.
fn scatter(cr: &cairo::Context, ctx: &pango::Context, d: &Data, (gx, gy, gw, gh): (f64, f64, f64, f64), ypos: &dyn Fn(f64) -> f64) {
    let xs: Vec<f64> = d
        .labels
        .iter()
        .enumerate()
        .map(|(i, l)| super::formula::parse_number(l).unwrap_or(i as f64 + 1.0))
        .collect();
    let (mut lo, mut hi) = (xs.iter().cloned().fold(f64::INFINITY, f64::min), xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    if !lo.is_finite() {
        return;
    }
    if hi == lo {
        hi = lo + 1.0;
    }
    let step = nice_step(hi - lo);
    lo = (lo / step).floor() * step;
    hi = (hi / step).ceil() * step;
    let xpos = |v: f64| gx + (v - lo) / (hi - lo) * gw;
    let fs = 9.0;
    let mut v = lo;
    while v <= hi + step * 0.01 {
        let x = xpos(v);
        cr.set_source_rgb(GRID.0, GRID.1, GRID.2);
        cr.set_line_width(0.6);
        cr.move_to(x, gy);
        cr.line_to(x, gy + gh);
        let _ = cr.stroke();
        cr.set_source_rgb(0.42, 0.45, 0.5);
        text(cr, ctx, &short(v), fs * 0.9, (x - 30.0, gy + gh + 4.0, 60.0), pango::Alignment::Center);
        v += step;
    }
    for (si, s) in d.series.iter().enumerate() {
        let (r, g, b) = SERIES[si % SERIES.len()];
        cr.set_source_rgb(r, g, b);
        for (x, y) in xs.iter().zip(s) {
            cr.new_sub_path();
            cr.arc(xpos(*x), ypos(*y), 3.5, 0.0, std::f64::consts::TAU);
        }
        let _ = cr.fill();
    }
}

#[cfg(test)]
mod tests {
    /// `NXO_SAMPLE_DIR=/tmp/x cargo test -- --ignored chart_pictures`: one PNG per kind.
    #[test]
    #[ignore]
    fn chart_pictures() {
        let dir = std::path::PathBuf::from(std::env::var("NXO_SAMPLE_DIR").expect("NXO_SAMPLE_DIR"));
        let ctx = crate::print::text_context();
        let names = ["North".to_string(), "South".to_string()];
        let labels: Vec<String> = ["1", "2", "3.5", "5", "6"].iter().map(|s| s.to_string()).collect();
        let series = [vec![3.0, 5.0, 4.0, 7.0, 6.0], vec![2.0, 1.0, 3.0, 2.5, 4.0]];
        for kind in super::ChartKind::ALL {
            let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 360, 240).unwrap();
            let cr = cairo::Context::new(&surface).unwrap();
            super::draw(&cr, &ctx, kind, &super::Data { names: &names, labels: &labels, series: &series }, (0.0, 0.0, 360.0, 240.0));
            drop(cr);
            let mut f = std::fs::File::create(dir.join(format!("chart-{}.png", kind.name()))).unwrap();
            surface.write_to_png(&mut f).unwrap();
        }
    }

    #[test]
    fn steps_are_round() {
        assert_eq!(super::nice_step(100.0), 20.0);
        assert_eq!(super::nice_step(7.0), 2.0);
        assert_eq!(super::short(25_000.0), "25k");
        assert_eq!(super::short(1_250_000.0), "1.3M");
    }
}
