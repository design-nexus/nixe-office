//! Write's tools on the control interface (`nixe_core::control`), for Nixe Assistant:
//! start, open, read, fill and save documents directly instead of typing into the window.
//! Nothing here throws away unsaved work: starting or opening another document is refused
//! while the open one has unsaved changes.

use super::model::Style;
use crate::{app, doc, print};
use nixe_core::control::{self, text};
use serde_json::{json, Value};

pub fn tools() -> Value {
    json!([
        {"name": "write.new_document", "description": "Start a new, empty Write document.", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "write.open", "description": "Open a document (inside the home folder) in Write.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
        {"name": "write.read", "description": "Read the open Write document: title, file and text (as Markdown).", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "write.insert_text", "description": "Insert text into the Write document at the cursor or the end; each line is a paragraph in the given style.", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}, "style": {"enum": ["body", "title", "heading1", "heading2", "heading3", "quote"]}, "at": {"enum": ["cursor", "end"]}}, "required": ["text"]}},
        {"name": "write.save", "description": "Save the Write document; give a path (inside the home folder; .nwrite, .odt, .docx, .md…) for a new document or to save under a new name.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "overwrite": {"type": "boolean"}}}},
        {"name": "write.export_pdf", "description": "Export the Write document as a PDF (a path inside the home folder ending in .pdf).", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "overwrite": {"type": "boolean"}}, "required": ["path"]}},
    ])
}

fn state() -> Value {
    json!({"title": doc::title(), "path": doc::path(), "unsaved": doc::dirty()})
}

fn no_unsaved_work() -> Result<(), String> {
    if doc::is_open() && doc::dirty() {
        return Err("the open document has unsaved changes; save it first".into());
    }
    Ok(())
}

pub fn handle(name: &str, args: Value) -> Result<Value, String> {
    match name {
        "write.new_document" => {
            no_unsaved_work()?;
            doc::new_document(None);
            Ok(json!({"document": state()}))
        }
        "write.open" => {
            no_unsaved_work()?;
            let path = control::home_path(&text(&args, "path")?)?;
            if !path.is_file() {
                return Err("no such file".into());
            }
            if !app::opens(&path) {
                return Err("Write doesn't open that kind of file".into());
            }
            doc::open(path.clone());
            if doc::path().as_deref() != Some(path.as_path()) {
                return Err("couldn't open it".into());
            }
            Ok(json!({"document": state()}))
        }
        "write.read" => {
            if !doc::is_open() {
                return Err("no document is open".into());
            }
            Ok(json!({"document": state(), "text": super::control_text().unwrap_or_default(), "untrusted_content": true}))
        }
        "write.insert_text" => {
            let insert = text(&args, "text")?;
            if insert.is_empty() || insert.len() > 100_000 {
                return Err("text must be 1 to 100000 bytes".into());
            }
            let style = match args["style"].as_str().unwrap_or("body") {
                "body" => Style::Normal,
                "title" => Style::Title,
                "heading1" => Style::H1,
                "heading2" => Style::H2,
                "heading3" => Style::H3,
                "quote" => Style::Quote,
                other => return Err(format!("unknown style {other}")),
            };
            if !doc::is_open() {
                doc::new_document(None);
            }
            super::control_insert(&insert, style, args["at"] == "end")?;
            Ok(json!({"document": state(), "inserted": insert.chars().count()}))
        }
        "write.save" => {
            if !doc::is_open() {
                return Err("no document is open".into());
            }
            let path = match args["path"].as_str() {
                Some(_) => control::save_target(&args)?,
                None => doc::path().ok_or("this document has no file yet; give a path")?,
            };
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !app::kind().save_formats().iter().any(|(_, x)| *x == ext) {
                let known: Vec<_> = app::kind().save_formats().iter().map(|(_, x)| format!(".{x}")).collect();
                return Err(format!("Write saves {}", known.join(", ")));
            }
            if !doc::save_to(&path) {
                return Err("couldn't save".into());
            }
            Ok(json!({"document": state(), "saved": path}))
        }
        "write.export_pdf" => {
            if !doc::is_open() {
                return Err("no document is open".into());
            }
            let path = control::save_target(&args)?;
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                return Err("the path must end in .pdf".into());
            }
            let ed = doc::editor().ok_or("Write isn't ready")?;
            print::export_pdf(ed.pages().as_ref(), &path).map_err(|e| format!("{e:#}"))?;
            Ok(json!({"document": state(), "exported": path}))
        }
        _ => Err(format!("unknown tool {name}")),
    }
}
