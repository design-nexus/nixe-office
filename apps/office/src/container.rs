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

/// True when the bytes look like a ZIP (native v2 files, and old `.nwrite` files).
pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

pub fn read_entry(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<Vec<u8>>> {
    let mut file = match archive.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if file.size() > MAX_ENTRY {
        bail!("{name} is too large");
    }
    let mut out = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut out)?;
    Ok(Some(out))
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
}
