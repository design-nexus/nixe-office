//! Which of the three apps this process is: Write, Sheets or Present. One binary
//! serves all three; each runs as its own application with its own window.

use std::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Write,
    Sheets,
    Present,
}

thread_local! {
    static KIND: Cell<Kind> = const { Cell::new(Kind::Write) };
}

pub fn set(kind: Kind) {
    KIND.with(|k| k.set(kind));
}

pub fn kind() -> Kind {
    KIND.with(|k| k.get())
}

impl Kind {
    pub fn parse(id: &str) -> Option<Kind> {
        match id.trim_start_matches("nixe-").trim_start_matches("nexus-") {
            "write" => Some(Kind::Write),
            "sheets" => Some(Kind::Sheets),
            "present" => Some(Kind::Present),
            _ => None,
        }
    }

    /// `write`, `sheets` or `present`: used in file and folder names.
    pub fn id(self) -> &'static str {
        match self {
            Kind::Write => "write",
            Kind::Sheets => "sheets",
            Kind::Present => "present",
        }
    }

    /// The first part of the top bar's path.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Write => "Write",
            Kind::Sheets => "Sheets",
            Kind::Present => "Present",
        }
    }

    pub fn full_name(self) -> &'static str {
        match self {
            Kind::Write => "Write",
            Kind::Sheets => "Sheets",
            Kind::Present => "Present",
        }
    }

    pub fn app_id(self) -> &'static str {
        match self {
            Kind::Write => "io.github.design_nexus.nixe.Write",
            Kind::Sheets => "io.github.design_nexus.nixe.Sheets",
            Kind::Present => "io.github.design_nexus.nixe.Present",
        }
    }

    /// "document", "spreadsheet", "presentation".
    pub fn noun(self) -> &'static str {
        match self {
            Kind::Write => "document",
            Kind::Sheets => "spreadsheet",
            Kind::Present => "presentation",
        }
    }

    /// The native file extension, without the dot.
    pub fn native_ext(self) -> &'static str {
        match self {
            Kind::Write => "nwrite",
            Kind::Sheets => "nsheets",
            Kind::Present => "npresent",
        }
    }

    /// Every extension this app opens.
    pub fn opens(self) -> &'static [&'static str] {
        match self {
            Kind::Write => &["nwrite", "docx", "odt", "md", "markdown", "txt"],
            Kind::Sheets => &["nsheets", "xlsx", "ods", "csv"],
            Kind::Present => &["npresent", "pptx", "odp"],
        }
    }

    /// The sidebar icon for the open file.
    pub fn icon(self) -> &'static str {
        match self {
            Kind::Write => "nxo-file-text-symbolic",
            Kind::Sheets => "nxo-file-spreadsheet-symbolic",
            Kind::Present => "nxo-presentation-symbolic",
        }
    }

    /// (name, extension) for each format the Save As dialog offers, native first.
    pub fn save_formats(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Kind::Write => &[
                ("Write document", "nwrite"),
                ("Word document", "docx"),
                ("OpenDocument text", "odt"),
                ("Markdown", "md"),
                ("Plain text", "txt"),
            ],
            Kind::Sheets => {
                &[
                    ("Sheets workbook", "nsheets"),
                    ("Excel workbook", "xlsx"),
                    ("OpenDocument spreadsheet", "ods"),
                    ("CSV (one sheet, values only)", "csv"),
                ]
            }
            Kind::Present => {
                &[("Present slides", "npresent"), ("PowerPoint slides", "pptx"), ("OpenDocument presentation", "odp")]
            }
        }
    }
}

/// True when `path`'s extension is one this app opens.
pub fn opens(path: &std::path::Path) -> bool {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    kind().opens().contains(&ext.as_str())
}
