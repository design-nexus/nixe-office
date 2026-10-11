//! OpenDocument text (.odt), as LibreOffice and others write it: paragraphs with
//! automatic styles over named ones, nested lists, tables, pictures, notes,
//! comments, and the page from the master page's layout.

use super::model::*;
use crate::odf::{self, DC, DRAW, OFFICE, STYLE, Styles, SVG, TABLE, TEXT, XLINK, attr, esc, pt};
use anyhow::{Context, Result};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

// ---------- Writing ----------

/// Automatic styles, made once per distinct set of properties.
#[derive(Default)]
struct Auto {
    paragraph: Vec<(String, String)>,
    text: Vec<(String, String)>,
    other: String,
}

impl Auto {
    /// A paragraph style over `parent` with these properties: (paragraph, text) XML attributes.
    fn paragraph(&mut self, parent: &str, extra: &str, para: &str, text: &str) -> String {
        let key = format!("{parent}|{extra}|{para}|{text}");
        if let Some((_, name)) = self.paragraph.iter().find(|(k, _)| *k == key) {
            return name.clone();
        }
        let name = format!("P{}", self.paragraph.len() + 1);
        self.paragraph.push((key, name.clone()));
        let _ = write!(
            self.other,
            r#"<style:style style:name="{name}" style:family="paragraph" style:parent-style-name="{parent}"{extra}>"#
        );
        if !para.is_empty() {
            let _ = write!(self.other, "<style:paragraph-properties{para}/>");
        }
        if !text.is_empty() {
            let _ = write!(self.other, "<style:text-properties{text}/>");
        }
        self.other.push_str("</style:style>");
        name
    }

    fn text(&mut self, props: &str) -> String {
        if let Some((_, name)) = self.text.iter().find(|(k, _)| k == props) {
            return name.clone();
        }
        let name = format!("T{}", self.text.len() + 1);
        self.text.push((props.to_string(), name.clone()));
        let _ = write!(
            self.other,
            r#"<style:style style:name="{name}" style:family="text"><style:text-properties{props}/></style:style>"#
        );
        name
    }
}

fn text_props(r: &TextRun) -> String {
    let mut p = String::new();
    if r.bold {
        p.push_str(r#" fo:font-weight="bold" style:font-weight-asian="bold" style:font-weight-complex="bold""#);
    }
    if r.italic {
        p.push_str(r#" fo:font-style="italic" style:font-style-asian="italic" style:font-style-complex="italic""#);
    }
    if r.underline {
        p.push_str(r#" style:text-underline-style="solid" style:text-underline-width="auto" style:text-underline-color="font-color""#);
    }
    if r.strike {
        p.push_str(r#" style:text-line-through-style="solid""#);
    }
    if r.superscript {
        p.push_str(r#" style:text-position="super 58%""#);
    } else if r.subscript {
        p.push_str(r#" style:text-position="sub 58%""#);
    }
    if let Some(f) = &r.font {
        let _ = write!(p, r#" style:font-name="{0}" fo:font-family="'{0}'""#, esc(f));
    }
    if let Some(s) = r.size {
        let _ = write!(p, r#" fo:font-size="{}""#, pt(s));
    }
    if let Some(c) = &r.color {
        let _ = write!(p, r#" fo:color="{}""#, esc(c));
    }
    if let Some(c) = &r.highlight {
        let _ = write!(p, r#" fo:background-color="{}""#, esc(c));
    }
    p
}

/// The named style each of ours is in LibreOffice.
fn style_name(s: Style) -> &'static str {
    match s {
        Style::Normal => "Standard",
        Style::Title => "Title",
        Style::H1 => "Heading_20_1",
        Style::H2 => "Heading_20_2",
        Style::H3 => "Heading_20_3",
        Style::Quote => "Quotations",
    }
}

struct Writer<'a> {
    doc: &'a Document,
    media: &'a BTreeMap<String, Vec<u8>>,
    auto: Auto,
    pictures: Vec<String>,
    frames: usize,
    notes: (usize, usize),
    /// Where each comment's last run is: (block, run).
    comment_ends: HashMap<u32, (usize, usize)>,
    open_comments: Vec<u32>,
    /// Tracked changes: one region per changed run, as `text:changed-region`s.
    changes: String,
    change_n: usize,
    fonts: Vec<String>,
    lost: Vec<String>,
}

impl Writer<'_> {
    fn font(&mut self, f: &Option<String>) {
        if let Some(f) = f
            && !self.fonts.contains(f)
        {
            self.fonts.push(f.clone());
        }
    }

    fn paragraph(&mut self, bi: usize, p: &Paragraph, extra: &str, out: &mut String) {
        let mut para = String::new();
        match p.align {
            Align::Left => {}
            Align::Center => para.push_str(r#" fo:text-align="center""#),
            Align::Right => para.push_str(r#" fo:text-align="end""#),
            Align::Justify => para.push_str(r#" fo:text-align="justify""#),
        }
        if p.list.is_none() && p.indent > 0 {
            let _ = write!(para, r#" fo:margin-left="{}""#, pt(p.indent as f64 * INDENT_STEP));
        }
        if let Some(ls) = p.line_spacing {
            let _ = write!(para, r#" fo:line-height="{}%""#, (ls * 100.0).round());
        }
        if let Some(v) = p.space_before {
            let _ = write!(para, r#" fo:margin-top="{}""#, pt(v));
        }
        if let Some(v) = p.space_after {
            let _ = write!(para, r#" fo:margin-bottom="{}""#, pt(v));
        }
        let (break_attr, master) = extra.split_once('|').unwrap_or((extra, ""));
        para.push_str(break_attr);
        let parent = style_name(p.style);
        let name = if para.is_empty() && master.is_empty() {
            parent.to_string()
        } else {
            self.auto.paragraph(parent, master, &para, "")
        };
        let (open, close) = match p.style {
            Style::H1 | Style::H2 | Style::H3 => {
                let level = match p.style {
                    Style::H1 => 1,
                    Style::H2 => 2,
                    _ => 3,
                };
                (format!(r#"<text:h text:style-name="{name}" text:outline-level="{level}">"#), "</text:h>")
            }
            _ => (format!(r#"<text:p text:style-name="{name}">"#), "</text:p>"),
        };
        out.push_str(&open);
        for (ri, run) in p.runs.iter().enumerate() {
            match run {
                Run::Text(t) => {
                    if let Some(rev) = t.deleted {
                        let id = self.change("deletion", rev, &format!("<text:p>{}</text:p>", odf::text_xml(&t.text)));
                        let _ = write!(out, r#"<text:change text:change-id="{id}"/>"#);
                        continue;
                    }
                    let inserted = t.inserted.map(|rev| self.change("insertion", rev, ""));
                    if let Some(id) = &inserted {
                        let _ = write!(out, r#"<text:change-start text:change-id="{id}"/>"#);
                    }
                    if let Some(c) = t.comment
                        && !self.open_comments.contains(&c)
                    {
                        self.open_comments.push(c);
                        if let Some(cm) = self.doc.comments.iter().find(|x| x.id == c) {
                            let _ = write!(
                                out,
                                r#"<office:annotation office:name="c{c}"><dc:creator>{}</dc:creator>{}<text:p>{}</text:p></office:annotation>"#,
                                esc(&cm.author),
                                if cm.date.is_empty() { String::new() } else { format!("<dc:date>{}</dc:date>", esc(cm.date.trim_end_matches('Z'))) },
                                odf::text_xml(&cm.text)
                            );
                        }
                    }
                    self.font(&t.font);
                    let props = text_props(t);
                    let body = odf::text_xml(&t.text);
                    let span = if props.is_empty() {
                        body
                    } else {
                        format!(r#"<text:span text:style-name="{}">{body}</text:span>"#, self.auto.text(&props))
                    };
                    match &t.link {
                        Some(href) => {
                            let _ = write!(out, r#"<text:a xlink:type="simple" xlink:href="{}">{span}</text:a>"#, esc(href));
                        }
                        None => out.push_str(&span),
                    }
                    if let Some(id) = &inserted {
                        let _ = write!(out, r#"<text:change-end text:change-id="{id}"/>"#);
                    }
                    if let Some(c) = t.comment
                        && self.comment_ends.get(&c) == Some(&(bi, ri))
                    {
                        let _ = write!(out, r#"<office:annotation-end office:name="c{c}"/>"#);
                    }
                }
                Run::Image(img) => {
                    let Some(bytes) = self.media.get(&img.image) else { continue };
                    let (pw, ph) = odf::png_size(bytes).unwrap_or((4, 3));
                    let (w, h) = (img.width, img.width * ph as f64 / pw as f64);
                    if !self.pictures.contains(&img.image) {
                        self.pictures.push(img.image.clone());
                    }
                    self.frames += 1;
                    let (style, anchor) = match img.wrap {
                        None => ("fr0", "as-char"),
                        Some(Side::Left) => ("frL", "paragraph"),
                        Some(Side::Right) => ("frR", "paragraph"),
                    };
                    let _ = write!(
                        out,
                        r#"<draw:frame draw:style-name="{style}" draw:name="Picture {}" text:anchor-type="{anchor}" svg:width="{}" svg:height="{}" draw:z-index="0"><draw:image xlink:href="Pictures/{}" xlink:type="simple" xlink:show="embed" xlink:actuate="onLoad"/></draw:frame>"#,
                        self.frames,
                        pt(w),
                        pt(h),
                        esc(&img.image)
                    );
                }
                Run::Note(n) => {
                    let (class, citation) = if n.end {
                        self.notes.1 += 1;
                        ("endnote", endnote_label(self.notes.1))
                    } else {
                        self.notes.0 += 1;
                        ("footnote", self.notes.0.to_string())
                    };
                    let id = self.notes.0 + self.notes.1;
                    let style = if n.end { "Endnote" } else { "Footnote" };
                    let _ = write!(
                        out,
                        r#"<text:note text:id="note{id}" text:note-class="{class}"><text:note-citation>{citation}</text:note-citation><text:note-body><text:p text:style-name="{style}">{}</text:p></text:note-body></text:note>"#,
                        odf::text_xml(&n.note)
                    );
                }
                Run::Drawing(_) => {
                    let what = "shapes and charts in the text".to_string();
                    if !self.lost.contains(&what) {
                        self.lost.push(what);
                    }
                }
            }
        }
        out.push_str(close);
    }

    /// A changed region for revision `rev`: its id, for the marks in the text.
    fn change(&mut self, kind: &str, rev: u32, inner: &str) -> String {
        self.change_n += 1;
        let id = format!("ct{}", self.change_n);
        let r = self.doc.revisions.iter().find(|r| r.id == rev);
        let author = r.map(|r| esc(&r.author)).unwrap_or_default();
        let date = r.map(|r| r.date.trim_end_matches('Z')).filter(|d| !d.is_empty()).unwrap_or("1970-01-01T00:00:00");
        let _ = write!(
            self.changes,
            r#"<text:changed-region xml:id="{id}" text:id="{id}"><text:{kind}><office:change-info><dc:creator>{author}</dc:creator><dc:date>{}</dc:date></office:change-info>{inner}</text:{kind}></text:changed-region>"#,
            esc(date)
        );
        id
    }

    fn table(&mut self, t: &Table, n: usize, break_before: bool, out: &mut String) {
        let cols = t.cols();
        let style = if break_before { "TblBreak" } else { "Tbl" };
        let _ = write!(
            out,
            r#"<table:table table:name="Table{n}" table:style-name="{style}"><table:table-column table:style-name="TblCol" table:number-columns-repeated="{cols}"/>"#
        );
        for (ri, row) in t.rows.iter().enumerate() {
            let head = ri == 0 && t.header;
            if head {
                out.push_str("<table:table-header-rows>");
            }
            out.push_str("<table:table-row>");
            for c in 0..cols {
                let text = row.get(c).map(String::as_str).unwrap_or("");
                let ps = if head { "Table_20_Heading" } else { "Table_20_Contents" };
                let cell = if head { "TblHead" } else { "TblCell" };
                let _ = write!(out, r#"<table:table-cell table:style-name="{cell}" office:value-type="string">"#);
                for line in text.split('\n') {
                    let _ = write!(out, r#"<text:p text:style-name="{ps}">{}</text:p>"#, odf::text_xml(line));
                }
                out.push_str("</table:table-cell>");
            }
            out.push_str("</table:table-row>");
            if head {
                out.push_str("</table:table-header-rows>");
            }
        }
        out.push_str("</table:table>");
    }

    fn contents(&self, out: &mut String) {
        out.push_str(r#"<text:table-of-content text:style-name="Sect0" text:protected="true" text:name="Table of Contents1"><text:table-of-content-source text:outline-level="3"><text:index-title-template text:style-name="Contents_20_Heading">Contents</text:index-title-template>"#);
        for level in 1..=3 {
            let _ = write!(
                out,
                r#"<text:table-of-content-entry-template text:outline-level="{level}" text:style-name="Contents_20_{level}"><text:index-entry-link-start/><text:index-entry-chapter/><text:index-entry-text/><text:index-entry-tab-stop style:type="right" style:leader-char="."/><text:index-entry-page-number/><text:index-entry-link-end/></text:table-of-content-entry-template>"#
            );
        }
        out.push_str(r#"</text:table-of-content-source><text:index-body><text:index-title text:name="Table of Contents1_Head"><text:p text:style-name="Contents_20_Heading">Contents</text:p></text:index-title>"#);
        for (level, text, _) in self.doc.headings() {
            let level = level.max(1);
            let _ = write!(out, r#"<text:p text:style-name="Contents_20_{level}">{}</text:p>"#, odf::text_xml(&text));
        }
        out.push_str("</text:index-body></text:table-of-content>");
    }
}

fn header_footer(tag: &str, text: &str, numbers: bool) -> String {
    if text.is_empty() && !numbers {
        return String::new();
    }
    let style = if tag == "header" { "Header" } else { "Footer" };
    let style = if numbers && text.is_empty() { format!("{style}Centre") } else { style.to_string() };
    let mut body = odf::text_xml(text);
    if numbers {
        if !text.is_empty() {
            body.push_str("<text:tab/>");
        }
        body.push_str(r#"<text:page-number text:select-page="current">1</text:page-number>"#);
    }
    format!(r#"<style:{tag}><text:p text:style-name="{style}">{body}</text:p></style:{tag}>"#)
}

pub fn write(doc: &Document, media: &BTreeMap<String, Vec<u8>>) -> Result<(Vec<u8>, Vec<String>)> {
    let mut w = Writer {
        doc,
        media,
        auto: Auto::default(),
        pictures: Vec::new(),
        frames: 0,
        notes: (0, 0),
        comment_ends: HashMap::new(),
        open_comments: Vec::new(),
        changes: String::new(),
        change_n: 0,
        fonts: Vec::new(),
        lost: Vec::new(),
    };
    for (bi, b) in doc.blocks.iter().enumerate() {
        if let Block::Paragraph(p) = b {
            for (ri, r) in p.runs.iter().enumerate() {
                if let Run::Text(t) = r
                    && let Some(c) = t.comment
                    && t.deleted.is_none()
                {
                    w.comment_ends.insert(c, (bi, ri));
                }
            }
        }
    }
    if doc.comments.iter().any(|c| c.resolved) {
        w.lost.push("which comments are resolved".into());
    }

    let page = &doc.page;
    let plain_first = page.plain_first_page && (!page.header.is_empty() || !page.footer.is_empty() || page.page_numbers);
    let mut body = String::new();
    let mut pending_break = false;
    let mut first = true;
    let mut tables = 0;
    let mut in_section = false;
    let mut sections = 0;
    let mut i = 0;
    let blocks = &doc.blocks;
    while i < blocks.len() {
        // The page break and the first page's master go on the next paragraph.
        let mut extra = String::new();
        if pending_break {
            extra.push_str(r#" fo:break-before="page""#);
        }
        if first && plain_first {
            extra.push_str(r#"| style:master-page-name="First_20_Page""#);
        }
        match &blocks[i] {
            Block::Paragraph(p) if p.list.is_some() => {
                // A run of list paragraphs of one kind is one list, nested by indent.
                let kind = p.list;
                let start = i;
                while i < blocks.len() && matches!(&blocks[i], Block::Paragraph(q) if q.list == kind) {
                    i += 1;
                }
                let items: Vec<(usize, &Paragraph)> = (start..i)
                    .map(|k| match &blocks[k] {
                        Block::Paragraph(q) => (k, q),
                        _ => unreachable!("only paragraphs were taken"),
                    })
                    .collect();
                let style = if kind == Some(List::Number) { "L_Number" } else { "L_Bullet" };
                let mut at = 0;
                let mut first_extra = extra.clone();
                list_xml(&mut w, &items, &mut at, 0, style, &mut first_extra, &mut body);
                pending_break = false;
                first = false;
                continue;
            }
            Block::Paragraph(p) => {
                w.paragraph(i, p, &extra, &mut body);
                pending_break = false;
                first = false;
            }
            Block::Table(t) => {
                if first && plain_first {
                    w.paragraph(i, &Paragraph::default(), &extra, &mut body);
                    pending_break = false;
                }
                tables += 1;
                w.table(t, tables, pending_break, &mut body);
                pending_break = false;
                first = false;
            }
            Block::PageBreak => pending_break = true,
            Block::Contents => {
                if pending_break || (first && plain_first) {
                    w.paragraph(i, &Paragraph::default(), &extra, &mut body);
                    pending_break = false;
                }
                w.contents(&mut body);
                first = false;
            }
            Block::Section(s) => {
                if in_section {
                    body.push_str("</text:section>");
                }
                sections += 1;
                let _ = write!(
                    w.auto.other,
                    r#"<style:style style:name="Sect{sections}" style:family="section"><style:section-properties text:dont-balance-text-columns="false"><style:columns fo:column-count="{}" fo:column-gap="{}"/></style:section-properties></style:style>"#,
                    s.columns.clamp(1, 3),
                    pt(page.column_gap)
                );
                let _ = write!(body, r#"<text:section text:style-name="Sect{sections}" text:name="Section{sections}">"#);
                in_section = true;
                pending_break |= s.new_page;
            }
        }
        i += 1;
    }
    if pending_break {
        body.push_str(r#"<text:p text:style-name="Break"/>"#);
    }
    if in_section {
        body.push_str("</text:section>");
    }
    if body.is_empty() {
        body.push_str(r#"<text:p text:style-name="Standard"/>"#);
    }

    let mut faces = String::new();
    for f in std::iter::once(DEFAULT_FONT.to_string()).chain(w.fonts.iter().cloned()) {
        let _ = write!(faces, r#"<style:font-face style:name="{0}" svg:font-family="'{0}'"/>"#, esc(&f));
    }
    let content = format!(
        r##"<?xml version="1.0" encoding="UTF-8"?><office:document-content {ns}><office:font-face-decls>{faces}</office:font-face-decls><office:automatic-styles>{frames}<style:style style:name="Tbl" style:family="table"><style:table-properties style:width="{width}" table:align="margins" table:border-model="collapsing"/></style:style><style:style style:name="TblBreak" style:family="table"><style:table-properties style:width="{width}" table:align="margins" table:border-model="collapsing" fo:break-before="page"/></style:style><style:style style:name="TblCol" style:family="table-column"/><style:style style:name="TblCell" style:family="table-cell"><style:table-cell-properties fo:padding="4pt" fo:border="0.5pt solid #9aa0aa"/></style:style><style:style style:name="TblHead" style:family="table-cell"><style:table-cell-properties fo:padding="4pt" fo:border="0.5pt solid #9aa0aa" fo:background-color="#eef0f3"/></style:style><style:style style:name="Sect0" style:family="section"/><style:style style:name="Break" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:break-before="page"/></style:style>{auto}</office:automatic-styles><office:body><office:text><text:sequence-decls><text:sequence-decl text:display-outline-level="0" text:name="Illustration"/><text:sequence-decl text:display-outline-level="0" text:name="Table"/></text:sequence-decls>{changes}{body}</office:text></office:body></office:document-content>"##,
        ns = odf::NAMESPACES,
        frames = r#"<style:style style:name="fr0" style:family="graphic"><style:graphic-properties style:vertical-pos="top" style:vertical-rel="baseline"/></style:style><style:style style:name="frL" style:family="graphic"><style:graphic-properties style:wrap="right" style:horizontal-pos="left" style:horizontal-rel="paragraph" style:vertical-pos="top" style:vertical-rel="paragraph" fo:margin-right="9pt"/></style:style><style:style style:name="frR" style:family="graphic"><style:graphic-properties style:wrap="left" style:horizontal-pos="right" style:horizontal-rel="paragraph" style:vertical-pos="top" style:vertical-rel="paragraph" fo:margin-left="9pt"/></style:style>"#,
        width = pt(page.content_width()),
        auto = w.auto.other,
        changes = if w.changes.is_empty() && !doc.tracking {
            String::new()
        } else {
            format!(r#"<text:tracked-changes text:track-changes="{}">{}</text:tracked-changes>"#, doc.tracking, w.changes)
        },
    );

    let paper = page.paper();
    let [mt, mb, ml, mr] = page.margins;
    let columns = if page.columns() > 1 {
        format!(r#"<style:columns fo:column-count="{}" fo:column-gap="{}"/>"#, page.columns(), pt(page.column_gap))
    } else {
        String::new()
    };
    let layout = format!(
        r#"<style:page-layout style:name="pm1"><style:page-layout-properties fo:page-width="{}" fo:page-height="{}" style:print-orientation="{}" fo:margin-top="{}" fo:margin-bottom="{}" fo:margin-left="{}" fo:margin-right="{}">{columns}</style:page-layout-properties><style:header-style><style:header-footer-properties fo:min-height="0pt" fo:margin-bottom="12pt"/></style:header-style><style:footer-style><style:header-footer-properties fo:min-height="0pt" fo:margin-top="12pt"/></style:footer-style></style:page-layout>"#,
        pt(paper.width),
        pt(paper.height),
        if page.landscape { "landscape" } else { "portrait" },
        pt(mt),
        pt(mb),
        pt(ml),
        pt(mr)
    );
    let mut masters = format!(
        r#"<style:master-page style:name="Standard" style:page-layout-name="pm1">{}{}</style:master-page>"#,
        header_footer("header", &page.header, false),
        header_footer("footer", &page.footer, page.page_numbers)
    );
    if plain_first {
        masters.push_str(r#"<style:master-page style:name="First_20_Page" style:display-name="First Page" style:page-layout-name="pm1" style:next-style-name="Standard"/>"#);
    }
    let mut list_styles = String::new();
    for (name, number) in [("L_Bullet", false), ("L_Number", true)] {
        let _ = write!(list_styles, r#"<text:list-style style:name="{name}">"#);
        for level in 1..=10 {
            let indent = pt(level as f64 * INDENT_STEP);
            let props = format!(
                r#"<style:list-level-properties text:list-level-position-and-space-mode="label-alignment"><style:list-level-label-alignment text:label-followed-by="listtab" text:list-tab-stop-position="{indent}" fo:text-indent="-12pt" fo:margin-left="{indent}"/></style:list-level-properties>"#
            );
            if number {
                let fmt = ["1", "a", "i"][(level - 1) % 3];
                let _ = write!(
                    list_styles,
                    r#"<text:list-level-style-number text:level="{level}" style:num-suffix="." style:num-format="{fmt}">{props}</text:list-level-style-number>"#
                );
            } else {
                let bullet = ["•", "◦", "▪"][(level - 1) % 3];
                let _ = write!(
                    list_styles,
                    r#"<text:list-level-style-bullet text:level="{level}" text:bullet-char="{bullet}">{props}</text:list-level-style-bullet>"#
                );
            }
        }
        list_styles.push_str("</text:list-style>");
    }
    let named = |name: &str, display: &str, parent: &str, extra: &str, para: String, text: String| {
        format!(
            r#"<style:style style:name="{name}" style:display-name="{display}" style:family="paragraph" style:parent-style-name="{parent}"{extra}><style:paragraph-properties{para}/><style:text-properties{text}/></style:style>"#
        )
    };
    let mut styles_xml = String::new();
    for s in Style::ALL {
        let (size, above, below, bold, italic) = s.look();
        let text = format!(
            r#" fo:font-size="{}"{}{}"#,
            pt(size),
            if bold { r#" fo:font-weight="bold""# } else { "" },
            if italic { r#" fo:font-style="italic""# } else { "" }
        );
        let para = format!(r#" fo:margin-top="{}" fo:margin-bottom="{}""#, pt(above), pt(below));
        let (display, extra) = match s {
            Style::Normal => ("Default Paragraph Style", ""),
            Style::Title => ("Title", r#" style:class="chapter""#),
            Style::H1 => ("Heading 1", r#" style:default-outline-level="1" style:class="text""#),
            Style::H2 => ("Heading 2", r#" style:default-outline-level="2" style:class="text""#),
            Style::H3 => ("Heading 3", r#" style:default-outline-level="3" style:class="text""#),
            Style::Quote => ("Quotations", r#" style:class="html""#),
        };
        let para = if s == Style::Quote { format!(r#"{para} fo:margin-left="28pt" fo:margin-right="28pt""#) } else { para };
        if s == Style::Normal {
            let _ = write!(
                styles_xml,
                r#"<style:style style:name="Standard" style:display-name="{display}" style:family="paragraph" style:class="text"><style:paragraph-properties{para}/><style:text-properties{text}/></style:style>"#
            );
        } else {
            styles_xml.push_str(&named(style_name(s), display, "Standard", extra, para, text));
        }
    }
    for (name, display, para, text) in [
        ("Header", "Header", r#" fo:text-align="start""#, r#" fo:font-size="9pt""#),
        ("HeaderCentre", "Header Centre", r#" fo:text-align="center""#, r#" fo:font-size="9pt""#),
        ("Footer", "Footer", r#" fo:text-align="start""#, r#" fo:font-size="9pt""#),
        ("FooterCentre", "Footer Centre", r#" fo:text-align="center""#, r#" fo:font-size="9pt""#),
        ("Footnote", "Footnote", "", r#" fo:font-size="9pt""#),
        ("Endnote", "Endnote", "", r#" fo:font-size="9pt""#),
        ("Table_20_Contents", "Table Contents", r#" fo:margin-top="0pt" fo:margin-bottom="0pt""#, ""),
        ("Table_20_Heading", "Table Heading", r#" fo:margin-top="0pt" fo:margin-bottom="0pt""#, r#" fo:font-weight="bold""#),
        ("Contents_20_Heading", "Contents Heading", "", r#" fo:font-size="16pt" fo:font-weight="bold""#),
        ("Contents_20_1", "Contents 1", "", ""),
        ("Contents_20_2", "Contents 2", r#" fo:margin-left="14pt""#, ""),
        ("Contents_20_3", "Contents 3", r#" fo:margin-left="28pt""#, ""),
    ] {
        styles_xml.push_str(&named(name, display, "Standard", "", para.to_string(), text.to_string()));
    }
    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-styles {ns}><office:font-face-decls>{faces}</office:font-face-decls><office:styles><style:default-style style:family="paragraph"><style:paragraph-properties style:writing-mode="page"/><style:text-properties style:font-name="{font}" fo:font-family="'{font}'" fo:font-size="{size}" fo:color="{color}" fo:language="en" fo:country="US"/></style:default-style><style:default-style style:family="graphic"/>{styles_xml}{list_styles}<text:notes-configuration text:note-class="footnote" style:num-format="1" text:start-value="0" text:footnotes-position="page" text:start-numbering-at="document"/><text:notes-configuration text:note-class="endnote" style:num-format="i" text:start-value="0"/></office:styles><office:automatic-styles>{layout}</office:automatic-styles><office:master-styles>{masters}</office:master-styles></office:document-styles>"#,
        ns = odf::NAMESPACES,
        font = DEFAULT_FONT,
        size = pt(DEFAULT_SIZE),
        color = DEFAULT_COLOR,
    );
    let extras: Vec<odf::Extra> = w
        .pictures
        .iter()
        .filter_map(|name| {
            media.get(name).map(|bytes| odf::Extra { path: format!("Pictures/{name}"), bytes, media_type: "image/png" })
        })
        .collect();
    let bytes = odf::package(odf::ODT, &content, &styles, &extras)?;
    Ok((bytes, w.lost))
}

/// A list: items at `depth` and deeper, as nested `text:list`s.
fn list_xml(
    w: &mut Writer,
    items: &[(usize, &Paragraph)],
    at: &mut usize,
    depth: u8,
    style: &str,
    first_extra: &mut String,
    out: &mut String,
) {
    if depth == 0 {
        let _ = write!(out, r#"<text:list text:style-name="{style}">"#);
    } else {
        out.push_str("<text:list>");
    }
    while *at < items.len() && items[*at].1.indent.min(MAX_INDENT) >= depth {
        out.push_str("<text:list-item>");
        let (bi, p) = items[*at];
        if p.indent.min(MAX_INDENT) == depth {
            let extra = std::mem::take(first_extra);
            w.paragraph(bi, p, &extra, out);
            *at += 1;
        }
        if *at < items.len() && items[*at].1.indent.min(MAX_INDENT) > depth {
            list_xml(w, items, at, depth + 1, style, first_extra, out);
        }
        out.push_str("</text:list-item>");
    }
    out.push_str("</text:list>");
}

// ---------- Reading ----------

/// Most cells a table keeps when reading.
const MAX_TABLE_ROWS: usize = 2000;
const MAX_TABLE_COLS: usize = 64;

struct Reader<'p, 'b> {
    styles: Styles,
    pkg: &'p mut odf::Package<'b>,
    cache: BTreeMap<String, Option<Vec<u8>>>,
    media: BTreeMap<String, Vec<u8>>,
    doc: Document,
    /// Comment names open at this point: office:name → our id.
    open: Vec<(String, u32)>,
    /// A comment with no end: it covers the next stretch of text.
    pending: Option<u32>,
    page_columns: u8,
    /// Tracked changes by region id: insertion or deletion, its revision, and a deletion's words.
    changes: std::collections::HashMap<String, (bool, u32, String)>,
    /// The insertion the text is in.
    inserting: Option<u32>,
}

/// A style name we treat as one of ours (its look is ours, not overrides).
fn builtin(name: &str) -> Option<Option<Style>> {
    let k = name.replace("_20_", " ").to_lowercase();
    Some(match k.as_str() {
        "title" => Some(Style::Title),
        "heading 1" => Some(Style::H1),
        "heading 2" => Some(Style::H2),
        "heading 3" | "heading 4" | "heading 5" | "heading 6" => Some(Style::H3),
        "quotations" | "quote" | "quotation" => Some(Style::Quote),
        "standard" | "text body" | "default paragraph style" | "normal" | "heading" | "list paragraph" | "list" => None,
        _ => return None,
    })
}

/// Our style for a named paragraph style, by its name or a parent's.
fn style_of(styles: &Styles, name: &str) -> Option<Style> {
    for n in styles.lineage("paragraph", name) {
        if let Some(s) = builtin(&n) {
            return s;
        }
    }
    None
}

impl Reader<'_, '_> {
    /// A text style's formatting laid over `base`.
    fn run_format(&self, family: &str, name: &str, base: &TextRun, own: bool) -> TextRun {
        let mut r = base.clone();
        let get = |key: &str| if own { self.styles.own(family, name, key) } else { self.styles.get(family, name, key) };
        if let Some(v) = get("text/font-weight") {
            r.bold = v == "bold" || v.parse::<u32>().is_ok_and(|w| w >= 600);
        }
        if let Some(v) = get("text/font-style") {
            r.italic = v == "italic" || v == "oblique";
        }
        if let Some(v) = get("text/text-underline-style") {
            r.underline = v != "none";
        }
        if let Some(v) = get("text/text-line-through-style") {
            r.strike = v != "none";
        }
        if let Some(v) = get("text/text-position") {
            let first = v.split_whitespace().next().unwrap_or("");
            let pct = first.trim_end_matches('%').parse::<f64>().unwrap_or(0.0);
            r.superscript = first == "super" || pct > 0.0;
            r.subscript = first == "sub" || pct < 0.0;
        }
        if let Some(v) = get("text/font-size").and_then(odf::length) {
            r.size = Some(v);
        }
        if let Some(c) = get("text/color").and_then(odf::color) {
            r.color = Some(c);
        }
        if let Some(c) = get("text/background-color") {
            r.highlight = odf::color(c);
        }
        let font = if own {
            self.styles.own(family, name, "text/font-name").map(|f| self.styles.face(f)).or_else(|| {
                self.styles.own(family, name, "text/font-family").map(|f| f.trim_matches(['\'', '"']).to_string())
            })
        } else {
            self.styles.font(family, name)
        };
        if let Some(f) = font {
            r.font = Some(f);
        }
        r
    }

    fn comment(&mut self, n: &roxmltree::Node) -> u32 {
        let id = self.doc.comments.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        let author = n.children().find(|c| c.has_tag_name((DC, "creator"))).and_then(|c| c.text()).unwrap_or("").to_string();
        let mut date = n.children().find(|c| c.has_tag_name((DC, "date"))).and_then(|c| c.text()).unwrap_or("").to_string();
        if date.starts_with("0000") {
            date.clear();
        }
        if !date.is_empty() {
            // Seconds, no fraction, in UTC's form.
            date.truncate(19);
            date.push('Z');
        }
        let text = odf::paragraphs_text(n).trim().to_string();
        self.doc.comments.push(Comment { id, author, date, text, resolved: false });
        id
    }

    /// The runs inside a paragraph (or a span or link within one).
    fn runs(&mut self, n: &roxmltree::Node, fmt: &TextRun, out: &mut Vec<Run>) {
        for c in n.children() {
            if c.is_text() {
                let mut r = fmt.clone();
                r.text = c.text().unwrap_or("").replace(['\n', '\r'], " ");
                self.mark(&mut r);
                out.push(Run::Text(r));
                continue;
            }
            let push = |s: &mut Self, out: &mut Vec<Run>, text: String| {
                let mut r = fmt.clone();
                r.text = text;
                s.mark(&mut r);
                out.push(Run::Text(r));
            };
            match (c.tag_name().namespace(), c.tag_name().name()) {
                (Some(TEXT), "span") => {
                    let f = match attr(&c, TEXT, "style-name") {
                        Some(s) => self.run_format("text", s, fmt, false),
                        None => fmt.clone(),
                    };
                    self.runs(&c, &f, out);
                }
                (Some(TEXT), "a") => {
                    let mut f = fmt.clone();
                    f.link = attr(&c, XLINK, "href").map(String::from).filter(|h| !h.is_empty());
                    self.runs(&c, &f, out);
                }
                (Some(TEXT), "s") => {
                    let k: usize = attr(&c, TEXT, "c").and_then(|v| v.parse().ok()).unwrap_or(1);
                    push(self, out, " ".repeat(k.min(1000)));
                }
                (Some(TEXT), "tab") => push(self, out, "\t".into()),
                (Some(TEXT), "line-break") => push(self, out, LINE_BREAK.to_string()),
                (Some(TEXT), "note") => {
                    let end = attr(&c, TEXT, "note-class") == Some("endnote");
                    let body = c.children().find(|x| x.has_tag_name((TEXT, "note-body")));
                    let note = body.map(|b| odf::paragraphs_text(&b)).unwrap_or_default();
                    out.push(Run::Note(Note { note, end }));
                }
                (Some(OFFICE), "annotation") => {
                    let id = self.comment(&c);
                    match attr(&c, OFFICE, "name") {
                        Some(name) => self.open.push((name.to_string(), id)),
                        None => self.pending = Some(id),
                    }
                }
                (Some(OFFICE), "annotation-end") => {
                    let name = attr(&c, OFFICE, "name").unwrap_or("");
                    self.open.retain(|(n, _)| n != name);
                }
                (Some(DRAW), "frame") => {
                    if let Some(img) = self.picture(&c) {
                        out.push(Run::Image(img));
                    }
                }
                (Some(DRAW), "a") => self.runs(&c, fmt, out),
                (Some(TEXT), "bookmark" | "bookmark-start" | "bookmark-end" | "soft-page-break" | "reference-mark")
                | (Some(TEXT), "sequence-decls") => {}
                (Some(TEXT), "change-start") => {
                    let id = attr(&c, TEXT, "change-id").unwrap_or("");
                    self.inserting = self.changes.get(id).filter(|ch| ch.0).map(|ch| ch.1);
                }
                (Some(TEXT), "change-end") => self.inserting = None,
                (Some(TEXT), "change") => {
                    // Deleted words, still shown until the change is accepted.
                    let id = attr(&c, TEXT, "change-id").unwrap_or("");
                    if let Some((false, rev, words)) = self.changes.get(id).cloned() {
                        let mut r = fmt.clone();
                        r.text = words;
                        r.deleted = Some(rev);
                        out.push(Run::Text(r));
                    }
                }
                // Fields (page numbers, dates, cross-references…) keep their shown text.
                (Some(TEXT), _) => {
                    let t = odf::plain_text(&c);
                    if !t.is_empty() {
                        push(self, out, t);
                    }
                }
                _ => {}
            }
        }
    }

    /// Applies the open comments and the insertion it's in to a run.
    fn mark(&mut self, r: &mut TextRun) {
        r.inserted = self.inserting;
        if let Some((_, id)) = self.open.last() {
            r.comment = Some(*id);
        } else if let Some(id) = self.pending.take() {
            r.comment = Some(id);
        }
    }

    fn picture(&mut self, frame: &roxmltree::Node) -> Option<Image> {
        let img = frame.children().find(|c| c.has_tag_name((DRAW, "image")))?;
        let href = attr(&img, XLINK, "href")?;
        let png = odf::picture(self.pkg, href, &mut self.cache)?;
        let name = format!("picture-{}.png", self.media.len() + 1);
        let column = self.doc.page.column_width();
        let width = attr(frame, SVG, "width")
            .and_then(odf::length)
            .or_else(|| odf::png_size(&png).map(|(w, _)| w as f64 * 0.75))
            .unwrap_or(200.0)
            .clamp(8.0, column);
        let anchored = !matches!(attr(frame, TEXT, "anchor-type"), Some("as-char") | None);
        let wrap = if anchored {
            let style = attr(frame, DRAW, "style-name").unwrap_or("");
            let pos = self.styles.get("graphic", style, "graphic/horizontal-pos");
            let mode = self.styles.get("graphic", style, "graphic/wrap");
            match (pos, mode) {
                (_, Some("none" | "run-through")) | (_, None) => None,
                (Some("right"), _) => Some(Side::Right),
                (Some("left" | "from-left"), _) => Some(Side::Left),
                _ => {
                    // Placed by position: the nearer side.
                    let x = attr(frame, SVG, "x").and_then(odf::length).unwrap_or(0.0);
                    Some(if x + width / 2.0 > column / 2.0 { Side::Right } else { Side::Left })
                }
            }
        } else {
            None
        };
        self.media.insert(name.clone(), png);
        Some(Image { image: name, width, wrap })
    }

    fn paragraph(&mut self, p: &roxmltree::Node, list: Option<(List, u8)>) {
        let style_name = attr(p, TEXT, "style-name").unwrap_or("Standard").to_string();
        let mut para = Paragraph { style: style_of(&self.styles, &style_name).unwrap_or_default(), ..Default::default() };
        if p.has_tag_name((TEXT, "h")) && para.style == Style::Normal {
            let level: u32 = attr(p, TEXT, "outline-level").and_then(|v| v.parse().ok()).unwrap_or(1);
            para.style = match level {
                0 | 1 => Style::H1,
                2 => Style::H2,
                _ => Style::H3,
            };
        }
        // A built-in style's look is ours; only other styles' settings are overrides.
        let plain = builtin(&style_name).is_some();
        let own = |k: &str| if plain { None } else { self.styles.own("paragraph", &style_name, k).map(String::from) };
        if own("paragraph/break-before").as_deref() == Some("page") && !self.doc.blocks.is_empty() {
            self.doc.blocks.push(Block::PageBreak);
        }
        para.align = match own("paragraph/text-align").as_deref() {
            Some("center") => Align::Center,
            Some("end" | "right") => Align::Right,
            Some("justify") => Align::Justify,
            Some(_) => Align::Left,
            None => Align::Left,
        };
        if let Some(v) = own("paragraph/line-height") {
            para.line_spacing = v.strip_suffix('%').and_then(|n| n.parse::<f64>().ok()).map(|n| n / 100.0).filter(|v| *v != 1.0);
        }
        para.space_before = own("paragraph/margin-top").as_deref().and_then(odf::length);
        para.space_after = own("paragraph/margin-bottom").as_deref().and_then(odf::length);
        match list {
            Some((kind, level)) => {
                para.list = Some(kind);
                para.indent = level.min(MAX_INDENT);
            }
            None => {
                if let Some(m) = own("paragraph/margin-left").as_deref().and_then(odf::length) {
                    para.indent = ((m / INDENT_STEP).round() as i64).clamp(0, MAX_INDENT as i64) as u8;
                }
            }
        }
        let break_after = own("paragraph/break-after").as_deref() == Some("page");
        let base = if plain { TextRun::default() } else { self.run_format("paragraph", &style_name, &TextRun::default(), true) };
        let mut runs = Vec::new();
        self.runs(p, &base, &mut runs);
        para.runs = runs;
        self.doc.blocks.push(Block::Paragraph(para));
        if break_after {
            self.doc.blocks.push(Block::PageBreak);
        }
    }

    fn list(&mut self, l: &roxmltree::Node, style: Option<String>, level: u8) {
        let style = attr(l, TEXT, "style-name").map(String::from).or(style);
        for item in l.children().filter(|c| c.is_element()) {
            let header = item.has_tag_name((TEXT, "list-header"));
            if !header && !item.has_tag_name((TEXT, "list-item")) {
                continue;
            }
            for c in item.children().filter(|c| c.is_element()) {
                match c.tag_name().name() {
                    "p" | "h" => {
                        // A paragraph's own list style wins over the list's.
                        let ps = attr(&c, TEXT, "style-name").and_then(|s| self.styles.list_style("paragraph", s));
                        let ls = style.as_deref().or(ps);
                        let numbered = ls
                            .and_then(|s| self.styles.lists.get(s))
                            .and_then(|levels| levels.get(level as usize).copied())
                            .unwrap_or(false);
                        let kind = if numbered { List::Number } else { List::Bullet };
                        self.paragraph(&c, if header { None } else { Some((kind, level)) });
                    }
                    "list" => self.list(&c, style.clone(), level.saturating_add(1)),
                    _ => self.block(&c),
                }
            }
        }
    }

    fn table(&mut self, t: &roxmltree::Node) {
        let mut rows: Vec<Vec<String>> = Vec::new();
        let header = t.children().any(|c| c.has_tag_name((TABLE, "table-header-rows")));
        for row in t.descendants().filter(|c| c.has_tag_name((TABLE, "table-row"))) {
            // Rows of a table inside this table belong to that one.
            if row.ancestors().filter(|a| a.has_tag_name((TABLE, "table"))).count() > 1 {
                continue;
            }
            let repeat: usize = attr(&row, TABLE, "number-rows-repeated").and_then(|v| v.parse().ok()).unwrap_or(1);
            let mut cells = Vec::new();
            for cell in row.children().filter(|c| c.has_tag_name((TABLE, "table-cell")) || c.has_tag_name((TABLE, "covered-table-cell"))) {
                let k: usize = attr(&cell, TABLE, "number-columns-repeated").and_then(|v| v.parse().ok()).unwrap_or(1);
                let text = if cell.tag_name().name() == "covered-table-cell" { String::new() } else { odf::paragraphs_text(&cell) };
                for _ in 0..k.min(MAX_TABLE_COLS) {
                    if cells.len() < MAX_TABLE_COLS {
                        cells.push(text.clone());
                    }
                }
            }
            for _ in 0..repeat.min(MAX_TABLE_ROWS) {
                if rows.len() < MAX_TABLE_ROWS {
                    rows.push(cells.clone());
                }
            }
        }
        // Repeated empty cells and rows pad a table out to the page; drop them.
        while rows.last().is_some_and(|r| r.iter().all(String::is_empty)) && rows.len() > 1 {
            rows.pop();
        }
        let cols = rows.iter().map(|r| r.iter().rposition(|c| !c.is_empty()).map_or(0, |i| i + 1)).max().unwrap_or(1).max(1);
        for r in &mut rows {
            r.resize(cols, String::new());
        }
        if rows.is_empty() {
            return;
        }
        self.doc.blocks.push(Block::Table(Table { rows, header }));
    }

    fn block(&mut self, n: &roxmltree::Node) {
        match (n.tag_name().namespace(), n.tag_name().name()) {
            (Some(TEXT), "p" | "h") => self.paragraph(n, None),
            (Some(TEXT), "list") => self.list(n, None, 0),
            (Some(TABLE), "table") => self.table(n),
            (Some(TEXT), "table-of-content") => self.doc.blocks.push(Block::Contents),
            (Some(TEXT), "section") => {
                let style = attr(n, TEXT, "style-name").unwrap_or("");
                let cols: u8 = self.styles.get("section", style, "section/column-count").and_then(|v| v.parse().ok()).unwrap_or(1);
                let cols = cols.clamp(1, 3);
                let changes = cols != self.page_columns;
                if changes {
                    self.doc.blocks.push(Block::Section(Section { columns: cols, new_page: false }));
                }
                for c in n.children().filter(|c| c.is_element()) {
                    self.block(&c);
                }
                if changes {
                    self.doc.blocks.push(Block::Section(Section { columns: self.page_columns, new_page: false }));
                }
            }
            (Some(TEXT), "index-body" | "illustration-index" | "alphabetical-index" | "user-index" | "bibliography" | "table-index" | "object-index") => {}
            (Some(TEXT), "tracked-changes" | "sequence-decls" | "variable-decls" | "user-field-decls" | "soft-page-break") => {}
            _ => {
                // Anything else that holds paragraphs (a frame's text box, say).
                for c in n.children().filter(|c| c.is_element()) {
                    self.block(&c);
                }
            }
        }
    }
}

/// Header or footer text, and whether it shows the page number.
fn header_text(n: Option<roxmltree::Node>) -> (String, bool) {
    let Some(n) = n else { return (String::new(), false) };
    if attr(&n, STYLE, "display") == Some("false") {
        return (String::new(), false);
    }
    let numbers = n.descendants().any(|c| c.has_tag_name((TEXT, "page-number")));
    let text: String = n
        .descendants()
        .filter(|c| c.has_tag_name((TEXT, "p")))
        .map(|p| {
            let mut s = String::new();
            for c in p.descendants() {
                if c.is_text() && !c.ancestors().any(|a| a.has_tag_name((TEXT, "page-number")) || a.has_tag_name((TEXT, "page-count"))) {
                    s.push_str(c.text().unwrap_or(""));
                }
            }
            s
        })
        .collect::<Vec<_>>()
        .join(" ");
    (text.trim().to_string(), numbers)
}

pub fn read(bytes: &[u8]) -> Result<(Document, BTreeMap<String, Vec<u8>>)> {
    let mut pkg = odf::Package::open(bytes, odf::ODT, "text document")?;
    let content = pkg.content()?;
    let styles_text = pkg.text("styles.xml").unwrap_or_default();
    let cdoc = roxmltree::Document::parse(&content).context("the document is damaged")?;
    let sdoc = roxmltree::Document::parse(&styles_text).ok();
    let mut parts = Vec::new();
    if let Some(s) = &sdoc {
        parts.push(s);
    }
    parts.push(&cdoc);
    let styles = Styles::read(&parts);

    let mut doc = Document::default();
    // The page: the master page the first paragraph names, else "Standard".
    let body = cdoc
        .descendants()
        .find(|n| n.has_tag_name((OFFICE, "text")))
        .context("it isn't a text document")?;
    let first_master = body
        .descendants()
        .filter(|n| !n.ancestors().any(|a| a.has_tag_name((TEXT, "tracked-changes")) || a.has_tag_name((OFFICE, "annotation"))))
        .find(|n| n.has_tag_name((TEXT, "p")) || n.has_tag_name((TEXT, "h")))
        .and_then(|p| attr(&p, TEXT, "style-name"))
        .and_then(|s| styles.own("paragraph", s, "master-page-name"))
        .filter(|m| !m.is_empty())
        .map(String::from);
    let master_node = |name: &str| {
        sdoc.as_ref()
            .and_then(|d| d.descendants().find(|n| n.has_tag_name((STYLE, "master-page")) && attr(n, STYLE, "name") == Some(name)))
    };
    let main_name = first_master
        .as_deref()
        .and_then(master_node)
        .and_then(|m| attr(&m, STYLE, "next-style-name").map(String::from))
        .or_else(|| first_master.clone())
        .unwrap_or_else(|| "Standard".into());
    let main = master_node(&main_name).or_else(|| {
        sdoc.as_ref().and_then(|d| d.descendants().find(|n| n.has_tag_name((STYLE, "master-page"))))
    });
    if let Some(m) = main {
        let layout = attr(&m, STYLE, "page-layout-name").unwrap_or("");
        let g = |k: &str| styles.get("page-layout", layout, &format!("layout/{k}")).and_then(odf::length);
        if let (Some(w), Some(h)) = (g("page-width"), g("page-height")) {
            doc.page.paper = crate::print::Paper::closest(w, h).unwrap_or("letter").into();
            doc.page.landscape = w > h;
        }
        let margins = [g("margin-top"), g("margin-bottom"), g("margin-left"), g("margin-right")];
        for (i, v) in margins.into_iter().enumerate() {
            if let Some(v) = v {
                doc.page.margins[i] = v.clamp(0.0, 288.0);
            }
        }
        if let Some(c) = styles.get("page-layout", layout, "layout/column-count").and_then(|v| v.parse::<u8>().ok()) {
            doc.page.columns = c.clamp(1, 3);
        }
        if let Some(g) = styles.get("page-layout", layout, "layout/column-gap").and_then(odf::length) {
            doc.page.column_gap = g;
        }
        let child = |tag: &str| m.children().find(|c| c.has_tag_name((STYLE, tag)));
        let (header, hn) = header_text(child("header"));
        let (footer, fnum) = header_text(child("footer"));
        doc.page.header = header;
        doc.page.footer = footer;
        doc.page.page_numbers = hn || fnum;
        let has = !doc.page.header.is_empty() || !doc.page.footer.is_empty() || doc.page.page_numbers;
        // A first page with nothing at its top and foot: a master of its own, or the main one's first-page parts.
        let first_plain = match first_master.as_deref().filter(|f| *f != main_name).and_then(master_node) {
            Some(f) => {
                let hf = |tag: &str| header_text(f.children().find(|c| c.has_tag_name((STYLE, tag))));
                let (a, b) = (hf("header"), hf("footer"));
                a.0.is_empty() && b.0.is_empty() && !a.1 && !b.1
            }
            None => ["header-first", "footer-first"].iter().any(|t| {
                child(t).is_some_and(|n| {
                    let (t, n2) = header_text(Some(n));
                    t.is_empty() && !n2
                })
            }),
        };
        doc.page.plain_first_page = has && first_plain;
    }

    let page_columns = doc.page.columns;
    let mut r = Reader {
        styles,
        pkg: &mut pkg,
        cache: BTreeMap::new(),
        media: BTreeMap::new(),
        doc,
        open: Vec::new(),
        pending: None,
        page_columns,
        changes: std::collections::HashMap::new(),
        inserting: None,
    };
    // Tracked changes: who and when for each region, and what a deletion took out.
    if let Some(tc) = body.children().find(|c| c.has_tag_name((TEXT, "tracked-changes"))) {
        r.doc.tracking = attr(&tc, TEXT, "track-changes") != Some("false");
        for region in tc.children().filter(|c| c.has_tag_name((TEXT, "changed-region"))) {
            let Some(id) = attr(&region, TEXT, "id").or_else(|| region.attribute(("http://www.w3.org/XML/1998/namespace", "id"))) else {
                continue;
            };
            let Some(kind) = region.children().find(|c| c.has_tag_name((TEXT, "insertion")) || c.has_tag_name((TEXT, "deletion"))) else {
                continue;
            };
            let info = kind.children().find(|c| c.has_tag_name((OFFICE, "change-info")));
            let field = |tag: &str| {
                info.and_then(|i| i.children().find(|c| c.has_tag_name((DC, tag)))).and_then(|c| c.text()).unwrap_or("").to_string()
            };
            let author = field("creator");
            let mut date = field("date");
            date.truncate(19);
            if !date.is_empty() && !date.starts_with("1970") {
                date.push('Z');
            } else {
                date.clear();
            }
            // One revision per author and moment.
            let rev = match r.doc.revisions.iter().find(|x| x.author == author && x.date == date) {
                Some(x) => x.id,
                None => {
                    let id = r.doc.revisions.len() as u32 + 1;
                    r.doc.revisions.push(Revision { id, author, date });
                    id
                }
            };
            let insertion = kind.has_tag_name((TEXT, "insertion"));
            let words = if insertion { String::new() } else { odf::paragraphs_text(&kind).replace('\n', " ") };
            r.changes.insert(id.to_string(), (insertion, rev, words));
        }
    }
    for c in body.children().filter(|c| c.is_element()) {
        r.block(&c);
    }
    let mut doc = r.doc;
    // A trailing section break back to the page's columns says nothing.
    while matches!(doc.blocks.last(), Some(Block::Section(_))) {
        doc.blocks.pop();
    }
    doc.prune_comments();
    doc.tidy();
    Ok((doc, r.media))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        crate::present::model::base64(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        )
        .unwrap()
    }

    #[test]
    fn round_trips_through_opendocument() {
        let mut media = BTreeMap::new();
        media.insert("pic.png".to_string(), png());
        let doc = Document {
            page: PageSetup {
                paper: "a4".into(),
                landscape: true,
                margins: [50.0, 60.0, 70.0, 80.0],
                header: "Report".into(),
                footer: "Draft".into(),
                page_numbers: true,
                plain_first_page: true,
                columns: 2,
                column_gap: 20.0,
            },
            blocks: vec![
                Block::Paragraph(Paragraph::text(Style::Title, "The title")),
                Block::Paragraph(Paragraph::text(Style::H1, "Part one")),
                Block::Paragraph(Paragraph {
                    align: Align::Center,
                    line_spacing: Some(1.5),
                    space_after: Some(14.0),
                    runs: vec![
                        Run::text("Plain  two spaces\tand a tab"),
                        Run::styled(" bold", |r| r.bold = true),
                        Run::styled(" big red", |r| {
                            r.size = Some(18.0);
                            r.color = Some("#cc0000".into());
                            r.font = Some("Noto Serif".into());
                        }),
                        Run::styled(" link", |r| r.link = Some("https://example.com".into())),
                        Run::styled("2", |r| r.superscript = true),
                        Run::text(&format!("line{LINE_BREAK}break")),
                        Run::Note(Note { note: "A footnote.".into(), end: false }),
                        Run::Note(Note { note: "An endnote.".into(), end: true }),
                    ],
                    ..Default::default()
                }),
                Block::Paragraph(Paragraph {
                    runs: vec![Run::text("before "), Run::styled("noted", |r| r.comment = Some(1)), Run::text(" after")],
                    ..Default::default()
                }),
                Block::Paragraph(Paragraph { list: Some(List::Number), runs: vec![Run::text("one")], ..Default::default() }),
                Block::Paragraph(Paragraph { list: Some(List::Number), indent: 1, runs: vec![Run::text("one a")], ..Default::default() }),
                Block::Paragraph(Paragraph { list: Some(List::Number), runs: vec![Run::text("two")], ..Default::default() }),
                Block::Paragraph(Paragraph { list: Some(List::Bullet), runs: vec![Run::text("dot")], ..Default::default() }),
                Block::PageBreak,
                Block::Paragraph(Paragraph {
                    indent: 2,
                    runs: vec![Run::Image(Image { image: "pic.png".into(), width: 120.0, wrap: Some(Side::Right) }), Run::text("beside")],
                    ..Default::default()
                }),
                Block::Table(Table { rows: vec![vec!["A".into(), "B".into()], vec!["1".into(), "two\nlines".into()]], header: true }),
                Block::Contents,
                Block::Paragraph(Paragraph::text(Style::Quote, "Quoted")),
                Block::Paragraph(Paragraph {
                    runs: vec![
                        Run::text("kept "),
                        Run::styled("added", |r| r.inserted = Some(1)),
                        Run::styled(" removed", |r| r.deleted = Some(2)),
                    ],
                    ..Default::default()
                }),
            ],
            revisions: vec![
                Revision { id: 1, author: "Ken".into(), date: "2026-03-04T05:06:07Z".into() },
                Revision { id: 2, author: "Sam".into(), date: "2026-03-05T05:06:07Z".into() },
            ],
            tracking: true,
            comments: vec![Comment { id: 1, author: "Ken".into(), date: "2026-01-02T03:04:05Z".into(), text: "Check this".into(), resolved: false }],
        };
        let (bytes, lost) = write(&doc, &media).unwrap();
        assert!(lost.is_empty(), "{lost:?}");
        assert_eq!(&bytes[30..38], b"mimetype");
        let (back, back_media) = read(&bytes).unwrap();
        assert_eq!(back.page, doc.page);
        assert_eq!(back.comments, doc.comments);
        assert_eq!((&back.revisions, back.tracking), (&doc.revisions, true));
        assert_eq!(back_media.values().next(), Some(&png()));
        let mut want = doc.clone();
        // The picture comes back under the reader's name.
        if let Block::Paragraph(p) = &mut want.blocks[9] {
            p.runs[0] = Run::Image(Image { image: "picture-1.png".into(), width: 120.0, wrap: Some(Side::Right) });
        }
        want.tidy();
        for (a, b) in want.blocks.iter().zip(&back.blocks) {
            assert_eq!(a, b);
        }
        assert_eq!(want.blocks.len(), back.blocks.len());
    }

    #[test]
    fn reads_libreoffice_styles() {
        // As LibreOffice writes it: automatic styles over named ones, a named list style.
        let content = format!(
            r#"<?xml version="1.0"?><office:document-content {ns}><office:automatic-styles>
            <style:style style:name="P1" style:family="paragraph" style:parent-style-name="Text_20_body"><style:paragraph-properties fo:text-align="end"/><style:text-properties fo:font-weight="bold"/></style:style>
            <style:style style:name="T1" style:family="text"><style:text-properties fo:font-style="italic" style:text-underline-style="solid"/></style:style>
            </office:automatic-styles><office:body><office:text>
            <text:h text:style-name="Heading_20_2" text:outline-level="2">Heading</text:h>
            <text:p text:style-name="P1">Right <text:span text:style-name="T1">it</text:span></text:p>
            <text:list text:style-name="L1"><text:list-item><text:p>a</text:p><text:list><text:list-item><text:p>b</text:p></text:list-item></text:list></text:list-item></text:list>
            <text:p text:style-name="Standard"><office:annotation><dc:creator>A</dc:creator><text:p>point</text:p></office:annotation>word</text:p>
            </office:text></office:body></office:document-content>"#,
            ns = odf::NAMESPACES
        );
        let styles = format!(
            r#"<?xml version="1.0"?><office:document-styles {ns}><office:styles>
            <style:style style:name="Text_20_body" style:family="paragraph" style:parent-style-name="Standard"/>
            <text:list-style style:name="L1"><text:list-level-style-number text:level="1"/><text:list-level-style-bullet text:level="2"/></text:list-style>
            </office:styles></office:document-styles>"#,
            ns = odf::NAMESPACES
        );
        let bytes = odf::package(odf::ODT, &content, &styles, &[]).unwrap();
        let (d, _) = read(&bytes).unwrap();
        let ps: Vec<&Paragraph> = d.blocks.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).collect();
        assert_eq!(ps[0].style, Style::H2);
        assert_eq!(ps[1].align, Align::Right);
        assert!(matches!(&ps[1].runs[0], Run::Text(t) if t.bold && t.text == "Right "));
        assert!(matches!(&ps[1].runs[1], Run::Text(t) if t.bold && t.italic && t.underline));
        assert_eq!((ps[2].list, ps[2].indent), (Some(List::Number), 0));
        assert_eq!((ps[3].list, ps[3].indent), (Some(List::Bullet), 1));
        assert_eq!(d.comments[0].text, "point");
        assert!(matches!(&ps[4].runs[0], Run::Text(t) if t.comment == Some(1)));
    }

    #[test]
    fn reads_a_file_collabora_saved() {
        // Our sample, opened and saved again by Collabora Office.
        let (d, media) = read(include_bytes!("../../testdata/collabora/sample.odt")).unwrap();
        assert_eq!((d.page.header.as_str(), d.page.page_numbers), ("My recipes", true));
        assert_eq!(d.comments.len(), 1);
        assert_eq!(d.comments[0].date, "", "a comment without a date stays without one");
        assert!(d.blocks.iter().any(|b| matches!(b, Block::Table(t) if t.header && t.rows[1][1] == "2 cups")));
        assert!(d.blocks.contains(&Block::Section(Section { columns: 2, new_page: false })));
        assert!(d.blocks.contains(&Block::Contents));
        let numbered = d.blocks.iter().filter(|b| matches!(b, Block::Paragraph(p) if p.list == Some(List::Number))).count();
        assert_eq!(numbered, 3);
        let picture = d.blocks.iter().find_map(|b| match b {
            Block::Paragraph(p) => p.runs.iter().find_map(|r| if let Run::Image(i) = r { Some(i.clone()) } else { None }),
            _ => None,
        });
        assert_eq!(picture.map(|i| i.wrap), Some(Some(Side::Right)));
        assert_eq!(media.len(), 1);
    }
}
