//! Nexus Office: Write, Sheets and Present in one binary. Each runs as its own
//! application (`nexus-write`, `nexus-sheets`, `nexus-present`).

mod app;
mod cmd;
mod color;
mod container;
mod doc;
mod fmt;
mod home;
mod odf;
mod paths;
mod prefs;
mod present;
mod print;
mod recent;
mod settings_dialog;
mod settings_page;
mod sheets;
mod theme;
mod widgets;
mod window;
mod xml;
mod write;

use gtk::prelude::*;
use gtk::{gio, glib};
use std::path::PathBuf;
use std::rc::Rc;

const USAGE: &str = "Usage: nexus-office --app write|sheets|present [OPTIONS] [FILE]\n\
       nexus-write [OPTIONS] [FILE]   (also nexus-sheets, nexus-present)\n\
\n\
  FILE                  open this file\n\
  --new [TEMPLATE]      start a blank file, or one from a template\n\
  --export-pdf OUT      export FILE as a PDF to OUT and quit, without a window\n";

fn main() -> glib::ExitCode {
    nixe_core::app::init(nixe_core::app::App { slug: "nexus-office", name: "Office" });
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return glib::ExitCode::SUCCESS;
    }
    // nexus-write, or nexus-office --app write.
    let from_name =
        args.first().and_then(|a| std::path::Path::new(a).file_name()).and_then(|n| app::Kind::parse(&n.to_string_lossy()));
    let from_flag = args.iter().position(|a| a == "--app").and_then(|i| args.get(i + 1)).and_then(|v| app::Kind::parse(v));
    let Some(kind) = from_flag.or(from_name) else {
        eprint!("{USAGE}");
        return glib::ExitCode::FAILURE;
    };
    app::set(kind);

    // GTK's Vulkan renderer enumerates every GPU at startup, which wakes a
    // sleeping discrete GPU on hybrid laptops. GL renders only on the one in use.
    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: still single-threaded; nothing else reads the environment yet.
        unsafe { std::env::set_var("GSK_RENDERER", "ngl") };
    }

    // Developer aid: a different id gives a separate instance.
    let id = std::env::var("NXO_APP_ID").unwrap_or_else(|_| kind.app_id().to_string());
    let application = gtk::Application::builder().application_id(id).flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE).build();
    application.connect_command_line(|application, cl| {
        let argv: Vec<String> = cl.arguments().iter().map(|a| a.to_string_lossy().to_string()).collect();
        let value = |flag: &str| {
            argv.iter().position(|a| a == flag).and_then(|i| argv.get(i + 1)).filter(|v| !v.starts_with("--")).cloned()
        };
        let mut files: Vec<PathBuf> = Vec::new();
        let mut skip = true; // argv[0]
        for a in &argv {
            if std::mem::take(&mut skip) {
                continue;
            }
            if a == "--app" || a == "--export-pdf" {
                skip = true;
                continue;
            }
            if a == "--new" {
                skip = value("--new").is_some();
                continue;
            }
            if a.starts_with("--") {
                continue;
            }
            // Relative paths resolve against the caller's directory, and URIs work too.
            if let Some(p) = cl.create_file_for_arg(a).path() {
                files.push(p);
            }
        }
        if doc::editor().is_none() {
            doc::set_editor(make_editor());
        }

        if let Some(out) = value("--export-pdf") {
            return export_headless(files.first(), cl.create_file_for_arg(&out).path());
        }

        window::present(application);
        if let Some(path) = files.into_iter().next() {
            doc::open(path);
        } else if argv.iter().any(|a| a == "--new") {
            let template =
                value("--new").and_then(|t| doc::editor().and_then(|e| e.templates().iter().find(|x| x.id == t).map(|x| x.id)));
            doc::new_document(template);
        }
        glib::ExitCode::SUCCESS
    });
    application.connect_shutdown(|_| prefs::flush());
    application.run()
}

fn make_editor() -> Rc<dyn doc::Editor> {
    match app::kind() {
        app::Kind::Write => write::Writer::create(),
        app::Kind::Sheets => sheets::Sheets::create(),
        app::Kind::Present => present::Present::create(),
    }
}

/// `--export-pdf OUT FILE`: open FILE, write OUT, and quit.
fn export_headless(file: Option<&PathBuf>, out: Option<PathBuf>) -> glib::ExitCode {
    let (Some(file), Some(out)) = (file, out) else {
        eprintln!("nexus-office: --export-pdf needs a file to open and a PDF to write");
        return glib::ExitCode::FAILURE;
    };
    theme::install();
    let Some(ed) = doc::editor() else { return glib::ExitCode::FAILURE };
    let result = ed.open(file).and_then(|_| print::export_pdf(ed.pages().as_ref(), &out));
    match result {
        Ok(()) => {
            println!("{}", out.display());
            glib::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("nexus-office: couldn't export {}: {e:#}", file.display());
            glib::ExitCode::FAILURE
        }
    }
}

/// The editors' checks need GTK, which must stay on one thread, so they run
/// in this one test, in turn.
#[cfg(test)]
mod gtk_tests {
    #[test]
    fn editors_behave() {
        if gtk::init().is_err() {
            eprintln!("no display; skipping the editor checks");
            return;
        }
        crate::write::tests::editor_behaves();
        crate::sheets::tests::editor_behaves();
        crate::present::tests::editor_behaves();
    }
}

/// Writes one file of each Office and OpenDocument kind to `$NXO_SAMPLE_DIR`, for checking them
/// in other software: `cargo test -- --ignored write_office_samples`.
#[cfg(test)]
mod samples {
    #[test]
    #[ignore]
    fn write_office_samples() {
        let Some(dir) = std::env::var_os("NXO_SAMPLE_DIR").map(std::path::PathBuf::from) else { return };
        assert!(gtk::init().is_ok());
        std::fs::create_dir_all(&dir).unwrap();
        crate::write::write_sample(&dir.join("sample.docx"));
        crate::sheets::write_sample(&dir.join("sample.xlsx"));
        crate::present::write_sample(&dir.join("sample.pptx"));
        crate::write::write_sample(&dir.join("sample.odt"));
        crate::sheets::write_sample(&dir.join("sample.ods"));
        crate::present::write_sample(&dir.join("sample.odp"));
        crate::present::write_export_samples(&dir);
    }
}

/// Opens every Office and OpenDocument file under `$NXO_CORPUS` with the right
/// reader, and lists what failed, crashed or was slow:
/// `NXO_CORPUS=/some/files cargo test --release -- --ignored read_corpus --nocapture`.
#[cfg(test)]
mod corpus {
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    fn files(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                files(&p, out);
            } else {
                out.push(p);
            }
        }
    }

    #[test]
    #[ignore]
    fn read_corpus() {
        let Some(dir) = std::env::var_os("NXO_CORPUS").map(PathBuf::from) else { return };
        assert!(gtk::init().is_ok());
        let mut all = Vec::new();
        files(&dir, &mut all);
        all.sort();
        // Keep the noise of each crash's message down; the list below says which file.
        std::panic::set_hook(Box::new(|info| eprintln!("    panic: {info}")));
        let (mut ok, mut failed, mut crashed, mut slow) = (0, Vec::new(), Vec::new(), Vec::new());
        for path in &all {
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let read: fn(&[u8], bool) -> anyhow::Result<String> = match ext.as_str() {
                "docx" | "odt" => crate::write::corpus_read,
                "xlsx" | "ods" => crate::sheets::corpus_read,
                "pptx" | "odp" => crate::present::corpus_read,
                _ => continue,
            };
            let Ok(bytes) = std::fs::read(path) else { continue };
            let name = path.strip_prefix(&dir).unwrap_or(path).display().to_string();
            eprintln!("{name}");
            let t = Instant::now();
            let open_doc = match crate::container::package_kind(&bytes) {
                Some(kind) => kind == crate::container::Package::Open,
                None => ext.starts_with('o'),
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read(&bytes, open_doc)));
            let secs = t.elapsed().as_secs_f64();
            match result {
                Ok(Ok(summary)) => {
                    ok += 1;
                    eprintln!("    {summary} ({secs:.2}s)");
                }
                Ok(Err(e)) => failed.push(format!("{name}: {e:#}")),
                Err(_) => crashed.push(name.clone()),
            }
            if secs > 3.0 {
                slow.push(format!("{name}: {secs:.1}s"));
            }
        }
        println!("\n{ok} opened, {} refused, {} crashed, {} slow", failed.len(), crashed.len(), slow.len());
        for (title, list) in [("REFUSED", &failed), ("CRASHED", &crashed), ("SLOW", &slow)] {
            println!("\n{title}:");
            for l in list {
                println!("  {l}");
            }
        }
    }
}

/// Feeds the readers damaged copies of real files: each takes one of a file's XML
/// parts and mangles it (spans cut, doubled or swapped, numbers made extreme, junk
/// and deep nesting put in), then reads the result. Crashes and slow reads are saved
/// to `$NXO_FUZZ_OUT` to reproduce:
/// `NXO_FUZZ_SEEDS=/files NXO_FUZZ_OUT=/out NXO_FUZZ_N=5000 cargo test --release -- --ignored fuzz_readers --nocapture`.
#[cfg(test)]
mod fuzz {
    use std::io::{Cursor, Read, Write};
    use std::path::PathBuf;
    use std::time::Instant;

    /// A package's entries: name and bytes.
    type Entries = Vec<(String, Vec<u8>)>;

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            // xorshift64*
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: usize) -> usize {
            if n == 0 { 0 } else { (self.next() % n as u64) as usize }
        }
    }

    /// The archive's entries, by name.
    fn unpack(bytes: &[u8]) -> Option<Entries> {
        let mut z = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
        let mut out = Vec::new();
        for i in 0..z.len() {
            let mut f = z.by_index(i).ok()?;
            let mut b = Vec::new();
            f.read_to_end(&mut b).ok()?;
            out.push((f.name().to_string(), b));
        }
        Some(out)
    }

    fn pack(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, b) in entries {
            if z.start_file(name.as_str(), opts).is_ok() {
                let _ = z.write_all(b);
            }
        }
        z.finish().map(|c| c.into_inner()).unwrap_or_default()
    }

    fn mangle(b: &mut Vec<u8>, rng: &mut Rng) {
        let len = b.len().max(1);
        let span = |rng: &mut Rng| {
            let at = rng.below(len);
            let n = 1 + rng.below(200.min(len - at).max(1));
            (at, (at + n).min(b.len()))
        };
        match rng.below(7) {
            0 => {
                let (a, e) = span(rng);
                b.drain(a..e);
            }
            1 => {
                let (a, e) = span(rng);
                let piece: Vec<u8> = b[a..e].to_vec();
                let at = rng.below(b.len() + 1);
                b.splice(at..at, piece);
            }
            2 => {
                // A number made extreme.
                let digits: Vec<usize> = (0..b.len()).filter(|i| b[*i].is_ascii_digit()).collect();
                if let Some(&i) = digits.get(rng.below(digits.len())) {
                    let end = (i..b.len()).find(|k| !b[*k].is_ascii_digit()).unwrap_or(b.len());
                    let pick = ["0", "-1", "4294967295", "2147483648", "99999999999999999999", "1e308", "-0", "NaN", "18446744073709551616"];
                    b.splice(i..end, pick[rng.below(pick.len())].bytes());
                }
            }
            3 => {
                let at = rng.below(b.len() + 1);
                let junk: &[&[u8]] = &[b"<", b">", b"\"", b"&", b"&amp;", b"\x00", b"\xff\xfe", b"]]>", b"<!--", b"/>", b"<x>"];
                b.splice(at..at, junk[rng.below(junk.len())].iter().copied());
            }
            4 => {
                // Deep nesting, either side of the limit.
                let at = rng.below(b.len() + 1);
                let depth = [500, 999, 1001, 5000][rng.below(4)];
                let mut deep = "<a>".repeat(depth).into_bytes();
                deep.extend("</a>".repeat(depth).bytes());
                b.splice(at..at, deep);
            }
            5 => {
                let (a1, e1) = span(rng);
                let (a2, e2) = span(rng);
                if e1 <= a2 {
                    let second: Vec<u8> = b[a2..e2].to_vec();
                    let first: Vec<u8> = b[a1..e1].to_vec();
                    b.splice(a2..e2, first);
                    b.splice(a1..e1, second);
                }
            }
            _ => {
                let at = rng.below(b.len() + 1);
                b.truncate(at);
            }
        }
    }

    #[test]
    #[ignore]
    fn fuzz_readers() {
        let (Some(seeds), Some(out)) = (std::env::var_os("NXO_FUZZ_SEEDS"), std::env::var_os("NXO_FUZZ_OUT")) else { return };
        let (seeds, out) = (PathBuf::from(seeds), PathBuf::from(out));
        let n: usize = std::env::var("NXO_FUZZ_N").ok().and_then(|v| v.parse().ok()).unwrap_or(1000);
        assert!(gtk::init().is_ok());
        std::fs::create_dir_all(&out).unwrap();
        let mut files: Vec<(PathBuf, Entries)> = std::fs::read_dir(&seeds)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter_map(|p| {
                let e = unpack(&std::fs::read(&p).ok()?)?;
                Some((p, e))
            })
            .collect();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        assert!(!files.is_empty(), "no seed files");
        std::panic::set_hook(Box::new(|_| {}));
        let (mut crashes, mut slow) = (0, 0);
        for case in 0..n {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (case as u64 + 1).wrapping_mul(0xBF58_476D_1CE4_E5B9));
            let (path, entries) = &files[rng.below(files.len())];
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let read: fn(&[u8], bool) -> anyhow::Result<String> = match ext.as_str() {
                "docx" | "odt" => crate::write::corpus_read,
                "xlsx" | "ods" => crate::sheets::corpus_read,
                "pptx" | "odp" => crate::present::corpus_read,
                _ => continue,
            };
            let mut entries = entries.clone();
            let xml: Vec<usize> = (0..entries.len()).filter(|i| entries[*i].0.ends_with(".xml") || entries[*i].0.ends_with(".rels")).collect();
            let Some(&pick) = xml.get(rng.below(xml.len())) else { continue };
            for _ in 0..1 + rng.below(3) {
                mangle(&mut entries[pick].1, &mut rng);
            }
            let bytes = pack(&entries);
            let t = Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read(&bytes, ext.starts_with('o'))));
            let secs = t.elapsed().as_secs_f64();
            let name = format!("case-{case}-{}.{ext}", entries[pick].0.replace('/', "_"));
            if result.is_err() {
                crashes += 1;
                println!("CRASH {name} (from {})", path.display());
                let _ = std::fs::write(out.join(&name), &bytes);
            } else if secs > 2.0 {
                slow += 1;
                println!("SLOW {secs:.1}s {name} (from {})", path.display());
                let _ = std::fs::write(out.join(&name), &bytes);
            }
        }
        println!("{n} cases: {crashes} crashed, {slow} slow");
    }
}
