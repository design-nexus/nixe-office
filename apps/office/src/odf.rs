//! What the three OpenDocument formats (.odt, .ods, .odp) share: the package (a ZIP
//! with `mimetype` first and stored, a manifest, `content.xml`, `styles.xml`,
//! `Pictures/`), lengths with units, and named styles that inherit from their parents.

use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Write as _};

pub const OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
pub const STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
pub const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
pub const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
pub const DRAW: &str = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0";
pub const FO: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
pub const XLINK: &str = "http://www.w3.org/1999/xlink";
pub const SVG: &str = "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0";
pub const PRESENTATION: &str = "urn:oasis:names:tc:opendocument:xmlns:presentation:1.0";
pub const NUMBER: &str = "urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0";
pub const DC: &str = "http://purl.org/dc/elements/1.1/";

/// The namespace declarations every part we write starts with.
pub const NAMESPACES: &str = concat!(
    r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
    r#"xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" "#,
    r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" "#,
    r#"xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" "#,
    r#"xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" "#,
    r#"xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" "#,
    r#"xmlns:xlink="http://www.w3.org/1999/xlink" "#,
    r#"xmlns:dc="http://purl.org/dc/elements/1.1/" "#,
    r#"xmlns:number="urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0" "#,
    r#"xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" "#,
    r#"xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" "#,
    r#"xmlns:of="urn:oasis:names:tc:opendocument:xmlns:of:1.2" "#,
    r#"office:version="1.3""#
);

pub const ODT: &str = "application/vnd.oasis.opendocument.text";
pub const ODS: &str = "application/vnd.oasis.opendocument.spreadsheet";
pub const ODP: &str = "application/vnd.oasis.opendocument.presentation";

/// Pictures larger than this are left out when reading.
const MAX_PART: u64 = 64 * 1024 * 1024;

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Characters XML can't hold.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// Points as ODF writes lengths.
pub fn pt(v: f64) -> String {
    format!("{}pt", (v * 100.0).round() / 100.0)
}

/// A length (`2.54cm`, `1in`, `12pt`, `10mm`, `16px`) in points.
pub fn length(s: &str) -> Option<f64> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let v: f64 = num.trim().parse().ok()?;
    let k = match unit {
        "pt" | "" => 1.0,
        "in" => 72.0,
        "cm" => 72.0 / 2.54,
        "mm" => 72.0 / 25.4,
        "pc" => 12.0,
        "px" => 0.75,
        _ => return None,
    };
    Some(v * k).filter(|v| v.is_finite())
}

/// `#RRGGBB` → `#rrggbb`, or None for anything else (`transparent`).
pub fn color(v: &str) -> Option<String> {
    let v = v.trim();
    let hex = v.strip_prefix('#')?;
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| format!("#{}", hex.to_lowercase()))
}

/// A file to put in the package beside the XML parts.
pub struct Extra<'a> {
    pub path: String,
    pub bytes: &'a [u8],
    pub media_type: &'a str,
}

/// Builds the package. `content` and `styles` are whole XML documents.
pub fn package(mime: &str, content: &str, styles: &str, extras: &[Extra]) -> Result<Vec<u8>> {
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("mimetype", stored)?;
    zip.write_all(mime.as_bytes())?;
    let mut manifest = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3"><manifest:file-entry manifest:full-path="/" manifest:version="1.3" manifest:media-type="{mime}"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>"#
    );
    for e in extras {
        manifest.push_str(&format!(
            r#"<manifest:file-entry manifest:full-path="{}" manifest:media-type="{}"/>"#,
            esc(&e.path),
            e.media_type
        ));
    }
    manifest.push_str("</manifest:manifest>");
    let meta = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-meta {NAMESPACES} xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0"><office:meta><meta:generator>Nexus Office {}</meta:generator></office:meta></office:document-meta>"#,
        env!("CARGO_PKG_VERSION")
    );
    for (name, body) in
        [("META-INF/manifest.xml", manifest.as_str()), ("content.xml", content), ("styles.xml", styles), ("meta.xml", &meta)]
    {
        zip.start_file(name, deflated)?;
        zip.write_all(body.as_bytes())?;
    }
    for e in extras {
        zip.start_file(e.path.as_str(), stored)?;
        zip.write_all(e.bytes)?;
    }
    Ok(zip.finish()?.into_inner())
}

/// An opened package.
pub struct Package<'b> {
    zip: zip::ZipArchive<Cursor<&'b [u8]>>,
}

impl<'b> Package<'b> {
    /// Opens the package, checking its `mimetype` (when it has one) names `mime`.
    pub fn open(bytes: &'b [u8], mime: &str, what: &str) -> Result<Package<'b>> {
        let zip = zip::ZipArchive::new(Cursor::new(bytes)).with_context(|| format!("it isn't an OpenDocument {what}"))?;
        let mut p = Package { zip };
        if let Some(m) = p.file("mimetype") {
            let m = String::from_utf8_lossy(&m);
            if !m.trim().starts_with(mime) {
                bail!("it isn't an OpenDocument {what}");
            }
        }
        Ok(p)
    }

    pub fn file(&mut self, path: &str) -> Option<Vec<u8>> {
        let f = self.zip.by_name(path.trim_start_matches("./").trim_start_matches('/')).ok()?;
        if f.size() > MAX_PART {
            return None;
        }
        crate::container::read_capped(f, MAX_PART).ok()
    }

    pub fn text(&mut self, path: &str) -> Option<String> {
        self.file(path).map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    pub fn content(&mut self) -> Result<String> {
        self.text("content.xml").context("the file has no content.xml")
    }
}

/// The text inside an element, as ODF lays it out: `text:s` is spaces, `text:tab` a
/// tab, `text:line-break` a new line, and nested spans and links give their text.
pub fn plain_text(n: &roxmltree::Node) -> String {
    let mut out = String::new();
    collect_text(n, &mut out);
    out
}

fn collect_text(n: &roxmltree::Node, out: &mut String) {
    for c in n.children() {
        if c.is_text() {
            out.push_str(c.text().unwrap_or(""));
        } else if c.tag_name().namespace() == Some(TEXT) {
            match c.tag_name().name() {
                "s" => {
                    let k: usize = attr(&c, TEXT, "c").and_then(|v| v.parse().ok()).unwrap_or(1);
                    out.extend(std::iter::repeat_n(' ', k.min(1000)));
                }
                "tab" => out.push('\t'),
                "line-break" => out.push('\n'),
                "note" | "bookmark-ref" => {}
                _ => collect_text(&c, out),
            }
        } else if c.tag_name().namespace() == Some(OFFICE) && c.tag_name().name() == "annotation" {
        } else {
            collect_text(&c, out);
        }
    }
}

/// A paragraph's lines (`text:p` and `text:h` children), joined by new lines.
pub fn paragraphs_text(n: &roxmltree::Node) -> String {
    n.descendants()
        .filter(|c| c.has_tag_name((TEXT, "p")) || c.has_tag_name((TEXT, "h")))
        .map(|p| plain_text(&p))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn attr<'a>(n: &roxmltree::Node<'a, '_>, ns: &str, local: &str) -> Option<&'a str> {
    n.attribute((ns, local))
}

/// Text written into XML with ODF's space rules: runs of spaces become `text:s`,
/// tabs `text:tab`, new lines `text:line-break`.
pub fn text_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut spaces = 0usize;
    let flush = |out: &mut String, spaces: &mut usize| {
        if *spaces > 0 {
            // The first space stays a space; the rest would collapse.
            out.push(' ');
            if *spaces > 1 {
                out.push_str(&format!(r#"<text:s text:c="{}"/>"#, *spaces - 1));
            }
            *spaces = 0;
        }
    };
    for (i, c) in s.chars().enumerate() {
        match c {
            ' ' if i == 0 => out.push_str("<text:s/>"),
            ' ' => spaces += 1,
            '\t' => {
                flush(&mut out, &mut spaces);
                out.push_str("<text:tab/>");
            }
            '\n' | '\u{2028}' => {
                flush(&mut out, &mut spaces);
                out.push_str("<text:line-break/>");
            }
            c => {
                flush(&mut out, &mut spaces);
                out.push_str(&esc(&c.to_string()));
            }
        }
    }
    flush(&mut out, &mut spaces);
    out
}

/// One named style: its family, parent, and properties keyed `"<kind>/<name>"`,
/// where kind is the properties element (`text`, `paragraph`, `cell`, `column`,
/// `row`, `graphic`, `page`, `layout`) and name the attribute's local name.
#[derive(Debug, Default, Clone)]
struct Style {
    parent: Option<String>,
    props: HashMap<String, String>,
    /// The number style a cell style uses.
    data_style: Option<String>,
    /// A paragraph style's list style.
    list: Option<String>,
}

/// Every style in `styles.xml` and `content.xml`, looked up through parents.
#[derive(Debug, Default)]
pub struct Styles {
    named: HashMap<(String, String), Style>,
    defaults: HashMap<String, Style>,
    /// Font face name → family.
    fonts: HashMap<String, String>,
    /// List style name → per level, numbered (true) or bulleted.
    pub lists: HashMap<String, Vec<bool>>,
    /// Master page name → page layout name.
    pub masters: HashMap<String, String>,
}

const PROPERTY_KINDS: [(&str, &str); 9] = [
    ("section-properties", "section"),
    ("text-properties", "text"),
    ("paragraph-properties", "paragraph"),
    ("table-cell-properties", "cell"),
    ("table-column-properties", "column"),
    ("table-row-properties", "row"),
    ("graphic-properties", "graphic"),
    ("drawing-page-properties", "page"),
    ("page-layout-properties", "layout"),
];

impl Styles {
    /// Reads the styles from each part's XML, later parts overriding earlier.
    pub fn read(parts: &[&roxmltree::Document]) -> Styles {
        let mut s = Styles::default();
        for doc in parts {
            for n in doc.descendants() {
                if n.tag_name().namespace() == Some(STYLE) && n.tag_name().name() == "font-face" {
                    if let (Some(name), Some(family)) = (attr(&n, STYLE, "name"), attr(&n, SVG, "font-family")) {
                        s.fonts.insert(name.into(), family.trim_matches(['\'', '"']).to_string());
                    }
                    continue;
                }
                match (n.tag_name().namespace(), n.tag_name().name()) {
                    (Some(STYLE), "style") | (Some(STYLE), "page-layout") | (Some(STYLE), "default-style") => {
                        let family = attr(&n, STYLE, "family").unwrap_or(if n.tag_name().name() == "page-layout" {
                            "page-layout"
                        } else {
                            ""
                        });
                        let mut st = Style {
                            parent: attr(&n, STYLE, "parent-style-name").map(String::from),
                            data_style: attr(&n, STYLE, "data-style-name").map(String::from),
                            list: attr(&n, STYLE, "list-style-name").map(String::from),
                            ..Default::default()
                        };
                        if let Some(m) = attr(&n, STYLE, "master-page-name") {
                            st.props.insert("master-page-name".into(), m.into());
                        }
                        for c in n.children().filter(|c| c.is_element()) {
                            let Some((_, kind)) = PROPERTY_KINDS.iter().find(|(e, _)| c.tag_name().name() == *e) else {
                                continue;
                            };
                            for a in c.attributes() {
                                st.props.insert(format!("{kind}/{}", a.name()), a.value().to_string());
                            }
                            // Columns sit in the properties as a child element.
                            if let Some(cols) = c.children().find(|x| x.has_tag_name((STYLE, "columns"))) {
                                if let Some(k) = attr(&cols, FO, "column-count") {
                                    st.props.insert(format!("{kind}/column-count"), k.into());
                                }
                                if let Some(g) = attr(&cols, FO, "column-gap") {
                                    st.props.insert(format!("{kind}/column-gap"), g.into());
                                }
                            }
                        }
                        if n.tag_name().name() == "default-style" {
                            s.defaults.insert(family.into(), st);
                        } else if let Some(name) = attr(&n, STYLE, "name") {
                            s.named.insert((family.into(), name.into()), st);
                        }
                    }
                    (Some(TEXT), "list-style") => {
                        let Some(name) = attr(&n, STYLE, "name") else { continue };
                        let mut levels = vec![false; 10];
                        for l in n.children().filter(|c| c.is_element()) {
                            let level: usize = attr(&l, TEXT, "level").and_then(|v| v.parse().ok()).unwrap_or(1);
                            if (1..=10).contains(&level) {
                                levels[level - 1] = l.tag_name().name() == "list-level-style-number";
                            }
                        }
                        s.lists.insert(name.into(), levels);
                    }
                    (Some(STYLE), "master-page") => {
                        if let (Some(name), Some(layout)) = (attr(&n, STYLE, "name"), attr(&n, STYLE, "page-layout-name")) {
                            s.masters.insert(name.into(), layout.into());
                        }
                    }
                    _ => {}
                }
            }
        }
        s
    }

    /// A property of a style, looking through its parents and the family's default.
    pub fn get(&self, family: &str, name: &str, key: &str) -> Option<&str> {
        let mut at = Some(name.to_string());
        let mut hops = 0;
        while let Some(n) = at {
            let Some(st) = self.named.get(&(family.to_string(), n)) else { break };
            if let Some(v) = st.props.get(key) {
                return Some(v);
            }
            at = st.parent.clone();
            hops += 1;
            if hops > 32 {
                break;
            }
        }
        self.defaults.get(family).and_then(|d| d.props.get(key)).map(String::as_str)
    }

    /// A property set on the style itself, not inherited.
    pub fn own(&self, family: &str, name: &str, key: &str) -> Option<&str> {
        self.named.get(&(family.to_string(), name.to_string())).and_then(|s| s.props.get(key)).map(String::as_str)
    }

    /// The family a font face name stands for.
    pub fn face(&self, name: &str) -> String {
        self.fonts.get(name).cloned().unwrap_or_else(|| name.to_string())
    }

    /// The style's own name and its parents' names, nearest first.
    pub fn lineage(&self, family: &str, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut at = Some(name.to_string());
        while let Some(n) = at {
            if out.contains(&n) || out.len() > 32 {
                break;
            }
            at = self.named.get(&(family.to_string(), n.clone())).and_then(|s| s.parent.clone());
            out.push(n);
        }
        out
    }

    pub fn data_style(&self, family: &str, name: &str) -> Option<&str> {
        self.named.get(&(family.to_string(), name.to_string())).and_then(|s| s.data_style.as_deref())
    }

    pub fn list_style(&self, family: &str, name: &str) -> Option<&str> {
        for n in self.lineage(family, name) {
            if let Some(l) = self.named.get(&(family.to_string(), n)).and_then(|s| s.list.as_deref()) {
                return Some(l);
            }
        }
        None
    }

    /// The font family a text style names (through `style:font-name` or `fo:font-family`).
    pub fn font(&self, family: &str, name: &str) -> Option<String> {
        if let Some(f) = self.get(family, name, "text/font-name") {
            return Some(self.fonts.get(f).cloned().unwrap_or_else(|| f.to_string()));
        }
        self.get(family, name, "text/font-family").map(|f| f.trim_matches(['\'', '"']).to_string())
    }
}

/// The pictures a package's XML refers to, read once each: path → bytes as PNG.
pub fn picture(pkg: &mut Package, href: &str, cache: &mut BTreeMap<String, Option<Vec<u8>>>) -> Option<Vec<u8>> {
    if href.contains("://") {
        return None;
    }
    cache
        .entry(href.to_string())
        .or_insert_with(|| pkg.file(href).and_then(|b| as_png(&b)))
        .clone()
}

/// A picture as PNG (the renderers and the native files use PNG).
pub fn as_png(bytes: &[u8]) -> Option<Vec<u8>> {
    use gtk::prelude::*;
    if bytes.starts_with(b"\x89PNG") {
        return Some(bytes.to_vec());
    }
    let t = gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from(bytes)).ok()?;
    Some(t.save_to_png_bytes().to_vec())
}

/// A PNG's width and height in pixels.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG") {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_and_text() {
        assert_eq!(length("1in"), Some(72.0));
        assert!((length("2.54cm").unwrap() - 72.0).abs() < 1e-9);
        assert_eq!(length("12pt"), Some(12.0));
        assert_eq!(length("auto"), None);
        assert_eq!(text_xml("a  b\tc\nd"), r#"a <text:s text:c="1"/>b<text:tab/>c<text:line-break/>d"#);
        let xml = format!(r#"<text:p xmlns:text="{TEXT}">a <text:s text:c="2"/>b<text:tab/>c<text:line-break/>d</text:p>"#);
        let doc = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(plain_text(&doc.root_element()), "a   b\tc\nd");
    }
}
