//! The native file format shared by all three apps: a ZIP holding
//!
//! - `mimetype` (stored first and uncompressed, like ODF, so tools can sniff it),
//! - `manifest.json` (`{"format": "nexus-write", "version": 3}`),
//! - `content.json` (the app's own document model),
//! - `media/…` (pictures, referenced from the content by name).

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

/// Version 3: the first Rust release. (Qt-era `.nwrite` files said 1 or 2 and held HTML.)
pub const VERSION: u32 = 3;
/// No single entry may be larger than this when reading.
const MAX_ENTRY: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub generator: String,
}

pub struct Container {
    pub content: Vec<u8>,
    pub media: BTreeMap<String, Vec<u8>>,
}

/// `format` is `nexus-write`, `nexus-sheets` or `nexus-present`.
pub fn write(format: &str, content: &[u8], media: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>> {
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("mimetype", stored)?;
    zip.write_all(format!("application/x-{format}").as_bytes())?;
    let manifest =
        Manifest { format: format.into(), version: VERSION, generator: format!("Nexus Office {}", env!("CARGO_PKG_VERSION")) };
    zip.start_file("manifest.json", deflated)?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    zip.start_file("content.json", deflated)?;
    zip.write_all(content)?;
    for (name, bytes) in media {
        // Pictures are already compressed.
        zip.start_file(format!("media/{name}"), stored)?;
        zip.write_all(bytes)?;
    }
    Ok(zip.finish()?.into_inner())
}

/// Where an entry is in an archive, by its name: exactly, or else as some writers
/// spell it (`xl\workbook.xml`, `[content_types].xml`).
pub fn entry_index<R: std::io::Read + std::io::Seek>(zip: &zip::ZipArchive<R>, name: &str) -> Option<usize> {
    let name = name.trim_start_matches('/');
    if let Some(i) = zip.index_for_name(name) {
        return Some(i);
    }
    let norm = |n: &str| n.trim_start_matches(['/', '\\']).replace('\\', "/").to_lowercase();
    let want = norm(name);
    let found = zip.file_names().find(|n| norm(n) == want)?.to_string();
    zip.index_for_name(&found)
}

/// Which kind of package some bytes are, whatever the file is called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Package {
    /// Word, Excel or PowerPoint.
    Office,
    /// OpenDocument.
    Open,
}

pub fn package_kind(bytes: &[u8]) -> Option<Package> {
    let zip = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    if zip.file_names().any(|n| n.eq_ignore_ascii_case("[Content_Types].xml")) {
        return Some(Package::Office);
    }
    zip.file_names().any(|n| n == "mimetype" || n == "META-INF/manifest.xml").then_some(Package::Open)
}

/// An Office part saved as "Strict Open XML", with its namespaces (and relationship
/// types) put in the usual form: `purl.oclc.org/ooxml/<area>/<name>` is
/// `schemas.openxmlformats.org/<area>/2006/<name>`.
pub fn transitional(xml: String) -> String {
    const STRICT: &str = "http://purl.oclc.org/ooxml/";
    if !xml.contains(STRICT) {
        return xml;
    }
    let mut out = String::with_capacity(xml.len() + 64);
    let mut rest = xml.as_str();
    while let Some(i) = rest.find(STRICT) {
        out.push_str(&rest[..i]);
        let after = &rest[i + STRICT.len()..];
        match after.find('/') {
            Some(slash) => {
                out.push_str("http://schemas.openxmlformats.org/");
                out.push_str(&after[..slash]);
                out.push_str("/2006");
                rest = &after[slash..];
            }
            None => {
                out.push_str(STRICT);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Why bytes that should be an Office or OpenDocument file aren't one, in words.
pub fn not_a_package(bytes: &[u8], what: &str) -> String {
    if bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0]) {
        // The old binary container: Office 97–2003 files, and password-protected ones.
        format!("it's protected with a password, or an older {what} (from Office 97–2003), which Nexus can't open")
    } else {
        let article = if what.starts_with(['A', 'E', 'I', 'O', 'U']) { "an" } else { "a" };
        format!("it isn't {article} {what}, or it's damaged")
    }
}

/// True when the bytes look like a ZIP (native v2 files, and old `.nwrite` files).
pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

pub fn read_entry(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<Vec<u8>>> {
    let Some(i) = entry_index(archive, name) else { return Ok(None) };
    let mut file = archive.by_index(i)?;
    if file.size() > MAX_ENTRY {
        bail!("{name} is too large");
    }
    Ok(Some(read_capped(&mut file, MAX_ENTRY).with_context(|| format!("{name} is too large or damaged"))?))
}

/// Read at most `max` bytes, failing past that. A ZIP entry's stated size can't
/// be trusted, so readers of untrusted archives go through this.
pub fn read_capped(r: impl Read, max: u64) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    r.take(max + 1).read_to_end(&mut out)?;
    if out.len() as u64 > max {
        return Err(std::io::Error::other("too large"));
    }
    Ok(out)
}

pub fn read(bytes: &[u8], format: &str) -> Result<Container> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("not a Nexus file")?;
    let manifest: Manifest =
        serde_json::from_slice(&read_entry(&mut archive, "manifest.json")?.context("the file has no manifest")?)
            .context("the manifest is damaged")?;
    if manifest.format != format {
        bail!("this is a {} file", manifest.format.replace("nexus-", "Nexus "));
    }
    if manifest.version > VERSION {
        bail!("it was made by a newer version of Nexus Office");
    }
    let content = read_entry(&mut archive, "content.json")?.context("the file has no content")?;
    let mut media = BTreeMap::new();
    let names: Vec<String> =
        archive.file_names().filter(|n| n.starts_with("media/") && !n.ends_with('/')).map(String::from).collect();
    for name in names {
        if let Some(b) = read_entry(&mut archive, &name)? {
            media.insert(name.trim_start_matches("media/").to_string(), b);
        }
    }
    Ok(Container { content, media })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let mut media = BTreeMap::new();
        media.insert("a.png".to_string(), vec![1, 2, 3]);
        let bytes = write("nexus-write", br#"{"x":1}"#, &media).unwrap();
        assert!(is_zip(&bytes));
        // File managers find the type at byte 38, as in ODF (packaging/nexus-office.xml).
        assert_eq!(&bytes[38..38 + 25], b"application/x-nexus-write");
        let c = read(&bytes, "nexus-write").unwrap();
        assert_eq!(c.content, br#"{"x":1}"#);
        assert_eq!(c.media["a.png"], vec![1, 2, 3]);
        assert!(read(&bytes, "nexus-sheets").is_err());
    }

    #[test]
    fn strict_namespaces_and_package_kinds() {
        let strict = r#"<p:sld xmlns:p="http://purl.oclc.org/ooxml/presentationml/main" xmlns:r="http://purl.oclc.org/ooxml/officeDocument/relationships"><r:x Type="http://purl.oclc.org/ooxml/officeDocument/relationships/slide"/></p:sld>"#;
        assert_eq!(
            transitional(strict.to_string()),
            r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><r:x Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide"/></p:sld>"#
        );
        let odt = crate::odf::package(crate::odf::ODT, "<x/>", "<x/>", &[]).unwrap();
        assert_eq!(package_kind(&odt), Some(Package::Open));
        assert_eq!(package_kind(b"not a zip"), None);
        assert!(not_a_package(&[0xd0, 0xcf, 0x11, 0xe0, 0], "Excel workbook").contains("password"));
        assert_eq!(not_a_package(b"x", "Excel workbook"), "it isn't an Excel workbook, or it's damaged");
    }
}

