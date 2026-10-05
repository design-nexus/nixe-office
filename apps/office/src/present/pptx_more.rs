//! The newer parts of PowerPoint exchange: tables, charts, transitions and
//! builds (entrance animations), written and read back.

use super::model::*;
use std::collections::HashMap;
use std::fmt::Write as _;

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
pub const CHART_URI: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const TABLE_URI: &str = "http://schemas.openxmlformats.org/drawingml/2006/table";
pub const CHART_TYPE: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";

use super::pptx::{emu, esc, hex};

fn frame_xfrm(x: f64, y: f64, w: f64, h: f64) -> String {
    format!(r#"<p:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></p:xfrm>"#, emu(x), emu(y), emu(w), emu(h))
}

// ---------- Tables ----------

pub fn table_frame(id: usize, o: &Object, rows: &[Vec<String>], header: bool, s: &Slide) -> String {
    let n_rows = rows.len().max(1);
    let n_cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let (cw, rh) = (o.w / n_cols as f64, o.h / n_rows as f64);
    let size = (rh * 0.42).clamp(9.0, s.body_size.unwrap_or(24.0).min(24.0));
    let col = hex(&s.color);
    let line = |tag: &str| format!(r#"<a:{tag} w="12700"><a:solidFill><a:srgbClr val="{col}"><a:alpha val="35000"/></a:srgbClr></a:solidFill></a:{tag}>"#);
    let borders = format!("{}{}{}{}", line("lnL"), line("lnR"), line("lnT"), line("lnB"));
    let mut grid = String::new();
    for _ in 0..n_cols {
        let _ = write!(grid, r#"<a:gridCol w="{}"/>"#, emu(cw));
    }
    let mut trs = String::new();
    for (i, row) in rows.iter().enumerate() {
        let _ = write!(trs, r#"<a:tr h="{}">"#, emu(rh));
        for j in 0..n_cols {
            let text = row.get(j).map(String::as_str).unwrap_or("");
            let bold = header && i == 0;
            let para = if text.is_empty() {
                r#"<a:p><a:endParaRPr lang="en-US" dirty="0"/></a:p>"#.to_string()
            } else {
                format!(
                    r#"<a:p><a:r><a:rPr lang="en-US" sz="{}"{} dirty="0"><a:solidFill><a:srgbClr val="{col}"/></a:solidFill><a:latin typeface="{}"/></a:rPr><a:t>{}</a:t></a:r></a:p>"#,
                    (size * 100.0).round() as i64,
                    if bold { r#" b="1""# } else { "" },
                    esc(&s.font),
                    esc(text)
                )
            };
            let fill = if bold {
                format!(r#"<a:solidFill><a:srgbClr val="{col}"><a:alpha val="12000"/></a:srgbClr></a:solidFill>"#)
            } else {
                "<a:noFill/>".to_string()
            };
            let _ = write!(
                trs,
                r#"<a:tc><a:txBody><a:bodyPr/><a:lstStyle/>{para}</a:txBody><a:tcPr anchor="ctr">{borders}{fill}</a:tcPr></a:tc>"#
            );
        }
        trs.push_str("</a:tr>");
    }
    format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="Table {id}"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr>{xf}<a:graphic><a:graphicData uri="{TABLE_URI}"><a:tbl><a:tblPr firstRow="{fr}" bandRow="0"/><a:tblGrid>{grid}</a:tblGrid>{trs}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
        xf = frame_xfrm(o.x, o.y, o.w, o.h),
        fr = if header { 1 } else { 0 },
    )
}

pub fn read_table(frame: &roxmltree::Node) -> Option<Kind> {
    let tbl = frame.descendants().find(|n| n.has_tag_name((A, "tbl")))?;
    let header = tbl
        .children()
        .find(|n| n.has_tag_name((A, "tblPr")))
        .and_then(|p| p.attribute("firstRow"))
        .is_some_and(|v| v == "1" || v == "true");
    let rows: Vec<Vec<String>> = tbl
        .children()
        .filter(|n| n.has_tag_name((A, "tr")))
        .map(|tr| {
            tr.children()
                .filter(|n| n.has_tag_name((A, "tc")))
                .map(|tc| {
                    tc.descendants()
                        .filter(|p| p.has_tag_name((A, "p")))
                        .map(|p| p.descendants().filter(|t| t.has_tag_name((A, "t"))).filter_map(|t| t.text()).collect::<String>())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect()
        })
        .collect();
    (!rows.is_empty()).then_some(Kind::Table { rows, header })
}

// ---------- Charts ----------

pub fn chart_frame(id: usize, o: &Object, rid: &str) -> String {
    format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="Chart {id}"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>{xf}<a:graphic><a:graphicData uri="{CHART_URI}"><c:chart xmlns:c="{C}" r:id="{rid}"/></a:graphicData></a:graphic></p:graphicFrame>"#,
        xf = frame_xfrm(o.x, o.y, o.w, o.h)
    )
}

/// A chart part with its numbers written in (no linked workbook).
pub fn chart_part(chart: ChartKind, names: &[String], labels: &[String], series: &[Vec<f64>]) -> String {
    const COLOURS: [&str; 6] = ["3D7BE0", "E36B55", "4CA878", "D69D3B", "9471D5", "3BADBF"];
    let mut sers = String::new();
    let take = if chart == ChartKind::Pie { 1 } else { series.len() };
    for (i, values) in series.iter().enumerate().take(take) {
        let scatter = chart == ChartKind::Scatter;
        // A scatter chart's x values are numbers; other charts' labels are text.
        let mut cat = if scatter {
            format!(r#"<c:xVal><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="{}"/>"#, labels.len())
        } else {
            format!(r#"<c:cat><c:strLit><c:ptCount val="{}"/>"#, labels.len())
        };
        for (k, l) in labels.iter().enumerate() {
            let v = if scatter {
                crate::sheets::formula::parse_number(l).unwrap_or(k as f64 + 1.0).to_string()
            } else {
                esc(l)
            };
            let _ = write!(cat, r#"<c:pt idx="{k}"><c:v>{v}</c:v></c:pt>"#);
        }
        cat.push_str(if scatter { "</c:numLit></c:xVal>" } else { "</c:strLit></c:cat>" });
        let (vo, vc) = if scatter { ("yVal", "yVal") } else { ("val", "val") };
        let mut val = format!(r#"<c:{vo}><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="{}"/>"#, values.len());
        for (k, v) in values.iter().enumerate() {
            let _ = write!(val, r#"<c:pt idx="{k}"><c:v>{v}</c:v></c:pt>"#);
        }
        let _ = write!(val, "</c:numLit></c:{vc}>");
        let colour = COLOURS[i % COLOURS.len()];
        let sp = match chart {
            ChartKind::Line => {
                format!(r#"<c:spPr><a:ln w="28575"><a:solidFill><a:srgbClr val="{colour}"/></a:solidFill></a:ln></c:spPr>"#)
            }
            ChartKind::Pie => String::new(),
            ChartKind::Scatter => r#"<c:spPr><a:ln w="19050"><a:noFill/></a:ln></c:spPr>"#.to_string(),
            _ => format!(r#"<c:spPr><a:solidFill><a:srgbClr val="{colour}"/></a:solidFill></c:spPr>"#),
        };
        let name = names.get(i).cloned().unwrap_or_else(|| format!("Series {}", i + 1));
        let marker = match chart {
            ChartKind::Line => r#"<c:marker><c:symbol val="circle"/></c:marker>"#.to_string(),
            ChartKind::Scatter => format!(
                r#"<c:marker><c:symbol val="circle"/><c:size val="7"/><c:spPr><a:solidFill><a:srgbClr val="{colour}"/></a:solidFill></c:spPr></c:marker>"#
            ),
            _ => String::new(),
        };
        let _ = write!(
            sers,
            r#"<c:ser><c:idx val="{i}"/><c:order val="{i}"/><c:tx><c:v>{}</c:v></c:tx>{sp}{marker}{cat}{val}</c:ser>"#,
            esc(&name)
        );
    }
    let axes_ids = r#"<c:axId val="111"/><c:axId val="222"/>"#;
    let plot = match chart {
        ChartKind::Bar => format!(
            r#"<c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/>{sers}<c:gapWidth val="80"/>{axes_ids}</c:barChart>"#
        ),
        ChartKind::Line => {
            format!(r#"<c:lineChart><c:grouping val="standard"/><c:varyColors val="0"/>{sers}<c:marker val="1"/>{axes_ids}</c:lineChart>"#)
        }
        ChartKind::Area => format!(r#"<c:areaChart><c:grouping val="standard"/><c:varyColors val="0"/>{sers}{axes_ids}</c:areaChart>"#),
        ChartKind::Pie => format!(r#"<c:pieChart><c:varyColors val="1"/>{sers}<c:firstSliceAng val="0"/></c:pieChart>"#),
        ChartKind::StackedBar => format!(
            r#"<c:barChart><c:barDir val="col"/><c:grouping val="stacked"/><c:varyColors val="0"/>{sers}<c:gapWidth val="80"/><c:overlap val="100"/>{axes_ids}</c:barChart>"#
        ),
        ChartKind::StackedArea => {
            format!(r#"<c:areaChart><c:grouping val="stacked"/><c:varyColors val="0"/>{sers}{axes_ids}</c:areaChart>"#)
        }
        ChartKind::Scatter => {
            format!(r#"<c:scatterChart><c:scatterStyle val="lineMarker"/><c:varyColors val="0"/>{sers}{axes_ids}</c:scatterChart>"#)
        }
    };
    let axes = if chart == ChartKind::Pie {
        String::new()
    } else if chart == ChartKind::Scatter {
        // Two value axes: x along the bottom, y up the side.
        r#"<c:valAx><c:axId val="111"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:numFmt formatCode="General" sourceLinked="0"/><c:tickLblPos val="nextTo"/><c:crossAx val="222"/><c:crosses val="autoZero"/></c:valAx><c:valAx><c:axId val="222"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:majorGridlines/><c:numFmt formatCode="General" sourceLinked="0"/><c:tickLblPos val="nextTo"/><c:crossAx val="111"/><c:crosses val="autoZero"/></c:valAx>"#.to_string()
    } else {
        r#"<c:catAx><c:axId val="111"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:numFmt formatCode="General" sourceLinked="0"/><c:tickLblPos val="nextTo"/><c:crossAx val="222"/><c:crosses val="autoZero"/></c:catAx><c:valAx><c:axId val="222"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:majorGridlines/><c:numFmt formatCode="General" sourceLinked="0"/><c:tickLblPos val="nextTo"/><c:crossAx val="111"/><c:crosses val="autoZero"/></c:valAx>"#.to_string()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><c:chartSpace xmlns:c="{C}" xmlns:a="{A}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><c:roundedCorners val="0"/><c:chart><c:autoTitleDeleted val="1"/><c:plotArea><c:layout/>{plot}{axes}</c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend><c:plotVisOnly val="1"/></c:chart><c:spPr><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></c:spPr></c:chartSpace>"#
    )
}

/// A chart part's kind and numbers (cached values, whatever they're linked to).
pub fn read_chart(xml: &str) -> Option<Kind> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let plot = doc.descendants().find(|n| n.has_tag_name((C, "plotArea")))?;
    let (chart, node) = plot.children().find_map(|n| {
        let stacked = n
            .children()
            .find(|c| c.has_tag_name((C, "grouping")))
            .and_then(|g| g.attribute("val"))
            .is_some_and(|v| v.contains("tacked"));
        let kind = match n.tag_name().name() {
            "barChart" | "bar3DChart" if stacked => ChartKind::StackedBar,
            "barChart" | "bar3DChart" => ChartKind::Bar,
            "lineChart" | "line3DChart" => ChartKind::Line,
            "areaChart" | "area3DChart" if stacked => ChartKind::StackedArea,
            "areaChart" | "area3DChart" => ChartKind::Area,
            "pieChart" | "pie3DChart" | "doughnutChart" => ChartKind::Pie,
            "scatterChart" | "bubbleChart" => ChartKind::Scatter,
            _ => return None,
        };
        Some((kind, n))
    })?;
    let points = |n: Option<roxmltree::Node>| -> Vec<(usize, String)> {
        n.map(|n| {
            n.descendants()
                .filter(|p| p.has_tag_name((C, "pt")))
                .filter_map(|p| {
                    let idx = p.attribute("idx")?.parse().ok()?;
                    let v = p.children().find(|c| c.has_tag_name((C, "v")))?.text()?.to_string();
                    Some((idx, v))
                })
                .collect()
        })
        .unwrap_or_default()
    };
    let (mut names, mut labels, mut series) = (Vec::new(), Vec::new(), Vec::new());
    for ser in node.children().filter(|n| n.has_tag_name((C, "ser"))).take(crate::sheets::model::CHART_SERIES) {
        let name = ser
            .children()
            .find(|n| n.has_tag_name((C, "tx")))
            .and_then(|t| t.descendants().find(|n| n.has_tag_name((C, "v"))))
            .and_then(|v| v.text())
            .unwrap_or("")
            .to_string();
        let vals = points(ser.children().find(|n| n.has_tag_name((C, "val")) || n.has_tag_name((C, "yVal"))));
        let len = vals.iter().map(|(i, _)| i + 1).max().unwrap_or(0);
        let mut values = vec![0.0; len];
        for (i, v) in vals {
            values[i] = v.trim().parse().unwrap_or(0.0);
        }
        if labels.is_empty() {
            let cats = points(ser.children().find(|n| n.has_tag_name((C, "cat")) || n.has_tag_name((C, "xVal"))));
            labels = (0..len).map(|i| cats.iter().find(|(k, _)| *k == i).map(|(_, v)| v.clone()).unwrap_or_default()).collect();
        }
        names.push(name);
        series.push(values);
    }
    if series.is_empty() {
        return None;
    }
    Some(Kind::Chart { chart, names, labels, series })
}

// ---------- Transitions ----------

pub fn transition(t: &Transition) -> String {
    let spd = if t.secs < 0.6 {
        "fast"
    } else if t.secs < 1.2 {
        "med"
    } else {
        "slow"
    };
    if t.kind == TransitionKind::MagicMove {
        // PowerPoint's Morph, with a fade for older versions.
        return format!(
            r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice xmlns:p159="http://schemas.microsoft.com/office/powerpoint/2015/09/main" Requires="p159"><p:transition spd="slow"><p159:morph option="byObject"/></p:transition></mc:Choice><mc:Fallback><p:transition spd="{spd}"><p:fade/></p:transition></mc:Fallback></mc:AlternateContent>"#
        );
    }
    let effect = match t.kind {
        TransitionKind::MagicMove => "<p:fade/>",
        TransitionKind::Fade => "<p:fade/>",
        TransitionKind::Push => r#"<p:push dir="l"/>"#,
        TransitionKind::Wipe => r#"<p:wipe dir="r"/>"#,
        TransitionKind::Dissolve => "<p:dissolve/>",
    };
    format!(r#"<p:transition spd="{spd}">{effect}</p:transition>"#)
}

pub fn read_transition(doc: &roxmltree::Document) -> Option<Transition> {
    // Inside mc:AlternateContent PowerPoint repeats the transition; take the first.
    let t = doc.descendants().find(|n| n.has_tag_name((P, "transition")))?;
    let secs = match t.attribute("dur").and_then(|d| d.parse::<f64>().ok()) {
        Some(ms) => ms / 1000.0,
        None => match t.attribute("spd") {
            Some("fast") => 0.5,
            Some("slow") => 1.5,
            _ => 0.75,
        },
    };
    let kind = t.children().filter(|c| c.is_element()).find_map(|c| match c.tag_name().name() {
        "morph" => Some(TransitionKind::MagicMove),
        "fade" => Some(TransitionKind::Fade),
        "push" | "cover" | "pull" => Some(TransitionKind::Push),
        "wipe" | "split" | "strips" => Some(TransitionKind::Wipe),
        "dissolve" | "randomBar" => Some(TransitionKind::Dissolve),
        _ => None,
    });
    // An unknown effect still comes in: as a fade.
    Some(Transition { kind: kind.unwrap_or(TransitionKind::Fade), secs: secs.clamp(0.2, 5.0) })
}

// ---------- Builds ----------

/// The slide's click sequence: one entrance per built object, in order.
/// `shape_id` maps our object ids to the shape ids written.
pub fn timing(s: &Slide, shape_id: &HashMap<u32, usize>) -> String {
    let steps = s.steps();
    if steps.is_empty() {
        return String::new();
    }
    let mut n = 3;
    let mut clicks = String::new();
    for step in steps {
        let Some(spid) = shape_id.get(&step.id) else { continue };
        let build = Build { order: 0, effect: step.effect };
        let out = step.out;
        let target = format!(r#"<p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl>"#);
        let (preset, subtype) = match build.effect {
            Effect::Appear => (1, 0),
            Effect::Fade => (10, 0),
            Effect::Fly => (2, 4),
        };
        let (a, b, c, d) = (n, n + 1, n + 2, n + 3);
        n += 5;
        // Coming in: shown at once, then faded or flown in. Going out: faded or
        // flown away, then hidden at the end.
        let (visibility, delay) = if out { ("hidden", if build.effect == Effect::Appear { 0 } else { 499 }) } else { ("visible", 0) };
        let show = format!(
            r#"<p:set><p:cBhvr><p:cTn id="{d}" dur="1" fill="hold"><p:stCondLst><p:cond delay="{delay}"/></p:stCondLst></p:cTn>{target}<p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val="{visibility}"/></p:to></p:set>"#
        );
        let (from, to) = if out { ("#ppt_y", "1+#ppt_h/2") } else { ("1+#ppt_h/2", "#ppt_y") };
        let motion = match build.effect {
            Effect::Appear => String::new(),
            Effect::Fade => format!(
                r#"<p:animEffect transition="{}" filter="fade"><p:cBhvr><p:cTn id="{}" dur="500"/>{target}</p:cBhvr></p:animEffect>"#,
                if out { "out" } else { "in" },
                d + 1
            ),
            Effect::Fly => format!(
                r##"<p:anim calcmode="lin" valueType="num"><p:cBhvr additive="base"><p:cTn id="{}" dur="500" fill="hold"/>{target}<p:attrNameLst><p:attrName>ppt_y</p:attrName></p:attrNameLst></p:cBhvr><p:tavLst><p:tav tm="0"><p:val><p:strVal val="{from}"/></p:val></p:tav><p:tav tm="100000"><p:val><p:strVal val="{to}"/></p:val></p:tav></p:tavLst></p:anim>"##,
                d + 1
            ),
        };
        let class = if out { "exit" } else { "entr" };
        let _ = write!(
            clicks,
            r#"<p:par><p:cTn id="{a}" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="{b}" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="{c}" presetID="{preset}" presetClass="{class}" presetSubtype="{subtype}" fill="hold" nodeType="clickEffect"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst>{motion}{show}</p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par>"#
        );
    }
    format!(
        r#"<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst><p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst>{clicks}</p:childTnLst></p:cTn><p:prevCondLst><p:cond evt="onPrev" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst><p:nextCondLst><p:cond evt="onNext" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"#
    )
}

/// Entrance and exit effects in click order: (shape id, order, effect, exit).
/// Effects that start with or after the previous one share its click.
pub fn read_builds(doc: &roxmltree::Document) -> Vec<(String, u32, Effect, bool)> {
    let mut out: Vec<(String, u32, Effect, bool)> = Vec::new();
    let mut order = 0;
    for ctn in doc
        .descendants()
        .filter(|n| n.has_tag_name((P, "cTn")) && matches!(n.attribute("presetClass"), Some("entr") | Some("exit")))
    {
        let exit = ctn.attribute("presetClass") == Some("exit");
        let Some(spid) =
            ctn.descendants().find(|n| n.has_tag_name((P, "spTgt"))).and_then(|t| t.attribute("spid")).map(String::from)
        else {
            continue;
        };
        if ctn.attribute("nodeType") == Some("clickEffect") || out.is_empty() {
            order += 1;
        }
        let effect = match ctn.attribute("presetID") {
            Some("1") => Effect::Appear,
            Some("2") | Some("47") => Effect::Fly,
            _ => Effect::Fade,
        };
        if !out.iter().any(|(s, _, _, x)| *s == spid && *x == exit) {
            out.push((spid, order, effect, exit));
        }
    }
    out
}
