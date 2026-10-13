//! Paper sizes, and putting pages on paper: PDF export and printing share one
//! renderer per app, so what prints is exactly what exports.

use crate::{prefs, window};
use anyhow::{Context, Result};
use gtk::prelude::*;
use std::path::Path;
use std::rc::Rc;

/// A page size in points (1/72 inch).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paper {
    pub width: f64,
    pub height: f64,
}

pub const LETTER: Paper = Paper { width: 612.0, height: 792.0 };
pub const A4: Paper = Paper { width: 595.28, height: 841.89 };
pub const LEGAL: Paper = Paper { width: 612.0, height: 1008.0 };
pub const A5: Paper = Paper { width: 419.53, height: 595.28 };

/// The papers offered: (id, name, size), portrait.
pub const PAPERS: [(&str, &str, Paper); 4] =
    [("letter", "Letter", LETTER), ("a4", "A4", A4), ("legal", "Legal", LEGAL), ("a5", "A5", A5)];

impl Paper {
    pub fn from_id(id: &str) -> Paper {
        PAPERS.iter().find(|p| p.0 == id).map_or(LETTER, |p| p.2)
    }

    /// The paper this size is, either way round; Letter when it's none of them.
    pub fn id(self) -> &'static str {
        Paper::closest(self.width, self.height).unwrap_or("letter")
    }

    /// The paper within a few points of `w` × `h`, either way round.
    pub fn closest(w: f64, h: f64) -> Option<&'static str> {
        let (short, long) = (w.min(h), w.max(h));
        PAPERS.iter().find(|p| (p.2.width - short).abs() < 6.0 && (p.2.height - long).abs() < 6.0).map(|p| p.0)
    }

    pub fn name(self) -> &'static str {
        let id = self.id();
        PAPERS.iter().find(|p| p.0 == id).map_or("Letter", |p| p.1)
    }

    /// Turned on its side.
    pub fn landscape(self) -> Paper {
        Paper { width: self.width.max(self.height), height: self.width.min(self.height) }
    }

    /// Paper measured in centimetres rather than inches.
    pub fn metric(id: &str) -> bool {
        id.starts_with('a')
    }

    /// The paper chosen in Settings, for new documents.
    pub fn preferred() -> Paper {
        Paper::from_id(&prefs::get().paper)
    }
}

/// A Pango context for laying out text in points (72 dpi), with no hinting, so
/// pages measure the same on screen, in the PDF and on paper.
pub fn text_context() -> gtk::pango::Context {
    let map = pangocairo::FontMap::default();
    use gtk::pango::prelude::*;
    let ctx = map.create_context();
    pangocairo::functions::context_set_resolution(&ctx, 72.0);
    let mut opts = cairo::FontOptions::new().expect("cairo font options");
    opts.set_hint_metrics(cairo::HintMetrics::Off);
    opts.set_hint_style(cairo::HintStyle::None);
    pangocairo::functions::context_set_font_options(&ctx, Some(&opts));
    ctx
}

/// A document laid out as pages.
pub trait Pages {
    fn count(&self) -> usize;
    /// Every page's size, in points.
    fn size(&self) -> (f64, f64);
    /// Draw page `index` with its top-left corner at the origin, in points.
    fn draw(&self, cr: &cairo::Context, index: usize);
}

pub fn export_pdf(pages: &dyn Pages, path: &Path) -> Result<()> {
    let (w, h) = pages.size();
    let tmp = path.with_extension("pdf.part");
    {
        let surface = cairo::PdfSurface::new(w, h, &tmp).context("couldn't create the PDF")?;
        let cr = cairo::Context::new(&surface)?;
        for i in 0..pages.count().max(1) {
            cr.save()?;
            pages.draw(&cr, i);
            cr.restore()?;
            cr.show_page()?;
        }
        surface.finish();
    }
    std::fs::rename(&tmp, path).with_context(|| format!("couldn't write {}", path.display()))?;
    Ok(())
}

/// Show the print dialog for these pages.
pub fn print(pages: Rc<dyn Pages>, title: &str) {
    let op = gtk::PrintOperation::new();
    op.set_job_name(title);
    op.set_n_pages(pages.count().max(1) as i32);
    op.set_unit(gtk::Unit::Points);
    op.set_use_full_page(true);
    op.set_embed_page_setup(true);
    let (w, h) = pages.size();
    let setup = gtk::PageSetup::new();
    let size = gtk::PaperSize::new_custom("nixe", "Document", w, h, gtk::Unit::Points);
    setup.set_paper_size(&size);
    setup.set_orientation(if w > h { gtk::PageOrientation::Landscape } else { gtk::PageOrientation::Portrait });
    op.set_default_page_setup(Some(&setup));
    op.connect_draw_page(move |_, ctx, index| {
        let cr = ctx.cairo_context();
        // Fit the page on the paper the printer actually has.
        let scale = (ctx.width() / w).min(ctx.height() / h);
        cr.scale(scale, scale);
        pages.draw(&cr, index.max(0) as usize);
    });
    let parent = window::window();
    if let Err(e) = op.run(gtk::PrintOperationAction::PrintDialog, parent.as_ref()) {
        window::toast(&format!("Couldn't print: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Two;
    impl Pages for Two {
        fn count(&self) -> usize {
            2
        }
        fn size(&self) -> (f64, f64) {
            (LETTER.width, LETTER.height)
        }
        fn draw(&self, cr: &cairo::Context, index: usize) {
            cr.rectangle(10.0 * index as f64, 10.0, 50.0, 50.0);
            let _ = cr.fill();
        }
    }

    #[test]
    fn exports_every_page() {
        let dir = std::env::temp_dir().join(format!("nxo-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("two.pdf");
        export_pdf(&Two, &out).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        // cairo packs the page tree into a compressed object stream; ask qpdf
        // for the page count when it's installed.
        if let Ok(out) = std::process::Command::new("qpdf").arg("--show-npages").arg(&out).output() {
            assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn paper_ids_round_trip() {
        assert_eq!(Paper::from_id("a4").id(), "a4");
        assert_eq!(Paper::from_id("letter").id(), "letter");
        assert_eq!(Paper::from_id("anything").name(), "Letter");
        for (id, name, _) in PAPERS {
            assert_eq!(Paper::from_id(id).id(), id);
            assert_eq!(Paper::from_id(id).landscape().name(), name);
        }
    }
}
