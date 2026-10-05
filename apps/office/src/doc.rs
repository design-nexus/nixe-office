//! The open file. Each app (Write, Sheets, Present) supplies an `Editor`; this
//! module owns everything around it: the file's path and saved state, opening and
//! saving (native or Office format, chosen by extension), the unsaved-changes
//! guard, crash recovery, PDF export and printing.

use crate::print::Pages;
use crate::{app, cmd, paths, prefs, print, recent, widgets, window};
use anyhow::Result;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub struct Template {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub icon: &'static str,
}

/// A group the editor adds to the sidebar (the outline, the sheets, the slides).
pub struct SidebarGroup {
    pub title: String,
    /// A flat `+` at the right of the group label.
    pub add: Option<(String, Rc<dyn Fn()>)>,
    pub widget: gtk::Widget,
}

/// Find and replace inside the open file.
pub trait Find {
    /// Select the next (or previous) match after the cursor. False when there's none.
    fn find(&self, query: &str, case: bool, forward: bool) -> bool;
    /// Replace the selected match, then select the next one.
    fn replace(&self, query: &str, with: &str, case: bool) -> bool;
    fn replace_all(&self, query: &str, with: &str, case: bool) -> usize;
    fn count(&self, query: &str, case: bool) -> usize;
    /// The find bar closed: drop any match highlights.
    fn done(&self) {}
}

pub type Shortcut = (&'static [&'static str], &'static str);
pub type Action = Rc<dyn Fn()>;
/// A row for the file menu: its label and what it does.
pub type MenuItem = (&'static str, Action);
type Then = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

pub trait Editor {
    /// The document page: its toolbar row and the editor under it.
    fn widget(&self) -> gtk::Widget;
    fn sidebar(&self) -> Option<SidebarGroup> {
        None
    }
    fn templates(&self) -> &'static [Template];
    /// Start a blank file, or one from a template.
    fn new_document(&self, template: Option<&str>);
    fn open(&self, path: &Path) -> Result<()>;
    /// Write the file in the format its extension names. Returns what that format
    /// couldn't keep ("speaker notes", "charts"), for a one-time note.
    fn save(&self, path: &Path) -> Result<Vec<String>>;
    /// The whole file in the native format, for crash recovery.
    fn snapshot(&self) -> Result<Vec<u8>>;
    /// Load a recovery snapshot.
    fn restore(&self, bytes: &[u8]) -> Result<()>;
    fn undo(&self);
    fn redo(&self);
    /// The status bar's readout.
    fn status(&self) -> String;
    fn find(&self) -> Option<Rc<dyn Find>> {
        None
    }
    fn pages(&self) -> Rc<dyn Pages>;
    fn focus(&self) {}
    /// This app's own shortcuts, for the F1 list.
    fn shortcuts(&self) -> &'static [Shortcut] {
        &[]
    }

    /// A key the window didn't use. True when the editor handled it.
    fn key(&self, _key: gtk::gdk::Key, _mods: gtk::gdk::ModifierType) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// A file was opened or started.
    Opened,
    /// The saved state or the path changed.
    Dirty,
    /// The status readout changed (cursor, selection, page).
    Status,
}

#[derive(Default)]
struct State {
    path: Option<PathBuf>,
    open: bool,
    dirty: bool,
    /// Formats whose "can't keep" note was already shown for this file.
    warned: HashSet<String>,
    /// Shown in the top bar until the file is saved ("Untitled", a template's name).
    untitled: String,
}

type Listener = Rc<dyn Fn(Change)>;

thread_local! {
    static EDITOR: RefCell<Option<Rc<dyn Editor>>> = const { RefCell::new(None) };
    static STATE: RefCell<State> = RefCell::new(State::default());
    static LISTENERS: RefCell<Vec<(glib::WeakRef<gtk::Widget>, Listener)>> = const { RefCell::new(Vec::new()) };
    /// This run's recovery snapshot name.
    static SESSION: String = format!("{}-{}", std::process::id(), recent::now());
}

pub fn set_editor(editor: Rc<dyn Editor>) {
    EDITOR.with(|e| *e.borrow_mut() = Some(editor));
}

pub fn editor() -> Option<Rc<dyn Editor>> {
    EDITOR.with(|e| e.borrow().clone())
}

pub fn subscribe(owner: &impl IsA<gtk::Widget>, f: impl Fn(Change) + 'static) {
    LISTENERS.with(|l| l.borrow_mut().push((owner.upcast_ref::<gtk::Widget>().downgrade(), Rc::new(f))));
}

fn emit(change: Change) {
    // Listeners may subscribe or emit in turn, so call them from a copy of the list.
    let listeners: Vec<Listener> = LISTENERS.with(|l| {
        l.borrow_mut().retain(|(w, _)| w.upgrade().is_some());
        l.borrow().iter().map(|(_, f)| f.clone()).collect()
    });
    for f in listeners {
        f(change);
    }
}

pub fn is_open() -> bool {
    STATE.with(|s| s.borrow().open)
}

pub fn dirty() -> bool {
    STATE.with(|s| s.borrow().dirty)
}

pub fn path() -> Option<PathBuf> {
    STATE.with(|s| s.borrow().path.clone())
}

/// The file's name for the top bar and the window title.
pub fn title() -> String {
    STATE.with(|s| {
        let s = s.borrow();
        match &s.path {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            None => s.untitled.clone(),
        }
    })
}

/// The editor calls this on every change to the content.
pub fn edited() {
    let was = STATE.with(|s| std::mem::replace(&mut s.borrow_mut().dirty, true));
    if !was {
        emit(Change::Dirty);
    }
    emit(Change::Status);
}

/// The editor calls this when its status readout may have changed.
pub fn status_changed() {
    emit(Change::Status);
}

fn opened(path: Option<PathBuf>, untitled: &str, dirty: bool) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.path = path;
        s.open = true;
        s.dirty = dirty;
        s.warned.clear();
        s.untitled = untitled.to_string();
    });
    clear_recovery();
    emit(Change::Opened);
    emit(Change::Dirty);
    emit(Change::Status);
    window::navigate("document");
    if let Some(e) = editor() {
        e.focus();
    }
}

// ---------- Unsaved changes ----------

/// Run `then` once unsaved changes are saved or let go; nothing if the user cancels.
pub fn guard(then: impl FnOnce() + 'static) {
    if !dirty() {
        then();
        return;
    }
    let name = title();
    let (dialog, card) = widgets::dialog(&format!("Save changes to “{name}”?"), 440);
    let d = widgets::label("Your changes will be lost if you don't save them.", "dim");
    d.set_wrap(true);
    card.append(&d);
    let buttons = widgets::hbox(8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(6);
    let cancel = gtk::Button::with_label("Cancel");
    let discard = gtk::Button::with_label("Don't save");
    discard.add_css_class("destructive-action");
    let save_b = gtk::Button::with_label("Save");
    save_b.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&discard);
    buttons.append(&save_b);
    card.append(&buttons);
    let then: Then = Rc::new(RefCell::new(Some(Box::new(then))));
    let dl = dialog.clone();
    cancel.connect_clicked(move |_| dl.close());
    let (dl, t) = (dialog.clone(), then.clone());
    discard.connect_clicked(move |_| {
        dl.close();
        clear_recovery();
        STATE.with(|s| s.borrow_mut().dirty = false);
        if let Some(f) = t.borrow_mut().take() {
            f();
        }
    });
    let (dl, t) = (dialog.clone(), then);
    save_b.connect_clicked(move |_| {
        dl.close();
        let t = t.clone();
        save_then(move || {
            if let Some(f) = t.borrow_mut().take() {
                f();
            }
        });
    });
    dialog.present();
    save_b.grab_focus();
}

/// Run `then` if `path` is free, or once the user agrees to replace it. For
/// names we added an extension to, which the file chooser never checked.
fn confirm_replace(path: &Path, then: impl FnOnce() + 'static) {
    if !path.exists() {
        then();
        return;
    }
    let (dialog, card) = widgets::dialog(&format!("Replace “{}”?", file_name(path)), 440);
    let d = widgets::label("A file with this name already exists. Replacing it overwrites what's in it.", "dim");
    d.set_wrap(true);
    card.append(&d);
    let buttons = widgets::hbox(8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(6);
    let cancel = gtk::Button::with_label("Cancel");
    let replace = gtk::Button::with_label("Replace");
    replace.add_css_class("destructive-action");
    buttons.append(&cancel);
    buttons.append(&replace);
    card.append(&buttons);
    let dl = dialog.clone();
    cancel.connect_clicked(move |_| dl.close());
    let dl = dialog.clone();
    let then = RefCell::new(Some(then));
    replace.connect_clicked(move |_| {
        dl.close();
        if let Some(f) = then.borrow_mut().take() {
            f();
        }
    });
    dialog.present();
    cancel.grab_focus();
}

// ---------- New and open ----------

pub fn new_document(template: Option<&'static str>) {
    let Some(ed) = editor() else { return };
    guard(move || {
        ed.new_document(template);
        let name = template
            .and_then(|id| ed.templates().iter().find(|t| t.id == id))
            .map(|t| t.title.to_string())
            .unwrap_or_else(|| format!("Untitled {}", app::kind().noun()));
        // A template starts as unsaved work, so closing asks first.
        opened(None, &name, template.is_some());
    });
}

pub fn open(path: PathBuf) {
    if !app::opens(&path) {
        window::toast(&format!("{} opens {} files.", app::kind().full_name(), list_exts(app::kind().opens())));
        return;
    }
    if self::path().as_deref() == Some(path.as_path()) && is_open() {
        window::navigate("document");
        return;
    }
    let Some(ed) = editor() else { return };
    guard(move || match ed.open(&path) {
        Ok(()) => {
            recent::record(&path);
            opened(Some(path), "", false);
        }
        Err(e) => window::toast(&format!("Couldn't open {}: {e:#}", file_name(&path))),
    });
}

fn list_exts(exts: &[&str]) -> String {
    let dotted: Vec<String> = exts.iter().map(|e| format!(".{e}")).collect();
    match dotted.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => dotted.join(""),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

fn start_folder() -> gio::File {
    let dir = path().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(paths::documents_dir);
    gio::File::for_path(if dir.is_dir() { dir } else { paths::home() })
}

fn filter(name: &str, exts: &[&str]) -> gtk::FileFilter {
    let f = gtk::FileFilter::new();
    f.set_name(Some(name));
    for e in exts {
        f.add_suffix(e);
    }
    f
}

pub fn open_dialog() {
    let kind = app::kind();
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    let all = filter(&format!("{} files", kind.name()), kind.opens());
    filters.append(&all);
    for (name, ext) in kind.save_formats() {
        filters.append(&filter(name, &[ext]));
    }
    let dialog = gtk::FileDialog::builder()
        .title(format!("Open a {}", kind.noun()))
        .modal(true)
        .filters(&filters)
        .default_filter(&all)
        .build();
    dialog.set_initial_folder(Some(&start_folder()));
    dialog.open(window::window().as_ref(), gio::Cancellable::NONE, |res| {
        if let Ok(file) = res
            && let Some(p) = file.path()
        {
            open(p);
        }
    });
}

// ---------- Saving ----------

pub fn save() {
    save_then(|| {});
}

/// Save to the current file (asking for a name the first time), then run `then`.
fn save_then(then: impl FnOnce() + 'static) {
    match path() {
        Some(p) => {
            if write_to(&p) {
                then();
            }
        }
        None => save_as_then(then),
    }
}

pub fn save_as() {
    save_as_then(|| {});
}

fn save_as_then(then: impl FnOnce() + 'static) {
    if !is_open() {
        return;
    }
    let kind = app::kind();
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    for (name, ext) in kind.save_formats() {
        filters.append(&filter(name, &[ext]));
    }
    let stem = match path() {
        Some(p) => p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        None => title(),
    };
    let ext = path()
        .and_then(|p| p.extension().map(|e| e.to_string_lossy().to_lowercase()))
        .unwrap_or_else(|| kind.native_ext().into());
    let dialog = gtk::FileDialog::builder()
        .title(format!("Save the {}", kind.noun()))
        .modal(true)
        .filters(&filters)
        .initial_name(format!("{stem}.{ext}"))
        .build();
    dialog.set_initial_folder(Some(&start_folder()));
    dialog.save(window::window().as_ref(), gio::Cancellable::NONE, move |res| {
        let Ok(file) = res else { return };
        let Some(mut p) = file.path() else { return };
        let known = p
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .is_some_and(|e| kind.save_formats().iter().any(|(_, x)| *x == e));
        if known {
            if write_to(&p) {
                then();
            }
            return;
        }
        let name = format!("{}.{}", file_name(&p), kind.native_ext());
        p.set_file_name(name);
        let target = p.clone();
        confirm_replace(&target, move || {
            if write_to(&p) {
                then();
            }
        });
    });
}

/// Save to `path` now. True when it worked.
fn write_to(path: &Path) -> bool {
    let Some(ed) = editor() else { return false };
    match ed.save(path) {
        Ok(lost) => {
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let first = STATE.with(|s| {
                let mut s = s.borrow_mut();
                let changed_path = s.path.as_deref() != Some(path);
                s.path = Some(path.to_path_buf());
                s.dirty = false;
                if changed_path {
                    s.warned.clear();
                }
                s.warned.insert(ext.clone())
            });
            recent::record(path);
            clear_recovery();
            emit(Change::Dirty);
            emit(Change::Status);
            if first && !lost.is_empty() {
                window::toast(&format!(
                    "Saved {}. A .{ext} file can't keep {}; save as .{} to keep everything.",
                    file_name(path),
                    join_and(&lost),
                    app::kind().native_ext()
                ));
            } else {
                window::toast(&format!("Saved {}.", file_name(path)));
            }
            true
        }
        Err(e) => {
            window::toast(&format!("Couldn't save {}: {e:#}", file_name(path)));
            false
        }
    }
}

pub fn join_and(items: &[String]) -> String {
    match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => items.join(""),
    }
}

// ---------- PDF and printing ----------

pub fn export_pdf_dialog() {
    let Some(ed) = editor() else { return };
    if !is_open() {
        return;
    }
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter("PDF", &["pdf"]));
    let stem = path().and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned())).unwrap_or_else(title);
    let dialog = gtk::FileDialog::builder()
        .title("Export as PDF")
        .modal(true)
        .filters(&filters)
        .initial_name(format!("{stem}.pdf"))
        .build();
    dialog.set_initial_folder(Some(&start_folder()));
    dialog.save(window::window().as_ref(), gio::Cancellable::NONE, move |res| {
        let Ok(file) = res else { return };
        let Some(mut p) = file.path() else { return };
        let export = move |p: PathBuf| match print::export_pdf(ed.pages().as_ref(), &p) {
            Ok(()) => window::toast(&format!("Exported {}.", file_name(&p))),
            Err(e) => window::toast(&format!("Couldn't export the PDF: {e:#}")),
        };
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            export(p);
            return;
        }
        let name = format!("{}.pdf", file_name(&p));
        p.set_file_name(name);
        let target = p.clone();
        confirm_replace(&target, move || export(p));
    });
}

pub fn print_now() {
    if let Some(ed) = editor()
        && is_open()
    {
        print::print(ed.pages(), &title());
    }
}

// ---------- Crash recovery ----------

fn session() -> String {
    SESSION.with(|s| s.clone())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RecoveryInfo {
    /// The file the work belongs to, if it had been saved before.
    path: Option<PathBuf>,
    title: String,
    saved: u64,
}

pub struct Recovery {
    pub snapshot: PathBuf,
    pub path: Option<PathBuf>,
    pub title: String,
    pub saved: u64,
}

fn snapshot_file(name: &str) -> PathBuf {
    paths::recovery_dir(app::kind().id()).join(format!("{name}.{}", app::kind().native_ext()))
}

/// Every few seconds, keep a copy of unsaved work.
pub fn start_recovery_timer() {
    glib::timeout_add_seconds_local(10, || {
        if dirty() && prefs::get().recovery {
            write_recovery();
        }
        glib::ControlFlow::Continue
    });
}

fn write_recovery() {
    let Some(ed) = editor() else { return };
    let Ok(bytes) = ed.snapshot() else { return };
    let file = snapshot_file(&session());
    let info = RecoveryInfo { path: path(), title: title(), saved: recent::now() };
    if cmd::atomic_write(&file, bytes).is_ok()
        && let Ok(text) = serde_json::to_string(&info)
    {
        let _ = cmd::atomic_write(&file.with_extension("json"), text);
    }
}

pub fn clear_recovery() {
    let file = snapshot_file(&session());
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_file(file.with_extension("json"));
}

/// Work left behind by an earlier run that ended without saving.
pub fn leftovers() -> Vec<Recovery> {
    let dir = paths::recovery_dir(app::kind().id());
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mine = session();
    let mut out: Vec<Recovery> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter(|p| p.file_stem().is_some_and(|s| s.to_string_lossy() != mine))
        .filter_map(|p| {
            let info: RecoveryInfo = serde_json::from_str(&std::fs::read_to_string(&p).ok()?).ok()?;
            let snapshot = p.with_extension(app::kind().native_ext());
            snapshot.exists().then_some(Recovery { snapshot, path: info.path, title: info.title, saved: info.saved })
        })
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.saved));
    out
}

pub fn discard(r: &Recovery) {
    let _ = std::fs::remove_file(&r.snapshot);
    let _ = std::fs::remove_file(r.snapshot.with_extension("json"));
}

pub fn restore(r: Recovery) {
    let Some(ed) = editor() else { return };
    guard(move || {
        let loaded = std::fs::read(&r.snapshot).map_err(anyhow::Error::from).and_then(|b| ed.restore(&b));
        match loaded {
            Ok(()) => {
                discard(&r);
                opened(r.path.clone(), &r.title, true);
                window::toast("Restored your unsaved work. Save it to keep it.");
            }
            Err(e) => window::toast(&format!("Couldn't restore that work: {e:#}")),
        }
    });
}

// ---------- The ⋯ menu ----------

fn menu_row(label: &str, keys: &str, act: Action, popover: &gtk::Popover) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("menu-item");
    let content = widgets::hbox(16);
    let l = widgets::label(label, "");
    l.set_hexpand(true);
    content.append(&l);
    if !keys.is_empty() {
        content.append(&widgets::label(keys, "dim"));
    }
    b.set_child(Some(&content));
    let p = popover.clone();
    b.connect_clicked(move |_| {
        p.popdown();
        act();
    });
    b
}

/// The file menu at the end of every editor's toolbar: the file commands, then
/// the app's own (`extra`).
pub fn file_menu(extra: Vec<MenuItem>) -> gtk::MenuButton {
    let menu = gtk::MenuButton::new();
    menu.set_icon_name("nxo-ellipsis-symbolic");
    menu.add_css_class("bar-button");
    menu.set_tooltip_text(Some("File: save, export, print"));
    menu.set_valign(gtk::Align::Center);
    let popover = gtk::Popover::new();
    popover.add_css_class("menu-popover");
    let list = widgets::vbox(0);
    let rows: Vec<(&str, &str, Action)> = vec![
        ("New blank file", "Ctrl+N", Rc::new(|| new_document(None))),
        ("Open…", "Ctrl+O", Rc::new(open_dialog)),
        ("Save", "Ctrl+S", Rc::new(save)),
        ("Save as…", "Ctrl+Shift+S", Rc::new(save_as)),
        ("Export as PDF…", "Ctrl+E", Rc::new(export_pdf_dialog)),
        ("Print…", "Ctrl+P", Rc::new(print_now)),
    ];
    for (label, keys, act) in rows {
        list.append(&menu_row(label, keys, act, &popover));
    }
    if !extra.is_empty() {
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        for (label, act) in extra {
            list.append(&menu_row(label, "", act, &popover));
        }
    }
    popover.set_child(Some(&list));
    menu.set_popover(Some(&popover));
    menu
}
