//! OpenDocument presentations (.odp): slides as `draw:page`s with title and
//! outline placeholders, text boxes, pictures, shapes, lines and tables, speaker
//! notes, and the master slide's objects on the master page.

use super::model::*;
use crate::odf::{self, DRAW, OFFICE, PRESENTATION, STYLE, SVG, Styles, TABLE, TEXT, XLINK, attr, esc, pt};
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::fmt::Write as _;

/// The ODF shape type for each of ours.
fn shape_type(s: Shape) -> &'static str {
    match s {
        Shape::Rectangle => "rectangle",
        Shape::Rounded => "round-rectangle",
        Shape::Ellipse => "ellipse",
        Shape::Triangle => "isosceles-triangle",
        Shape::Diamond => "diamond",
        Shape::Arrow => "right-arrow",
        Shape::Star => "star5",
        Shape::Hexagon => "hexagon",
    }
}

fn shape_from_type(t: &str) -> Shape {
    match t {
        "round-rectangle" | "round-square" => Shape::Rounded,
        "ellipse" | "circle" | "ring" => Shape::Ellipse,
        "isosceles-triangle" | "right-triangle" => Shape::Triangle,
        "diamond" => Shape::Diamond,
        "right-arrow" | "left-arrow" | "up-arrow" | "down-arrow" | "mso-spt13" => Shape::Arrow,
        "star5" | "star4" | "star6" | "star8" | "mso-spt187" => Shape::Star,
        "hexagon" | "octagon" | "pentagon" => Shape::Hexagon,
        _ => Shape::Rectangle,
    }
}

/// Our layouts by the name we give their ODF page layouts.
fn layout_id(l: Layout) -> &'static str {
    match l {
        Layout::Title => "Nixe-Title",
        Layout::Content => "Nixe-Content",
        Layout::TwoColumns => "Nixe-TwoColumns",
        Layout::Picture => "Nixe-Picture",
        Layout::Section => "Nixe-Section",
        Layout::Blank => "Nixe-Blank",
    }
}

// ---------- Writing ----------

#[derive(Default)]
struct Auto {
    named: Vec<(String, String)>,
    xml: String,
}

impl Auto {
    /// A style of `family` with this inner XML, made once.
    fn style(&mut self, family: &str, prefix: &str, inner: String) -> String {
        let key = format!("{family}|{inner}");
        if let Some((_, n)) = self.named.iter().find(|(k, _)| *k == key) {
            return n.clone();
        }
        let name = format!("{prefix}{}", self.named.len() + 1);
        let _ = write!(self.xml, r#"<style:style style:name="{name}" style:family="{family}">{inner}</style:style>"#);
        self.named.push((key, name.clone()));
        name
    }
}

/// The xfrm attributes for a box turned about its centre.
/// The element id builds point at, then the frame (turned about its centre).
fn place(o: &Object, id: &str) -> String {
    format!(r#"xml:id="{id}" draw:id="{id}" {}"#, frame(o))
}

fn frame(o: &Object) -> String {
    if o.rotation == 0.0 {
        return format!(r#"svg:x="{}" svg:y="{}" svg:width="{}" svg:height="{}""#, pt(o.x), pt(o.y), pt(o.w), pt(o.h));
    }
    let t = o.rotation * PI / 180.0;
    let (cx, cy) = o.center();
    // Where the top-left corner ends up, turned clockwise on screen.
    let (dx, dy) = (-o.w / 2.0, -o.h / 2.0);
    let (tx, ty) = (cx + dx * t.cos() - dy * t.sin(), cy + dx * t.sin() + dy * t.cos());
    format!(
        r#"svg:width="{}" svg:height="{}" draw:transform="rotate({}) translate({} {})""#,
        pt(o.w),
        pt(o.h),
        -t,
        pt(tx),
        pt(ty)
    )
}

struct Writer<'a> {
    /// Element ids start with this: the slide's number, or the master's.
    prefix: String,
    /// Embedded chart documents, in order: "Object 1" is the first.
    charts: Vec<String>,
    auto: Auto,
    media: &'a BTreeMap<String, Vec<u8>>,
    pictures: Vec<String>,
    lost: Vec<String>,
}

impl Writer<'_> {
    fn lose(&mut self, what: &str) {
        if !self.lost.iter().any(|l| l == what) {
            self.lost.push(what.into());
        }
    }

    /// Text properties for the slide's look at `size`.
    fn text_props(s: &Slide, size: f64, bold: bool) -> String {
        format!(
            r#"<style:text-properties fo:font-family="'{}'" fo:font-size="{}" fo:color="{}"{}{}/>"#,
            esc(&s.font),
            pt(size),
            esc(&s.color),
            if bold || s.bold { r#" fo:font-weight="bold""# } else { "" },
            if s.italic { r#" fo:font-style="italic""# } else { "" }
        )
    }

    /// Paragraphs of slide text; `{n}` is the slide number on the master.
    fn paragraphs(&mut self, s: &Slide, text: &str, size: f64, bold: bool, centred: bool) -> String {
        let props = format!(
            r#"<style:paragraph-properties fo:text-align="{}"/>{}"#,
            if centred { "center" } else { "start" },
            Writer::text_props(s, size, bold)
        );
        let p = self.auto.style("paragraph", "P", props);
        let mut out = String::new();
        for line in text.split('\n') {
            let body = line.split("{n}").map(odf::text_xml).collect::<Vec<_>>().join("<text:page-number>1</text:page-number>");
            let _ = write!(out, r#"<text:p text:style-name="{p}">{body}</text:p>"#);
        }
        out
    }

    fn graphic(&mut self, family: &str, o: Option<&Object>, fill: Option<&str>, middle: bool) -> String {
        let mut g = String::from("<style:graphic-properties");
        match fill {
            Some(c) => {
                let _ = write!(g, r#" draw:fill="solid" draw:fill-color="{}""#, esc(c));
            }
            None => g.push_str(r#" draw:fill="none""#),
        }
        match o.and_then(|o| o.outline.as_ref()) {
            Some(l) => {
                let _ = write!(g, r#" draw:stroke="solid" svg:stroke-color="{}" svg:stroke-width="{}""#, esc(&l.color), pt(l.width));
            }
            None => g.push_str(r#" draw:stroke="none""#),
        }
        if o.is_some_and(|o| o.shadow) {
            g.push_str(r##" draw:shadow="visible" draw:shadow-offset-x="3pt" draw:shadow-offset-y="3pt" draw:shadow-color="#000000" draw:shadow-opacity="35%""##);
        }
        let _ = write!(
            g,
            r#" draw:textarea-vertical-align="{}" fo:padding-top="0pt" fo:padding-bottom="0pt" fo:padding-left="0pt" fo:padding-right="0pt" draw:auto-grow-height="false"/>"#,
            if middle { "middle" } else { "top" }
        );
        self.auto.style(family, if family == "presentation" { "pr" } else { "gr" }, g)
    }

    /// A title (bold, centred top to bottom) or body placeholder.
    fn placeholder(&mut self, s: &Slide, class: &str, text: &str, f: Frame, size: f64) -> String {
        let title = class == "title";
        let top = !title && s.layout.body_at_top();
        let style = self.graphic("presentation", None, None, !top);
        let body = self.paragraphs(s, text, size, title, f.5);
        format!(
            r#"<draw:frame presentation:style-name="{style}" presentation:class="{class}"{} svg:x="{}" svg:y="{}" svg:width="{}" svg:height="{}"><draw:text-box>{body}</draw:text-box></draw:frame>"#,
            if text.is_empty() { r#" presentation:placeholder="true""# } else { "" },
            pt(f.0),
            pt(f.1),
            pt(f.2),
            pt(f.3)
        )
    }

    fn object(&mut self, s: &Slide, o: &Object, out: &mut String) {
        let id = format!("{}o{}", self.prefix, o.id);
        let place = |o: &Object| place(o, &id);
        match &o.kind {
            Kind::Text { text } => {
                let style = self.graphic("graphic", Some(o), None, false);
                let body = self.paragraphs(s, text, s.body_size.unwrap_or(24.0), false, false);
                let _ = write!(out, r#"<draw:frame draw:style-name="{style}" {}><draw:text-box>{body}</draw:text-box></draw:frame>"#, place(o));
            }
            Kind::Image { image } => {
                if !self.media.contains_key(image) {
                    return;
                }
                if !self.pictures.contains(image) {
                    self.pictures.push(image.clone());
                }
                let style = self.graphic("graphic", Some(o), None, false);
                let _ = write!(
                    out,
                    r#"<draw:frame draw:style-name="{style}" {}><draw:image xlink:href="Pictures/{}" xlink:type="simple" xlink:show="embed" xlink:actuate="onLoad"/></draw:frame>"#,
                    place(o),
                    esc(image)
                );
            }
            Kind::Shape { shape, fill } => {
                let style = self.graphic("graphic", Some(o), Some(fill), true);
                let _ = write!(
                    out,
                    r#"<draw:custom-shape draw:style-name="{style}" {}><draw:enhanced-geometry svg:viewBox="0 0 21600 21600" draw:type="{}"/></draw:custom-shape>"#,
                    place(o),
                    shape_type(*shape)
                );
            }
            Kind::Line { arrow_start, arrow_end } => {
                let (color, width) = o.line_look();
                let mut g = format!(r#"<style:graphic-properties draw:stroke="solid" svg:stroke-color="{}" svg:stroke-width="{}""#, esc(&color), pt(width));
                if *arrow_start {
                    let _ = write!(g, r#" draw:marker-start="Arrow" draw:marker-start-width="{}""#, pt(width * 3.0));
                }
                if *arrow_end {
                    let _ = write!(g, r#" draw:marker-end="Arrow" draw:marker-end-width="{}""#, pt(width * 3.0));
                }
                g.push_str("/>");
                let style = self.auto.style("graphic", "gr", g);
                let (cx, cy) = o.center();
                let t = o.rotation * PI / 180.0;
                let (hx, hy) = (o.w / 2.0 * t.cos(), o.w / 2.0 * t.sin());
                let _ = write!(
                    out,
                    r#"<draw:line xml:id="{id}" draw:id="{id}" draw:style-name="{style}" svg:x1="{}" svg:y1="{}" svg:x2="{}" svg:y2="{}"/>"#,
                    pt(cx - hx),
                    pt(cy - hy),
                    pt(cx + hx),
                    pt(cy + hy)
                );
            }
            Kind::Table { rows, header } => {
                let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
                let size = s.body_size.unwrap_or(18.0).min(18.0);
                let cell = self.auto.style(
                    "table-cell",
                    "ce",
                    r#"<style:graphic-properties draw:fill="none"/><style:paragraph-properties fo:border="0.75pt solid #808080" fo:padding="4pt"/>"#.into(),
                );
                let head = self.auto.style(
                    "table-cell",
                    "ce",
                    r##"<style:graphic-properties draw:fill="solid" draw:fill-color="#e8ebf0"/><style:paragraph-properties fo:border="0.75pt solid #808080" fo:padding="4pt"/>"##.into(),
                );
                let _ = write!(
                    out,
                    r#"<draw:frame {}><table:table{}><table:table-column table:number-columns-repeated="{cols}"/>"#,
                    place(o),
                    if *header { r#" table:use-first-row-styles="true""# } else { "" }
                );
                for (ri, row) in rows.iter().enumerate() {
                    out.push_str("<table:table-row>");
                    let is_head = *header && ri == 0;
                    for c in 0..cols {
                        let text = row.get(c).map(String::as_str).unwrap_or("");
                        let body = self.paragraphs(s, text, size, is_head, false);
                        let _ = write!(out, r#"<table:table-cell table:style-name="{}">{body}</table:table-cell>"#, if is_head { &head } else { &cell });
                    }
                    out.push_str("</table:table-row>");
                }
                out.push_str("</table:table></draw:frame>");
            }
            Kind::Chart { chart, names, labels, series } => {
                self.charts.push(super::odp_more::chart_document(*chart, names, labels, series, (o.w, o.h)));
                let n = self.charts.len();
                let style = self.graphic("graphic", Some(o), None, false);
                let _ = write!(
                    out,
                    r#"<draw:frame draw:style-name="{style}" {}><draw:object xlink:href="./Object {n}" xlink:type="simple" xlink:show="embed" xlink:actuate="onLoad"/></draw:frame>"#,
                    place(o)
                );
            }
            Kind::Media { .. } => self.lose("video and audio"),
        }
    }
}

pub fn write(deck: &Deck, media: &BTreeMap<String, Vec<u8>>) -> Result<(Vec<u8>, Vec<String>)> {
    let mut w = Writer { prefix: String::new(), charts: Vec::new(), auto: Auto::default(), media, pictures: Vec::new(), lost: Vec::new() };
    let mut pages = String::new();
    let mut fonts: Vec<String> = vec![DEFAULT_FONT.into()];
    for (i, s) in deck.slides.iter().enumerate() {
        if !fonts.contains(&s.font) {
            fonts.push(s.font.clone());
        }
        if s.transition.is_some_and(|t| t.kind == TransitionKind::MagicMove) {
            w.lose("Magic Move (it plays as a fade)");
        }
        w.prefix = format!("s{}", i + 1);
        let page_id = format!("p{}", i + 1);
        if s.hide_master && !deck.master.is_empty() {
            w.lose("hiding the master slide's objects on a slide");
        }
        let dp = w.auto.style(
            "drawing-page",
            "dp",
            format!(
                r#"<style:drawing-page-properties presentation:background-visible="true" draw:background-size="full" draw:fill="solid" draw:fill-color="{}"{}/>"#,
                esc(&s.background),
                s.transition.as_ref().map(super::odp_more::transition_props).unwrap_or_default()
            ),
        );
        let _ = write!(
            pages,
            r#"<draw:page draw:name="page{}" xml:id="{page_id}" draw:id="{page_id}" draw:style-name="{dp}" draw:master-page-name="Default" presentation:presentation-page-layout-name="{}">"#,
            i + 1,
            layout_id(s.layout)
        );
        if let Some(f) = s.title_frame() {
            pages.push_str(&w.placeholder(s, "title", &s.title, f, s.title_size.unwrap_or(f.4)));
        }
        if let Some(f) = s.body_frame() {
            let class = if matches!(s.layout, Layout::Title | Layout::Section | Layout::Picture) { "subtitle" } else { "outline" };
            pages.push_str(&w.placeholder(s, class, &s.body, f, s.body_size.unwrap_or(f.4)));
        }
        if let Some(f) = s.body2_frame() {
            pages.push_str(&w.placeholder(s, "outline", &s.body2, f, s.body_size.unwrap_or(f.4)));
        }
        for o in &s.objects {
            w.object(s, o, &mut pages);
        }
        let prefix = w.prefix.clone();
        pages.push_str(&super::odp_more::timing(s, &page_id, |id| format!("{prefix}o{id}")));
        if !s.notes.is_empty() {
            let body = w.paragraphs(s, &s.notes, 12.0, false, false);
            let _ = write!(
                pages,
                r#"<presentation:notes><draw:frame presentation:class="notes" svg:x="60pt" svg:y="400pt" svg:width="480pt" svg:height="320pt"><draw:text-box>{}</draw:text-box></draw:frame></presentation:notes>"#,
                body.replace(&format!(r#"fo:color="{}""#, s.color), r##"fo:color="#000000""##)
            );
        }
        pages.push_str("</draw:page>");
    }
    // The master slide's objects, drawn with the first slide's look.
    let look = deck.slides.first().cloned().unwrap_or_default();
    w.prefix = "m".into();
    let mut master = String::new();
    for o in &deck.master {
        w.object(&look, o, &mut master);
    }
    let mut faces = String::new();
    for f in &fonts {
        let _ = write!(faces, r#"<style:font-face style:name="{0}" svg:font-family="'{0}'"/>"#, esc(f));
    }
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content {ns}><office:font-face-decls>{faces}</office:font-face-decls><office:automatic-styles>{auto}</office:automatic-styles><office:body><office:presentation>{pages}</office:presentation></office:body></office:document-content>"#,
        ns = odf::NAMESPACES,
        auto = w.auto.xml,
    );
    let mut layouts = String::new();
    for l in Layout::ALL {
        let _ = write!(layouts, r#"<style:presentation-page-layout style:name="{}">"#, layout_id(l));
        for (class, f) in [("title", title_frame(l)), (if l.body_at_top() { "outline" } else { "subtitle" }, body_frame(l)), ("outline", body2_frame(l))] {
            if let Some(f) = f {
                let _ = write!(
                    layouts,
                    r#"<presentation:placeholder presentation:object="{class}" svg:x="{}" svg:y="{}" svg:width="{}" svg:height="{}"/>"#,
                    pt(f.0),
                    pt(f.1),
                    pt(f.2),
                    pt(f.3)
                );
            }
        }
        layouts.push_str("</style:presentation-page-layout>");
    }
    // The master's objects need their styles here, in styles.xml's own automatic styles.
    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-styles {ns}><office:font-face-decls>{faces}</office:font-face-decls><office:styles><draw:marker draw:name="Arrow" svg:viewBox="0 0 20 30" svg:d="M10 0l-10 30h20z"/><style:default-style style:family="graphic"><style:graphic-properties draw:stroke="none" draw:fill="none"/><style:text-properties style:font-name="{font}" fo:font-size="24pt"/></style:default-style>{layouts}</office:styles><office:automatic-styles><style:page-layout style:name="PM1"><style:page-layout-properties fo:margin-top="0pt" fo:margin-bottom="0pt" fo:margin-left="0pt" fo:margin-right="0pt" fo:page-width="{w}" fo:page-height="{h}" style:print-orientation="landscape"/></style:page-layout><style:style style:name="Mdp1" style:family="drawing-page"><style:drawing-page-properties draw:background-size="full" draw:fill="none"/></style:style>{auto}</office:automatic-styles><office:master-styles><style:master-page style:name="Default" style:page-layout-name="PM1" draw:style-name="Mdp1">{master}</style:master-page></office:master-styles></office:document-styles>"#,
        ns = odf::NAMESPACES,
        font = DEFAULT_FONT,
        w = pt(W),
        h = pt(H),
        auto = w.auto.xml,
    );
    let mut extras: Vec<odf::Extra> = w
        .pictures
        .iter()
        .filter_map(|name| media.get(name).map(|bytes| odf::Extra { path: format!("Pictures/{name}"), bytes, media_type: "image/png" }))
        .collect();
    for (k, chart) in w.charts.iter().enumerate() {
        let dir = format!("Object {}/", k + 1);
        extras.push(odf::Extra { path: format!("{dir}content.xml"), bytes: chart.as_bytes(), media_type: "text/xml" });
        extras.push(odf::Extra { path: format!("{dir}styles.xml"), bytes: super::odp_more::CHART_STYLES.as_bytes(), media_type: "text/xml" });
        extras.push(odf::Extra { path: dir, bytes: &[], media_type: "application/vnd.oasis.opendocument.chart" });
    }
    let bytes = odf::package(odf::ODP, &content, &styles, &extras)?;
    Ok((bytes, w.lost))
}

// ---------- Reading ----------

struct Reader<'p, 'b> {
    styles: Styles,
    pkg: &'p mut odf::Package<'b>,
    cache: BTreeMap<String, Option<Vec<u8>>>,
    media: BTreeMap<String, Vec<u8>>,
    /// Slide points per file point, across and down.
    scale: (f64, f64),
}

/// Text in a frame, with the slide number field as `{n}`.
fn frame_text(n: &roxmltree::Node) -> String {
    let mut lines = Vec::new();
    for p in n.descendants().filter(|c| c.has_tag_name((TEXT, "p")) || c.has_tag_name((TEXT, "h"))) {
        let mut s = String::new();
        for c in p.descendants() {
            if c.has_tag_name((TEXT, "page-number")) {
                s.push_str("{n}");
            } else if c.is_text() && !c.ancestors().any(|a| a.has_tag_name((TEXT, "page-number"))) {
                s.push_str(c.text().unwrap_or(""));
            } else if c.has_tag_name((TEXT, "tab")) {
                s.push('\t');
            } else if c.has_tag_name((TEXT, "s")) {
                let k: usize = attr(&c, TEXT, "c").and_then(|v| v.parse().ok()).unwrap_or(1);
                s.push_str(&" ".repeat(k.min(100)));
            } else if c.has_tag_name((TEXT, "line-break")) {
                s.push('\n');
            }
        }
        lines.push(s);
    }
    lines.join("\n")
}

impl Reader<'_, '_> {
    fn len(&self, v: Option<&str>, across: bool) -> Option<f64> {
        v.and_then(odf::length).map(|v| v * if across { self.scale.0 } else { self.scale.1 })
    }

    /// x, y, w, h and rotation (degrees clockwise) of a shape.
    fn geometry(&self, n: &roxmltree::Node) -> Option<(f64, f64, f64, f64, f64)> {
        let w = self.len(attr(n, SVG, "width"), true)?;
        let h = self.len(attr(n, SVG, "height"), false)?;
        if let Some(t) = attr(n, DRAW, "transform").map(|t| t.replace(" (", "(")) {
            // rotate(a) translate(x y), as LibreOffice writes it.
            let num = |s: &str| s.trim().trim_end_matches(|c: char| c.is_ascii_alphabetic()).parse::<f64>().ok();
            let rot = t.split("rotate(").nth(1).and_then(|r| r.split(')').next()).and_then(num).unwrap_or(0.0);
            let tr: Vec<&str> =
                t.split("translate(").nth(1).and_then(|r| r.split(')').next()).map(|r| r.split([' ', ',']).filter(|x| !x.is_empty()).collect()).unwrap_or_default();
            let tx = self.len(tr.first().copied(), true).unwrap_or(0.0);
            let ty = self.len(tr.get(1).copied(), false).unwrap_or(0.0);
            let t = -rot;
            let (cx, cy) = (tx + w / 2.0 * t.cos() - h / 2.0 * t.sin(), ty + w / 2.0 * t.sin() + h / 2.0 * t.cos());
            let deg = (t * 180.0 / PI).rem_euclid(360.0);
            return Some((cx - w / 2.0, cy - h / 2.0, w, h, if deg.abs() < 0.01 { 0.0 } else { deg }));
        }
        let x = self.len(attr(n, SVG, "x"), true).unwrap_or(0.0);
        let y = self.len(attr(n, SVG, "y"), false).unwrap_or(0.0);
        Some((x, y, w, h, 0.0))
    }

    fn style_of<'n>(n: &roxmltree::Node<'n, '_>) -> (&'static str, Option<&'n str>) {
        match attr(n, PRESENTATION, "style-name") {
            Some(s) => ("presentation", Some(s)),
            None => ("graphic", attr(n, DRAW, "style-name")),
        }
    }

    fn deco(&self, n: &roxmltree::Node, o: &mut Object) {
        let (family, Some(style)) = Reader::style_of(n) else { return };
        let g = |k: &str| self.styles.get(family, style, &format!("graphic/{k}"));
        if g("stroke").is_some_and(|s| s != "none")
            && let Some(c) = g("stroke-color").and_then(odf::color)
        {
            let width = g("stroke-width").and_then(odf::length).unwrap_or(1.0).max(0.5);
            o.outline = Some(Outline { color: c, width });
        }
        o.shadow = g("shadow") == Some("visible");
    }

    fn fill(&self, n: &roxmltree::Node) -> Option<String> {
        let (family, Some(style)) = Reader::style_of(n) else { return None };
        if self.styles.get(family, style, "graphic/fill") == Some("none") {
            return None;
        }
        self.styles.get(family, style, "graphic/fill-color").and_then(odf::color)
    }

    /// One shape on a slide or the master, as an object.
    fn object(&mut self, n: &roxmltree::Node, id: u32) -> Option<Object> {
        let name = n.tag_name().name();
        if n.tag_name().namespace() != Some(DRAW) {
            return None;
        }
        if name == "line" {
            let g = |k: &str, across: bool| self.len(attr(n, SVG, k), across);
            let (x1, y1, x2, y2) = (g("x1", true)?, g("y1", false)?, g("x2", true)?, g("y2", false)?);
            let (cx, cy) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
            let len = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt().max(24.0);
            let deg = (y2 - y1).atan2(x2 - x1) * 180.0 / PI;
            let (family, style) = Reader::style_of(n);
            let style = style.unwrap_or("");
            let g = |k: &str| self.styles.get(family, style, &format!("graphic/{k}"));
            let kind = Kind::Line { arrow_start: g("marker-start").is_some_and(|m| !m.is_empty()), arrow_end: g("marker-end").is_some_and(|m| !m.is_empty()) };
            let mut o = Object::new(id, kind, (cx - len / 2.0, cy - LINE_BOX / 2.0, len, LINE_BOX));
            o.rotation = if deg.abs() < 0.01 { 0.0 } else { deg.rem_euclid(360.0) };
            o.outline = Some(Outline {
                color: g("stroke-color").and_then(odf::color).unwrap_or_else(|| "#202329".into()),
                width: g("stroke-width").and_then(odf::length).unwrap_or(LINE_WIDTH).max(0.5),
            });
            return Some(o);
        }
        let (x, y, w, h, rot) = self.geometry(n)?;
        let kind = match name {
            "frame" => {
                // A table's frame also holds a picture of it, for software without tables.
                if let Some(t) = n.children().find(|c| c.has_tag_name((TABLE, "table"))) {
                    let mut rows: Vec<Vec<String>> = Vec::new();
                    for row in t.descendants().filter(|c| c.has_tag_name((TABLE, "table-row"))).take(50) {
                        let cells = row
                            .children()
                            .filter(|c| c.has_tag_name((TABLE, "table-cell")) || c.has_tag_name((TABLE, "covered-table-cell")))
                            .take(20)
                            .map(|c| frame_text(&c))
                            .collect();
                        rows.push(cells);
                    }
                    let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
                    for r in &mut rows {
                        r.resize(cols, String::new());
                    }
                    if rows.is_empty() {
                        return None;
                    }
                    // A heading row says so, or is shaded unlike the next row.
                    let fills: Vec<Option<&str>> = t
                        .descendants()
                        .filter(|c| c.has_tag_name((TABLE, "table-row")))
                        .take(2)
                        .map(|row| {
                            let cell = row.children().find(|c| c.has_tag_name((TABLE, "table-cell")));
                            let style = cell.and_then(|c| attr(&c, TABLE, "style-name")).or_else(|| attr(&row, TABLE, "default-cell-style-name"))?;
                            (self.styles.get("table-cell", style, "graphic/fill") == Some("solid"))
                                .then(|| self.styles.get("table-cell", style, "graphic/fill-color"))
                                .flatten()
                        })
                        .collect();
                    let shaded = fills.len() == 2 && fills[0].is_some() && fills[0] != fills[1];
                    Kind::Table { rows, header: attr(&t, TABLE, "use-first-row-styles") == Some("true") || shaded }
                } else if let Some(obj) = n.children().find(|c| c.has_tag_name((DRAW, "object"))) {
                    // An embedded chart: its document sits in a folder of the package.
                    let href = attr(&obj, XLINK, "href")?.trim_start_matches("./").trim_end_matches('/').to_string();
                    let xml = self.pkg.text(&format!("{href}/content.xml"))?;
                    super::odp_more::read_chart(&xml)?
                } else if let Some(img) = n.children().find(|c| c.has_tag_name((DRAW, "image"))) {
                    let href = attr(&img, XLINK, "href")?;
                    let png = odf::picture(self.pkg, href, &mut self.cache)?;
                    let file = format!("picture-{}.png", self.media.len() + 1);
                    self.media.insert(file.clone(), png);
                    Kind::Image { image: file }
                } else {
                    let text = frame_text(&n.children().find(|c| c.has_tag_name((DRAW, "text-box")))?);
                    if text.trim().is_empty() {
                        return None;
                    }
                    Kind::Text { text }
                }
            }
            "custom-shape" | "rect" | "ellipse" | "circle" | "polygon" | "regular-polygon" => {
                let text = frame_text(n);
                if !text.trim().is_empty() {
                    Kind::Text { text }
                } else {
                    let shape = match name {
                        "rect" if attr(n, DRAW, "corner-radius").is_some() => Shape::Rounded,
                        "rect" => Shape::Rectangle,
                        "ellipse" | "circle" => Shape::Ellipse,
                        "polygon" | "regular-polygon" => Shape::Hexagon,
                        _ => n
                            .children()
                            .find(|c| c.has_tag_name((DRAW, "enhanced-geometry")))
                            .and_then(|g| attr(&g, DRAW, "type"))
                            .map(shape_from_type)
                            .unwrap_or(Shape::Rectangle),
                    };
                    let fill = self.fill(n).unwrap_or_else(|| "#5273cf".into());
                    Kind::Shape { shape, fill }
                }
            }
            _ => return None,
        };
        let mut o = Object::new(id, kind, (x, y, w, h));
        o.rotation = rot;
        self.deco(n, &mut o);
        o.clamp();
        Some(o)
    }

    /// The slide's look from the first paragraph and text styles in a placeholder.
    fn look(&self, frame: &roxmltree::Node, s: &mut Slide, title: bool) {
        let Some(p) = frame.descendants().find(|c| c.has_tag_name((TEXT, "p"))) else { return };
        let mut names: Vec<(&str, &str)> = Vec::new();
        if let Some(span) = p.descendants().find(|c| c.has_tag_name((TEXT, "span"))).and_then(|c| attr(&c, TEXT, "style-name")) {
            names.push(("text", span));
        }
        if let Some(ps) = attr(&p, TEXT, "style-name") {
            names.push(("paragraph", ps));
        }
        if let (f, Some(st)) = Reader::style_of(frame) {
            names.push((f, st));
        }
        let get = |k: &str| names.iter().find_map(|(f, n)| self.styles.get(f, n, k));
        if let Some(size) = get("text/font-size").and_then(odf::length) {
            let size = size * self.scale.1;
            if title {
                s.title_size = Some(size);
            } else {
                s.body_size = Some(size);
            }
        }
        if let Some(c) = get("text/color").and_then(odf::color) {
            s.color = c;
        }
        let font = names.iter().find_map(|(f, n)| self.styles.font(f, n));
        if let Some(f) = font.filter(|f| !f.is_empty()) {
            s.font = f;
        }
        if !title {
            s.bold = get("text/font-weight").is_some_and(|v| v == "bold");
        }
        s.italic |= get("text/font-style") == Some("italic");
    }
}

pub fn read(bytes: &[u8]) -> Result<(Deck, BTreeMap<String, Vec<u8>>)> {
    let mut pkg = odf::Package::open(bytes, odf::ODP, "presentation")?;
    let content = pkg.content()?;
    let styles_text = pkg.text("styles.xml").unwrap_or_default();
    let cdoc = crate::xml::parse(&content).context("the presentation is damaged")?;
    let sdoc = crate::xml::parse(&styles_text).ok();
    let mut parts = Vec::new();
    if let Some(s) = &sdoc {
        parts.push(s);
    }
    parts.push(&cdoc);
    let styles = Styles::read(&parts);
    // The page size, to scale everything onto our 960 × 540.
    let layout = sdoc
        .as_ref()
        .and_then(|d| d.descendants().find(|n| n.has_tag_name((STYLE, "master-page"))))
        .and_then(|m| attr(&m, STYLE, "page-layout-name").map(String::from));
    let size = |k: &str| layout.as_deref().and_then(|l| styles.get("page-layout", l, &format!("layout/{k}"))).and_then(odf::length);
    let (pw, ph) = (size("page-width").unwrap_or(W), size("page-height").unwrap_or(H));
    let mut r = Reader { styles, pkg: &mut pkg, cache: BTreeMap::new(), media: BTreeMap::new(), scale: (W / pw.max(1.0), H / ph.max(1.0)) };

    let body = cdoc.descendants().find(|n| n.has_tag_name((OFFICE, "presentation"))).context("it isn't a presentation")?;
    let mut slides = Vec::new();
    for page in body.children().filter(|n| n.has_tag_name((DRAW, "page"))).take(MAX_SLIDES) {
        let mut s = Slide { font: DEFAULT_FONT.into(), ..Default::default() };
        // Element ids (which builds point at) to our objects.
        let mut ids: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        let element_id = |n: &roxmltree::Node| {
            n.attribute(("http://www.w3.org/XML/1998/namespace", "id")).or_else(|| attr(n, DRAW, "id")).map(String::from)
        };
        if let Some(dp) = attr(&page, DRAW, "style-name")
            && r.styles.get("drawing-page", dp, "page/fill") != Some("none")
            && let Some(c) = r.styles.get("drawing-page", dp, "page/fill-color").and_then(odf::color)
        {
            s.background = c;
        }
        let mut title: Option<(String, f64, bool)> = None;
        // Where the title and first body sit, to recognise our layouts after another app renames them.
        let mut frames: [Option<(f64, f64, f64, f64)>; 2] = [None, None];
        let mut subtitle: Option<String> = None;
        let mut outlines: Vec<(String, f64, f64, f64)> = Vec::new();
        for n in page.children().filter(|c| c.is_element()) {
            match attr(&n, PRESENTATION, "class") {
                Some("title") => {
                    let (x, y, w, h, _) = r.geometry(&n).unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0));
                    frames[0] = Some((x, y, w, h));
                    let centred = n.descendants().any(|p| p.has_tag_name((TEXT, "p")))
                        && r.styles.get("presentation", attr(&n, PRESENTATION, "style-name").unwrap_or(""), "graphic/textarea-vertical-align") == Some("middle");
                    r.look(&n, &mut s, true);
                    title = Some((frame_text(&n), y + h / 2.0, centred));
                }
                Some("subtitle") => {
                    if let Some((x, y, w, h, _)) = r.geometry(&n) {
                        frames[1] = Some((x, y, w, h));
                    }
                    r.look(&n, &mut s, false);
                    subtitle = Some(frame_text(&n));
                }
                Some("outline") => {
                    let (x, y, w, h, _) = r.geometry(&n).unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0));
                    if outlines.is_empty() {
                        frames[1] = Some((x, y, w, h));
                    }
                    r.look(&n, &mut s, false);
                    outlines.push((frame_text(&n), x, y, w));
                }
                Some("notes" | "page-number" | "footer" | "date-time" | "header" | "handout") => {}
                Some(_) => {}
                None if n.has_tag_name((PRESENTATION, "notes")) => {
                    // The notes page's text (its slide picture has none).
                    s.notes = n
                        .children()
                        .filter(|f| f.has_tag_name((DRAW, "frame")) && f.children().any(|c| c.has_tag_name((DRAW, "text-box"))))
                        .map(|f| frame_text(&f))
                        .collect::<Vec<_>>()
                        .join("\n");
                }
                None if n.has_tag_name((DRAW, "g")) => {
                    for c in n.children().filter(|c| c.is_element()) {
                        if s.objects.len() < MAX_OBJECTS
                            && let Some(o) = r.object(&c, s.next_id())
                        {
                            if let Some(e) = element_id(&c).or_else(|| element_id(&n)) {
                                ids.insert(e, o.id);
                            }
                            s.objects.push(o);
                        }
                    }
                }
                None => {
                    if s.objects.len() < MAX_OBJECTS
                        && let Some(o) = r.object(&n, s.next_id())
                    {
                        if let Some(e) = element_id(&n) {
                            ids.insert(e, o.id);
                        }
                        s.objects.push(o);
                    }
                }
            }
        }
        // The transition, and the builds by click.
        let dp = attr(&page, DRAW, "style-name").unwrap_or("").to_string();
        s.transition = super::odp_more::read_transition(&page, |k| r.styles.get("drawing-page", &dp, k).map(String::from));
        for (target, click, effect, out) in super::odp_more::read_builds(&page) {
            if let Some(id) = ids.get(&target)
                && let Some(o) = s.objects.iter_mut().find(|o| o.id == *id)
            {
                let b = Some(Build { order: click, effect });
                if out {
                    o.build_out = b;
                } else {
                    o.build = b;
                }
            }
        }
        // Our own layouts come back by name; others by what's on the slide.
        let named = attr(&page, PRESENTATION, "presentation-page-layout-name")
            .and_then(|n| Layout::ALL.into_iter().find(|l| layout_id(*l) == n))
            .or_else(|| {
                let near = |a: Option<(f64, f64, f64, f64)>, b: Option<Frame>| match (a, b) {
                    (Some(a), Some(b)) => [(a.0, b.0), (a.1, b.1), (a.2, b.2), (a.3, b.3)].iter().all(|(u, v)| (u - v).abs() < 2.0),
                    (None, None) => true,
                    _ => false,
                };
                Layout::ALL.into_iter().find(|l| *l != Layout::Blank && near(frames[0], title_frame(*l)) && near(frames[1], body_frame(*l)))
            });
        let side_by_side = outlines.len() == 2 && (outlines[0].2 - outlines[1].2).abs() < 40.0 && (outlines[0].1 - outlines[1].1).abs() > 200.0;
        let has_picture = s.objects.iter().any(|o| matches!(o.kind, Kind::Image { .. }));
        let captioned = title.as_ref().is_some_and(|t| t.1 > H * 0.7) && has_picture;
        s.layout = named.unwrap_or(match (&title, subtitle.is_some(), outlines.len()) {
            _ if side_by_side => Layout::TwoColumns,
            _ if captioned => Layout::Picture,
            (Some(_), true, _) => Layout::Title,
            (Some(_), false, n) if n > 0 => Layout::Content,
            (Some(t), false, _) if t.2 && s.objects.is_empty() => Layout::Section,
            (Some(_), false, _) => Layout::Content,
            (None, _, n) if n > 0 || subtitle.is_some() => Layout::Content,
            (None, _, _) => Layout::Blank,
        });
        if side_by_side && outlines[1].1 < outlines[0].1 {
            outlines.swap(0, 1);
        }
        s.title = title.map(|t| t.0).unwrap_or_default();
        let mut bodies = outlines.into_iter().map(|o| o.0);
        s.body = subtitle.or_else(|| bodies.next()).unwrap_or_default();
        if s.layout == Layout::TwoColumns {
            s.body2 = bodies.next().unwrap_or_default();
        } else {
            for more in bodies {
                s.body = if s.body.is_empty() { more } else { format!("{}\n{more}", s.body) };
            }
        }
        // Placeholders where the slide has them, if not where the layout would.
        s.frames.title = moved(frames[0], title_frame(s.layout));
        if s.layout != Layout::TwoColumns {
            s.frames.body = moved(frames[1], body_frame(s.layout));
        }
        // A layout's own sizes aren't overrides.
        if s.title_size.is_some() && s.title_size == title_frame(s.layout).map(|f| f.4) {
            s.title_size = None;
        }
        if s.body_size.is_some() && s.body_size == body_frame(s.layout).map(|f| f.4) {
            s.body_size = None;
        }
        slides.push(s);
    }
    if slides.is_empty() {
        slides.push(Slide::default());
    }
    // The master page's own objects (not its placeholders) are the master slide.
    let mut master = Vec::new();
    if let Some(m) = sdoc.as_ref().and_then(|d| d.descendants().find(|n| n.has_tag_name((STYLE, "master-page")))) {
        for (k, n) in m.children().filter(|c| c.is_element() && attr(c, PRESENTATION, "class").is_none()).enumerate() {
            if master.len() < MAX_OBJECTS
                && let Some(o) = r.object(&n, k as u32 + 1)
            {
                master.push(o);
            }
        }
    }
    let media = std::mem::take(&mut r.media);
    Ok((Deck { slides, master, layouts: Default::default() }, media))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_opendocument() {
        let png = base64("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==").unwrap();
        let mut media = BTreeMap::new();
        media.insert("p.png".to_string(), png.clone());
        let mut deck = super::super::templates::build("trip-plan").unwrap();
        deck.slides.push(Slide { layout: Layout::TwoColumns, title: "Pros".into(), body: "a\nb".into(), body2: "c".into(), ..Default::default() });
        let mut pic = Slide { layout: Layout::Picture, title: "Lake".into(), body: "Dawn".into(), ..Default::default() };
        let (x, y, w, h) = picture_frame(Layout::Picture).unwrap();
        pic.objects.push(Object::new(1, Kind::Image { image: "p.png".into() }, (x, y, w, h)));
        deck.slides.push(pic);
        let s = &mut deck.slides[1];
        s.notes = "Say hello".into();
        s.objects.push(Object::new(1, Kind::Shape { shape: Shape::Star, fill: "#e47b89".into() }, (100.0, 100.0, 200.0, 100.0)));
        let mut turned = Object::new(2, Kind::Text { text: "Turned & {n}".into() }, (500.0, 300.0, 300.0, 80.0));
        turned.rotation = 30.0;
        turned.outline = Some(Outline { color: "#112233".into(), width: 2.0 });
        s.objects.push(turned);
        let mut line = Object::new(3, Kind::Line { arrow_start: false, arrow_end: true }, (100.0, 400.0, 300.0, LINE_BOX));
        line.rotation = 45.0;
        line.outline = Some(Outline { color: "#336699".into(), width: 4.0 });
        s.objects.push(line);
        s.objects.push(Object::new(4, Kind::Table { rows: vec![vec!["A".into(), "B".into()], vec!["1".into(), "2".into()]], header: true }, (60.0, 60.0, 300.0, 120.0)));
        deck.master.push(Object::new(1, Kind::Text { text: "{n}".into() }, (880.0, 500.0, 60.0, 30.0)));
        // A title moved from its layout's place.
        deck.slides[2].frames.title = Some((120.0, 30.0, 600.0, 90.0));
        let (bytes, lost) = write(&deck, &media).unwrap();
        assert!(lost.is_empty(), "{lost:?}");
        let (back, back_media) = read(&bytes).unwrap();
        assert_eq!(back.slides.len(), deck.slides.len());
        assert_eq!(back_media.values().next(), Some(&png));
        for (a, b) in deck.slides.iter().zip(&back.slides) {
            assert_eq!((a.layout, &a.title, &a.body, &a.body2, &a.notes), (b.layout, &b.title, &b.body, &b.body2, &b.notes));
            assert_eq!((&a.background, &a.color, &a.font, a.bold, a.italic), (&b.background, &b.color, &b.font, b.bold, b.italic));
            assert_eq!((a.title_size, a.body_size), (b.title_size, b.body_size));
            assert_eq!(a.frames, b.frames, "{}", a.title);
            assert_eq!(a.objects.len(), b.objects.len());
            for (oa, ob) in a.objects.iter().zip(&b.objects) {
                if !matches!(oa.kind, Kind::Image { .. }) {
                    assert_eq!(oa.kind, ob.kind);
                }
                for (u, v) in [(oa.x, ob.x), (oa.y, ob.y), (oa.w, ob.w), (oa.h, ob.h), (oa.rotation, ob.rotation)] {
                    assert!((u - v).abs() < 0.05, "{oa:?} vs {ob:?}");
                }
                assert_eq!(oa.outline, ob.outline);
            }
        }
        assert_eq!(back.master.len(), 1);
        assert!(matches!(&back.master[0].kind, Kind::Text { text } if text == "{n}"));
    }

    #[test]
    fn reads_a_file_collabora_saved() {
        // Collabora renames the layouts, writes "rotate (…)" with spaces, adds a
        // picture to each table, and drops the notes' placeholder class.
        let (deck, _) = read(include_bytes!("../../testdata/collabora/sample.odp")).unwrap();
        let layouts: Vec<Layout> = deck.slides.iter().map(|s| s.layout).collect();
        assert_eq!(layouts, [Layout::Title, Layout::Content, Layout::Content, Layout::Section]);
        assert!(deck.slides.iter().all(|s| !s.notes.is_empty()));
        let s = &deck.slides[1];
        assert!(s.objects.iter().any(|o| matches!(&o.kind, Kind::Table { header: true, rows } if rows[0][0] == "Heading 1")));
        let arrow = s.objects.iter().find(|o| matches!(o.kind, Kind::Shape { shape: Shape::Arrow, .. })).unwrap();
        assert!((arrow.rotation - 15.0).abs() < 0.01 && (arrow.x - 150.0).abs() < 0.1 && (arrow.y - 40.0).abs() < 0.1, "{arrow:?}");
        assert!(matches!(&deck.master[0].kind, Kind::Text { text } if text == "{n}"));
    }

    #[test]
    fn charts_transitions_and_builds() {
        let mut deck = Deck::default();
        for kind in TransitionKind::ALL.into_iter().filter(|k| *k != TransitionKind::MagicMove) {
            deck.slides.push(Slide { title: kind.name().into(), transition: Some(Transition { kind, secs: 1.25 }), ..Default::default() });
        }
        let s = &mut deck.slides[0];
        let mut a = Object::new(1, Kind::Shape { shape: Shape::Rectangle, fill: "#336699".into() }, (100.0, 200.0, 100.0, 100.0));
        a.build = Some(Build { order: 1, effect: Effect::Fade });
        let mut b = Object::new(2, Kind::Shape { shape: Shape::Ellipse, fill: "#993366".into() }, (300.0, 200.0, 100.0, 100.0));
        b.build = Some(Build { order: 2, effect: Effect::Fly });
        let mut c = Object::new(3, Kind::Shape { shape: Shape::Star, fill: "#669933".into() }, (500.0, 200.0, 100.0, 100.0));
        c.build = Some(Build { order: 3, effect: Effect::Appear });
        c.build_out = Some(Build { order: 4, effect: Effect::Fade });
        let chart = Object::new(4, Kind::sample_chart(), (600.0, 300.0, 300.0, 200.0));
        let mut stacked = Object::new(5, Kind::sample_chart(), (100.0, 320.0, 300.0, 200.0));
        if let Kind::Chart { chart, .. } = &mut stacked.kind {
            *chart = ChartKind::StackedArea;
        }
        s.objects = vec![a, b, c, chart, stacked];
        let (bytes, lost) = write(&deck, &BTreeMap::new()).unwrap();
        assert!(lost.is_empty(), "{lost:?}");
        let (back, _) = read(&bytes).unwrap();
        for (x, y) in deck.slides.iter().zip(&back.slides) {
            assert_eq!(x.transition, y.transition, "{}", x.title);
        }
        let (x, y) = (&deck.slides[0], &back.slides[0]);
        assert_eq!(x.steps(), y.steps());
        for (o, p) in x.objects.iter().zip(&y.objects) {
            assert_eq!((&o.kind, o.build, o.build_out), (&p.kind, p.build, p.build_out));
        }
    }

    #[test]
    fn reads_animation_and_charts_collabora_saved() {
        // A deck with every transition, builds and a chart, saved again by Collabora Office.
        let (deck, _) = read(include_bytes!("../../testdata/collabora/animated.odp")).unwrap();
        let kinds: Vec<Option<TransitionKind>> = deck.slides.iter().map(|s| s.transition.map(|t| t.kind)).collect();
        assert_eq!(kinds[1..9].iter().flatten().count(), 8, "{kinds:?}");
        assert_eq!(
            kinds[1..9].to_vec(),
            [
                TransitionKind::Fade,
                TransitionKind::Push,
                TransitionKind::Cover,
                TransitionKind::Uncover,
                TransitionKind::Wipe,
                TransitionKind::Split,
                TransitionKind::Zoom,
                TransitionKind::Dissolve,
            ]
            .map(Some)
        );
        // The builds and chart are on the first slide.
        let s = &deck.slides[0];
        let effects: Vec<(Effect, bool)> = s.steps().iter().map(|st| (st.effect, st.out)).collect();
        assert_eq!(effects, [(Effect::Fade, false), (Effect::Fly, false), (Effect::Appear, false), (Effect::Fade, true)]);
        assert!(s.objects.iter().any(|o| matches!(&o.kind, Kind::Chart { chart: ChartKind::Bar, series, .. } if series[1][3] == 27.0)));
    }
}
