//! The main window (Nyx Design 2.0): the shared header (the app and the file; search on
//! Home; the readout, view buttons, zoom, Find, the Inspector and Settings), the sidebar
//! (Home, the open file, and the editor's own group: the outline or the slides) and the
//! pages (Home and the document). Settings open as a card over the window.

use crate::{app, doc, home, prefs, settings_dialog, theme, widgets};
use gtk::prelude::*;
use nyx_ui::chrome;
use nyx_ui::commandbar::CommandBar;
use nyx_ui::inspector::Inspector;
use gtk::{gdk, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct Ui {
    window: gtk::ApplicationWindow,
    stack: gtk::Stack,
    nav: gtk::Box,
    home_item: gtk::Button,
    doc_item: gtk::Button,
    doc_label: gtk::Label,
    current: String,
    overlay: gtk::Overlay,
    /// The header, hidden in fullscreen.
    chrome: Vec<gtk::Widget>,
    header: chrome::AppHeader,
    /// The document's readout, view buttons and zoom: shown with a document open.
    doc_tools: Vec<gtk::Widget>,
    readout: gtk::Label,
    search: gtk::SearchEntry,
    find: FindBar,
    /// The header's sidebar toggle.
    side_toggle: gtk::Button,
    /// The editor has its own sidebar group. Without one, the document page
    /// has no sidebar at all.
    has_group: bool,
    fullscreen: bool,
}

impl Ui {
    fn sidebar_wanted(&self) -> bool {
        self.current != "document" || self.has_group
    }

    /// Show the sidebar and its toggle where they belong.
    fn refresh_nav(&self) {
        let wanted = self.sidebar_wanted();
        self.nav.set_visible(wanted && !self.fullscreen);
        self.side_toggle.set_visible(wanted);
    }
}

#[derive(Clone)]
struct FindBar {
    bar: gtk::Box,
    query: gtk::SearchEntry,
    with: gtk::Entry,
    case: gtk::ToggleButton,
    count: gtk::Label,
}

thread_local! {
    static UI: RefCell<Option<Rc<RefCell<Ui>>>> = const { RefCell::new(None) };
    static NARROW: Cell<bool> = const { Cell::new(false) };
    /// Set once unsaved changes are dealt with, so closing goes through.
    static FORCE_CLOSE: Cell<bool> = const { Cell::new(false) };
    static TOAST: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
}

fn ui() -> Option<Rc<RefCell<Ui>>> {
    UI.with(|u| u.borrow().clone())
}

pub fn present(app: &gtk::Application) {
    if let Some(ui) = ui() {
        let window = ui.borrow().window.clone();
        window.present();
        return;
    }
    theme::install();
    install_icons();
    build(app);
    navigate("home");
    if let Some(ui) = ui() {
        ui.borrow().window.present();
    }
    doc::start_recovery_timer();
    if prefs::take_broken() {
        toast("Your settings file couldn't be read, so defaults are in use. The old file is kept as settings.toml.bak.");
    }
}

/// Our symbolic icons (Lucide, see data/icons/LICENSE; strokes turned into fills so GTK can recolour them), written to the cache once
/// and added to the icon search path.
fn install_icons() {
    macro_rules! icons {
        ($($name:literal),* $(,)?) => {
            &[$(($name, include_str!(concat!("../data/icons/", $name, ".svg")))),*]
        };
    }
    const ICONS: &[(&str, &str)] = icons![
        "square-check",
        "film",
        "music",
        "crown",
        "book-marked",
        "between-horizontal-start",
        "grid-2x2",
        "wrap-text",
        "sigma",
        "rotate-cw",
        "rotate-ccw",
        "triangle",
        "diamond",
        "star",
        "hexagon",
        "arrow-big-right",
        "squircle",
        "pen-line",
        "layers-2",
        "file-pen-line",
        "check-check",
        "x-circle",
        "table-cells-merge",
        "message-square-text",
        "highlighter",
        "group",
        "ungroup",
        "align-horizontal-distribute-center",
        "align-vertical-distribute-center",
        "align-start-vertical",
        "align-center-vertical",
        "align-end-vertical",
        "align-start-horizontal",
        "align-center-horizontal",
        "align-end-horizontal",
        "sparkles",
        "wand-sparkles",
        "layers",
        "list-tree",
        "columns-2",
        "separator-horizontal",
        "superscript",
        "subscript",
        "link",
        "unfold-vertical",
        "omega",
        "gallery-thumbnails",
        "paintbrush",
        "message-square-plus",
        "align-center",
        "align-left",
        "align-right",
        "arrow-left",
        "arrow-right",
        "arrow-up-narrow-wide",
        "bold",
        "chart-area",
        "chart-column",
        "chart-line",
        "chart-pie",
        "check",
        "chevron-down",
        "clipboard-paste",
        "clock-3",
        "copy",
        "currency",
        "decimal",
        "ellipsis",
        "file-down",
        "file-plus",
        "file-spreadsheet",
        "files",
        "file-text",
        "filter",
        "folder-open",
        "house",
        "image",
        "italic",
        "layout-grid",
        "list-ordered",
        "list",
        "palette",
        "panel-left",
        "percent",
        "play",
        "presentation",
        "redo-2",
        "save",
        "scissors",
        "settings-2",
        "shapes",
        "sliders-horizontal",
        "table-2",
        "type",
        "undo-2",
        "x",
        "underline",
        "heading",
        "printer",
        "search",
        "replace",
        "case-sensitive",
        "baseline",
        "paint-bucket",
        "snowflake",
        "calendar",
        "plus",
        "trash-2",
        "monitor",
        "square",
        "circle",
        "text-cursor",
        "notebook-pen",
        "mail",
        "chef-hat",
        "plane",
        "piggy-bank",
        "package",
        "party-popper",
        "book-open",
        "file-type",
        "arrow-down",
        "arrow-up",
        "grip-vertical",
        "eraser",
        "strikethrough",
        "indent-increase",
        "indent-decrease",
        "minus",
        "table-rows-split",
        "ruler",
    ];
    let dir = crate::paths::cache_dir().join("icons");
    for (name, svg) in ICONS {
        let path = dir.join(format!("nxo-{name}-symbolic.svg"));
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*svg) {
            let _ = crate::cmd::atomic_write(&path, svg);
        }
    }
    if let Some(display) = gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&dir);
    }
}

fn nav_button(icon: &str, title: &str, tooltip: &str) -> (gtk::Button, gtk::Label) {
    let button = gtk::Button::new();
    button.add_css_class("nav-item");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.append(&gtk::Image::from_icon_name(icon));
    let l = widgets::label(title, "nav-label");
    l.set_hexpand(true);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    l.add_css_class("compact-hide");
    content.append(&l);
    button.set_child(Some(&content));
    button.set_tooltip_text(Some(tooltip));
    (button, l)
}

fn build(app: &gtk::Application) {
    let kind = app::kind();
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(kind.full_name())
        .default_width(1120)
        .default_height(800)
        .build();
    window.add_css_class("office-window");
    window.add_css_class(&format!("app-{}", kind.id()));
    // No client-side titlebar: Hyprland manages the window.
    window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    nyx_ui::keys::install(&window, nyx_ui::keys::Common { shortcuts: Some(show_shortcuts), ..Default::default() });
    window.set_icon_name(Some(kind.app_id()));
    let editor = doc::editor().expect("the editor is set before the window is built");

    // ----- Sidebar -----
    let nav = gtk::Box::new(gtk::Orientation::Vertical, 0);
    nav.add_css_class("settings-navigation");
    nav.set_hexpand(false);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let (home_item, _) = nav_button("nxo-house-symbolic", "Home", "New files, templates and recent files");
    home_item.connect_clicked(|_| navigate("home"));
    list.append(&home_item);
    let (doc_item, doc_label) = nav_button(kind.icon(), "", &format!("The open {}", kind.noun()));
    doc_item.connect_clicked(|_| navigate("document"));
    doc_item.set_visible(false);
    list.append(&doc_item);
    let has_group = editor.sidebar().is_some();
    if let Some(group) = editor.sidebar() {
        let head = widgets::hbox(0);
        head.add_css_class("nav-group");
        head.add_css_class("compact-hide");
        let l = widgets::label(&group.title.to_uppercase(), "");
        l.set_hexpand(true);
        l.set_valign(gtk::Align::Center);
        head.append(&l);
        if let Some((tip, act)) = group.add {
            let add = widgets::bar_button("nxo-plus-symbolic", &tip);
            add.add_css_class("nav-add");
            add.connect_clicked(move |_| act());
            head.append(&add);
        }
        let section = widgets::vbox(0);
        section.add_css_class("editor-group");
        section.append(&head);
        group.widget.add_css_class("compact-hide");
        section.append(&group.widget);
        section.set_visible(false);
        list.append(&section);
        let s = section.clone();
        doc::subscribe(&section, move |c| {
            if c == doc::Change::Opened {
                s.set_visible(true);
            }
        });
    }
    let nav_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .vexpand(true)
        .child(&list)
        .build();
    nav.append(&nav_scroll);

    // ----- Pages -----
    let stack = gtk::Stack::new();
    stack.add_css_class("settings-content");
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(if prefs::get().reduce_motion { 0 } else { 160 });

    let home_page = widgets::page("home");
    home_page.body.add_css_class("content-page");
    home::build(&home_page);
    let home_clamp = home_page.root.clone();
    stack.add_named(&home_clamp, Some("home"));

    let find = find_bar();
    let doc_page = widgets::vbox(0);
    doc_page.add_css_class("document-page");
    let editor_widget = editor.widget();
    editor_widget.set_vexpand(true);
    // The find bar sits under the editor's toolbar row (its first child).
    if let Some(bx) = editor_widget.downcast_ref::<gtk::Box>()
        && let Some(toolbar) = bx.first_child()
    {
        bx.insert_child_after(&find.bar, Some(&toolbar));
    } else {
        doc_page.append(&find.bar);
    }
    doc_page.append(&editor_widget);
    stack.add_named(&doc_page, Some("document"));
    // Ctrl+scroll zooms, before the editor's own scrolling sees it.
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
    // A touchpad sends many small steps: add them up to a wheel click's worth.
    let pending = std::cell::Cell::new(0.0);
    wheel.connect_scroll(move |c, _, dy| {
        if !c.current_event_state().contains(gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let step = if c.unit() == gdk::ScrollUnit::Surface { dy / 40.0 } else { dy };
        let total = pending.get() + step;
        if total.abs() >= 1.0 {
            doc::zoom_step(total < 0.0);
            pending.set(0.0);
        } else {
            pending.set(total);
        }
        glib::Propagation::Stop
    });
    doc_page.add_controller(wheel);
    editor.set_zoom(doc::zoom());

    // ----- Header -----
    // The app and the file; search on Home; the document's readout, view buttons and
    // zoom, then Find, the Inspector and Settings.
    let header = chrome::app_header(&window, kind.icon(), kind.name());
    let side_toggle = widgets::bar_button("sidebar-show-symbolic", "Collapse or expand the sidebar (Ctrl+Shift+B)");
    side_toggle.connect_clicked(|_| toggle_sidebar());
    header.start.append(&side_toggle);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search recent files (Ctrl+F)"));
    search.add_css_class("bar-search");
    search.set_width_chars(8);
    search.set_max_width_chars(28);
    search.connect_search_changed(|e| home::set_query(e.text().trim()));
    search.connect_stop_search(|e| e.set_text(""));
    header.center.append(&search);
    header.end.set_spacing(4);
    let readout = widgets::label("", "header-status");
    readout.set_ellipsize(gtk::pango::EllipsizeMode::End);
    readout.set_max_width_chars(30);
    readout.set_margin_end(4);
    header.end.append(&readout);
    let views = widgets::hbox(2);
    for w in STATUS_TOOLS.with(|t| t.take()) {
        views.append(&w);
    }
    let focus = widgets::bar_button("view-conceal-symbolic", "Focus mode: just the document (Ctrl+Shift+F)");
    focus.connect_clicked(|_| toggle_focus());
    views.append(&focus);
    let full = widgets::bar_button("view-fullscreen-symbolic", "Full screen (F11)");
    full.connect_clicked(|_| toggle_fullscreen());
    views.append(&full);
    header.end.append(&views);
    let (zoom, level) = zoom_control();
    header.end.append(&zoom);
    // Find and the Inspector belong to the document; Home has its search field.
    let doc_actions = widgets::hbox(4);
    if let Some((_, inspector)) = CHROME.with(|c| c.borrow().clone()) {
        doc_actions.append(&find_button());
        let toggle = inspector.toggle_button();
        toggle.add_css_class("bar-button");
        doc_actions.append(toggle);
    }
    header.end.append(&doc_actions);
    header.end.append(&chrome::settings_button(settings_dialog::open));
    {
        let (r, views, zoom, actions) = (readout.clone(), views.clone(), zoom.clone(), doc_actions.clone());
        doc::subscribe(&readout, move |_| {
            refresh_status(&r);
            let shown = current() == "document" && doc::is_open();
            views.set_visible(shown);
            actions.set_visible(shown);
            zoom.set_visible(shown && !narrow());
            level.set_label(&format!("{:.0}%", doc::zoom() * 100.0));
        });
    }

    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.set_hexpand(true);
    column.append(&stack);

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_vexpand(true);
    body.append(&nav);
    body.append(&column);

    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.add_css_class("window-frame");
    frame.append(&header.root);
    frame.append(&body);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&frame));
    window.set_child(Some(&overlay));

    install_keys(&window);
    window.connect_close_request(|w| {
        if FORCE_CLOSE.with(|f| f.get()) || !doc::dirty() {
            doc::clear_recovery();
            return glib::Propagation::Proceed;
        }
        let w = w.clone();
        doc::guard(move || {
            FORCE_CLOSE.with(|f| f.set(true));
            w.close();
        });
        glib::Propagation::Stop
    });

    // Drop a file anywhere on the window to open it.
    let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    drop.connect_drop(|_, value, _, _| {
        let Ok(list) = value.get::<gdk::FileList>() else { return false };
        match list.files().into_iter().filter_map(|f| f.path()).find(|p| app::opens(p)) {
            Some(p) => {
                doc::open(p);
                true
            }
            None => {
                let kind = app::kind();
                toast(&format!(
                    "{} opens {} files.",
                    kind.full_name(),
                    kind.opens().iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(", ")
                ));
                false
            }
        }
    });
    window.add_controller(drop);

    // Narrow windows (a tiled half-screen) get an icon-only sidebar.
    let apply_width = {
        let nav = nav.clone();
        let last_width = Rc::new(Cell::new(0));
        move |w: &gtk::ApplicationWindow| {
            let allocated = if w.width() > 0 { w.width() } else { w.default_width() };
            let width = w.surface().map(|s| s.width()).filter(|width| *width > 0).map_or(allocated, |width| allocated.min(width));
            let half_screen = w.surface()
                .and_then(|surface| surface.display().monitor_at_surface(&surface))
                .is_some_and(|monitor| width <= monitor.geometry().width() / 2 + 32);
            let narrow = width > 0 && (width < 980 || half_screen);
            let resized = last_width.replace(width) != width;
            settings_dialog::fit(w);
            if narrow && resized {
                close_narrow_panels();
            }
            if narrow == NARROW.with(|n| n.get()) && nav.has_css_class("sized") {
                return;
            }
            nav.add_css_class("sized");
            set_narrow(narrow);
            refresh_compact();
        }
    };
    let aw = apply_width.clone();
    window.connect_default_width_notify(move |w| aw(w));
    let aw = apply_width.clone();
    window.connect_realize(move |w| {
        aw(w);
        if let Some(surface) = w.surface() {
            let (aw, w) = (aw.clone(), w.clone());
            // The compositor can shrink a tiled surface while GTK keeps a
            // wider minimum allocation for the open panes. Widget resize
            // signals then do not fire, but the surface width does change.
            surface.connect_width_notify(move |_| {
                let (aw, w) = (aw.clone(), w.clone());
                glib::idle_add_local_once(move || aw(&w));
            });
        }
    });
    // Tiled windows are resized by the compositor without touching the default
    // size: an invisible layer over the whole window reports each real size change.
    let probe = gtk::DrawingArea::new();
    probe.set_can_target(false);
    probe.set_can_focus(false);
    overlay.add_overlay(&probe);
    overlay.set_measure_overlay(&probe, false);
    {
        let (aw, w2) = (apply_width.clone(), window.clone());
        probe.connect_resize(move |_, _, _| {
            let (aw, w2) = (aw.clone(), w2.clone());
            glib::idle_add_local_once(move || aw(&w2));
        });
    }

    let state = Ui {
        window: window.clone(),
        stack,
        nav,
        home_item,
        doc_item,
        doc_label,
        current: String::new(),
        overlay,
        chrome: vec![header.root.clone().upcast()],
        doc_tools: vec![views.upcast(), zoom.upcast()],
        readout,
        header,
        search,
        find,
        side_toggle,
        has_group,
        fullscreen: false,
    };
    UI.with(|u| *u.borrow_mut() = Some(Rc::new(RefCell::new(state))));

    // The window title and the sidebar follow the open file.
    let w = window.clone();
    doc::subscribe(&window, move |c| {
        if matches!(c, doc::Change::Opened | doc::Change::Dirty) {
            let kind = app::kind();
            w.set_title(Some(&if doc::is_open() {
                format!("{}{} — {}", if doc::dirty() { "• " } else { "" }, doc::title(), kind.full_name())
            } else {
                kind.full_name().to_string()
            }));
            refresh_title();
            refresh_subtitle();
            if let Some(ui) = ui() {
                let u = ui.borrow();
                u.doc_item.set_visible(doc::is_open());
                u.doc_label.set_text(&doc::title());
                u.doc_item.set_tooltip_text(Some(&doc::title()));
            }
        }
    });
}

thread_local! {
    static CHROME: RefCell<Option<(Rc<CommandBar>, Rc<Inspector>)>> = const { RefCell::new(None) };
    /// View buttons the editor puts in the status bar, before the bar is built.
    static STATUS_TOOLS: RefCell<Vec<gtk::Widget>> = const { RefCell::new(Vec::new()) };
}

/// The editor's command bar and Inspector: the window finishes the bar (find,
/// the Inspector toggle, the file's name) and remembers whether the
/// Inspector is open.
pub fn attach_chrome(bar: &Rc<CommandBar>, inspector: &Rc<Inspector>) {
    // The file's name is the header's subtitle, not a title on the bar.
    bar.title_button().set_visible(false);
    let kind = app::kind().id();
    let open = prefs::get().inspector.get(kind).copied().unwrap_or(kind != "sheets");
    inspector.set_open(open);
    inspector.connect_open(move |on| {
        prefs::update(|p| {
            p.inspector.insert(kind.to_string(), on);
        })
    });
    // Before the Inspector floats over the document, the sidebar goes to icons
    // to make room for it (not saved as a setting; closing it brings it back).
    inspector.connect_floating(|floating| {
        if floating && self::inspector().is_some_and(|i| i.is_open()) {
            make_room(true);
        }
    });
    inspector.connect_open(|open| {
        if open && self::inspector().is_some_and(|i| i.is_floating()) {
            make_room(true);
        } else if !open {
            make_room(false);
        }
    });
    CHROME.with(|c| *c.borrow_mut() = Some((bar.clone(), inspector.clone())));
}

thread_local! {
    /// The sidebar shows only icons to make room for the Inspector.
    static ROOM: Cell<bool> = const { Cell::new(false) };
}

fn make_room(on: bool) {
    if ROOM.with(|r| r.get()) == on {
        return;
    }
    if on && (narrow() || prefs::get().sidebar_collapsed) {
        return;
    }
    ROOM.with(|r| r.set(on));
    refresh_compact();
}

fn inspector() -> Option<Rc<Inspector>> {
    CHROME.with(|c| c.borrow().as_ref().map(|(_, i)| i.clone()))
}

/// Ctrl+Alt+I (and Ctrl+F1): show or hide the Inspector.
fn toggle_inspector() {
    if let Some(i) = inspector() {
        i.toggle();
    }
}

/// The command bar keeps no title of its own (the header shows the file).
fn refresh_title() {
    let Some(bar) = CHROME.with(|c| c.borrow().as_ref().map(|(b, _)| b.clone())) else { return };
    bar.set_title("", "", false);
    bar.title_button().set_visible(false);
}

/// The header's subtitle: the open file (a dot while it has unsaved changes) or Home.
fn refresh_subtitle() {
    let Some(ui) = ui() else { return };
    let u = ui.borrow();
    let text = if u.current == "document" && doc::is_open() {
        format!("{}{}", doc::title(), if doc::dirty() { " •" } else { "" })
    } else {
        "Home".to_string()
    };
    u.header.set_subtitle(&text);
}

/// A view button for the header (the chart pane, the sorter).
/// The editor adds them while it builds, before the window exists.
pub fn status_tool(w: &impl IsA<gtk::Widget>) {
    STATUS_TOOLS.with(|t| t.borrow_mut().push(w.clone().upcast()));
}

thread_local! {
    /// In focus mode.
    static FOCUS: Cell<bool> = const { Cell::new(false) };
}

/// Focus mode (Ctrl+Shift+F): just the document. The Inspector and the sidebar hide
/// until it's turned off again; the header stays, to turn it off.
pub fn toggle_focus() {
    let Some(ui) = ui() else { return };
    let on = !FOCUS.with(|f| f.get());
    FOCUS.with(|f| f.set(on));
    if let Some(i) = inspector() {
        i.suspend(on);
    }
    let u = ui.borrow();
    if on {
        u.nav.set_visible(false);
    } else {
        u.refresh_nav();
    }
}

/// Find, for the end of the command bar.
pub fn find_button() -> gtk::Button {
    let b = widgets::bar_button("nxo-search-symbolic", "Find and replace (Ctrl+F, Ctrl+H)");
    b.connect_clicked(|_| find_or_search(false));
    b
}

/// Ctrl+F: the find bar while a file is showing; otherwise search recent files.
fn find_or_search(replace: bool) {
    if current() == "document" && doc::is_open() {
        show_find(replace);
        return;
    }
    navigate("home");
    let Some(ui) = ui() else { return };
    let search = ui.borrow().search.clone();
    search.grab_focus();
}

// ---------- Find and replace ----------

fn find_bar() -> FindBar {
    let bar = widgets::hbox(6);
    bar.add_css_class("find-bar");
    bar.set_visible(false);
    let query = gtk::SearchEntry::new();
    query.set_placeholder_text(Some("Find"));
    query.set_width_chars(18);
    query.set_hexpand(true);
    let case = gtk::ToggleButton::new();
    case.set_icon_name("nxo-case-sensitive-symbolic");
    case.add_css_class("search-option");
    case.set_tooltip_text(Some("Match case"));
    let prev = widgets::bar_button("nxo-arrow-up-symbolic", "Previous match (Shift+Enter)");
    let next = widgets::bar_button("nxo-arrow-down-symbolic", "Next match (Enter)");
    let count = widgets::label("", "find-count");
    count.add_css_class("dim");
    let with = gtk::Entry::new();
    with.set_placeholder_text(Some("Replace with"));
    with.set_width_chars(16);
    with.set_hexpand(true);
    let one = gtk::Button::with_label("Replace");
    one.set_tooltip_text(Some("Replace this match and find the next (Ctrl+H)"));
    let all = gtk::Button::with_label("All");
    all.set_tooltip_text(Some("Replace every match"));
    let close = widgets::bar_button("window-close-symbolic", "Close (Esc)");
    bar.append(&query);
    bar.append(&case);
    bar.append(&prev);
    bar.append(&next);
    bar.append(&count);
    bar.append(&with);
    bar.append(&one);
    bar.append(&all);
    bar.append(&close);
    let fb = FindBar { bar, query, with, case, count };

    let go = |forward: bool| {
        move || {
            let Some(ui) = ui() else { return };
            let f = ui.borrow().find.clone();
            let q = f.query.text().to_string();
            if q.is_empty() {
                return;
            }
            if let Some(t) = doc::editor().and_then(|e| e.find())
                && !t.find(&q, f.case.is_active(), forward)
            {
                f.count.set_text("No matches");
            }
        }
    };
    let (n, p) = (go(true), go(false));
    next.connect_clicked(move |_| n());
    prev.connect_clicked(move |_| p());
    let (n, p) = (go(true), go(false));
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, mods| {
        if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
            if mods.contains(gdk::ModifierType::SHIFT_MASK) {
                p()
            } else {
                n()
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    fb.query.add_controller(keys);
    fb.query.connect_search_changed(|_| refresh_find(true));
    fb.case.connect_toggled(|_| refresh_find(true));
    fb.query.connect_stop_search(|_| hide_find());
    close.connect_clicked(|_| hide_find());
    let replace_one = || {
        let Some(ui) = ui() else { return };
        let f = ui.borrow().find.clone();
        let q = f.query.text().to_string();
        if let Some(t) = doc::editor().and_then(|e| e.find())
            && !q.is_empty()
        {
            t.replace(&q, &f.with.text(), f.case.is_active());
        }
        refresh_find(false);
    };
    one.connect_clicked(move |_| replace_one());
    fb.with.connect_activate(move |_| replace_one());
    all.connect_clicked(|_| {
        let Some(ui) = ui() else { return };
        let f = ui.borrow().find.clone();
        let q = f.query.text().to_string();
        if let Some(t) = doc::editor().and_then(|e| e.find())
            && !q.is_empty()
        {
            let n = t.replace_all(&q, &f.with.text(), f.case.is_active());
            toast(&format!("Replaced {}.", crate::fmt::count(n, "match", "matches")));
        }
        refresh_find(false);
    });
    fb
}

/// Recount the matches; when the query changed, jump to the first one.
fn refresh_find(jump: bool) {
    let Some(ui) = ui() else { return };
    let f = ui.borrow().find.clone();
    let q = f.query.text().to_string();
    let Some(t) = doc::editor().and_then(|e| e.find()) else { return };
    if q.is_empty() {
        f.count.set_text("");
        t.done();
        return;
    }
    let n = t.count(&q, f.case.is_active());
    f.count.set_text(&if n == 0 { "No matches".to_string() } else { crate::fmt::count(n, "match", "matches") });
    if jump && n > 0 {
        t.find(&q, f.case.is_active(), true);
        f.query.grab_focus();
    }
}

fn show_find(replace: bool) {
    let Some(ui) = ui() else { return };
    let f = ui.borrow().find.clone();
    f.bar.set_visible(true);
    if replace {
        f.with.grab_focus();
    } else {
        f.query.grab_focus();
        f.query.select_region(0, -1);
    }
    refresh_find(false);
}

fn hide_find() {
    let Some(ui) = ui() else { return };
    let f = ui.borrow().find.clone();
    if !f.bar.is_visible() {
        return;
    }
    f.bar.set_visible(false);
    if let Some(t) = doc::editor().and_then(|e| e.find()) {
        t.done();
    }
    if let Some(e) = doc::editor() {
        e.focus();
    }
}

// ---------- Status bar and shortcuts ----------

/// − 100% + in the header; the number goes back to 100%.
fn zoom_control() -> (gtk::Box, gtk::Button) {
    let b = widgets::hbox(0);
    b.add_css_class("status-zoom");
    let button = |label: &str, tip: &str, f: fn()| {
        let x = gtk::Button::with_label(label);
        x.add_css_class("status-help");
        x.set_tooltip_text(Some(tip));
        x.connect_clicked(move |_| f());
        b.append(&x);
        x
    };
    button("−", "Zoom out (Ctrl+Alt+−)", || doc::zoom_step(false));
    let level = button("100%", "Actual size (Ctrl+Alt+0)", || doc::set_zoom(1.0));
    level.add_css_class("tnum");
    button("+", "Zoom in (Ctrl+Alt+=)", || doc::zoom_step(true));
    (b, level)
}

fn refresh_status(readout: &gtk::Label) {
    let text = if current() == "document" && doc::is_open() {
        doc::editor().map(|e| e.status()).unwrap_or_default()
    } else {
        crate::fmt::count(home::recent_count(), "recent file", "recent files")
    };
    readout.set_text(&text);
}

/// Shortcuts every app shares, grouped for the F1 list.
pub const FILE_SHORTCUTS: &[doc::Shortcut] = &[
    (&["Ctrl", "N"], "New blank file"),
    (&["Ctrl", "O"], "Open a file"),
    (&["Ctrl", "S"], "Save"),
    (&["Ctrl", "Shift", "S"], "Save as (also as an Office file)"),
    (&["Ctrl", "E"], "Export as PDF"),
    (&["Ctrl", "P"], "Print"),
    (&["Ctrl", "Alt", "="], "Zoom in (Ctrl+Alt+−: out, Ctrl+Alt+0: actual size; or Ctrl+scroll)"),
];

pub const WINDOW_SHORTCUTS: &[doc::Shortcut] = &[
    (&["Ctrl", "F"], "Find in the file, or search recent files"),
    (&["Ctrl", "H"], "Find and replace"),
    (&["Ctrl", "Z"], "Undo"),
    (&["Ctrl", "Shift", "Z"], "Redo (also Ctrl+Y)"),
    (&["Ctrl", "Shift", "B"], "Collapse or expand the sidebar"),
    (&["Ctrl", ","], "Settings"),
    (&["F11"], "Fullscreen"),
    (&["Ctrl", "Alt", "I"], "Show or hide the Inspector (also Ctrl+F1)"),
    (&["Ctrl", "Shift", "F"], "Focus mode: just the document"),
    (&["Esc"], "Close the find bar or search"),
    (&["F1"], "Show these shortcuts (or ?)"),
    (&["Ctrl", "Q"], "Quit (also Ctrl+W)"),
];

/// Every shortcut, by group, for the F1 list and the settings card.
pub fn shortcut_groups() -> Vec<(String, Vec<doc::Shortcut>)> {
    let mut groups = vec![("Files".to_string(), FILE_SHORTCUTS.to_vec())];
    if let Some(e) = doc::editor() {
        let own = e.shortcuts();
        if !own.is_empty() {
            groups.push((app::kind().name().to_string(), own.to_vec()));
        }
    }
    groups.push(("Window".to_string(), WINDOW_SHORTCUTS.to_vec()));
    groups
}

pub fn show_shortcuts() {
    let (dialog, card) = widgets::dialog("Keyboard shortcuts", 520);
    let content = widgets::vbox(0);
    for (i, (title, keys)) in shortcut_groups().into_iter().enumerate() {
        let h = widgets::label(&title.to_uppercase(), "group-title");
        if i == 0 {
            h.add_css_class("page-first");
        }
        content.append(&h);
        let list = widgets::vbox(0);
        list.add_css_class("group-list");
        for (k, what) in keys {
            list.append(&widgets::row(what, "", Some(widgets::key_caps(k).upcast_ref())));
        }
        content.append(&list);
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        // Scrolls without a scrollbar, which would cover the key caps.
        .vscrollbar_policy(gtk::PolicyType::External)
        .propagate_natural_height(true)
        .max_content_height(560)
        .child(&content)
        .build();
    card.append(&scroll);
    let close = gtk::Button::with_label("Close");
    close.set_halign(gtk::Align::End);
    let d = dialog.clone();
    close.connect_clicked(move |_| d.close());
    card.append(&close);
    dialog.present();
}

pub fn overlay() -> Option<gtk::Overlay> {
    ui().map(|u| u.borrow().overlay.clone())
}

// ---------- Sidebar ----------

/// The sidebar shows only icons when the window is narrow or the user collapsed it.
fn refresh_compact() {
    let Some(ui) = ui() else { return };
    let nav = ui.borrow().nav.clone();
    let compact = narrow() || prefs::get().sidebar_collapsed || ROOM.with(|r| r.get());
    if compact {
        nav.add_css_class("compact");
    } else {
        nav.remove_css_class("compact");
    }
    set_compact_hidden(&nav, compact);
}

pub fn toggle_sidebar() {
    if ui().is_some_and(|u| !u.borrow().sidebar_wanted()) {
        return;
    }
    // Icons only to make room for the Inspector: the toggle just brings it back.
    if ROOM.with(|r| r.replace(false)) {
        refresh_compact();
        return;
    }
    prefs::update(|p| p.sidebar_collapsed = !p.sidebar_collapsed);
    refresh_compact();
}

/// Hide everything marked `compact-hide` in the sidebar when it's icon-only.
fn set_compact_hidden(root: &gtk::Box, compact: bool) {
    fn walk(w: &gtk::Widget, compact: bool) {
        if w.has_css_class("compact-hide") {
            w.set_visible(!compact);
            if compact {
                return;
            }
        }
        if w.has_css_class("nav-item")
            && let Some(content) = w.downcast_ref::<gtk::Button>().and_then(|b| b.child())
        {
            content.set_halign(if compact { gtk::Align::Center } else { gtk::Align::Fill });
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            walk(&c, compact);
            child = c.next_sibling();
        }
    }
    walk(root.upcast_ref(), compact);
}

// ---------- Keys ----------

fn install_keys(window: &gtk::ApplicationWindow) {
    // Capture phase: these work wherever focus is.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let w2 = window.clone();
    keys.connect_key_pressed(move |_, key, _, mods| {
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        let stop = glib::Propagation::Stop;
        if settings_dialog::is_open() {
            return match key {
                gdk::Key::Escape => {
                    settings_dialog::escape();
                    stop
                }
                gdk::Key::f if ctrl => {
                    settings_dialog::focus_search();
                    stop
                }
                gdk::Key::q if ctrl => {
                    w2.close();
                    stop
                }
                _ => glib::Propagation::Proceed,
            };
        }
        // A line of text being typed (a cell, the find field) keeps its own undo.
        let in_entry = gtk::prelude::GtkWindowExt::focus(&w2).is_some_and(|f| f.is::<gtk::Text>());
        let in_doc = current() == "document" && doc::is_open();
        let k = key.to_lower();
        if ctrl && mods.contains(gdk::ModifierType::ALT_MASK) && in_doc {
            match k {
                gdk::Key::equal | gdk::Key::plus | gdk::Key::KP_Add => doc::zoom_step(true),
                gdk::Key::minus | gdk::Key::KP_Subtract => doc::zoom_step(false),
                gdk::Key::_0 | gdk::Key::KP_0 => doc::set_zoom(1.0),
                gdk::Key::i => toggle_inspector(),
                _ => return glib::Propagation::Proceed,
            }
            return stop;
        }
        if ctrl {
            let handled = match k {
                gdk::Key::f if shift && in_doc => {
                    toggle_focus();
                    true
                }
                gdk::Key::f => {
                    find_or_search(false);
                    true
                }
                gdk::Key::h if in_doc => {
                    find_or_search(true);
                    true
                }
                gdk::Key::b if shift => {
                    toggle_sidebar();
                    true
                }
                gdk::Key::b if !in_doc => {
                    toggle_sidebar();
                    true
                }
                gdk::Key::n => {
                    doc::new_document(None);
                    true
                }
                gdk::Key::o => {
                    doc::open_dialog();
                    true
                }
                gdk::Key::s if shift => {
                    doc::save_as();
                    true
                }
                gdk::Key::s => {
                    doc::save();
                    true
                }
                gdk::Key::e if doc::is_open() => {
                    doc::export_pdf_dialog();
                    true
                }
                gdk::Key::p if doc::is_open() => {
                    doc::print_now();
                    true
                }
                gdk::Key::comma => {
                    settings_dialog::open();
                    true
                }
                gdk::Key::z if in_doc && !in_entry && shift => {
                    if let Some(e) = doc::editor() {
                        e.redo();
                    }
                    true
                }
                gdk::Key::z if in_doc && !in_entry => {
                    if let Some(e) = doc::editor() {
                        e.undo();
                    }
                    true
                }
                gdk::Key::y if in_doc && !in_entry => {
                    if let Some(e) = doc::editor() {
                        e.redo();
                    }
                    true
                }
                gdk::Key::q | gdk::Key::w => {
                    w2.close();
                    true
                }
                _ => false,
            };
            if handled {
                return stop;
            }
        }
        match key {
            gdk::Key::F1 if ctrl => {
                toggle_inspector();
                return stop;
            }
            gdk::Key::F1 => {
                show_shortcuts();
                return stop;
            }
            gdk::Key::F11 => {
                toggle_fullscreen();
                return stop;
            }
            gdk::Key::Escape => {
                let search = ui().map(|u| u.borrow().search.clone());
                let find_open = ui().is_some_and(|u| u.borrow().find.bar.is_visible());
                if find_open && in_doc {
                    hide_find();
                    return stop;
                }
                if in_doc && inspector().is_some_and(|i| i.escape()) {
                    return stop;
                }
                if let Some(s) = search.filter(|s| !s.text().is_empty()) {
                    s.set_text("");
                    return stop;
                }
                if w2.is_fullscreen() && !in_doc {
                    set_fullscreen(false);
                    return stop;
                }
            }
            _ => {}
        }
        if in_doc
            && let Some(e) = doc::editor()
            && e.key(key, mods)
        {
            return stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);
}

pub fn toggle_fullscreen() {
    let on = window().is_some_and(|w| w.is_fullscreen());
    set_fullscreen(!on);
}

pub fn set_fullscreen(on: bool) {
    let Some(w) = window() else { return };
    if on {
        w.fullscreen();
    } else {
        w.unfullscreen();
    }
    let Some(ui) = ui() else { return };
    ui.borrow_mut().fullscreen = on;
    let u = ui.borrow();
    u.refresh_nav();
    for c in &u.chrome {
        c.set_visible(!on);
    }
}

fn set_narrow(narrow: bool) {
    NARROW.with(|n| n.set(narrow));
    crate::write::fit_comment_panel();
    let Some(ui) = ui() else { return };
    let u = ui.borrow();
    // A half-screen header keeps the actions; the readout and zoom wait for room.
    u.readout.set_visible(!narrow);
    u.search.set_max_width_chars(if narrow { 14 } else { 28 });
    if let Some(zoom) = u.doc_tools.get(1) {
        zoom.set_visible(!narrow && u.current == "document" && doc::is_open());
    }
    for name in ["home", "document"] {
        if let Some(page) = u.stack.child_by_name(name) {
            let body =
                page.downcast_ref::<gtk::ScrolledWindow>().and_then(|s| s.child()).and_then(|v| v.first_child()).unwrap_or(page);
            if narrow {
                body.add_css_class("narrow");
            } else {
                body.remove_css_class("narrow");
            }
        }
    }
}

fn close_narrow_panels() {
    if let Some(i) = inspector() {
        i.set_open(false);
    }
    crate::sheets::close_chart_pane();
}

pub fn narrow() -> bool {
    NARROW.with(|n| n.get())
}

pub fn navigate(id: &str) {
    let Some(ui) = ui() else { return };
    if id == "settings" {
        settings_dialog::open();
        return;
    }
    let id = if id == "document" && !doc::is_open() { "home" } else { id };
    {
        let mut u = ui.borrow_mut();
        u.stack.set_visible_child_name(id);
        for (item, on) in [(&u.home_item, id == "home"), (&u.doc_item, id == "document")] {
            if on {
                item.add_css_class("active");
            } else {
                item.remove_css_class("active");
            }
        }
        u.current = id.to_string();
        u.refresh_nav();
        u.search.set_visible(id == "home");
    }
    refresh_subtitle();
    if id == "home" {
        home::refresh();
    }
    doc::status_changed();
    if id == "document"
        && let Some(e) = doc::editor()
    {
        e.focus();
    }
}

/// Show a short message at the bottom of the window. It replaces any toast showing.
pub fn toast(message: &str) {
    let Some(overlay) = overlay() else {
        eprintln!("nyx-office: {message}");
        return;
    };
    if let Some(old) = TOAST.with(|t| t.borrow_mut().take()) {
        overlay.remove_overlay(&old);
    }
    let label = gtk::Label::new(Some(message));
    label.set_wrap(true);
    label.set_max_width_chars(70);
    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bx.add_css_class("toast");
    bx.append(&label);
    bx.set_halign(gtk::Align::Center);
    bx.set_valign(gtk::Align::End);
    overlay.add_overlay(&bx);
    let widget: gtk::Widget = bx.upcast();
    TOAST.with(|t| *t.borrow_mut() = Some(widget.clone()));
    glib::timeout_add_local_once(std::time::Duration::from_millis(3500), move || {
        let mine = TOAST.with(|t| t.borrow().as_ref() == Some(&widget));
        if mine {
            TOAST.with(|t| t.borrow_mut().take());
            overlay.remove_overlay(&widget);
        }
    });
}

pub fn current() -> String {
    ui().map(|u| u.borrow().current.clone()).unwrap_or_default()
}

pub fn window() -> Option<gtk::ApplicationWindow> {
    ui().map(|u| u.borrow().window.clone())
}
