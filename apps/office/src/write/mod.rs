//! Nexus Write: the word processor.

mod autocorrect;
mod buffer;
mod changes;
mod comments;
mod docx;
mod figure;
mod layout;
mod legacy;
mod links;
mod markdown;
mod odt;
mod marks;
pub mod model;
mod picture;
mod spacing;
mod painter;
mod symbols;
mod table;
mod templates;
mod view;
mod wrap;

use crate::doc::{self, Find, SidebarGroup, Template};
use crate::print::{Pages, Paper};
use crate::{color, container, prefs, widgets, window};
use anyhow::{Result, bail};
use buffer::{ParaProps, Tables};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use model::*;
use picture::{PX, Picture};
use sourceview5::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::{Rc, Weak};
use table::TableWidget;

const FORMAT: &str = "nexus-write";
const SIZES: &[&str] =
    &["8", "9", "10", "10.5", "11", "12", "13", "14", "15.5", "16", "18", "20", "24", "26", "28", "32", "36", "48", "60", "72"];
const UNDO_LIMIT: usize = 100;

struct Snapshot {
    doc: Document,
    cursor: i32,
}

struct Controls {
    style: gtk::DropDown,
    font: gtk::DropDown,
    fonts: Vec<String>,
    size: gtk::DropDown,
    bold: gtk::ToggleButton,
    italic: gtk::ToggleButton,
    underline: gtk::ToggleButton,
    strike: gtk::ToggleButton,
    superscript: gtk::ToggleButton,
    subscript: gtk::ToggleButton,
    color: Rc<color::ColorButton>,
    highlight: Rc<color::ColorButton>,
    align: Vec<(Align, gtk::ToggleButton)>,
    bullet: gtk::ToggleButton,
    number: gtk::ToggleButton,
    picture: gtk::Box,
    picture_width: gtk::Scale,
    picture_wrap: gtk::DropDown,
    track: gtk::ToggleButton,
    figure: gtk::Box,
    figure_width: gtk::Scale,
    figure_fill: Rc<color::ColorButton>,
    figure_data: gtk::Button,
}

pub struct Writer {
    me: RefCell<Weak<Writer>>,
    root: gtk::Box,
    buffer: sourceview5::Buffer,
    view: view::PageView,
    paper: gtk::Box,
    scroll: gtk::ScrolledWindow,
    setup: RefCell<PageSetup>,
    media: RefCell<BTreeMap<String, Vec<u8>>>,
    tables: Tables,
    marks: marks::Marks,
    comments: RefCell<Vec<Comment>>,
    revisions: RefCell<Vec<Revision>>,
    /// New edits are tracked as changes.
    tracking: Cell<bool>,
    /// An autocorrection is being made (its own edits aren't corrected again).
    correcting: Cell<bool>,
    /// The comment cards beside the page.
    comment_panel: gtk::Box,
    controls: RefCell<Option<Controls>>,
    outline: gtk::Box,
    /// True while the buffer is being filled from a document: no edits recorded.
    loading: Cell<bool>,
    /// True while toolbar controls are being set from the cursor: their handlers ignore it.
    syncing: Cell<bool>,
    /// The formatting the next typed text gets.
    typing: RefCell<TextRun>,
    /// The format painter's copied look, while it's on.
    painter: RefCell<Option<painter::Painter>>,
    painter_button: RefCell<Option<gtk::ToggleButton>>,
    /// Where the text being inserted starts (set just before each insert).
    insert_at: Cell<Option<(i32, i32, bool)>>,
    pasting: Cell<bool>,
    undo: RefCell<Vec<Snapshot>>,
    redo: RefCell<Vec<Snapshot>>,
    group: RefCell<Option<glib::SourceId>>,
    pages: Cell<usize>,
    pages_job: RefCell<Option<glib::SourceId>>,
    outline_job: RefCell<Option<glib::SourceId>>,
    /// The spell checker on the buffer. GTK doesn't tell it about tables, marks
    /// or pictures going in, so it's replaced afterwards (and set aside while a
    /// document loads) to keep its count of the text right.
    spelling: RefCell<Option<libspelling::TextBufferAdapter>>,
    checker: libspelling::Checker,
}

thread_local! {
    static WRITER: RefCell<Option<Rc<Writer>>> = const { RefCell::new(None) };
}

fn writer() -> Option<Rc<Writer>> {
    WRITER.with(|w| w.borrow().clone())
}

/// Settings changed the spelling switch or language.
pub fn spelling_changed() {
    if let Some(w) = writer() {
        w.apply_spelling();
    }
}

/// (code, name) for every installed dictionary.
pub fn spelling_languages() -> Vec<(String, String)> {
    libspelling::init();
    let model = libspelling::Provider::default().list_languages();
    let mut out: Vec<(String, String)> = (0..model.n_items())
        .filter_map(|i| model.item(i).and_downcast::<libspelling::Language>())
        .filter_map(|l| Some((l.code()?.to_string(), l.name()?.to_string())))
        .collect();
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

impl Writer {
    pub fn create() -> Rc<dyn doc::Editor> {
        Writer::build()
    }

    fn build() -> Rc<Writer> {
        libspelling::init();
        let buffer = sourceview5::Buffer::new(None);
        buffer.set_enable_undo(false);
        buffer.set_highlight_matching_brackets(false);
        buffer::setup_tags(buffer.upcast_ref());
        buffer.set_text("\n");
        let view = view::PageView::new(&buffer);
        view.add_css_class("page-text");
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        view.set_pixels_below_lines((6.0 * PX) as i32);
        view.set_pixels_inside_wrap(2);
        view.set_vexpand(true);

        let paper = widgets::vbox(0);
        paper.add_css_class("paper");
        paper.set_halign(gtk::Align::Center);
        paper.append(&view);
        let column = widgets::vbox(0);
        column.add_css_class("paper-column");
        let desk_row = widgets::hbox(16);
        desk_row.set_halign(gtk::Align::Center);
        desk_row.append(&paper);
        column.append(&desk_row);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .child(&widgets::zoom_box(&column))
            .build();
        scroll.add_css_class("desk");

        let checker = libspelling::Checker::default();

        let root = widgets::vbox(0);
        root.add_css_class("write-editor");
        let outline = widgets::vbox(0);
        outline.add_css_class("outline");

        let w = Rc::new(Writer {
            me: RefCell::new(Weak::new()),
            root: root.clone(),
            buffer,
            view,
            paper,
            scroll,
            setup: RefCell::new(PageSetup { paper: prefs::get().paper, ..Default::default() }),
            media: RefCell::new(BTreeMap::new()),
            tables: Rc::new(RefCell::new(Vec::new())),
            marks: Rc::new(RefCell::new(Vec::new())),
            comments: RefCell::new(Vec::new()),
            revisions: RefCell::new(Vec::new()),
            tracking: Cell::new(false),
            correcting: Cell::new(false),
            comment_panel: widgets::vbox(10),
            controls: RefCell::new(None),
            outline,
            loading: Cell::new(false),
            syncing: Cell::new(false),
            typing: RefCell::new(TextRun::default()),
            painter: RefCell::new(None),
            painter_button: RefCell::new(None),
            insert_at: Cell::new(None),
            pasting: Cell::new(false),
            undo: RefCell::new(Vec::new()),
            redo: RefCell::new(Vec::new()),
            group: RefCell::new(None),
            pages: Cell::new(1),
            pages_job: RefCell::new(None),
            outline_job: RefCell::new(None),
            spelling: RefCell::new(None),
            checker,
        });
        *w.me.borrow_mut() = Rc::downgrade(&w);
        w.comment_panel.add_css_class("comment-panel");
        w.comment_panel.set_valign(gtk::Align::Start);
        w.comment_panel.set_visible(false);
        desk_row.append(&w.comment_panel);
        let toolbar = w.build_toolbar();
        root.append(&toolbar);
        root.append(&w.scroll);
        w.connect_buffer();
        w.connect_view();
        w.connect_links();
        w.apply_page();
        w.attach_spelling();
        WRITER.with(|x| *x.borrow_mut() = Some(w.clone()));
        w
    }

    /// End the current burst of typing, so the next edit is a new undo step.
    #[cfg(test)]
    fn end_group(&self) {
        if let Some(id) = self.group.borrow_mut().take() {
            id.remove();
        }
    }

    fn rc(&self) -> Rc<Writer> {
        self.me.borrow().upgrade().expect("the writer is alive")
    }

    fn tb(&self) -> &gtk::TextBuffer {
        self.buffer.upcast_ref()
    }

    // ---------- Toolbar ----------

    fn build_toolbar(self: &Rc<Self>) -> gtk::Box {
        let bar = widgets::hbox(0);
        bar.add_css_class("toolbar");
        bar.add_css_class("doc-toolbar");
        let flow = widgets::wrap_row(4, 4);
        flow.add_css_class("tool-row");
        flow.set_hexpand(true);
        flow.set_valign(gtk::Align::Center);
        bar.append(&flow);
        let group = || {
            let g = widgets::hbox(2);
            g.add_css_class("tool-group");
            g
        };
        let me = Rc::downgrade(self);
        let act = move |f: fn(&Writer)| {
            let me = me.clone();
            move || {
                if let Some(w) = me.upgrade() {
                    f(&w);
                }
            }
        };

        // Undo and redo.
        let g = group();
        let undo = widgets::bar_button("nxo-undo-2-symbolic", "Undo (Ctrl+Z)");
        let f = act(doc::Editor::undo);
        undo.connect_clicked(move |_| f());
        let redo = widgets::bar_button("nxo-redo-2-symbolic", "Redo (Ctrl+Shift+Z)");
        let f = act(doc::Editor::redo);
        redo.connect_clicked(move |_| f());
        g.append(&undo);
        g.append(&redo);
        flow.append(&g);

        // Paragraph style, font and size.
        let g = group();
        let names: Vec<&str> = Style::ALL.iter().map(|s| s.name()).collect();
        let style = gtk::DropDown::from_strings(&names);
        style.set_tooltip_text(Some("Paragraph style (Ctrl+0 to Ctrl+3)"));
        style.add_css_class("style-picker");
        let me = Rc::downgrade(self);
        style.connect_selected_notify(move |d| {
            if let Some(w) = me.upgrade()
                && !w.syncing.get()
            {
                let s = Style::ALL[d.selected() as usize % Style::ALL.len()];
                w.set_style(s);
            }
        });
        g.append(&style);
        let fonts = font_families(&self.view);
        let font_refs: Vec<&str> = fonts.iter().map(String::as_str).collect();
        let font = gtk::DropDown::from_strings(&font_refs);
        font.set_enable_search(true);
        font.set_expression(Some(gtk::PropertyExpression::new(
            gtk::StringObject::static_type(),
            None::<gtk::Expression>,
            "string",
        )));
        font.set_tooltip_text(Some("Font"));
        font.add_css_class("font-picker");
        let me = Rc::downgrade(self);
        let fl = fonts.clone();
        font.connect_selected_notify(move |d| {
            if let Some(w) = me.upgrade()
                && !w.syncing.get()
                && let Some(f) = fl.get(d.selected() as usize)
            {
                let f = if f == DEFAULT_FONT { None } else { Some(f.clone()) };
                w.apply_char(move |r| r.font = f.clone());
            }
        });
        g.append(&font);
        let size = gtk::DropDown::from_strings(SIZES);
        size.set_tooltip_text(Some("Text size in points"));
        size.add_css_class("size-picker");
        let me = Rc::downgrade(self);
        size.connect_selected_notify(move |d| {
            if let Some(w) = me.upgrade()
                && !w.syncing.get()
                && let Some(s) = SIZES.get(d.selected() as usize).and_then(|s| s.parse::<f64>().ok())
            {
                w.apply_char(move |r| r.size = Some(s));
            }
        });
        g.append(&size);
        flow.append(&g);

        // Character formatting.
        let g = group();
        let toggle = |icon: &str, tip: &str, f: fn(&Writer)| {
            let b = gtk::ToggleButton::new();
            b.set_icon_name(icon);
            b.add_css_class("tool-toggle");
            b.set_tooltip_text(Some(tip));
            let me = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(w) = me.upgrade()
                    && !w.syncing.get()
                {
                    f(&w);
                }
            });
            b
        };
        let bold = toggle("nxo-bold-symbolic", "Bold (Ctrl+B)", |w| w.toggle_flag(|r| &mut r.bold));
        let italic = toggle("nxo-italic-symbolic", "Italic (Ctrl+I)", |w| w.toggle_flag(|r| &mut r.italic));
        let underline = toggle("nxo-underline-symbolic", "Underline (Ctrl+U)", |w| w.toggle_flag(|r| &mut r.underline));
        let strike = toggle("nxo-strikethrough-symbolic", "Strikethrough", |w| w.toggle_flag(|r| &mut r.strike));
        let superscript = toggle("nxo-superscript-symbolic", "Superscript (Ctrl+Shift+=)", |w| w.toggle_script(true));
        let subscript = toggle("nxo-subscript-symbolic", "Subscript (Ctrl+=)", |w| w.toggle_script(false));
        for b in [&bold, &italic, &underline, &strike, &superscript, &subscript] {
            g.append(b);
        }
        let me = Rc::downgrade(self);
        let link = widgets::bar_button("nxo-link-symbolic", "Add or edit a link (Ctrl+K)");
        link.connect_clicked(move |_| {
            if let Some(w) = me.upgrade() {
                w.link_dialog();
            }
        });
        g.append(&link);
        g.append(&self.symbols_button());
        g.append(&self.painter_button());
        let me = Rc::downgrade(self);
        let color = color::button("nxo-baseline-symbolic", "Text color", color::TEXT, "Automatic", move |c| {
            if let Some(w) = me.upgrade() {
                w.apply_char(move |r| r.color = c.clone().filter(|c| c != DEFAULT_COLOR));
            }
        });
        let me = Rc::downgrade(self);
        let highlight = color::button("nxo-paint-bucket-symbolic", "Highlight", color::HIGHLIGHT, "None", move |c| {
            if let Some(w) = me.upgrade() {
                w.apply_char(move |r| r.highlight = c.clone());
            }
        });
        g.append(&color.button);
        g.append(&highlight.button);
        flow.append(&g);

        // Alignment.
        let g = group();
        let mut align = Vec::new();
        for (a, icon, tip) in [
            (Align::Left, "nxo-align-left-symbolic", "Align left (Ctrl+Shift+L)"),
            (Align::Center, "nxo-align-center-symbolic", "Center (Ctrl+Shift+E)"),
            (Align::Right, "nxo-align-right-symbolic", "Align right (Ctrl+Shift+R)"),
            (Align::Justify, "format-justify-fill-symbolic", "Justify (Ctrl+Shift+J)"),
        ] {
            let b = gtk::ToggleButton::new();
            b.set_icon_name(icon);
            b.add_css_class("tool-toggle");
            b.set_tooltip_text(Some(tip));
            let me = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(w) = me.upgrade()
                    && !w.syncing.get()
                {
                    w.set_align(a);
                }
            });
            g.append(&b);
            align.push((a, b));
        }
        flow.append(&g);

        // Lists and indents.
        let g = group();
        let bullet = toggle("nxo-list-symbolic", "Bulleted list (Ctrl+Shift+8)", |w| w.toggle_list(List::Bullet));
        let number = toggle("nxo-list-ordered-symbolic", "Numbered list (Ctrl+Shift+7)", |w| w.toggle_list(List::Number));
        g.append(&bullet);
        g.append(&number);
        let less = widgets::bar_button("nxo-indent-decrease-symbolic", "Decrease indent (Shift+Tab in a list)");
        let f = act(|w| w.indent(-1));
        less.connect_clicked(move |_| f());
        let more = widgets::bar_button("nxo-indent-increase-symbolic", "Increase indent (Tab in a list)");
        let f = act(|w| w.indent(1));
        more.connect_clicked(move |_| f());
        g.append(&less);
        g.append(&more);
        g.append(&self.spacing_button());
        flow.append(&g);

        // Insert.
        let g = group();
        let image = widgets::bar_button("nxo-image-symbolic", "Insert a picture");
        let f = act(|w| w.pick_picture());
        image.connect_clicked(move |_| f());
        g.append(&image);
        g.append(&self.table_button());
        g.append(&self.shapes_menu());
        let chart = widgets::bar_button("nxo-chart-column-symbolic", "Insert a chart (its numbers in Chart data…)");
        let f = act(|w| w.insert_chart());
        chart.connect_clicked(move |_| f());
        g.append(&chart);
        let note = widgets::bar_button("nxo-superscript-symbolic", "Insert a footnote (Ctrl+Alt+F)");
        let f = act(|w| w.insert_note(false));
        note.connect_clicked(move |_| f());
        g.append(&note);
        let endnote = widgets::bar_button("nxo-notebook-pen-symbolic", "Insert an endnote: its words go at the end of the document (Ctrl+Alt+D)");
        let f = act(|w| w.insert_note(true));
        endnote.connect_clicked(move |_| f());
        g.append(&endnote);
        let comment = widgets::bar_button("nxo-message-square-plus-symbolic", "Comment on the selection (Ctrl+Alt+M)");
        let f = act(|w| w.add_comment());
        comment.connect_clicked(move |_| f());
        g.append(&comment);
        let (track, review) = self.changes_buttons();
        let review_group = group();
        review_group.append(&track);
        review_group.append(&review);
        let toc = widgets::bar_button("nxo-list-tree-symbolic", "Insert a table of contents");
        let f = act(|w| w.insert_mark_block(marks::MarkKind::Contents));
        toc.connect_clicked(move |_| f());
        g.append(&toc);
        let brk = widgets::bar_button("nxo-separator-horizontal-symbolic", "Insert a page break (Ctrl+Enter)");
        let f = act(|w| w.insert_mark_block(marks::MarkKind::PageBreak));
        brk.connect_clicked(move |_| f());
        g.append(&brk);
        let section = widgets::bar_button("nxo-between-horizontal-start-symbolic", "Insert a section break: change the columns from here on");
        let f = act(|w| w.insert_mark_block(marks::MarkKind::Section(Section { columns: 2, new_page: false })));
        section.connect_clicked(move |_| f());
        g.append(&section);
        flow.append(&g);
        flow.append(&review_group);

        // Shown while the cursor is beside a picture.
        let picture = group();
        picture.append(&widgets::label("Picture", "dim"));
        let picture_width = gtk::Scale::with_range(gtk::Orientation::Horizontal, 10.0, 100.0, 1.0);
        picture_width.set_size_request(120, -1);
        picture_width.set_tooltip_text(Some("Picture width, as a share of the text width"));
        let me = Rc::downgrade(self);
        picture_width.connect_value_changed(move |s| {
            if let Some(w) = me.upgrade()
                && !w.syncing.get()
            {
                w.set_picture_width(s.value() / 100.0);
            }
        });
        picture.append(&picture_width);
        let wrap = gtk::DropDown::from_strings(&["In line", "Wrap, left", "Wrap, right"]);
        wrap.set_tooltip_text(Some("In the line, or at the side of the paragraph with the text wrapping beside it"));
        let me = Rc::downgrade(self);
        wrap.connect_selected_notify(move |d| {
            if let Some(w) = me.upgrade()
                && !w.syncing.get()
            {
                w.set_picture_wrap(match d.selected() {
                    1 => Some(Side::Left),
                    2 => Some(Side::Right),
                    _ => None,
                });
            }
        });
        picture.append(&wrap);
        picture.set_visible(false);
        flow.append(&picture);
        let (figure, figure_width, figure_fill, figure_data) = self.figure_tools();
        flow.append(&figure);

        let me = Rc::downgrade(self);
        let setup: Rc<dyn Fn()> = Rc::new(move || {
            if let Some(w) = me.upgrade() {
                w.page_setup_dialog();
            }
        });
        let file = doc::file_menu(vec![("Page setup…", setup)]);
        bar.append(&file);

        *self.controls.borrow_mut() = Some(Controls {
            style,
            font,
            fonts,
            size,
            bold,
            italic,
            underline,
            strike,
            superscript,
            subscript,
            color,
            highlight,
            align,
            bullet,
            number,
            picture,
            picture_width,
            picture_wrap: wrap,
            track,
            figure,
            figure_width,
            figure_fill,
            figure_data,
        });
        bar
    }

    fn table_button(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-table-2-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Insert a table"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        let content = widgets::vbox(10);
        content.add_css_class("prompt-box");
        content.append(&widgets::label("Insert a table", "popover-title"));
        let rows = gtk::SpinButton::with_range(1.0, 50.0, 1.0);
        rows.set_value(3.0);
        let cols = gtk::SpinButton::with_range(1.0, 10.0, 1.0);
        cols.set_value(3.0);
        let line = widgets::hbox(8);
        line.append(&widgets::label("Rows", "dim"));
        line.append(&rows);
        line.append(&widgets::label("Columns", "dim"));
        line.append(&cols);
        content.append(&line);
        let insert = gtk::Button::with_label("Insert");
        insert.add_css_class("suggested-action");
        insert.set_halign(gtk::Align::End);
        let me = Rc::downgrade(self);
        let p = pop.clone();
        insert.connect_clicked(move |_| {
            p.popdown();
            if let Some(w) = me.upgrade() {
                w.insert_table(rows.value() as usize, cols.value() as usize);
            }
        });
        content.append(&insert);
        pop.set_child(Some(&content));
        menu.set_popover(Some(&pop));
        menu
    }

    /// Set the toolbar from the formatting at the cursor.
    fn sync_controls(&self) {
        let Some(c) = self.controls.borrow().as_ref().map(|c| (c.style.clone(), c.font.clone(), c.fonts.clone(), c.size.clone()))
        else {
            return;
        };
        let (style_dd, font_dd, fonts, size_dd) = c;
        self.syncing.set(true);
        let cursor = self.cursor_iter();
        let p = buffer::para_props(self.tb(), &cursor);
        let r = self.typing.borrow().clone();
        style_dd.set_selected(Style::ALL.iter().position(|s| *s == p.style).unwrap_or(0) as u32);
        let family = r.font.clone().unwrap_or_else(|| DEFAULT_FONT.to_string());
        if let Some(i) = fonts.iter().position(|f| f.eq_ignore_ascii_case(&family)) {
            font_dd.set_selected(i as u32);
        }
        let size = r.size.unwrap_or(p.style.look().0);
        let label = buffer::fmt_size(size);
        size_dd.set_selected(SIZES.iter().position(|s| *s == label).map(|i| i as u32).unwrap_or(gtk::INVALID_LIST_POSITION));
        if let Some(c) = self.controls.borrow().as_ref() {
            c.bold.set_active(r.bold || p.style.look().3);
            c.italic.set_active(r.italic);
            c.underline.set_active(r.underline);
            c.strike.set_active(r.strike);
            c.superscript.set_active(r.superscript);
            c.subscript.set_active(r.subscript);
            c.color.set_current(Some(r.color.as_deref().unwrap_or(DEFAULT_COLOR)));
            c.highlight.set_current(r.highlight.as_deref());
            for (a, b) in &c.align {
                b.set_active(*a == p.align);
            }
            c.bullet.set_active(p.list == Some(List::Bullet));
            c.number.set_active(p.list == Some(List::Number));
            let pic = self.picture_at_cursor();
            c.picture.set_visible(pic.is_some());
            self.sync_figure_tools();
            if let Some(pic) = pic {
                c.picture_width.set_value((pic.width_pt() / self.setup.borrow().content_width() * 100.0).round());
                c.picture_wrap.set_selected(match pic.wrap() {
                    Some(Side::Left) => 1,
                    Some(Side::Right) => 2,
                    None => 0,
                });
            }
        }
        self.syncing.set(false);
    }

    // ---------- Buffer signals ----------

    fn connect_buffer(self: &Rc<Self>) {
        let b = self.tb();
        // The spell checker keeps its own count of the text and misses the
        // placeholder character a table, mark or picture adds; recount after each.
        for signal in ["insert-child-anchor", "insert-paintable"] {
            let me = Rc::downgrade(self);
            b.connect_local(signal, true, move |_| {
                if let Some(w) = me.upgrade()
                    && w.spelling.borrow().is_some()
                {
                    w.detach_spelling();
                    w.attach_spelling();
                }
                None
            });
        }
        let me = Rc::downgrade(self);
        b.connect_insert_text(move |b, at, text| {
            let Some(w) = me.upgrade() else { return };
            if w.loading.get() {
                return;
            }
            w.begin_edit();
            // Text typed at the very end goes before the final newline.
            let _ = b;
            w.insert_at.set(Some((at.offset(), text.chars().count() as i32, text.contains('\n'))));
        });
        let me = Rc::downgrade(self);
        b.connect_insert_paintable(move |_, _, _| {
            if let Some(w) = me.upgrade()
                && !w.loading.get()
            {
                w.begin_edit();
            }
        });
        let me = Rc::downgrade(self);
        b.connect_delete_range(move |b, start, end| {
            let Some(w) = me.upgrade() else { return };
            if w.loading.get() {
                return;
            }
            // A deletion reaching the end takes the final newline too; `changed`
            // puts it back. (Stopping the deletion here would leave the spell
            // checker, which already saw it, out of step with the text.)
            w.begin_edit();
            // Removing a table's line drops its widget too.
            let (s, e) = (start.offset(), end.offset());
            w.tables.borrow_mut().retain(|(a, _)| {
                let it = b.iter_at_child_anchor(a);
                !(it.offset() >= s && it.offset() < e) && !a.is_deleted()
            });
        });
        let me = Rc::downgrade(self);
        b.connect_changed(move |b| {
            let Some(w) = me.upgrade() else { return };
            if w.loading.get() {
                return;
            }
            // The buffer always ends with a newline, for the last paragraph's tags.
            let mut last = b.end_iter();
            if !last.backward_char() || last.char() != '\n' {
                w.loading.set(true);
                let line = b.end_iter().line();
                let props = buffer::para_props(b, &b.iter_at_line(line).unwrap_or_else(|| b.start_iter()));
                let mut end = b.end_iter();
                b.insert(&mut end, "\n");
                buffer::set_para(b, line, line, props);
                w.loading.set(false);
            }
            w.after_change();
        });
        let me = Rc::downgrade(self);
        b.connect_mark_set(move |b, iter, mark| {
            let Some(w) = me.upgrade() else { return };
            if w.loading.get() || mark != &b.get_insert() {
                return;
            }
            // Keep the cursor off the line after the final newline.
            if iter.is_end() && b.char_count() > 0 {
                let mut it = b.end_iter();
                it.backward_char();
                let sel = b.selection_bound();
                let sel_it = b.iter_at_mark(&sel);
                if sel_it.is_end() {
                    b.place_cursor(&it);
                } else {
                    b.move_mark(&b.get_insert(), &it);
                }
                return;
            }
            w.pick_up_format();
            w.sync_controls();
            doc::status_changed();
        });
    }

    fn connect_view(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        self.view.connect_paste_clipboard(move |_| {
            if let Some(w) = me.upgrade() {
                w.pasting.set(true);
                let me = Rc::downgrade(&w);
                glib::idle_add_local_once(move || {
                    if let Some(w) = me.upgrade() {
                        w.pasting.set(false);
                    }
                });
            }
        });
        // A right-click on a word moves the cursor there, so the spelling
        // suggestions in the menu are for that word.
        let click = gtk::GestureClick::new();
        click.set_button(3);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = Rc::downgrade(self);
        click.connect_pressed(move |_, _, x, y| {
            let Some(w) = me.upgrade() else { return };
            let b = w.tb();
            if b.has_selection() {
                return;
            }
            let (bx, by) = w.view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
            if let Some(it) = w.view.iter_at_location(bx, by) {
                b.place_cursor(&it);
            }
            if let Some(sp) = w.spelling.borrow().as_ref() {
                sp.update_corrections();
            }
        });
        self.view.add_controller(click);
        // Re-fit the page when the window's width changes.
        let me = Rc::downgrade(self);
        self.scroll.hadjustment().connect_page_size_notify(move |_| {
            if let Some(w) = me.upgrade() {
                w.fit_paper();
            }
        });
    }

    /// The formatting for typing: that of the character before the cursor, or at
    /// the start of a paragraph the one after it.
    fn pick_up_format(&self) {
        let it = self.cursor_iter();
        let mut before = it;
        let r = if it.starts_line() || !before.backward_char() || before.paintable().is_some() {
            buffer::char_format(&it)
        } else {
            buffer::char_format(&before)
        };
        // Typing extends a comment or a link only inside it, not at its end.
        let after = buffer::char_format(&it);
        let comment = r.comment.filter(|c| after.comment == Some(*c));
        let link = r.link.clone().filter(|l| after.link.as_ref() == Some(l));
        // Typed text is never deleted text; while tracking it's marked as added.
        let mut typing = TextRun { text: String::new(), comment, link, ..r };
        self.tracked_typing(&mut typing, &it);
        *self.typing.borrow_mut() = typing;
    }

    fn after_change(&self) {
        let b = self.tb();
        if let Some((start, len, newline)) = self.insert_at.take() {
            let s = b.iter_at_offset(start);
            let e = b.iter_at_offset(start + len);
            if !self.pasting.get() {
                let typing = self.typing.borrow().clone();
                buffer::set_format(b, &s, &e, &typing);
            }
            let first = s.line();
            let last = e.line();
            if newline && last > first {
                // New paragraphs continue the one they were split from; a heading
                // ended with Enter is followed by normal text.
                let mut p = buffer::para_props(b, &s);
                let at_end = e.ends_line()
                    && e.char() == '\n'
                    && b.iter_at_line(last).is_some_and(|l| l.offset() == e.offset() || e.ends_line());
                if p.style.is_heading() && at_end {
                    p.style = Style::Normal;
                }
                buffer::set_para(b, first + 1, last, p);
            }
            buffer::normalize_lines(b, first, first);
            // Format the typed text, then the cursor keeps that format.
            if !self.pasting.get() {
                let t = self.typing.borrow().clone();
                *self.typing.borrow_mut() = t;
            }
            // One character typed: maybe a straight quote to curl, or a symbol to make.
            // (Only typing in the page: not find and replace or other commands.)
            if len == 1 && !newline && !self.pasting.get() && self.view.has_focus() {
                self.autocorrect(start);
            }
        } else {
            let line = self.cursor_iter().line();
            buffer::normalize_lines(b, line, line);
        }
        self.edited();
    }

    fn edited(&self) {
        self.renumber_notes();
        doc::edited();
        self.view.queue_draw();
        self.schedule_pages();
        self.schedule_outline();
        self.sync_controls();
    }

    // ---------- Undo ----------

    fn snapshot_now(&self) -> Snapshot {
        Snapshot { doc: self.document(), cursor: self.cursor_iter().offset() }
    }

    /// Note the state before an edit, once per burst of typing.
    fn begin_edit(&self) {
        if self.group.borrow().is_none() {
            let snap = self.snapshot_now();
            let mut undo = self.undo.borrow_mut();
            undo.push(snap);
            if undo.len() > UNDO_LIMIT {
                undo.remove(0);
            }
            self.redo.borrow_mut().clear();
        } else if let Some(id) = self.group.borrow_mut().take() {
            id.remove();
        }
        let me = self.me.borrow().clone();
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(900), move || {
            if let Some(w) = me.upgrade() {
                w.group.borrow_mut().take();
            }
        });
        *self.group.borrow_mut() = Some(id);
    }

    /// A command (bold, a list, a picture) is its own undo step.
    fn checkpoint(&self) {
        if let Some(id) = self.group.borrow_mut().take() {
            id.remove();
        }
        self.begin_edit();
        if let Some(id) = self.group.borrow_mut().take() {
            id.remove();
        }
    }

    fn restore_snapshot(&self, s: Snapshot) {
        let media = self.media.borrow().clone();
        self.load_document(&s.doc, &media);
        let it = self.tb().iter_at_offset(s.cursor);
        self.tb().place_cursor(&it);
        self.view.scroll_to_mark(&self.tb().get_insert(), 0.1, false, 0.0, 0.0);
        self.edited();
    }

    // ---------- Reading and writing the buffer ----------

    fn cursor_iter(&self) -> gtk::TextIter {
        self.tb().iter_at_mark(&self.tb().get_insert())
    }

    pub fn document(&self) -> Document {
        let mut d = Document {
            page: self.setup.borrow().clone(),
            blocks: buffer::read(self.tb(), &self.tables, &self.marks),
            comments: self.comments.borrow().clone(),
            revisions: self.revisions.borrow().clone(),
            tracking: self.tracking.get(),
        };
        d.prune_comments();
        d
    }

    fn make_table(&self) -> impl Fn(&Table) -> Rc<TableWidget> + use<> {
        let me = self.me.borrow().clone();
        move |t: &Table| {
            let me2 = me.clone();
            let w = TableWidget::new(
                t,
                0,
                Rc::new(move || {
                    if let Some(w) = me2.upgrade()
                        && !w.loading.get()
                    {
                        w.begin_edit();
                        w.edited();
                    }
                }),
            );
            let me3 = me.clone();
            let weak = Rc::downgrade(&w);
            w.set_on_delete(Rc::new(move || {
                if let (Some(w), Some(t)) = (me3.upgrade(), weak.upgrade()) {
                    w.delete_table(&t);
                }
            }));
            w
        }
    }

    fn load_document(&self, d: &Document, media: &BTreeMap<String, Vec<u8>>) {
        self.detach_spelling();
        self.loading.set(true);
        *self.setup.borrow_mut() = d.page.clone();
        *self.media.borrow_mut() = media.clone();
        *self.comments.borrow_mut() = d.comments.clone();
        *self.revisions.borrow_mut() = d.revisions.clone();
        self.tracking.set(d.tracking);
        let make = self.make_table();
        let make_mark = self.make_mark();
        buffer::Loader {
            buffer: self.tb(),
            view: self.view.upcast_ref(),
            media,
            tables: &self.tables,
            width_px: self.text_width_px(),
            make_table: &make,
            marks: &self.marks,
            make_mark: &make_mark,
        }
        .load(&d.blocks);
        self.loading.set(false);
        self.attach_spelling();
        self.renumber_notes();
        self.refresh_comments();
        self.refresh_changes();
        self.apply_page();
        self.pick_up_format();
        self.sync_controls();
        self.schedule_pages();
        self.schedule_outline();
        self.view.queue_draw();
    }

    fn load_fresh(&self, d: &Document, media: &BTreeMap<String, Vec<u8>>) {
        self.undo.borrow_mut().clear();
        self.redo.borrow_mut().clear();
        if let Some(id) = self.group.borrow_mut().take() {
            id.remove();
        }
        self.load_document(d, media);
        self.scroll.vadjustment().set_value(0.0);
    }

    fn native_bytes(&self) -> Result<Vec<u8>> {
        let d = self.document();
        let used = d.image_names();
        let media: BTreeMap<String, Vec<u8>> =
            self.media.borrow().iter().filter(|(k, _)| used.contains(k)).map(|(k, v)| (k.clone(), v.clone())).collect();
        container::write(FORMAT, &serde_json::to_vec(&d)?, &media)
    }

    fn read_native(bytes: &[u8]) -> Result<(Document, BTreeMap<String, Vec<u8>>)> {
        if !container::is_zip(bytes) {
            bail!("it isn't a Nexus Write file");
        }
        // Qt-era files hold document.html instead of content.json.
        let legacy = zip::ZipArchive::new(std::io::Cursor::new(bytes)).is_ok_and(|a| a.index_for_name("document.html").is_some());
        if legacy {
            return legacy::read(bytes);
        }
        let c = container::read(bytes, FORMAT)?;
        let mut d: Document = serde_json::from_slice(&c.content)?;
        d.tidy();
        Ok((d, c.media))
    }

    // ---------- Page ----------

    /// The text column's width on screen, in pixels.
    fn text_width_px(&self) -> i32 {
        // With columns, the screen shows one column's width, so lines break as printed.
        (self.setup.borrow().column_width() * PX).round() as i32
    }

    fn apply_page(&self) {
        let s = self.setup.borrow().clone();
        let [t, b, _, _] = s.margins;
        self.view.set_margin_top((t * PX) as i32);
        self.view.set_margin_bottom((b * PX) as i32);
        self.paper.set_size_request(-1, (s.paper().height * PX) as i32);
        self.fit_paper();
    }

    /// The page is drawn at its real width when it fits; on a narrow window the
    /// side margins shrink first, then the text column.
    fn fit_paper(&self) {
        let s = self.setup.borrow().clone();
        // Zoomed, the pages have the room of a window that much smaller or larger.
        let avail = (self.scroll.hadjustment().page_size() / doc::zoom()) as i32 - 32;
        let paper_w = (s.paper().width * PX) as i32;
        let text_w = self.text_width_px();
        let (left, right) = ((s.margins[2] * PX) as i32, (s.margins[3] * PX) as i32);
        let (w, l, r) = if avail <= 0 || avail >= paper_w {
            (text_w, left, right)
        } else if avail >= text_w + 48 {
            let side = (avail - text_w) / 2;
            (text_w, side, side)
        } else {
            (avail.max(240) - 48, 24, 24)
        };
        self.view.set_margin_start(l);
        self.view.set_margin_end(r);
        self.view.set_size_request(w, -1);
        for (_, t) in self.tables.borrow().iter() {
            t.set_width(w - 4);
        }
        for (_, m, widget) in self.marks.borrow().iter() {
            if !matches!(m, marks::Mark::Note(_)) {
                widget.set_size_request(w - 4, -1);
            }
        }
    }

    fn page_setup_dialog(&self) {
        let s = self.setup.borrow().clone();
        let (dialog, card) = widgets::dialog("Page setup", 480);
        let list = widgets::vbox(0);
        list.add_css_class("group-list");
        let paper = Rc::new(RefCell::new(s.paper.clone()));
        let pp = paper.clone();
        list.append(&widgets::segmented_row(
            "Paper",
            "",
            widgets::opts(&crate::print::PAPERS.map(|p| (p.0, p.1))),
            &s.paper,
            move |v| *pp.borrow_mut() = v,
        ));
        let landscape = Rc::new(Cell::new(s.landscape));
        let ll = landscape.clone();
        list.append(&widgets::segmented_row(
            "Orientation",
            "",
            widgets::opts(&[("portrait", "Portrait"), ("landscape", "Landscape")]),
            if s.landscape { "landscape" } else { "portrait" },
            move |v| ll.set(v == "landscape"),
        ));
        // Margins in inches on US paper, centimetres on A4 and A5.
        let metric = crate::print::Paper::metric(&s.paper);
        let (unit, per) = if metric { ("cm", 72.0 / 2.54) } else { ("in", 72.0) };
        let spins: Vec<gtk::SpinButton> = s
            .margins
            .iter()
            .map(|m| {
                let sp = gtk::SpinButton::with_range(0.5 * 72.0 / per, 2.0 * 72.0 / per, 0.05);
                sp.set_digits(2);
                sp.set_value(m / per);
                sp
            })
            .collect();
        for (i, name) in ["Top margin", "Bottom margin", "Left margin", "Right margin"].iter().enumerate() {
            let line = widgets::hbox(6);
            line.append(&spins[i]);
            line.append(&widgets::label(unit, "dim"));
            list.append(&widgets::row(name, "", Some(line.upcast_ref())));
        }
        let header = gtk::Entry::new();
        header.set_text(&s.header);
        header.set_max_length(120);
        header.set_placeholder_text(Some("None"));
        list.append(&widgets::row("Header", "Repeats at the top of every page.", Some(header.upcast_ref())));
        let footer = gtk::Entry::new();
        footer.set_text(&s.footer);
        footer.set_max_length(120);
        footer.set_placeholder_text(Some("None"));
        list.append(&widgets::row("Footer", "Repeats at the bottom of every page.", Some(footer.upcast_ref())));
        let numbers = gtk::Switch::new();
        numbers.set_active(s.page_numbers);
        list.append(&widgets::row("Page numbers", "At the bottom of every page.", Some(numbers.upcast_ref())));
        let plain_first = gtk::Switch::new();
        plain_first.set_active(s.plain_first_page);
        list.append(&widgets::row(
            "Plain first page",
            "No header, footer or page number on the first page, for a letter or a title page.",
            Some(plain_first.upcast_ref()),
        ));
        let columns = Rc::new(Cell::new(s.columns()));
        let cc = columns.clone();
        list.append(&widgets::segmented_row(
            "Columns",
            "Print and PDF set the text in columns; the editor shows one column's width.",
            widgets::opts(&[("1", "One"), ("2", "Two"), ("3", "Three")]),
            &s.columns().to_string(),
            move |v| cc.set(v.parse().unwrap_or(1)),
        ));
        card.append(&list);
        let buttons = widgets::hbox(8);
        buttons.set_halign(gtk::Align::End);
        buttons.set_margin_top(6);
        let cancel = gtk::Button::with_label("Cancel");
        let apply = gtk::Button::with_label("Apply");
        apply.add_css_class("suggested-action");
        buttons.append(&cancel);
        buttons.append(&apply);
        card.append(&buttons);
        let d = dialog.clone();
        cancel.connect_clicked(move |_| d.close());
        let d = dialog.clone();
        let me = self.me.borrow().clone();
        apply.connect_clicked(move |_| {
            d.close();
            let Some(w) = me.upgrade() else { return };
            let mut margins = [0.0; 4];
            for (i, sp) in spins.iter().enumerate() {
                margins[i] = (sp.value() * per).clamp(36.0, 144.0);
            }
            let new = PageSetup {
                paper: paper.borrow().clone(),
                landscape: landscape.get(),
                margins,
                header: header.text().to_string(),
                footer: footer.text().to_string(),
                page_numbers: numbers.is_active(),
                plain_first_page: plain_first.is_active(),
                columns: columns.get() as u8,
                column_gap: w.setup.borrow().column_gap,
            };
            if new != *w.setup.borrow() {
                w.checkpoint();
                *w.setup.borrow_mut() = new;
                w.apply_page();
                w.edited();
            }
        });
        dialog.present();
    }

    // ---------- Commands ----------

    /// The text a character command applies to: the selection, else the word
    /// the cursor is inside. None at a word's edge (the command sets the typing format).
    fn target(&self) -> Option<(gtk::TextIter, gtk::TextIter)> {
        let b = self.tb();
        if let Some((s, e)) = b.selection_bounds() {
            return Some((s, e));
        }
        let it = self.cursor_iter();
        if it.inside_word() && !it.starts_word() && !it.ends_word() {
            let (mut s, mut e) = (it, it);
            s.backward_word_start();
            e.forward_word_end();
            return Some((s, e));
        }
        None
    }

    fn apply_char(&self, f: impl Fn(&mut TextRun)) {
        match self.target() {
            Some((s, e)) => self.apply_char_range(s.offset(), e.offset(), &f),
            None => {
                f(&mut self.typing.borrow_mut());
                self.sync_controls();
            }
        }
        self.view.grab_focus();
    }

    /// Change the formatting of characters `so..eo`, as one undo step.
    fn apply_char_range(&self, so: i32, eo: i32, f: &dyn Fn(&mut TextRun)) {
        let b = self.tb();
        self.checkpoint();
        // Each stretch keeps its other formatting.
        let mut at = so;
        while at < eo {
            let it = b.iter_at_offset(at);
            let mut next = it;
            next.forward_to_tag_toggle(None::<&gtk::TextTag>);
            let end = next.offset().min(eo).max(at + 1);
            let mut r = buffer::char_format(&it);
            f(&mut r);
            buffer::set_format(b, &it, &b.iter_at_offset(end), &r);
            at = end;
        }
        self.pick_up_format();
        self.edited();
        self.view.grab_focus();
    }

    /// Whether all of the target (or the typing format, with none) has `test`.
    fn all_have(&self, test: &dyn Fn(&TextRun) -> bool) -> bool {
        match self.target() {
            Some((s, e)) => {
                let mut it = s;
                while it < e {
                    if !test(&buffer::char_format(&it)) {
                        return false;
                    }
                    if !it.forward_to_tag_toggle(None::<&gtk::TextTag>) {
                        break;
                    }
                }
                true
            }
            None => test(&self.typing.borrow()),
        }
    }

    fn toggle_flag(&self, field: fn(&mut TextRun) -> &mut bool) {
        // On if any of the target is off, else off.
        let on = !self.all_have(&|r| *field(&mut r.clone()));
        self.apply_char(move |r| *field(r) = on);
    }

    fn selected_lines(&self) -> (i32, i32) {
        let b = self.tb();
        match b.selection_bounds() {
            Some((s, e)) => {
                let last = if e.starts_line() && e.line() > s.line() { e.line() - 1 } else { e.line() };
                (s.line(), last)
            }
            None => {
                let l = self.cursor_iter().line();
                (l, l)
            }
        }
    }

    fn change_paras(&self, f: impl Fn(&mut ParaProps)) {
        self.checkpoint();
        let (first, last) = self.selected_lines();
        buffer::update_para(self.tb(), first, last, f);
        self.edited();
        self.view.grab_focus();
    }

    fn set_style(&self, s: Style) {
        self.change_paras(|p| {
            p.style = s;
            if s.is_heading() {
                p.list = None;
            }
        });
    }

    fn set_align(&self, a: Align) {
        self.change_paras(|p| p.align = a);
    }

    fn toggle_list(&self, l: List) {
        let cur = buffer::para_props(self.tb(), &self.cursor_iter()).list;
        let new = if cur == Some(l) { None } else { Some(l) };
        self.change_paras(|p| {
            p.list = new;
            if new.is_some() && p.style.is_heading() {
                p.style = Style::Normal;
            }
        });
    }

    fn indent(&self, by: i32) {
        self.change_paras(|p| p.indent = (p.indent as i32 + by).clamp(0, MAX_INDENT as i32) as u8);
    }

    fn picture_at_cursor(&self) -> Option<Picture> {
        let it = self.cursor_iter();
        let mut before = it;
        it.paintable()
            .and_downcast::<Picture>()
            .or_else(|| if before.backward_char() { before.paintable().and_downcast::<Picture>() } else { None })
    }

    fn set_picture_width(&self, share: f64) {
        let Some(pic) = self.picture_at_cursor() else { return };
        self.begin_edit();
        pic.set_width_pt(self.setup.borrow().content_width() * share.clamp(0.1, 1.0));
        self.view.queue_resize();
        if pic.wrap().is_some() {
            self.apply_wraps();
        }
        doc::edited();
        self.schedule_pages();
    }

    fn pick_picture(&self) {
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        let f = gtk::FileFilter::new();
        f.set_name(Some("Pictures"));
        f.add_pixbuf_formats();
        filters.append(&f);
        let dialog = gtk::FileDialog::builder().title("Insert a picture").modal(true).filters(&filters).build();
        dialog.set_initial_folder(Some(&gio::File::for_path(crate::paths::home().join("Pictures"))));
        let me = self.me.borrow().clone();
        dialog.open(window::window().as_ref(), gio::Cancellable::NONE, move |res| {
            let (Ok(file), Some(w)) = (res, me.upgrade()) else { return };
            if let Err(e) = w.insert_picture(&file) {
                window::toast(&format!("Couldn't insert that picture: {e:#}"));
            }
        });
    }

    fn insert_picture(&self, file: &gio::File) -> Result<()> {
        // Every picture is kept as PNG, so the PDF and Word files can use it as is.
        let texture = gdk::Texture::from_file(file)?;
        let png = texture.save_to_png_bytes().to_vec();
        let name = {
            let media = self.media.borrow();
            let mut n = media.len() + 1;
            while media.contains_key(&format!("picture-{n}.png")) {
                n += 1;
            }
            format!("picture-{n}.png")
        };
        self.media.borrow_mut().insert(name.clone(), png.clone());
        let width = picture::fitting_width(texture.width(), self.setup.borrow().content_width());
        let pic = Picture::new(&name, &png, width).ok_or_else(|| anyhow::anyhow!("unreadable picture"))?;
        self.checkpoint();
        let mut at = self.cursor_iter();
        self.tb().insert_paintable(&mut at, &pic);
        self.edited();
        self.view.grab_focus();
        Ok(())
    }

    fn insert_table(&self, rows: usize, cols: usize) {
        self.checkpoint();
        let b = self.tb();
        self.loading.set(true);
        let mut at = self.cursor_iter();
        // A table stands on its own line.
        if !at.starts_line() {
            b.insert(&mut at, "\n");
        }
        if !at.ends_line() {
            b.insert(&mut at, "\n");
            at.backward_char();
        }
        let line = at.line();
        buffer::set_para(b, line, line, ParaProps::default());
        let anchor = b.create_child_anchor(&mut at);
        let w = (self.make_table())(&Table::new(rows, cols));
        w.set_width(self.view.width_request() - 4);
        self.view.add_child_at_anchor(&w.grid, &anchor);
        self.tables.borrow_mut().push((anchor, w.clone()));
        self.loading.set(false);
        self.edited();
        if let Some(e) = w.entries().first() {
            e.grab_focus();
        }
    }

    fn delete_table(&self, t: &Rc<TableWidget>) {
        let anchor = self.tables.borrow().iter().find(|(_, w)| Rc::ptr_eq(w, t)).map(|(a, _)| a.clone());
        let Some(anchor) = anchor else { return };
        self.checkpoint();
        let b = self.tb();
        let mut s = b.iter_at_child_anchor(&anchor);
        let mut e = s;
        e.forward_char();
        if e.char() == '\n' {
            e.forward_char();
        }
        b.delete(&mut s, &mut e);
        self.edited();
        self.view.grab_focus();
    }

    // ---------- Outline, pages and spelling ----------

    fn schedule_outline(&self) {
        if let Some(id) = self.outline_job.borrow_mut().take() {
            id.remove();
        }
        let me = self.me.borrow().clone();
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(350), move || {
            if let Some(w) = me.upgrade() {
                w.outline_job.borrow_mut().take();
                w.refresh_outline();
            }
        });
        *self.outline_job.borrow_mut() = Some(id);
    }

    fn refresh_outline(&self) {
        while let Some(c) = self.outline.first_child() {
            self.outline.remove(&c);
        }
        let b = self.tb();
        let mut any = false;
        for line in 0..b.line_count() - 1 {
            let Some(start) = b.iter_at_line(line) else { continue };
            let p = buffer::para_props(b, &start);
            if !p.style.is_heading() {
                continue;
            }
            let mut end = start;
            end.forward_to_line_end();
            let text = b.text(&start, &end, false).trim().to_string();
            if text.is_empty() {
                continue;
            }
            any = true;
            let item = gtk::Button::new();
            item.add_css_class("outline-item");
            item.add_css_class(&format!("level-{}", p.style.id()));
            let l = widgets::label(&text, "");
            l.set_ellipsize(gtk::pango::EllipsizeMode::End);
            item.set_child(Some(&l));
            item.set_tooltip_text(Some(&text));
            let me = self.me.borrow().clone();
            item.connect_clicked(move |_| {
                let Some(w) = me.upgrade() else { return };
                window::navigate("document");
                if let Some(it) = w.tb().iter_at_line(line) {
                    w.tb().place_cursor(&it);
                    let mark = w.tb().get_insert();
                    w.view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.1);
                    w.view.grab_focus();
                }
            });
            self.outline.append(&item);
        }
        if !any {
            let l = widgets::label("Headings show up here.", "outline-empty");
            l.set_wrap(true);
            self.outline.append(&l);
        }
        self.refresh_contents();
        self.apply_wraps();
    }

    fn schedule_pages(&self) {
        if let Some(id) = self.pages_job.borrow_mut().take() {
            id.remove();
        }
        let me = self.me.borrow().clone();
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
            if let Some(w) = me.upgrade() {
                w.pages_job.borrow_mut().take();
                let n = layout::paginate(&w.document(), &w.media.borrow()).count();
                w.pages.set(n);
                doc::status_changed();
            }
        });
        *self.pages_job.borrow_mut() = Some(id);
    }

    /// Put a new spell checker on the buffer: it counts the text afresh.
    fn attach_spelling(&self) {
        let sp = libspelling::TextBufferAdapter::new(&self.buffer, &self.checker);
        self.view.set_extra_menu(Some(&sp.menu_model()));
        self.view.insert_action_group("spelling", Some(&sp));
        *self.spelling.borrow_mut() = Some(sp);
        self.apply_spelling();
    }

    /// Take the spell checker off, with its underlines.
    fn detach_spelling(&self) {
        let Some(sp) = self.spelling.borrow_mut().take() else { return };
        let tag = sp.tag();
        self.view.insert_action_group("spelling", None::<&gtk::gio::ActionGroup>);
        self.view.set_extra_menu(None::<&gtk::gio::MenuModel>);
        // Disposing stops it following the buffer right away (not whenever its
        // last reference goes), before its underline tag is taken out.
        // SAFETY: `sp` is dropped right after and not used again; the view no
        // longer holds its menu or actions.
        unsafe { sp.run_dispose() };
        drop(sp);
        if let Some(tag) = tag {
            let b = self.tb();
            b.remove_tag(&tag, &b.start_iter(), &b.end_iter());
            b.tag_table().remove(&tag);
        }
    }

    fn apply_spelling(&self) {
        let p = prefs::get();
        let Some(sp) = self.spelling.borrow().clone() else { return };
        if let Some(checker) = sp.checker() {
            let lang = if p.spelling_language.is_empty() {
                libspelling::Provider::default().default_code().map(|c| c.to_string()).unwrap_or_default()
            } else {
                p.spelling_language.clone()
            };
            if !lang.is_empty() {
                checker.set_language(&lang);
            }
        }
        sp.set_enabled(p.spellcheck);
    }

    // ---------- Find ----------

    fn search_flags(case: bool) -> gtk::TextSearchFlags {
        let mut f = gtk::TextSearchFlags::TEXT_ONLY | gtk::TextSearchFlags::VISIBLE_ONLY;
        if !case {
            f |= gtk::TextSearchFlags::CASE_INSENSITIVE;
        }
        f
    }

    fn selection_matches(&self, query: &str, case: bool) -> bool {
        let Some((s, e)) = self.tb().selection_bounds() else { return false };
        let t = self.tb().text(&s, &e, false);
        if case { t == query } else { t.to_lowercase() == query.to_lowercase() }
    }
}

fn font_families(view: &impl IsA<gtk::Widget>) -> Vec<String> {
    let mut names: Vec<String> = view.pango_context().list_families().iter().map(|f| f.name().to_string()).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    if !names.iter().any(|n| n == DEFAULT_FONT) {
        names.insert(0, DEFAULT_FONT.to_string());
    }
    names
}

impl Find for Writer {
    fn find(&self, query: &str, case: bool, forward: bool) -> bool {
        let b = self.tb();
        let flags = Writer::search_flags(case);
        let (sel_s, sel_e) = b.selection_bounds().unwrap_or_else(|| {
            let c = self.cursor_iter();
            (c, c)
        });
        let hit = if forward {
            sel_e.forward_search(query, flags, None).or_else(|| b.start_iter().forward_search(query, flags, None))
        } else {
            sel_s.backward_search(query, flags, None).or_else(|| b.end_iter().backward_search(query, flags, None))
        };
        match hit {
            Some((s, e)) => {
                b.select_range(&s, &e);
                let mark = b.get_insert();
                self.view.scroll_to_mark(&mark, 0.1, false, 0.0, 0.0);
                true
            }
            None => false,
        }
    }

    fn replace(&self, query: &str, with: &str, case: bool) -> bool {
        if !self.selection_matches(query, case) {
            return Find::find(self, query, case, true);
        }
        self.checkpoint();
        let b = self.tb();
        if let Some((mut s, mut e)) = b.selection_bounds() {
            let format = buffer::char_format(&s);
            b.delete(&mut s, &mut e);
            *self.typing.borrow_mut() = format;
            b.insert(&mut s, with);
        }
        Find::find(self, query, case, true)
    }

    fn replace_all(&self, query: &str, with: &str, case: bool) -> usize {
        if query.is_empty() {
            return 0;
        }
        self.checkpoint();
        let b = self.tb();
        let flags = Writer::search_flags(case);
        let mut n = 0;
        let mut from = b.start_iter().offset();
        loop {
            let it = b.iter_at_offset(from);
            let Some((mut s, mut e)) = it.forward_search(query, flags, None) else { break };
            let format = buffer::char_format(&s);
            let at = s.offset();
            b.delete(&mut s, &mut e);
            *self.typing.borrow_mut() = format;
            b.insert(&mut s, with);
            from = at + with.chars().count() as i32;
            n += 1;
            if n > 100_000 {
                break;
            }
        }
        // Table cells too.
        for (_, t) in self.tables.borrow().iter() {
            for e in t.entries() {
                let text = e.text().to_string();
                let replaced = if case { text.replace(query, with) } else { replace_ci(&text, query, with) };
                if replaced != text {
                    n += if case {
                        text.matches(query).count()
                    } else {
                        text.to_lowercase().matches(&query.to_lowercase()).count()
                    };
                    e.set_text(&replaced);
                }
            }
        }
        n
    }

    fn count(&self, query: &str, case: bool) -> usize {
        let b = self.tb();
        let (s, e) = b.bounds();
        b.remove_tag_by_name("find", &s, &e);
        if query.is_empty() {
            return 0;
        }
        let flags = Writer::search_flags(case);
        let mut n = 0;
        let mut it = b.start_iter();
        while let Some((s, e)) = it.forward_search(query, flags, None) {
            b.apply_tag_by_name("find", &s, &e);
            n += 1;
            it = e;
            if n > 10_000 {
                break;
            }
        }
        n
    }

    fn done(&self) {
        let b = self.tb();
        let (s, e) = b.bounds();
        b.remove_tag_by_name("find", &s, &e);
    }
}

fn replace_ci(text: &str, query: &str, with: &str) -> String {
    let lower = text.to_lowercase();
    let q = query.to_lowercase();
    if q.is_empty() || lower.len() != text.len() {
        return text.to_string();
    }
    let mut out = String::new();
    let mut last = 0;
    for (i, _) in lower.match_indices(&q) {
        out.push_str(&text[last..i]);
        out.push_str(with);
        last = i + q.len();
    }
    out.push_str(&text[last..]);
    out
}

impl doc::Editor for Writer {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn sidebar(&self) -> Option<SidebarGroup> {
        Some(SidebarGroup { title: "Outline".into(), add: None, widget: self.outline.clone().upcast() })
    }

    fn templates(&self) -> &'static [Template] {
        templates::TEMPLATES
    }

    fn new_document(&self, template: Option<&str>) {
        let mut d = template.and_then(templates::build).unwrap_or_default();
        d.page.paper = Paper::preferred().id().into();
        d.tidy();
        self.load_fresh(&d, &BTreeMap::new());
    }

    fn open(&self, path: &Path) -> Result<()> {
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let (d, media) = match ext.as_str() {
            "nwrite" => Writer::read_native(&std::fs::read(path)?)?,
            "docx" => docx::read(&std::fs::read(path)?)?,
            "odt" => odt::read(&std::fs::read(path)?)?,
            "md" | "markdown" => markdown::read(&String::from_utf8_lossy(&std::fs::read(path)?), path.parent()),
            "txt" | "text" => (markdown::read_text(&String::from_utf8_lossy(&std::fs::read(path)?)), BTreeMap::new()),
            _ => bail!("Nexus Write opens .nwrite, .docx, .odt, .md and .txt files"),
        };
        self.load_fresh(&d, &media);
        Ok(())
    }

    fn save(&self, path: &Path) -> Result<Vec<String>> {
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        match ext.as_str() {
            "nwrite" => {
                crate::cmd::atomic_write(path, self.native_bytes()?)?;
                Ok(Vec::new())
            }
            "docx" => {
                let d = self.document();
                let (bytes, lost) = docx::write(&d, &self.media.borrow())?;
                crate::cmd::atomic_write(path, bytes)?;
                Ok(lost)
            }
            "odt" => {
                let d = self.document();
                let (bytes, lost) = odt::write(&d, &self.media.borrow())?;
                crate::cmd::atomic_write(path, bytes)?;
                Ok(lost)
            }
            "md" | "markdown" => {
                let d = self.document();
                let folder = markdown::pictures_dir(path);
                let (text, pictures, lost) = markdown::write(&d, &folder);
                // Pictures go in a folder beside the file.
                if !pictures.is_empty() {
                    let dir = path.with_file_name(&folder);
                    let media = self.media.borrow();
                    for name in &pictures {
                        if let Some(bytes) = media.get(name) {
                            crate::cmd::atomic_write(&dir.join(name), bytes)?;
                        }
                    }
                }
                crate::cmd::atomic_write(path, text)?;
                Ok(lost)
            }
            "txt" | "text" => {
                crate::cmd::atomic_write(path, markdown::write_text(&self.document()))?;
                Ok(vec!["formatting, pictures and tables' layout".into()])
            }
            _ => bail!("Nexus Write saves .nwrite, .docx, .odt, .md and .txt files"),
        }
    }

    fn snapshot(&self) -> Result<Vec<u8>> {
        self.native_bytes()
    }

    fn restore(&self, bytes: &[u8]) -> Result<()> {
        let (d, media) = Writer::read_native(bytes)?;
        self.load_fresh(&d, &media);
        Ok(())
    }

    fn undo(&self) {
        if let Some(id) = self.group.borrow_mut().take() {
            id.remove();
        }
        let Some(prev) = self.undo.borrow_mut().pop() else { return };
        let now = self.snapshot_now();
        self.redo.borrow_mut().push(now);
        self.restore_snapshot(prev);
    }

    fn redo(&self) {
        if let Some(id) = self.group.borrow_mut().take() {
            id.remove();
        }
        let Some(next) = self.redo.borrow_mut().pop() else { return };
        let now = self.snapshot_now();
        self.undo.borrow_mut().push(now);
        self.restore_snapshot(next);
    }

    fn status(&self) -> String {
        let b = self.tb();
        let (s, e) = b.bounds();
        let words = b.text(&s, &e, false).split_whitespace().count();
        let pages = self.pages.get().max(1);
        let paper = self.setup.borrow().paper();
        let words = crate::fmt::count(words, "word", "words");
        let pages = crate::fmt::count(pages, "page", "pages");
        if let Some((s, e)) = b.selection_bounds() {
            let n = b.text(&s, &e, false).split_whitespace().count();
            return format!("{} selected · {words} · {pages} · {}", crate::fmt::count(n, "word", "words"), paper.name());
        }
        if self.tracking.get() {
            return format!("{words} · {pages} · {} · Tracking changes", paper.name());
        }
        let cols = self.setup.borrow().columns();
        if cols > 1 {
            return format!("{words} · {pages} · {} · {cols} columns", paper.name());
        }
        format!("{words} · {pages} · {}", paper.name())
    }

    fn find(&self) -> Option<Rc<dyn Find>> {
        Some(self.rc())
    }

    fn pages(&self) -> Rc<dyn Pages> {
        Rc::new(layout::paginate(&self.document(), &self.media.borrow()))
    }

    fn set_zoom(&self, zoom: f64) {
        // The scrolled window put the zoom box in a viewport.
        let zoomed = self.scroll.child().and_then(|v| v.first_child()).and_downcast::<gtk::Box>();
        if let Some(b) = zoomed {
            widgets::set_zoom(&b, zoom);
        }
        self.fit_paper();
    }

    fn focus(&self) {
        self.view.grab_focus();
    }

    fn shortcuts(&self) -> &'static [doc::Shortcut] {
        &[
            (&["Ctrl", "B"], "Bold"),
            (&["Ctrl", "I"], "Italic"),
            (&["Ctrl", "U"], "Underline"),
            (&["Ctrl", "1"], "Heading 1 (Ctrl+2, Ctrl+3 for smaller)"),
            (&["Ctrl", "0"], "Normal text"),
            (&["Ctrl", "Shift", "8"], "Bulleted list"),
            (&["Ctrl", "Shift", "7"], "Numbered list"),
            (&["Tab"], "In a list: indent (Shift+Tab: outdent)"),
            (&["Ctrl", "Shift", "L"], "Align left (E center, R right, J justify)"),
            (&["Ctrl", "Space"], "Clear character formatting"),
            (&["Ctrl", "K"], "Add or edit a link (Ctrl+click opens one)"),
            (&["Ctrl", "Shift", "="], "Superscript (Ctrl+=: subscript)"),
            (&["Ctrl", "Enter"], "Page break"),
            (&["Shift", "Enter"], "New line in the same paragraph"),
            (&["Ctrl", "Alt", "F"], "Footnote (Ctrl+Alt+D: endnote)"),
            (&["Ctrl", "Alt", "M"], "Comment on the selection"),
            (&["Ctrl", "Shift", "T"], "Track changes on or off"),
        ]
    }

    fn key(&self, key: gdk::Key, mods: gdk::ModifierType) -> bool {
        if !self.view.has_focus() {
            return false;
        }
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        let k = key.to_lower();
        if key == gdk::Key::Escape && self.painting() {
            self.stop_painting();
            return true;
        }
        if ctrl && shift && k == gdk::Key::t {
            self.set_tracking(!self.tracking.get());
            return true;
        }
        if self.tracked_key(key, ctrl) {
            return true;
        }
        // Shift+Enter: a new line in the same paragraph.
        if shift && !ctrl && matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
            let b = self.tb();
            b.delete_selection(true, true);
            b.insert_interactive_at_cursor(&LINE_BREAK.to_string(), true);
            self.view.scroll_mark_onscreen(&b.get_insert());
            return true;
        }
        if ctrl && mods.contains(gdk::ModifierType::ALT_MASK) {
            match k {
                gdk::Key::f => self.insert_note(false),
                gdk::Key::d => self.insert_note(true),
                gdk::Key::m => self.add_comment(),
                _ => return false,
            }
            return true;
        }
        if ctrl && shift {
            match k {
                gdk::Key::_8 | gdk::Key::asterisk => self.toggle_list(List::Bullet),
                gdk::Key::_7 | gdk::Key::ampersand => self.toggle_list(List::Number),
                gdk::Key::l => self.set_align(Align::Left),
                gdk::Key::e => self.set_align(Align::Center),
                gdk::Key::r => self.set_align(Align::Right),
                gdk::Key::j => self.set_align(Align::Justify),
                gdk::Key::plus | gdk::Key::equal => self.toggle_script(true),
                _ => return false,
            }
            return true;
        }
        if ctrl {
            match k {
                gdk::Key::b => self.toggle_flag(|r| &mut r.bold),
                gdk::Key::i => self.toggle_flag(|r| &mut r.italic),
                gdk::Key::u => self.toggle_flag(|r| &mut r.underline),
                gdk::Key::k => self.link_dialog(),
                gdk::Key::equal => self.toggle_script(false),
                gdk::Key::_0 => self.set_style(Style::Normal),
                gdk::Key::_1 => self.set_style(Style::H1),
                gdk::Key::_2 => self.set_style(Style::H2),
                gdk::Key::_3 => self.set_style(Style::H3),
                // Clearing formatting keeps the text's comment and link.
                gdk::Key::space => self.apply_char(|r| {
                    *r = TextRun {
                        comment: r.comment,
                        link: r.link.clone(),
                        inserted: r.inserted,
                        deleted: r.deleted,
                        ..Default::default()
                    }
                }),
                gdk::Key::Return | gdk::Key::KP_Enter => self.insert_mark_block(marks::MarkKind::PageBreak),
                _ => return false,
            }
            return true;
        }
        let cursor = self.cursor_iter();
        let p = buffer::para_props(self.tb(), &cursor);
        match key {
            gdk::Key::Tab | gdk::Key::ISO_Left_Tab if p.list.is_some() => {
                self.indent(if shift || key == gdk::Key::ISO_Left_Tab { -1 } else { 1 });
                true
            }
            // Enter on an empty list item ends the list.
            gdk::Key::Return | gdk::Key::KP_Enter if p.list.is_some() && cursor.starts_line() && cursor.ends_line() && !shift => {
                self.change_paras(|p| {
                    p.list = None;
                    p.indent = 0;
                });
                true
            }
            // Backspace at the start of a list item or an indented paragraph
            // takes away the list, then the indent, before joining lines.
            gdk::Key::BackSpace
                if cursor.starts_line()
                    && !self.tb().has_selection()
                    && (p.list.is_some() || p.indent > 0 || p.style == Style::Quote) =>
            {
                self.change_paras(|p| {
                    if p.list.is_some() {
                        p.list = None;
                    } else if p.indent > 0 {
                        p.indent -= 1;
                    } else {
                        p.style = Style::Normal;
                    }
                });
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
pub fn write_sample(path: &Path) {
    let mut d = templates::build("recipe").expect("a template");
    d.blocks.push(Block::Table(Table {
        rows: vec![vec!["Item".into(), "Qty".into()], vec!["Flour".into(), "2 cups".into()]],
        header: true,
    }));
    d.page.page_numbers = true;
    d.page.header = "My recipes".into();
    d.blocks.insert(1, Block::Contents);
    if let Some(Block::Paragraph(p)) = d.blocks.get_mut(2) {
        p.runs.push(Run::Note(Note { note: "Serves four hungry people.".into(), end: false }));
    }
    d.blocks.push(Block::PageBreak);
    d.blocks.push(Block::Paragraph(Paragraph::text(Style::H2, "Variations")));
    d.blocks.push(Block::Paragraph(Paragraph {
        runs: vec![Run::text("Swap the "), Run::styled("butter for olive oil", |r| r.comment = Some(1)), Run::text(" if you like.")],
        ..Default::default()
    }));
    d.comments.push(Comment { id: 1, author: "Ken".into(), text: "Does this still bake well?".into(), ..Default::default() });
    d.blocks.push(Block::Paragraph(Paragraph {
        runs: vec![
            Run::Image(Image { image: "dot.png".into(), width: 110.0, wrap: Some(Side::Right) }),
            Run::text("Serve warm with a spoon of cream. Leftovers keep for three days in the fridge and "),
            Run::styled("reheat well in a low oven", |r| r.inserted = Some(1)),
            Run::styled(" can be microwaved", |r| r.deleted = Some(1)),
            Run::text(". This paragraph runs on long enough to show the text wrapping beside the picture and then carrying on below it at the full width of the page."),
        ],
        ..Default::default()
    }));
    d.revisions.push(Revision { id: 1, author: "Ken".into(), date: "2026-10-05T10:00:00Z".into() });
    d.blocks.push(Block::Paragraph(Paragraph {
        runs: vec![
            Run::text("Oven times by dish: "),
            Run::Drawing(Drawing { drawing: Figure::sample_chart(), width: 300.0, height: 170.0 }),
            Run::Drawing(Drawing {
                drawing: Figure::Shape { shape: crate::present::model::Shape::Star, fill: "#e0a43a".into() },
                width: 60.0,
                height: 60.0,
            }),
        ],
        ..Default::default()
    }));
    d.blocks.push(Block::Section(Section { columns: 2, new_page: false }));
    d.blocks.push(Block::Paragraph(Paragraph {
        runs: vec![
            Run::text(&"Two columns from here on, after a section break that carries on down the same page. ".repeat(6)),
            Run::Note(Note { note: "Endnotes gather at the end of the document.".into(), end: true }),
        ],
        ..Default::default()
    }));
    let dot = crate::present::model::base64(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    )
    .expect("a png");
    let media: BTreeMap<String, Vec<u8>> = [("dot.png".to_string(), dot)].into();
    let odt = path.extension().is_some_and(|e| e == "odt");
    let (bytes, _) = if odt { odt::write(&d, &media) } else { docx::write(&d, &media) }.expect("a document");
    std::fs::write(path, bytes).expect("written");
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::doc::Editor;

    #[test]
    fn replaces_ignoring_case() {
        assert_eq!(replace_ci("Cat cat CAT", "cat", "dog"), "dog dog dog");
        assert_eq!(replace_ci("abc", "x", "y"), "abc");
    }

    fn para(d: &Document, i: usize) -> Paragraph {
        match &d.blocks[i] {
            Block::Paragraph(p) => p.clone(),
            b => panic!("block {i} is {b:?}"),
        }
    }

    fn line_end(w: &Writer, line: i32) -> gtk::TextIter {
        let mut it = w.tb().iter_at_line(line).unwrap();
        if !it.ends_line() {
            it.forward_to_line_end();
        }
        it
    }

    /// Run from `crate::gtk_tests`: GTK lives on one thread, so the editors' checks
    /// run one after another in a single test.
    pub fn editor_behaves() {
        let dir = std::env::temp_dir().join(format!("nxo-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let w = Writer::build();

        // A template survives the trip through the text buffer.
        let mut recipe = templates::build("recipe").unwrap();
        recipe.tidy();
        w.load_fresh(&recipe, &BTreeMap::new());
        assert_eq!(w.document().blocks, recipe.blocks);

        // Autocorrect: the quote just typed curls; "--" becomes a dash.
        w.load_fresh(&Document { blocks: vec![Block::Paragraph(Paragraph::text(Style::Normal, ""))], ..Default::default() }, &BTreeMap::new());
        for ch in ["\"", "H", "i", "-", "-"] {
            let mut at = line_end(&w, 0);
            w.tb().insert(&mut at, ch);
            w.autocorrect(line_end(&w, 0).offset() - 1);
        }
        assert_eq!(para(&w.document(), 0).plain_text(), "\u{201c}Hi\u{2014}");

        // Spacing survives the trip through the buffer.
        let spaced = Document {
            blocks: vec![Block::Paragraph(Paragraph {
                line_spacing: Some(1.15),
                space_before: Some(18.0),
                runs: vec![Run::text("Airy")],
                ..Default::default()
            })],
            ..Default::default()
        };
        w.load_fresh(&spaced, &BTreeMap::new());
        assert_eq!(w.document().blocks, spaced.blocks);

        // Typing continues the formatting before the cursor.
        let d = Document {
            blocks: vec![Block::Paragraph(Paragraph {
                runs: vec![Run::styled("Hello", |r| r.bold = true)],
                ..Default::default()
            })],
            ..Default::default()
        };
        w.load_fresh(&d, &BTreeMap::new());
        let mut at = line_end(&w, 0);
        w.tb().place_cursor(&at);
        w.tb().insert(&mut at, " there");
        let p = para(&w.document(), 0);
        assert_eq!(p.plain_text(), "Hello there");
        assert_eq!(p.runs.len(), 1, "{:?}", p.runs);

        // Bold on a selection, then undo and redo it.
        w.end_group();
        let s = w.tb().iter_at_offset(0);
        let e = w.tb().iter_at_offset(5);
        w.tb().select_range(&s, &e);
        w.toggle_flag(|r| &mut r.bold);
        assert!(matches!(&para(&w.document(), 0).runs[0], Run::Text(t) if !t.bold && t.text == "Hello"));
        w.undo();
        assert!(matches!(&para(&w.document(), 0).runs[0], Run::Text(t) if t.bold && t.text == "Hello there"));
        w.redo();
        assert!(matches!(&para(&w.document(), 0).runs[0], Run::Text(t) if !t.bold));

        // A link on "there"; typing at its end isn't linked, and the link is found whole.
        w.end_group();
        w.set_link(6, 11, Some("https://example.com".into()));
        let mut at = line_end(&w, 0);
        w.tb().place_cursor(&at);
        w.tb().insert(&mut at, "!");
        let linked: Vec<(String, bool)> = para(&w.document(), 0)
            .runs
            .iter()
            .filter_map(|r| if let Run::Text(t) = r { Some((t.text.clone(), t.link.is_some())) } else { None })
            .collect();
        assert_eq!(linked, [("Hello".into(), false), (" ".into(), false), ("there".into(), true), ("!".into(), false)]);
        let (s, e, uri) = w.link_at(&w.tb().iter_at_offset(8)).expect("a link");
        assert_eq!((s.offset(), e.offset(), uri.as_str()), (6, 11, "https://example.com"));
        // Superscript, then subscript in its place.
        w.end_group();
        w.tb().select_range(&w.tb().iter_at_offset(0), &w.tb().iter_at_offset(1));
        w.toggle_script(true);
        assert!(matches!(&para(&w.document(), 0).runs[0], Run::Text(t) if t.superscript && t.text == "H"));
        w.toggle_script(false);
        assert!(matches!(&para(&w.document(), 0).runs[0], Run::Text(t) if t.subscript && !t.superscript));
        for _ in 0..4 {
            w.undo();
        }
        let p = para(&w.document(), 0);
        assert_eq!(p.plain_text(), "Hello there");
        assert!(p.runs.iter().all(|r| matches!(r, Run::Text(t) if t.link.is_none() && !t.superscript && !t.subscript)));

        // Enter after a heading starts normal text; Enter in a list continues it.
        let d = Document {
            blocks: vec![
                Block::Paragraph(Paragraph::text(Style::H2, "Heading")),
                Block::Paragraph(Paragraph { list: Some(List::Bullet), runs: vec![Run::text("item")], ..Default::default() }),
            ],
            ..Default::default()
        };
        w.load_fresh(&d, &BTreeMap::new());
        let mut at = line_end(&w, 0);
        w.tb().place_cursor(&at);
        w.tb().insert(&mut at, "\n");
        let mut at = line_end(&w, 2);
        w.tb().place_cursor(&at);
        w.tb().insert(&mut at, "\nnext");
        let doc = w.document();
        assert_eq!(doc.blocks.len(), 4, "{:?}", doc.blocks);
        assert_eq!(para(&doc, 0).style, Style::H2);
        assert_eq!(para(&doc, 1).style, Style::Normal);
        assert_eq!(para(&doc, 2).list, Some(List::Bullet));
        assert_eq!(para(&doc, 3).list, Some(List::Bullet));
        assert_eq!(para(&doc, 3).plain_text(), "next");

        // Joining two paragraphs keeps the first one's look.
        let mut s = line_end(&w, 0);
        let mut e = s;
        e.forward_char();
        w.tb().delete(&mut s, &mut e);
        assert_eq!(para(&w.document(), 0).style, Style::H2);

        // The final newline can't be deleted.
        let (mut s, mut e) = w.tb().bounds();
        w.tb().delete(&mut s, &mut e);
        assert_eq!(w.tb().text(&w.tb().start_iter(), &w.tb().end_iter(), true), "\n");

        // Save and open again.
        let journal = templates::build("journal").unwrap();
        w.load_fresh(&journal, &BTreeMap::new());
        let path = dir.join("journal.nwrite");
        Editor::save(&*w, &path).unwrap();
        let before = w.document();
        w.load_fresh(&Document::default(), &BTreeMap::new());
        Editor::open(&*w, &path).unwrap();
        assert_eq!(w.document(), before);
        // As OpenDocument: the words and headings come back.
        let odt = dir.join("journal.odt");
        assert!(Editor::save(&*w, &odt).unwrap().is_empty());
        Editor::open(&*w, &odt).unwrap();
        assert_eq!(w.document().headings(), before.headings());
        Editor::open(&*w, &path).unwrap();

        // PDF export.
        let pdf = dir.join("journal.pdf");
        crate::print::export_pdf(w.pages().as_ref(), &pdf).unwrap();
        assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));

        // A file from the Qt version opens.
        let old = dir.join("old.nwrite");
        {
            use std::io::Write;
            let mut z = zip::ZipWriter::new(std::fs::File::create(&old).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("manifest.json", o).unwrap();
            z.write_all(br#"{"format":"nexus-write","version":2,"content":"document.html","page":{"top":72,"bottom":72,"left":90,"right":90,"header":"Hi","footer":"","pageNumbers":true}}"#).unwrap();
            z.start_file("document.html", o).unwrap();
            z.write_all(b"<html><body><h1>Old</h1><p>text <b>bold</b></p></body></html>").unwrap();
            z.finish().unwrap();
        }
        Editor::open(&*w, &old).unwrap();
        let d = w.document();
        assert_eq!(para(&d, 0).style, Style::H1);
        assert_eq!(d.page.margins[2], 90.0);
        assert!(d.page.page_numbers);

        // Find and replace.
        assert_eq!(Find::count(&*w, "TEXT", false), 1);
        assert_eq!(Find::replace_all(&*w, "text", "words", false), 1);
        assert_eq!(para(&w.document(), 1).plain_text(), "words bold");

        // Page breaks, contents, footnotes and comments survive the buffer and undo.
        let rich = Document {
            blocks: vec![
                Block::Contents,
                Block::Paragraph(Paragraph::text(Style::H1, "Intro")),
                Block::Paragraph(Paragraph {
                    runs: vec![
                        Run::styled("Said", |r| r.comment = Some(1)),
                        Run::Note(Note { note: "Source.".into(), end: false }),
                        Run::text(" so."),
                    ],
                    ..Default::default()
                }),
                Block::PageBreak,
                Block::Paragraph(Paragraph::text(Style::Normal, "Next page")),
            ],
            comments: vec![Comment { id: 1, text: "Who said?".into(), ..Default::default() }],
            ..Default::default()
        };
        w.load_fresh(&rich, &BTreeMap::new());
        assert_eq!(w.document(), rich);
        w.refresh_comments();
        assert!(w.comment_panel.is_visible());
        // A new footnote at the end of the last paragraph, numbered after the first.
        w.end_group();
        let at = line_end(&w, 4);
        w.tb().place_cursor(&at);
        w.insert_note(false);
        let d = w.document();
        assert!(matches!(para(&d, 4).runs.last(), Some(Run::Note(_))));
        let numbers: Vec<String> = w
            .marks
            .borrow()
            .iter()
            .filter_map(|(_, m, wd)| {
                if let marks::Mark::Note(_) = m { wd.downcast_ref::<gtk::Button>()?.child()?.downcast::<gtk::Label>().ok().map(|l| l.text().to_string()) } else { None }
            })
            .collect();
        assert_eq!(numbers.len(), 2);
        assert!(numbers.contains(&"1".to_string()) && numbers.contains(&"2".to_string()));
        Editor::undo(&*w);
        assert_eq!(w.document(), rich);
        // A page break inserted mid-text stands on its own line.
        let at = line_end(&w, 1);
        w.tb().place_cursor(&at);
        w.insert_mark_block(marks::MarkKind::PageBreak);
        assert!(matches!(w.document().blocks[2], Block::PageBreak));
        // Tracked changes: typing is marked as added, deleting marks old text.
        w.load_fresh(&Document { blocks: vec![Block::Paragraph(Paragraph::text(Style::Normal, "one two"))], ..Default::default() }, &BTreeMap::new());
        w.set_tracking(true);
        w.end_group();
        let mut at = line_end(&w, 0);
        w.tb().place_cursor(&at);
        w.pick_up_format();
        w.tb().insert(&mut at, " three");
        let mark = |w: &Writer| -> Vec<(String, bool, bool)> {
            para(&w.document(), 0)
                .runs
                .iter()
                .filter_map(|r| if let Run::Text(t) = r { Some((t.text.clone(), t.inserted.is_some(), t.deleted.is_some())) } else { None })
                .collect()
        };
        assert_eq!(mark(&w), [("one two".into(), false, false), (" three".into(), true, false)]);
        // Backspace on added text removes it; on old text it marks it.
        assert!(w.tracked_key(gdk::Key::BackSpace, false));
        assert_eq!(mark(&w)[1].0, " thre");
        w.tb().place_cursor(&w.tb().iter_at_offset(3));
        assert!(w.tracked_key(gdk::Key::BackSpace, false));
        assert_eq!(
            mark(&w),
            [("on".into(), false, false), ("e".into(), false, true), (" two".into(), false, false), (" thre".into(), true, false)]
        );
        // Undoable, then accept all: the deletion goes, the addition stays plain.
        Editor::undo(&*w);
        assert_eq!(mark(&w).len(), 2);
        Editor::redo(&*w);
        w.settle(true, None);
        assert_eq!(mark(&w), [("on two thre".into(), false, false)]);
        assert!(w.document().revisions.is_empty());
        w.set_tracking(false);

        // A picture wrapped at the left: kept, and its paragraph makes room.
        let png = crate::present::model::base64(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        )
        .unwrap();
        let mut media = BTreeMap::new();
        media.insert("p.png".to_string(), png);
        let wrapped = Document {
            blocks: vec![Block::Paragraph(Paragraph {
                runs: vec![Run::Image(Image { image: "p.png".into(), width: 120.0, wrap: Some(Side::Left) }), Run::text("Beside it")],
                ..Default::default()
            })],
            ..Default::default()
        };
        w.load_fresh(&wrapped, &media);
        assert_eq!(w.document().blocks, wrapped.blocks);
        w.apply_wraps();
        let names: Vec<String> = w.tb().start_iter().tags().iter().filter_map(|t| t.name().map(|n| n.to_string())).collect();
        assert!(names.iter().any(|n| n.starts_with("wl:")), "{names:?}");
        w.tb().place_cursor(&w.tb().start_iter());
        w.set_picture_wrap(None);
        assert!(matches!(para(&w.document(), 0).runs[0], Run::Image(Image { wrap: None, .. })));

        // A shape and a chart in the text survive the buffer, and print.
        let drawn = Document {
            blocks: vec![Block::Paragraph(Paragraph {
                runs: vec![
                    Run::text("x"),
                    Run::Drawing(Drawing { drawing: Figure::Shape { shape: crate::present::model::Shape::Hexagon, fill: "#5273cf".into() }, width: 60.0, height: 60.0 }),
                ],
                ..Default::default()
            })],
            ..Default::default()
        };
        w.load_fresh(&drawn, &BTreeMap::new());
        assert_eq!(w.document().blocks, drawn.blocks);
        w.tb().place_cursor(&line_end(&w, 0));
        assert!(w.figure_at_cursor().is_some());
        w.change_figure(|d| d.width = 120.0);
        assert!(matches!(para(&w.document(), 0).runs[1], Run::Drawing(Drawing { width: 120.0, .. })));
        w.insert_chart();
        crate::print::export_pdf(w.pages().as_ref(), &dir.join("drawn.pdf")).unwrap();

        // Sections and endnotes survive the buffer; endnotes count i, ii.
        let sectioned = Document {
            blocks: vec![
                Block::Paragraph(Paragraph {
                    runs: vec![
                        Run::text("A"),
                        Run::Note(Note { note: "Foot.".into(), end: false }),
                        Run::Note(Note { note: "End one.".into(), end: true }),
                        Run::Note(Note { note: "End two.".into(), end: true }),
                    ],
                    ..Default::default()
                }),
                Block::Section(Section { columns: 3, new_page: false }),
                Block::Paragraph(Paragraph::text(Style::Normal, "B")),
            ],
            ..Default::default()
        };
        w.load_fresh(&sectioned, &BTreeMap::new());
        assert_eq!(w.document().blocks, sectioned.blocks);
        let labels: Vec<String> = w
            .marks
            .borrow()
            .iter()
            .filter_map(|(_, m, wd)| {
                if let marks::Mark::Note(_) = m { wd.downcast_ref::<gtk::Button>()?.child()?.downcast::<gtk::Label>().ok().map(|l| l.text().to_string()) } else { None }
            })
            .collect();
        assert_eq!(labels, ["1", "i", "ii"]);

        // Deleting the commented text drops its comment.
        w.load_fresh(&rich, &BTreeMap::new());
        let (mut s, mut e) = (w.tb().iter_at_line(2).unwrap(), w.tb().iter_at_line(2).unwrap());
        e.forward_chars(4);
        w.tb().delete(&mut s, &mut e);
        assert!(w.document().comments.is_empty());
        // The format painter: characters onto a selection, a paragraph's look onto a clicked paragraph.
        let doc = Document {
            blocks: vec![
                Block::Paragraph(Paragraph {
                    runs: vec![Run::styled("Loud", |r| {
                        r.bold = true;
                        r.color = Some("#cc0000".into());
                    })],
                    ..Default::default()
                }),
                Block::Paragraph(Paragraph::text(Style::Normal, "quiet words here")),
                Block::Paragraph(Paragraph { style: Style::H2, align: Align::Center, ..Paragraph::text(Style::H2, "Heading") }),
                Block::Paragraph(Paragraph::text(Style::Normal, "plain")),
            ],
            ..Default::default()
        };
        w.load_fresh(&doc, &BTreeMap::new());
        let b = w.tb().clone();
        b.place_cursor(&b.iter_at_offset(1));
        painter::start_now(&w, false);
        assert!(w.painting());
        let line1 = b.iter_at_line(1).unwrap().offset();
        b.select_range(&b.iter_at_offset(line1 + 6), &b.iter_at_offset(line1 + 11));
        painter::paint_now(&w);
        assert!(!w.painting(), "one click paints once");
        let Block::Paragraph(p) = &w.document().blocks[1] else { panic!() };
        assert!(matches!(&p.runs[1], Run::Text(t) if t.text == "words" && t.bold && t.color.as_deref() == Some("#cc0000")));
        // A paragraph's look, by clicking in another paragraph; double-click keeps it on.
        b.place_cursor(&b.iter_at_line(2).unwrap());
        painter::start_now(&w, true);
        b.place_cursor(&b.iter_at_line(3).unwrap());
        painter::paint_now(&w);
        assert!(w.painting());
        let Block::Paragraph(p) = &w.document().blocks[3] else { panic!() };
        assert_eq!((p.style, p.align), (Style::H2, Align::Center));
        w.stop_painting();

        let _ = std::fs::remove_dir_all(dir);
    }
}
