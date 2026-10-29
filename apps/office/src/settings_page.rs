//! The groups shown in the settings card.

use crate::widgets::{self, Page};
use crate::{app, prefs};

pub fn build(page: &Page) {
    let p = prefs::get();
    let kind = app::kind();

    // ----- Editing, per app -----
    match kind {
        app::Kind::Write => {
            let g = page.group("Writing");
            let (r, _) = widgets::switch_row(
                "Check spelling as you type",
                "Underline misspelled words. Right-click one for suggestions.",
                p.spellcheck,
                |on| {
                    prefs::update(|p| p.spellcheck = on);
                    crate::write::spelling_changed();
                },
            );
            g.add(&r);
            let (r, _) = widgets::switch_row(
                "Smart quotes and symbols",
                "As you type, \"quotes\" become curly, -- a dash —, ... an ellipsis …, and (c) ©.",
                p.autocorrect,
                |on| prefs::update(|p| p.autocorrect = on),
            );
            g.add(&r);
            let langs = crate::write::spelling_languages();
            if !langs.is_empty() {
                let mut options = vec![(String::new(), "System language".to_string())];
                options.extend(langs);
                let (r, _) = widgets::choice_row(
                    "Spelling language",
                    "The dictionary used for checking.",
                    options,
                    &p.spelling_language,
                    |id| {
                        prefs::update(|p| p.spelling_language = id);
                        crate::write::spelling_changed();
                    },
                );
                g.add(&r);
            }
        }
        app::Kind::Sheets => {}
        app::Kind::Present => {
            let g = page.group("Presenting");
            let (r, _) = widgets::switch_row(
                "Presenter view",
                "With a second screen connected, show your notes, the next slide and a timer on this one while presenting.",
                p.presenter_view,
                |on| prefs::update(|p| p.presenter_view = on),
            );
            g.add(&r);
        }
    }

    // ----- Files -----
    let g = page.group("Files");
    g.add(&widgets::segmented_row(
        "Paper size",
        "For new files, PDF export and printing.",
        widgets::opts(&crate::print::PAPERS.map(|p| (p.0, p.1))),
        &p.paper,
        |v| prefs::update(|p| p.paper = v),
    ));
    let (r, _) = widgets::switch_row(
        "Keep unsaved work",
        "Save a copy of your changes every few seconds, so they can be brought back if the app closes unexpectedly.",
        p.recovery,
        |on| prefs::update(|p| p.recovery = on),
    );
    g.add(&r);
    let (r, _) = widgets::switch_row(
        "Save automatically",
        "Save the file a few seconds after each change, and when closing it, without asking. A new file is saved this way once it has a name.",
        p.autosave,
        |on| prefs::update(|p| p.autosave = on),
    );
    g.add(&r);
    g.note(&format!("Open a file from a terminal or a binding with <tt>nyx-{} FILE</tt>.", kind.id()));
}
