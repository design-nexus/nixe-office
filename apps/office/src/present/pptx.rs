//! PowerPoint files: slide text, colours, fonts, pictures, rectangles and
//! ellipses both ways; speaker notes come in from PowerPoint but aren't written.

use super::model::*;
use super::pptx_more as more;
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::{Cursor, Read, Write};

/// English Metric Units per point.
const EMU: f64 = 12700.0;
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

pub(super) fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' => {}
            c => o.push(c),
        }
    }
    o
}

pub(super) fn hex(c: &str) -> String {
    let h = c.trim_start_matches('#');
    if h.len() == 6 { h.to_uppercase() } else { "202536".into() }
}

pub(super) fn emu(v: f64) -> i64 {
    (v * EMU).round() as i64
}

fn rels(entries: &[(String, &str, String)]) -> String {
    let mut s = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{PKG}">"#);
    for (id, kind, target) in entries {
        let _ = write!(s, r#"<Relationship Id="{id}" Type="{R}/{kind}" Target="{}"/>"#, esc(target));
    }
    s.push_str("</Relationships>");
    s
}

fn xfrm(x: f64, y: f64, w: f64, h: f64) -> String {
    format!(r#"<a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#, emu(x), emu(y), emu(w), emu(h))
}

/// What goes in a text shape.
struct TextShape<'a> {
    id: usize,
    name: &'a str,
    /// The placeholder type, or None for a plain text box.
    ph: Option<&'a str>,
    text: &'a str,
    frame: (f64, f64, f64, f64),
    size: f64,
    centred: bool,
    bold: bool,
}

fn text_shape(t: TextShape, s: &Slide) -> String {
    let TextShape { id, name, ph, text, frame, size, centred, bold } = t;
    let (x, y, w, h) = frame;
    let ph = ph.map(|t| format!(r#"<p:ph type="{t}"/>"#)).unwrap_or_default();
    let mut paras = String::new();
    for line in text.split('\n') {
        let _ = write!(
            paras,
            r#"<a:p>{algn}<a:r><a:rPr lang="en-US" sz="{sz}"{b}{i} dirty="0"><a:solidFill><a:srgbClr val="{col}"/></a:solidFill><a:latin typeface="{font}"/></a:rPr><a:t>{t}</a:t></a:r></a:p>"#,
            algn = if centred { r#"<a:pPr algn="ctr"/>"# } else { "" },
            sz = (size * 100.0).round() as i64,
            b = if bold || s.bold { r#" b="1""# } else { "" },
            i = if s.italic { r#" i="1""# } else { "" },
            col = hex(&s.color),
            font = esc(&s.font),
            t = esc(line),
        );
    }
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr txBox="1"/><p:nvPr>{ph}</p:nvPr></p:nvSpPr><p:spPr>{xf}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap="square" anchor="{anchor}"><a:normAutofit/></a:bodyPr><a:lstStyle/>{paras}</p:txBody></p:sp>"#,
        name = esc(name),
        xf = xfrm(x, y, w, h),
        anchor = if centred || ph.contains("title") || ph.contains("Title") { "ctr" } else { "t" },
    )
}

/// What writing collects besides the slide XML.
struct Out {
    parts: Vec<(String, Vec<u8>)>,
    slide_rels: Vec<(String, &'static str, String)>,
    media_n: usize,
    charts: usize,
}

/// One object as a shape, picture, table or chart.
fn object_xml(o: &Object, id: usize, s: &Slide, media: &BTreeMap<String, Vec<u8>>, out: &mut Out) -> String {
    let mut tree = String::new();
    match &o.kind {
        Kind::Text { text } => tree.push_str(&text_shape(
            TextShape {
                id,
                name: "Text box",
                ph: None,
                text,
                frame: (o.x, o.y, o.w, o.h),
                size: s.body_size.unwrap_or(24.0),
                centred: false,
                bold: false,
            },
            s,
        )),
        Kind::Shape { shape, fill } => {
            let _ = write!(
                tree,
                r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="Shape {id}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst="{}"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="{}"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr></p:sp>"#,
                xfrm(o.x, o.y, o.w, o.h),
                if *shape == Shape::Ellipse { "ellipse" } else { "rect" },
                hex(fill)
            );
        }
        Kind::Image { image } => {
            let Some(bytes) = media.get(image) else { return tree };
            out.media_n += 1;
            let rid = format!("rId{}", out.slide_rels.len() + 1);
            let name = format!("image{}.png", out.media_n);
            out.parts.push((format!("ppt/media/{name}"), bytes.clone()));
            out.slide_rels.push((rid.clone(), "image", format!("../media/{name}")));
            let _ = write!(
                tree,
                r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="Picture {id}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#,
                xfrm(o.x, o.y, o.w, o.h)
            );
        }
        Kind::Table { rows, header } => tree.push_str(&more::table_frame(id, o, rows, *header, s)),
        Kind::Chart { chart, names, labels, series } => {
            out.charts += 1;
            let rid = format!("rId{}", out.slide_rels.len() + 1);
            out.parts.push((
                format!("ppt/charts/chart{}.xml", out.charts),
                more::chart_part(*chart, names, labels, series).into_bytes(),
            ));
            out.slide_rels.push((rid.clone(), "chart", format!("../charts/chart{}.xml", out.charts)));
            tree.push_str(&more::chart_frame(id, o, &rid));
        }
    }
    tree
}

/// The deck as a .pptx, and what PowerPoint files won't keep.
pub fn write(deck: &Deck, media: &BTreeMap<String, Vec<u8>>) -> Result<(Vec<u8>, Vec<String>)> {
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    let n = deck.slides.len();
    let mut types = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#,
    );
    for i in 1..=n {
        let _ = write!(
            types,
            r#"<Override PartName="/ppt/slides/slide{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#
        );
    }
    parts.push(("_rels/.rels".into(), rels(&[("rId1".into(), "officeDocument", "ppt/presentation.xml".into())]).into_bytes()));

    let mut pres = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentation xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>"#
    );
    let mut pres_rels = vec![("rId1".to_string(), "slideMaster", "slideMasters/slideMaster1.xml".to_string())];
    for i in 0..n {
        let _ = write!(pres, r#"<p:sldId id="{}" r:id="rId{}"/>"#, 256 + i, i + 2);
        pres_rels.push((format!("rId{}", i + 2), "slide", format!("slides/slide{}.xml", i + 1)));
    }
    let _ = write!(
        pres,
        r#"</p:sldIdLst><p:sldSz cx="{}" cy="{}"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#,
        emu(W),
        emu(H)
    );
    parts.push(("ppt/presentation.xml".into(), pres.into_bytes()));
    parts.push(("ppt/_rels/presentation.xml.rels".into(), rels(&pres_rels).into_bytes()));

    let empty_tree =
        r#"<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree>"#;
    parts.push((
        "ppt/slideMasters/slideMaster1.xml".into(),
        format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldMaster xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld>{empty_tree}</p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"#).into_bytes(),
    ));
    parts.push((
        "ppt/slideMasters/_rels/slideMaster1.xml.rels".into(),
        rels(&[
            ("rId1".into(), "slideLayout", "../slideLayouts/slideLayout1.xml".into()),
            ("rId2".into(), "theme", "../theme/theme1.xml".into()),
        ])
        .into_bytes(),
    ));
    parts.push((
        "ppt/slideLayouts/slideLayout1.xml".into(),
        format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldLayout xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}" type="blank" preserve="1"><p:cSld name="Nexus Office">{empty_tree}</p:cSld></p:sldLayout>"#).into_bytes(),
    ));
    parts.push((
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(),
        rels(&[("rId1".into(), "slideMaster", "../slideMasters/slideMaster1.xml".into())]).into_bytes(),
    ));
    let mut clr = String::new();
    for (k, v) in [
        ("dk1", "202536"),
        ("lt1", "FFFFFF"),
        ("dk2", "343A46"),
        ("lt2", "F4F5F7"),
        ("accent1", "5273CF"),
        ("accent2", "D66A76"),
        ("accent3", "65A888"),
        ("accent4", "B38CD8"),
        ("accent5", "D89A58"),
        ("accent6", "5EA8B7"),
        ("hlink", "5273CF"),
        ("folHlink", "8057A3"),
    ] {
        let _ = write!(clr, r#"<a:{k}><a:srgbClr val="{v}"/></a:{k}>"#);
    }
    parts.push((
        "ppt/theme/theme1.xml".into(),
        format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="{A}" name="Nexus Office"><a:themeElements><a:clrScheme name="Nexus Office">{clr}</a:clrScheme><a:fontScheme name="Nexus Office"><a:majorFont><a:latin typeface="{DEFAULT_FONT}"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="{DEFAULT_FONT}"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Nexus Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#).into_bytes(),
    ));

    let mut out = Out { parts, slide_rels: Vec::new(), media_n: 0, charts: 0 };
    for (i, s) in deck.slides.iter().enumerate() {
        let mut tree = String::from(empty_tree.trim_end_matches("</p:spTree>"));
        out.slide_rels = vec![("rId1".to_string(), "slideLayout", "../slideLayouts/slideLayout1.xml".to_string())];
        let mut id = 2;
        if let Some(f) = title_frame(s.layout) {
            let ph = if s.layout == Layout::Title { "ctrTitle" } else { "title" };
            tree.push_str(&text_shape(
                TextShape {
                    id,
                    name: "Title",
                    ph: Some(ph),
                    text: &s.title,
                    frame: (f.0, f.1, f.2, f.3),
                    size: s.title_size.unwrap_or(f.4),
                    centred: f.5,
                    bold: true,
                },
                s,
            ));
            id += 1;
        }
        if let Some(f) = body_frame(s.layout) {
            let ph = if s.layout == Layout::Title { "subTitle" } else { "body" };
            tree.push_str(&text_shape(
                TextShape {
                    id,
                    name: "Content",
                    ph: Some(ph),
                    text: &s.body,
                    frame: (f.0, f.1, f.2, f.3),
                    size: s.body_size.unwrap_or(f.4),
                    centred: f.5,
                    bold: false,
                },
                s,
            ));
            id += 1;
        }
        // Shape ids for every object first, so builds can point at them.
        let mut shape_id: HashMap<u32, usize> = HashMap::new();
        let mut written_groups: Vec<u32> = Vec::new();
        for o in &s.objects {
            if let Some(g) = o.group {
                if written_groups.contains(&g) {
                    continue;
                }
                written_groups.push(g);
                let members: Vec<&Object> = s.objects.iter().filter(|m| m.group == Some(g)).collect();
                let x0 = members.iter().map(|m| m.x).fold(f64::INFINITY, f64::min);
                let y0 = members.iter().map(|m| m.y).fold(f64::INFINITY, f64::min);
                let x1 = members.iter().map(|m| m.x + m.w).fold(f64::NEG_INFINITY, f64::max);
                let y1 = members.iter().map(|m| m.y + m.h).fold(f64::NEG_INFINITY, f64::max);
                let (gx, gy, gw, gh) = (emu(x0), emu(y0), emu(x1 - x0), emu(y1 - y0));
                // Child space equals the group's own, so members keep slide coordinates.
                let _ = write!(
                    tree,
                    r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="{id}" name="Group {id}"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="{gx}" y="{gy}"/><a:ext cx="{gw}" cy="{gh}"/><a:chOff x="{gx}" y="{gy}"/><a:chExt cx="{gw}" cy="{gh}"/></a:xfrm></p:grpSpPr>"#
                );
                id += 1;
                for m in members {
                    shape_id.insert(m.id, id);
                    tree.push_str(&object_xml(m, id, s, media, &mut out));
                    id += 1;
                }
                tree.push_str("</p:grpSp>");
                continue;
            }
            shape_id.insert(o.id, id);
            tree.push_str(&object_xml(o, id, s, media, &mut out));
            id += 1;
        }
        tree.push_str("</p:spTree>");
        let slide = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="{}"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>{tree}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{}{}</p:sld>"#,
            hex(&s.background),
            s.transition.as_ref().map(more::transition).unwrap_or_default(),
            more::timing(s, &shape_id),
        );
        out.parts.push((format!("ppt/slides/slide{}.xml", i + 1), slide.into_bytes()));
        out.parts.push((format!("ppt/slides/_rels/slide{}.xml.rels", i + 1), rels(&out.slide_rels).into_bytes()));
    }
    for k in 1..=out.charts {
        let _ = write!(types, r#"<Override PartName="/ppt/charts/chart{k}.xml" ContentType="{}"/>"#, more::CHART_TYPE);
    }
    types.push_str("</Types>");
    let mut parts = out.parts;
    parts.insert(0, ("[Content_Types].xml".into(), types.into_bytes()));

    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in parts {
        zip.start_file(name, opts)?;
        zip.write_all(&bytes)?;
    }
    let bytes = zip.finish()?.into_inner();
    let lost =
        if deck.slides.iter().any(|s| !s.notes.trim().is_empty()) { vec!["speaker notes".to_string()] } else { Vec::new() };
    Ok((bytes, lost))
}

// ---------- Reading ----------

fn part(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name.trim_start_matches('/')).ok()?;
    if f.size() > 64 * 1024 * 1024 {
        return None;
    }
    let mut v = Vec::new();
    f.read_to_end(&mut v).ok()?;
    Some(v)
}

fn text_part(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    part(zip, name).map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn resolve(base: &str, rel: &str) -> String {
    if let Some(abs) = rel.strip_prefix('/') {
        return abs.to_string();
    }
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn folder(p: &str) -> &str {
    p.rsplit_once('/').map(|(a, _)| a).unwrap_or("")
}

fn rels_of(p: &str) -> String {
    match p.rsplit_once('/') {
        Some((d, f)) => format!("{d}/_rels/{f}.rels"),
        None => format!("_rels/{p}.rels"),
    }
}

/// id → (type ending, absolute path)
fn read_rels(xml: &str, base: &str) -> HashMap<String, (String, String)> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return HashMap::new() };
    doc.descendants()
        .filter(|n| n.has_tag_name("Relationship"))
        .filter_map(|n| {
            let kind = n.attribute("Type")?.rsplit('/').next()?.to_string();
            Some((n.attribute("Id")?.to_string(), (kind, resolve(base, n.attribute("Target")?))))
        })
        .collect()
}

/// An attribute in the relationships namespace (`r:id`, `r:embed`).
fn attr_ns<'a>(n: &roxmltree::Node<'a, '_>, local: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == local && a.namespace() == Some(R)).map(|a| a.value())
}

/// A shape's box in slide points, with any groups it sits in applied.
fn geometry(n: &roxmltree::Node) -> Option<(f64, f64, f64, f64)> {
    // `a:xfrm` for shapes and pictures, `p:xfrm` for graphic frames.
    let x = n.descendants().find(|c| c.tag_name().name() == "xfrm")?;
    let (mut x0, mut y0, mut w, mut h) = xfrm_box(&x, "off", "ext")?;
    for g in n.ancestors().skip(1).filter(|a| a.has_tag_name((P, "grpSp"))) {
        let Some(gx) = g
            .children()
            .find(|c| c.has_tag_name((P, "grpSpPr")))
            .and_then(|p| p.children().find(|c| c.has_tag_name((A, "xfrm"))))
        else {
            continue;
        };
        let (Some((ox, oy, ew, eh)), Some((cx, cy, cw, ch))) = (xfrm_box(&gx, "off", "ext"), xfrm_box(&gx, "chOff", "chExt"))
        else {
            continue;
        };
        let (sx, sy) = (if cw > 0.0 { ew / cw } else { 1.0 }, if ch > 0.0 { eh / ch } else { 1.0 });
        (x0, y0, w, h) = (ox + (x0 - cx) * sx, oy + (y0 - cy) * sy, w * sx, h * sy);
    }
    Some((x0, y0, w, h))
}

fn xfrm_box(x: &roxmltree::Node, off: &str, ext: &str) -> Option<(f64, f64, f64, f64)> {
    let o = x.children().find(|c| c.has_tag_name((A, off)))?;
    let e = x.children().find(|c| c.has_tag_name((A, ext)))?;
    let v = |n: &roxmltree::Node, k: &str| n.attribute(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / EMU;
    Some((v(&o, "x"), v(&o, "y"), v(&e, "cx"), v(&e, "cy")))
}

/// The shape's id in the file (`p:cNvPr id`), which animations refer to.
fn shape_id(n: &roxmltree::Node) -> Option<String> {
    n.descendants().find(|c| c.has_tag_name((P, "cNvPr"))).and_then(|c| c.attribute("id")).map(String::from)
}

/// Which top-level group a shape is in, numbered from 1.
fn group_number(n: &roxmltree::Node) -> Option<u32> {
    let top = n.ancestors().skip(1).filter(|a| a.has_tag_name((P, "grpSp"))).last()?;
    let tree = top.parent()?;
    let i = tree.children().filter(|c| c.has_tag_name((P, "grpSp"))).position(|c| c == top)?;
    Some(i as u32 + 1)
}

/// The text of a shape's paragraphs, one line each.
fn shape_text(n: &roxmltree::Node) -> String {
    let Some(body) = n.children().find(|c| c.has_tag_name((P, "txBody"))) else { return String::new() };
    body.children()
        .filter(|p| p.has_tag_name((A, "p")))
        .map(|p| p.descendants().filter(|t| t.has_tag_name((A, "t"))).filter_map(|t| t.text()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end_matches('\n')
        .to_string()
}

/// A picture from the file, as PNG (the renderer and the native file use PNG).
fn as_png(bytes: &[u8]) -> Option<Vec<u8>> {
    use gtk::prelude::*;
    if bytes.starts_with(b"\x89PNG") {
        return Some(bytes.to_vec());
    }
    let t = gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from(bytes)).ok()?;
    Some(t.save_to_png_bytes().to_vec())
}

pub fn read(bytes: &[u8]) -> Result<(Deck, BTreeMap<String, Vec<u8>>)> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).context("it isn't a PowerPoint file")?;
    let root = read_rels(&text_part(&mut zip, "_rels/.rels").unwrap_or_default(), "");
    let pres_path = root
        .values()
        .find(|(k, _)| k == "officeDocument")
        .map(|(_, p)| p.clone())
        .unwrap_or_else(|| "ppt/presentation.xml".into());
    let pres = text_part(&mut zip, &pres_path).context("the presentation is missing")?;
    let pdoc = roxmltree::Document::parse(&pres).context("the presentation is damaged")?;
    let prels = read_rels(&text_part(&mut zip, &rels_of(&pres_path)).unwrap_or_default(), folder(&pres_path));
    // The slide size, to scale everything onto our 960 × 540 canvas.
    let (sw, sh) = pdoc
        .descendants()
        .find(|n| n.has_tag_name((P, "sldSz")))
        .and_then(|n| Some((n.attribute("cx")?.parse::<f64>().ok()? / EMU, n.attribute("cy")?.parse::<f64>().ok()? / EMU)))
        .unwrap_or((W, H));
    let (fx, fy) = (W / sw.max(1.0), H / sh.max(1.0));
    let ids: Vec<String> =
        pdoc.descendants().filter(|n| n.has_tag_name((P, "sldId"))).filter_map(|n| attr_ns(&n, "id").map(String::from)).collect();
    let mut media = BTreeMap::new();
    let mut slides = Vec::new();
    for id in ids.into_iter().take(MAX_SLIDES) {
        let Some((_, path)) = prels.get(&id) else { continue };
        let Some(xml) = text_part(&mut zip, path) else { continue };
        let doc = roxmltree::Document::parse(&xml).with_context(|| format!("{path} is damaged"))?;
        let srels = read_rels(&text_part(&mut zip, &rels_of(path)).unwrap_or_default(), folder(path));
        let mut s = Slide { layout: Layout::Blank, ..Default::default() };
        let mut spids: HashMap<String, u32> = HashMap::new();
        if let Some(c) = doc
            .descendants()
            .find(|n| n.has_tag_name((P, "bg")))
            .and_then(|bg| bg.descendants().find(|n| n.has_tag_name((A, "srgbClr"))))
            .and_then(|c| c.attribute("val"))
        {
            s.background = format!("#{}", c.to_lowercase());
        }
        let (mut title, mut body, mut centre_title, mut section) = (None::<String>, Vec::<String>::new(), false, false);
        let mut colour = None;
        for sp in doc.descendants().filter(|n| n.has_tag_name((P, "sp"))) {
            let ph =
                sp.descendants().find(|n| n.has_tag_name((P, "ph"))).map(|p| p.attribute("type").unwrap_or("body").to_string());
            let text = shape_text(&sp);
            // The first run's look stands for the slide's.
            if let Some(rpr) = sp.descendants().find(|n| n.has_tag_name((A, "rPr"))) {
                let size = rpr.attribute("sz").and_then(|v| v.parse::<f64>().ok()).map(|v| v / 100.0);
                if rpr.attribute("b") == Some("1") && ph.as_deref().is_some_and(|p| !p.contains("itle")) {
                    s.bold = true;
                }
                if rpr.attribute("i") == Some("1") {
                    s.italic = true;
                }
                if let Some(f) = rpr
                    .descendants()
                    .find(|n| n.has_tag_name((A, "latin")))
                    .and_then(|n| n.attribute("typeface"))
                    .filter(|f| !f.starts_with('+'))
                {
                    s.font = f.to_string();
                }
                if colour.is_none() {
                    colour = rpr
                        .descendants()
                        .find(|n| n.has_tag_name((A, "srgbClr")))
                        .and_then(|n| n.attribute("val"))
                        .map(|v| format!("#{}", v.to_lowercase()));
                }
                match ph.as_deref() {
                    Some("title") | Some("ctrTitle") => s.title_size = size,
                    Some(_) => s.body_size = s.body_size.or(size),
                    None => {}
                }
            }
            let geo = geometry(&sp).map(|(x, y, w, h)| (x * fx, y * fy, w * fx, h * fy));
            match ph.as_deref() {
                Some("title") | Some("ctrTitle") => {
                    centre_title |= ph.as_deref() == Some("ctrTitle");
                    // A centred plain title is a section slide.
                    section |= ph.as_deref() == Some("title")
                        && sp.descendants().any(|n| n.has_tag_name((A, "pPr")) && n.attribute("algn") == Some("ctr"));
                    title = Some(text);
                }
                Some("body") | Some("subTitle") | Some("obj") => body.push(text),
                Some(_) => {}
                None => {
                    let fill = sp
                        .children()
                        .find(|n| n.has_tag_name((P, "spPr")))
                        .and_then(|p| p.children().find(|n| n.has_tag_name((A, "solidFill"))))
                        .and_then(|f| f.descendants().find(|n| n.has_tag_name((A, "srgbClr"))))
                        .and_then(|c| c.attribute("val"))
                        .map(|v| format!("#{}", v.to_lowercase()));
                    let prst = sp
                        .descendants()
                        .find(|n| n.has_tag_name((A, "prstGeom")))
                        .and_then(|g| g.attribute("prst"))
                        .unwrap_or("rect");
                    let Some((x, y, w, h)) = geo else { continue };
                    let kind = if !text.trim().is_empty() {
                        Kind::Text { text }
                    } else if let Some(fill) = fill {
                        Kind::Shape { shape: if prst == "ellipse" { Shape::Ellipse } else { Shape::Rectangle }, fill }
                    } else {
                        continue;
                    };
                    if s.objects.len() < MAX_OBJECTS {
                        let mut o = Object::new(s.next_id(), kind, (x, y, w, h));
                        o.clamp();
                        o.group = group_number(&sp);
                        if let Some(sid) = shape_id(&sp) {
                            spids.insert(sid, o.id);
                        }
                        s.objects.push(o);
                    }
                }
            }
        }
        for pic in doc.descendants().filter(|n| n.has_tag_name((P, "pic"))) {
            let Some(rid) = pic.descendants().find(|n| n.has_tag_name((A, "blip"))).and_then(|b| attr_ns(&b, "embed")) else {
                continue;
            };
            let Some((_, target)) = srels.get(rid) else { continue };
            let Some(png) = part(&mut zip, target).and_then(|b| as_png(&b)) else { continue };
            let Some((x, y, w, h)) = geometry(&pic).map(|(x, y, w, h)| (x * fx, y * fy, w * fx, h * fy)) else { continue };
            let name = format!("picture-{}.png", media.len() + 1);
            media.insert(name.clone(), png);
            if s.objects.len() < MAX_OBJECTS {
                let mut o = Object::new(s.next_id(), Kind::Image { image: name }, (x, y, w, h));
                o.clamp();
                o.group = group_number(&pic);
                if let Some(sid) = shape_id(&pic) {
                    spids.insert(sid, o.id);
                }
                s.objects.push(o);
            }
        }
        // Tables and charts.
        for frame in doc.descendants().filter(|n| n.has_tag_name((P, "graphicFrame"))) {
            let Some(data) = frame.descendants().find(|n| n.has_tag_name((A, "graphicData"))) else { continue };
            let kind = if data.descendants().any(|n| n.has_tag_name((A, "tbl"))) {
                more::read_table(&frame)
            } else if data.attribute("uri") == Some(more::CHART_URI) {
                data.children()
                    .find(|n| n.tag_name().name() == "chart")
                    .and_then(|c| attr_ns(&c, "id"))
                    .and_then(|rid| srels.get(rid))
                    .and_then(|(_, target)| text_part(&mut zip, target))
                    .and_then(|xml| more::read_chart(&xml))
            } else {
                None
            };
            let (Some(kind), Some((x, y, w, h))) = (kind, geometry(&frame).map(|(x, y, w, h)| (x * fx, y * fy, w * fx, h * fy)))
            else {
                continue;
            };
            if s.objects.len() < MAX_OBJECTS {
                let mut o = Object::new(s.next_id(), kind, (x, y, w, h));
                o.clamp();
                o.group = group_number(&frame);
                if let Some(sid) = shape_id(&frame) {
                    spids.insert(sid, o.id);
                }
                s.objects.push(o);
            }
        }
        // Groups of one (members left out) aren't groups.
        let groups: Vec<u32> = s.objects.iter().filter_map(|o| o.group).collect();
        for o in &mut s.objects {
            if o.group.is_some_and(|g| groups.iter().filter(|x| **x == g).count() < 2) {
                o.group = None;
            }
        }
        s.transition = more::read_transition(&doc);
        for (sid, order, effect) in more::read_builds(&doc) {
            if let Some(oid) = spids.get(&sid)
                && let Some(o) = s.objects.iter_mut().find(|o| o.id == *oid)
            {
                o.build = Some(Build { order, effect });
            }
        }
        // Speaker notes, from the notes slide's body.
        if let Some((_, npath)) = srels.values().find(|(k, _)| k == "notesSlide")
            && let Some(nx) = text_part(&mut zip, npath)
            && let Ok(nd) = roxmltree::Document::parse(&nx)
        {
            s.notes = nd
                .descendants()
                .filter(|n| n.has_tag_name((P, "sp")))
                .filter(|sp| {
                    sp.descendants().any(|p| p.has_tag_name((P, "ph")) && p.attribute("type").is_none_or(|t| t == "body"))
                })
                .map(|sp| shape_text(&sp))
                .collect::<Vec<_>>()
                .join("\n");
        }
        if let Some(c) = colour {
            s.color = c;
        }
        s.layout = match (&title, body.is_empty()) {
            (Some(_), _) if centre_title => Layout::Title,
            (Some(_), _) if section => Layout::Section,
            (Some(_), false) => Layout::Content,
            // A title over objects (a "title only" slide) keeps its title at the top.
            (Some(_), true) if !s.objects.is_empty() => Layout::Content,
            (Some(_), true) => Layout::Section,
            (None, false) => Layout::Content,
            (None, true) => Layout::Blank,
        };
        s.title = title.unwrap_or_default();
        s.body = body.join("\n");
        slides.push(s);
    }
    if slides.is_empty() {
        bail!("the presentation has no slides");
    }
    Ok((Deck { slides }, media))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_powerpoint() {
        // A 1×1 PNG.
        let png =
            base64("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==").unwrap();
        let mut media = BTreeMap::new();
        media.insert("p.png".to_string(), png.clone());
        let mut deck = super::super::templates::build("trip-plan").unwrap();
        deck.slides.push(Slide { layout: Layout::Section, title: "Thanks".into(), body: "See you".into(), ..Default::default() });
        deck.slides[1].objects.push(Object::new(1, Kind::Shape { shape: Shape::Ellipse, fill: "#e47b89".into() }, (100.0, 100.0, 200.0, 100.0)));
        deck.slides[1].objects.push(Object::new(2, Kind::Text { text: "Note this & that".into() }, (500.0, 300.0, 300.0, 80.0)));
        deck.slides[1].objects.push(Object::new(3, Kind::Image { image: "p.png".into() }, (10.0, 10.0, 64.0, 64.0)));
        deck.slides[2].font = "Noto Serif".into();
        deck.slides[2].italic = true;
        let (bytes, lost) = write(&deck, &media).unwrap();
        assert_eq!(lost, vec!["speaker notes"]);
        let (back, back_media) = read(&bytes).unwrap();
        assert_eq!(back.slides.len(), 5);
        assert_eq!(back.slides[4].layout, Layout::Section);
        let (a, b) = (&deck.slides[0], &back.slides[0]);
        assert_eq!(b.layout, Layout::Title);
        assert_eq!((b.title.as_str(), b.body.as_str()), (a.title.as_str(), a.body.as_str()));
        assert_eq!(b.background, a.background);
        assert_eq!(b.color, a.color);
        assert_eq!(back.slides[1].layout, Layout::Content);
        assert_eq!(back.slides[1].body, deck.slides[1].body);
        let objs = &back.slides[1].objects;
        assert!(
            objs.iter().any(|o| matches!(&o.kind, Kind::Shape { shape: Shape::Ellipse, fill } if fill == "#e47b89")
                && (o.x - 100.0).abs() < 0.01)
        );
        assert!(objs.iter().any(|o| matches!(&o.kind, Kind::Text { text } if text == "Note this & that")));
        assert!(objs.iter().any(|o| matches!(o.kind, Kind::Image { .. })));
        assert_eq!(back_media.values().next(), Some(&png));
        assert_eq!(back.slides[2].font, "Noto Serif");
        assert!(back.slides[2].italic);
        assert_eq!(back.slides[3].layout, Layout::Content);
    }

    #[test]
    fn tables_charts_groups_transitions_and_builds() {
        let mut deck = Deck::default();
        let s = &mut deck.slides[0];
        s.layout = Layout::Blank;
        s.transition = Some(Transition { kind: TransitionKind::Push, secs: 0.75 });
        let mut table = Object::new(1, Kind::Table { rows: vec![vec!["Item".into(), "Cost".into()], vec!["Rent".into(), "1200".into()]], header: true }, (40.0, 40.0, 400.0, 120.0));
        table.build = Some(Build { order: 2, effect: Effect::Fade });
        let mut chart = Object::new(2, Kind::sample_chart(), (480.0, 40.0, 420.0, 300.0));
        chart.build = Some(Build { order: 1, effect: Effect::Fly });
        let mut a = Object::new(3, Kind::Shape { shape: Shape::Rectangle, fill: "#336699".into() }, (100.0, 300.0, 100.0, 100.0));
        let mut b = Object::new(4, Kind::Text { text: "Grouped".into() }, (220.0, 320.0, 160.0, 60.0));
        a.group = Some(1);
        b.group = Some(1);
        b.build = Some(Build { order: 3, effect: Effect::Appear });
        s.objects = vec![table, chart, a, b];
        let (bytes, _) = write(&deck, &BTreeMap::new()).unwrap();
        let (back, _) = read(&bytes).unwrap();
        let s = &back.slides[0];
        assert_eq!(s.transition.map(|t| t.kind), Some(TransitionKind::Push));
        let find = |f: fn(&Kind) -> bool| s.objects.iter().find(|o| f(&o.kind)).expect("the object came back");
        let t = find(|k| matches!(k, Kind::Table { .. }));
        assert_eq!(t.kind, deck.slides[0].objects[0].kind);
        assert!((t.x - 40.0).abs() < 0.01 && (t.w - 400.0).abs() < 0.01);
        let c = find(|k| matches!(k, Kind::Chart { .. }));
        assert_eq!(c.kind, Kind::sample_chart());
        let shape = find(|k| matches!(k, Kind::Shape { .. }));
        let text = find(|k| matches!(k, Kind::Text { .. }));
        assert!(shape.group.is_some() && shape.group == text.group);
        assert!((text.x - 220.0).abs() < 0.01 && (text.y - 320.0).abs() < 0.01);
        assert!(c.group.is_none());
        // The click order and effects survive.
        let steps: Vec<Effect> =
            s.build_steps().iter().map(|id| s.objects.iter().find(|o| o.id == *id).unwrap().build.unwrap().effect).collect();
        assert_eq!(steps, [Effect::Fly, Effect::Fade, Effect::Appear]);
    }
}
