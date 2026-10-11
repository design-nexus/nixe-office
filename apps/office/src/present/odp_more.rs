//! The OpenDocument presentation parts beyond shapes: slide transitions (a page
//! style's `smil:type` plus a `transitionFilter`), builds (the page's timing tree,
//! as LibreOffice's presets), and charts (an embedded chart document with its own
//! table of numbers).

use super::model::*;
use crate::odf::{self, ANIM, CHART, OFFICE, PRESENTATION, SMIL, TABLE, attr, esc, pt};
use std::fmt::Write as _;

// ---------- Transitions ----------

/// ODF's (type, subtype, reversed) for each transition.
fn smil(kind: TransitionKind) -> (&'static str, &'static str, bool) {
    match kind {
        TransitionKind::Fade | TransitionKind::MagicMove => ("fade", "crossfade", false),
        TransitionKind::Push => ("pushWipe", "fromLeft", false),
        TransitionKind::Cover => ("slideWipe", "fromLeft", false),
        TransitionKind::Uncover => ("slideWipe", "fromLeft", true),
        TransitionKind::Wipe => ("barWipe", "leftToRight", false),
        TransitionKind::Split => ("barnDoorWipe", "vertical", false),
        TransitionKind::Zoom => ("zoom", "", false),
        TransitionKind::Dissolve => ("dissolve", "", false),
    }
}

fn speed(secs: f64) -> &'static str {
    if secs < 0.6 {
        "fast"
    } else if secs < 1.2 {
        "medium"
    } else {
        "slow"
    }
}

/// The page style's attributes for a transition.
pub fn transition_props(t: &Transition) -> String {
    let (ty, sub, rev) = smil(t.kind);
    let mut s = format!(r#" presentation:transition-speed="{}" smil:type="{ty}""#, speed(t.secs));
    if !sub.is_empty() {
        let _ = write!(s, r#" smil:subtype="{sub}""#);
    }
    if rev {
        s.push_str(r#" smil:direction="reverse""#);
    }
    s
}

/// The transition as the timing tree has it, with its exact length.
fn transition_filter(t: &Transition, page_id: &str) -> String {
    let (ty, sub, rev) = smil(t.kind);
    format!(
        r#"<anim:par smil:begin="{page_id}.begin"><anim:transitionFilter smil:dur="{}s" smil:type="{ty}"{}{}/></anim:par>"#,
        (t.secs * 1000.0).round() / 1000.0,
        if sub.is_empty() { String::new() } else { format!(r#" smil:subtype="{sub}""#) },
        if rev { r#" smil:direction="reverse""# } else { "" }
    )
}

/// A transition from its ODF type and subtype.
fn kind_of(ty: &str, sub: &str, reversed: bool) -> TransitionKind {
    match ty {
        "fade" if sub == "crossfade" || sub.is_empty() => TransitionKind::Fade,
        "fade" => TransitionKind::Fade,
        "pushWipe" => TransitionKind::Push,
        "slideWipe" if reversed => TransitionKind::Uncover,
        "slideWipe" => TransitionKind::Cover,
        "barWipe" | "waterfallWipe" | "blindsWipe" => TransitionKind::Wipe,
        "barnDoorWipe" | "barnVeeWipe" => TransitionKind::Split,
        "zoom" | "irisWipe" | "ellipseWipe" | "fourBoxWipe" => TransitionKind::Zoom,
        "dissolve" | "randomBarWipe" | "checkerBoardWipe" => TransitionKind::Dissolve,
        _ => TransitionKind::Fade,
    }
}

/// The slide's transition: from the timing tree's filter when there is one (it
/// has the length), else from the page style.
pub fn read_transition(page: &roxmltree::Node, style: impl Fn(&str) -> Option<String>) -> Option<Transition> {
    let filter = page
        .children()
        .filter(|c| c.has_tag_name((ANIM, "par")))
        .flat_map(|root| root.descendants())
        .find(|n| {
            n.has_tag_name((ANIM, "transitionFilter"))
                && attr(n, SMIL, "targetElement").is_none()
                && n.parent().and_then(|p| attr(&p, SMIL, "begin")).is_some_and(|b| b.ends_with(".begin"))
        });
    let secs_of = |d: &str| d.trim_end_matches('s').parse::<f64>().ok();
    if let Some(f) = filter {
        let ty = attr(&f, SMIL, "type").unwrap_or("fade");
        let kind = kind_of(ty, attr(&f, SMIL, "subtype").unwrap_or(""), attr(&f, SMIL, "direction") == Some("reverse"));
        let secs = attr(&f, SMIL, "dur").and_then(secs_of).unwrap_or(0.75);
        return Some(Transition { kind, secs: secs.clamp(0.2, 5.0) });
    }
    let ty = style("page/type")?;
    let kind = kind_of(&ty, &style("page/subtype").unwrap_or_default(), style("page/direction").as_deref() == Some("reverse"));
    let secs = match style("page/transition-speed").as_deref() {
        Some("fast") => 0.5,
        Some("slow") => 1.5,
        _ => 0.75,
    };
    Some(Transition { kind, secs })
}

// ---------- Builds ----------

/// One click's animation of the object with element id `target`, as LibreOffice's presets.
fn step_xml(step: &Step, target: &str) -> String {
    let (class, preset, body) = match (step.out, step.effect) {
        (false, Effect::Appear) => ("entrance", "ooo-entrance-appear", String::new()),
        (false, Effect::Fade) => (
            "entrance",
            "ooo-entrance-fade-in",
            format!(r#"<anim:transitionFilter smil:dur="0.5s" smil:targetElement="{target}" smil:type="fade" smil:subtype="crossfade"/>"#),
        ),
        (false, Effect::Fly) => (
            "entrance",
            "ooo-entrance-fly-in",
            format!(r#"<anim:animate smil:dur="0.5s" smil:fill="hold" smil:targetElement="{target}" smil:attributeName="y" smil:values="1+height/2;y" smil:keyTimes="0;1"/>"#),
        ),
        (true, Effect::Appear) => ("exit", "ooo-exit-disappear", String::new()),
        (true, Effect::Fade) => (
            "exit",
            "ooo-exit-fade-out",
            format!(r#"<anim:transitionFilter smil:dur="0.5s" smil:targetElement="{target}" smil:type="fade" smil:subtype="crossfade" smil:mode="out"/>"#),
        ),
        (true, Effect::Fly) => (
            "exit",
            "ooo-exit-fly-out",
            format!(r#"<anim:animate smil:dur="0.5s" smil:fill="hold" smil:targetElement="{target}" smil:attributeName="y" smil:values="y;1+height/2" smil:keyTimes="0;1"/>"#),
        ),
    };
    let sub = match (step.effect, step.out) {
        (Effect::Fly, false) => r#" presentation:preset-sub-type="from-bottom""#,
        (Effect::Fly, true) => r#" presentation:preset-sub-type="to-bottom""#,
        _ => "",
    };
    // Shown at the start of an entrance; hidden at the end of an exit.
    let set = if step.out {
        let at = if step.effect == Effect::Appear { "0s" } else { "0.499s" };
        format!(r#"<anim:set smil:begin="{at}" smil:dur="0.001s" smil:fill="hold" smil:targetElement="{target}" smil:attributeName="visibility" smil:to="hidden"/>"#)
    } else {
        format!(r#"<anim:set smil:begin="0s" smil:dur="0.001s" smil:fill="hold" smil:targetElement="{target}" smil:attributeName="visibility" smil:to="visible"/>"#)
    };
    format!(
        r#"<anim:par smil:begin="next" smil:fill="hold"><anim:par smil:begin="0s" smil:fill="hold"><anim:par smil:begin="0s" smil:fill="hold" presentation:node-type="on-click" presentation:preset-class="{class}" presentation:preset-id="{preset}"{sub}>{body}{set}</anim:par></anim:par></anim:par>"#
    )
}

/// The page's timing tree: its transition and its builds, click by click.
pub fn timing(s: &Slide, page_id: &str, element_id: impl Fn(u32) -> String) -> String {
    let steps = s.steps();
    if steps.is_empty() && s.transition.is_none() {
        return String::new();
    }
    let mut out = String::from(r#"<anim:par smil:dur="indefinite" smil:restart="never" presentation:node-type="timing-root">"#);
    if let Some(t) = &s.transition {
        out.push_str(&transition_filter(t, page_id));
    }
    if !steps.is_empty() {
        out.push_str(r#"<anim:seq smil:dur="indefinite" presentation:node-type="main-sequence">"#);
        for st in &steps {
            out.push_str(&step_xml(st, &element_id(st.id)));
        }
        out.push_str("</anim:seq>");
    }
    out.push_str("</anim:par>");
    out
}

/// The builds in a page's main sequence: (element id, click, effect, going out).
pub fn read_builds(page: &roxmltree::Node) -> Vec<(String, u32, Effect, bool)> {
    let Some(seq) = page
        .descendants()
        .find(|n| n.has_tag_name((ANIM, "seq")) && attr(n, PRESENTATION, "node-type") == Some("main-sequence"))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut click = 0;
    for group in seq.children().filter(|c| c.is_element()) {
        click += 1;
        // Effects started with the click or after it share its number.
        for e in group.descendants().filter(|n| attr(n, PRESENTATION, "preset-class").is_some()) {
            let class = attr(&e, PRESENTATION, "preset-class").unwrap_or("");
            if class != "entrance" && class != "exit" {
                continue;
            }
            let preset = attr(&e, PRESENTATION, "preset-id").unwrap_or("");
            let effect = if preset.contains("fade") || preset.contains("dissolve") {
                Effect::Fade
            } else if preset.contains("fly") || preset.contains("move") || preset.contains("wipe") {
                Effect::Fly
            } else {
                Effect::Appear
            };
            let Some(target) = e.descendants().find_map(|n| attr(&n, SMIL, "targetElement")) else { continue };
            out.push((target.to_string(), click, effect, class == "exit"));
        }
    }
    out
}

// ---------- Charts ----------

fn class_of(kind: ChartKind) -> &'static str {
    match kind {
        ChartKind::Bar | ChartKind::StackedBar => "chart:bar",
        ChartKind::Line => "chart:line",
        ChartKind::Area | ChartKind::StackedArea => "chart:area",
        ChartKind::Pie => "chart:circle",
        ChartKind::Scatter => "chart:scatter",
        ChartKind::HBar => "chart:bar",
        ChartKind::Doughnut => "chart:ring",
    }
}

fn col(c: usize) -> String {
    crate::sheets::formula::col_name(c as u32)
}

/// An embedded chart document: the chart, and its numbers in its own table.
pub fn chart_document(kind: ChartKind, names: &[String], labels: &[String], series: &[Vec<f64>], (w, h): (f64, f64)) -> String {
    let rows = labels.len().max(series.iter().map(Vec::len).max().unwrap_or(0));
    let last = rows + 1;
    // No fill behind the plot, as on our own charts.
    let mut styles = String::from(
        r#"<style:style style:name="chM" style:family="chart"><style:graphic-properties draw:stroke="none" draw:fill="none"/></style:style><style:style style:name="chW" style:family="chart"><style:graphic-properties draw:stroke="none" draw:fill="none"/></style:style>"#,
    );
    let _ = write!(
        styles,
        r#"<style:style style:name="chP" style:family="chart"><style:chart-properties chart:stacked="{}" chart:vertical="{}"/></style:style>"#,
        kind.stacked(),
        kind == ChartKind::HBar
    );
    let mut sers = String::new();
    for (i, _) in series.iter().enumerate() {
        let (r, g, b) = crate::sheets::chart::SERIES[i % crate::sheets::chart::SERIES.len()];
        let hex = format!("#{:02x}{:02x}{:02x}", (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
        let _ = write!(
            styles,
            r#"<style:style style:name="chS{i}" style:family="chart"><style:graphic-properties draw:fill="solid" draw:fill-color="{hex}" svg:stroke-color="{hex}" svg:stroke-width="0.06cm"/></style:style>"#
        );
        let c = col(i + 1);
        let domain = if kind == ChartKind::Scatter {
            format!(r#"<chart:domain table:cell-range-address="local-table.$A$2:.$A${last}"/>"#)
        } else {
            String::new()
        };
        let _ = write!(
            sers,
            r#"<chart:series chart:style-name="chS{i}" chart:values-cell-range-address="local-table.${c}$2:.${c}${last}" chart:label-cell-address="local-table.${c}$1" chart:class="{}">{domain}</chart:series>"#,
            class_of(kind)
        );
    }
    let axes = if kind.round() {
        String::new()
    } else {
        format!(
            r#"<chart:axis chart:dimension="x" chart:name="primary-x"><chart:categories table:cell-range-address="local-table.$A$2:.$A${last}"/></chart:axis><chart:axis chart:dimension="y" chart:name="primary-y"><chart:grid chart:class="major"/></chart:axis>"#
        )
    };
    // The table: labels down the first column, one column per series.
    let mut table = format!(
        r#"<table:table table:name="local-table"><table:table-header-columns><table:table-column/></table:table-header-columns><table:table-columns><table:table-column table:number-columns-repeated="{}"/></table:table-columns><table:table-header-rows><table:table-row><table:table-cell><text:p/></table:table-cell>"#,
        series.len().max(1)
    );
    for i in 0..series.len() {
        let name = names.get(i).cloned().unwrap_or_else(|| format!("Series {}", i + 1));
        let _ = write!(table, r#"<table:table-cell office:value-type="string"><text:p>{}</text:p></table:table-cell>"#, esc(&name));
    }
    table.push_str("</table:table-row></table:table-header-rows><table:table-rows>");
    for r in 0..rows {
        let label = labels.get(r).map(String::as_str).unwrap_or("");
        table.push_str("<table:table-row>");
        match label.parse::<f64>() {
            Ok(x) if kind == ChartKind::Scatter => {
                let _ = write!(table, r#"<table:table-cell office:value-type="float" office:value="{x}"><text:p>{}</text:p></table:table-cell>"#, esc(label));
            }
            _ => {
                let _ = write!(table, r#"<table:table-cell office:value-type="string"><text:p>{}</text:p></table:table-cell>"#, esc(label));
            }
        }
        for s in series {
            match s.get(r) {
                Some(v) => {
                    let _ = write!(table, r#"<table:table-cell office:value-type="float" office:value="{v}"><text:p>{v}</text:p></table:table-cell>"#);
                }
                None => table.push_str("<table:table-cell/>"),
            }
        }
        table.push_str("</table:table-row>");
    }
    table.push_str("</table:table-rows></table:table>");
    let legend = if series.len() > 1 || kind.round() {
        r#"<chart:legend chart:legend-position="bottom" style:legend-expansion="wide"/>"#
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content {ns}><office:automatic-styles>{styles}</office:automatic-styles><office:body><office:chart><chart:chart svg:width="{w}" svg:height="{h}" chart:class="{class}" chart:style-name="chM">{legend}<chart:plot-area chart:style-name="chP" table:cell-range-address="local-table.$A$1:.${lastc}${last}" chart:data-source-has-labels="both">{axes}{sers}<chart:wall chart:style-name="chW"/></chart:plot-area>{table}</chart:chart></office:chart></office:body></office:document-content>"#,
        ns = odf::NAMESPACES,
        w = pt(w),
        h = pt(h),
        class = class_of(kind),
        lastc = col(series.len().max(1)),
    )
}

/// The styles part an embedded chart document carries.
pub const CHART_STYLES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?><office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
    r#"office:version="1.3"><office:styles/></office:document-styles>"#
);

/// A chart from its embedded document: kind, series names, labels, values.
pub fn read_chart(xml: &str) -> Option<Kind> {
    let doc = crate::xml::parse(xml).ok()?;
    let chart = doc.descendants().find(|n| n.has_tag_name((CHART, "chart")))?;
    let class = attr(&chart, CHART, "class").unwrap_or("chart:bar");
    let styles = odf::Styles::read(&[&doc]);
    let plot = chart.descendants().find(|n| n.has_tag_name((CHART, "plot-area")));
    let stacked = plot
        .and_then(|p| attr(&p, CHART, "style-name"))
        .and_then(|s| styles.get("chart", s, "chart/stacked").or_else(|| styles.get("chart", s, "chart/percentage")))
        == Some("true");
    let sideways = plot.and_then(|p| attr(&p, CHART, "style-name")).and_then(|s| styles.get("chart", s, "chart/vertical")) == Some("true");
    let kind = match class.trim_start_matches("chart:") {
        "line" => ChartKind::Line,
        "area" if stacked => ChartKind::StackedArea,
        "area" => ChartKind::Area,
        "ring" => ChartKind::Doughnut,
        "circle" => ChartKind::Pie,
        "scatter" | "bubble" => ChartKind::Scatter,
        _ if stacked => ChartKind::StackedBar,
        _ if sideways => ChartKind::HBar,
        _ => ChartKind::Bar,
    };
    // The numbers: header row names the series, the first column labels the points.
    let table = chart.descendants().find(|n| n.has_tag_name((TABLE, "table")))?;
    let mut rows: Vec<Vec<(String, Option<f64>)>> = Vec::new();
    for row in table.descendants().filter(|n| n.has_tag_name((TABLE, "table-row"))).take(500) {
        let mut cells = Vec::new();
        for c in row.children().filter(|c| c.has_tag_name((TABLE, "table-cell"))) {
            let k: usize = attr(&c, TABLE, "number-columns-repeated").and_then(|v| v.parse().ok()).unwrap_or(1).min(16);
            let text = odf::paragraphs_text(&c);
            let num = attr(&c, OFFICE, "value").and_then(|v| v.parse::<f64>().ok());
            for _ in 0..k {
                cells.push((text.clone(), num));
            }
        }
        rows.push(cells);
    }
    let head = rows.first()?.clone();
    let n = head.len().saturating_sub(1).min(crate::sheets::model::CHART_SERIES);
    if n == 0 {
        return None;
    }
    let names: Vec<String> = head.iter().skip(1).take(n).map(|(t, _)| t.clone()).collect();
    let body = &rows[1..];
    let labels: Vec<String> = body.iter().map(|r| r.first().map(|(t, _)| t.clone()).unwrap_or_default()).collect();
    let series: Vec<Vec<f64>> =
        (0..n).map(|i| body.iter().map(|r| r.get(i + 1).and_then(|(t, v)| v.or_else(|| t.parse().ok())).unwrap_or(0.0)).collect()).collect();
    Some(Kind::Chart { chart: kind, names, labels, series })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chart_kind_round_trips() {
        let Kind::Chart { names, labels, series, .. } = Kind::sample_chart() else { unreachable!() };
        for kind in ChartKind::ALL {
            // OpenDocument's embedded chart, and PowerPoint's chart part.
            let odf = chart_document(kind, &names, &labels, &series, (300.0, 200.0));
            assert!(matches!(read_chart(&odf), Some(Kind::Chart { chart, .. }) if chart == kind), "{kind:?} in OpenDocument");
            let pptx = super::super::pptx_more::chart_part(kind, &names, &labels, &series);
            assert!(matches!(super::super::pptx_more::read_chart(&pptx), Some(Kind::Chart { chart, .. }) if chart == kind), "{kind:?} in PowerPoint");
        }
    }
}

