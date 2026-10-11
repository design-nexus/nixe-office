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
