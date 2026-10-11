# Nexus Office Suite — notes for working on this repo

- One binary, `nexus-office`, runs as Write, Sheets or Present: `--app write`, or the
  launcher scripts `nexus-write` / `nexus-sheets` / `nexus-present` (`packaging/`).
  Each is its own GApplication (`io.github.design_nexus.Write` …) with its own window.
  Config, data and state folders are shared: `nexus-office`. GTK4 (gtk4-rs 0.11), no
  libadwaita. The look follows `~/Projects/STYLE.md`; the shell (`theme.rs`, `prefs.rs`,
  `widgets.rs`, `settings_dialog.rs`, `style.css`, `paths.rs`, `cmd.rs`, `fmt.rs`)
  started as copies of Nexus PDF's. Every window colour is a `@theme_*` token. Page
  content (paper, ink, cells, slides, charts) keeps fixed document colours so the screen
  matches print; don't theme it.
- **Shell** (`window.rs`, `home.rs`, `doc.rs`): the window has two pages, Home and the
  document. `doc.rs` owns the open file (path, dirty, unsaved-changes guard, Save/Save
  As by extension, PDF export, print, crash-recovery snapshots in
  `~/.local/state/nexus-office/recovery/<app>/`). Each app implements `doc::Editor`
  (widget, sidebar group, templates, open/save/snapshot, undo/redo, status, `Find`,
  `Pages`, keys). Editors call `doc::edited()` on every change and
  `doc::status_changed()` when the readout may change. The find bar is inserted after
  the editor widget's first child (its toolbar). Toolbars use `widgets::wrap_row` (a
  `LayoutManager` that wraps groups onto more lines at half-screen width).
  Zoom (`doc::zoom`/`set_zoom`, kept in `~/.local/state/nexus-office/zoom-<app>`, not
  the shared settings) reaches editors through `Editor::set_zoom`. Write and Sheets
  wrap their view in `widgets::zoom_box` (a layout manager that allocates the child
  at 1/zoom with a scale transform, so GTK maps input back); Present multiplies its
  fitted slide scale and lets the canvas grow inside a scrolled window.
- **Saving**: `save(path)` writes the format the extension names and returns what that
  format couldn't keep; `doc.rs` shows that note once per file and format.
  Native files are ZIPs (`container.rs`): `mimetype` first and stored (byte 38, used by
  `packaging/nexus-office.xml`), `manifest.json` (version 3), `content.json`, `media/`.
  Qt-era files: `.nwrite` ZIP with `document.html` (`write/legacy.rs`), `.nsheets` and
  `.npresent` plain JSON (`from_legacy_json`).
- **Autosave** (`prefs.autosave`): `doc::edited` schedules `write_quietly` a few seconds
  after the last change when the file has a path; `guard` saves instead of asking.
- **Printing and PDF** share one renderer per app (`print::Pages`), laid out in points
  with `print::text_context()` (72 dpi, no hinting).
- **Write** (`write/`): a `TextView` subclass (`view.rs`, draws list markers in the
  margin) over a `sourceview5::Buffer` (libspelling needs one). Formatting is tags
  (`buffer.rs` explains the scheme). The buffer always ends with a newline so the last
  paragraph has a character to carry its paragraph tags. Undo is our own (whole-document
  snapshots, one per burst of typing or per command), because GTK's undo ignores tags.
  Pictures are `picture::Picture` paintables (always PNG); tables are child-anchor
  widgets (`table.rs`), and so are page breaks, the table of contents and footnote
  marks (`marks.rs`, listed in `Writer::marks`). Comments are `cm:<id>` tags over the
  text plus `Document.comments`; `comments.rs` has their cards and the marks' commands.
  `layout.rs` paginates for PDF/print and the page count: columns, footnotes at each
  column's foot, and a second pass to put page numbers in the table of contents.
  With columns on, the editor is one column wide, so lines break as printed.
  Tracked changes (`changes.rs`) are `ins:<id>` / `del:<id>` tags plus
  `Document.revisions`; while tracking, Backspace/Delete/typing over a selection go
  through `tracked_key` (before the TextView sees them), so deleted text stays.
  Pictures wrapped beside a paragraph (`Image.wrap`) take no room in the line; the page
  view draws them and `wrap.rs` gives the paragraph margin tags (`wl:`/`wr:`/`wb:`,
  never saved). Print sets the first lines narrower beside the picture.
  Endnotes are notes with `end: true` (marks i, ii, iii), printed under "Notes" at
  the end. `Block::Section` sets the columns from there on (the pager keeps a
  section's top on the page); Word keeps a section's settings at its end, so
  `docx.rs` shifts them by one each way. The editor stays the first section's width.
  Paragraph spacing is `Paragraph.line_spacing` (a multiple, Word's `w:line`/240) and
  `space_before`/`space_after` (points; None is the style's), as `ls:`/`sb:`/`sa:` tags;
  print uses Pango's line-height attribute, the same one the `ls:` tag sets.
  Markdown and plain text (`markdown.rs`, pulldown-cmark): code is runs in the
  `monospace` font; pictures are read relative to the file and written to a
  "<name> images" folder beside it. Write is offered for text/markdown and text/plain,
  but `install.sh --default` leaves those types alone.
  Links are `link:<address>` tags (`links.rs`: the Ctrl+K dialog, Ctrl+click, the
  hover tooltip); in Word they're `w:hyperlink` with an external relationship, in a
  PDF a cairo `Link` tag per line. Superscript and subscript are the `sup`/`sub` flags.
  Typing at the end of a link or comment doesn't extend it (`pick_up_format`).
  Smart quotes and symbols (`autocorrect.rs`) replace a single typed character in
  `after_change`, only while the page has focus; multi-character ones are skipped while
  tracking changes.
  Shapes and charts in the text are `Run::Drawing` (`figure.rs`: a paintable drawn
  with Cairo, reusing Present's shapes and Sheets' charts); in Word they're `wps`
  shapes and chart parts.
- **Spelling**: GTK doesn't send commit notifications for child anchors or
  paintables, so libspelling's count of the text drifts (and later asserts) when one
  goes in. The adapter is replaced after each such insert and set aside during loads
  (`attach_spelling` / `detach_spelling`); keep that when adding new anchored widgets.
- **Sheets** (`sheets/`): `model.rs` (no GTK) holds the workbook; `formula.rs` parses to
  an AST, evaluates through `Cells`, and shifts references for copy/fill. `Calc` caches
  every value per change, working top to bottom so chains don't recurse deeply. Sorting
  reorders rows (undoable; refused when merged cells are in the data); filtering is a
  view (`view_rows`). Merged cells (`Sheet.merges`) only show as merged while no filter
  is on. Frozen panes are `Sheet.freeze_rows`/`freeze_cols` (on-screen rows; the grid
  draws the scrolled columns, then the frozen ones over them, each clipped to its pane).
  Number formats are `NumFmt` (the toolbar's five) or `Format.pattern`, an Excel format
  code drawn by `numfmt.rs` and written to Excel as is. Dropdown lists
  (`Sheet.lists`, `lists.rs`) are checked in `commit_raw` (`checked_entry`); in Excel
  they're list data validations. Paste Special is `Workbook::paste_as` with the values
  `copy_with_values` worked out when copying. Highlight rules (`rules.rs`) apply through `Calc::look`, used by the grid and
  print. Functions beyond the core live in `functions.rs`; borders, wrapped cells and
  Summarize (SUMIFS-style formulas on a new sheet) in `summary.rs`. The grid (`grid.rs`) is
  a `DrawingArea` with its own adjustments; selection is in on-screen rows
  (`src_row` maps to the sheet). The cell editor is an overlay `Entry` placed with
  margins. `xlsx.rs` reads with roxmltree (styles, merges, notes and conditional
  formats included) and writes with rust_xlsxwriter. Sheets are tabs under the grid;
  Sheets has no sidebar group, so `window.rs` hides the sidebar on its document page.
  Inserting and deleting rows and columns is `structure.rs` (`Workbook::restructure`:
  rewrites references on every sheet, `$` ones too, and moves merges, rules, charts,
  tables and the filter); the grid's right-click menu is `menu.rs`. Rows can't be
  inserted or deleted while a filter is on. Hidden rows and columns
  (`Sheet.hidden_rows`/`hidden_cols`) are zero-sized through `row_h`/`col_w`
  (`size_of_row`/`size_of_col` give their own size); the grid skips them in drawing
  and `past_hidden` keeps the cursor off them; print leaves them out.
  Charts placed on the sheet (`Sheet.charts`, pixels from the cells' top left) and
  table regions (`Sheet.tables`, styled through `Calc::look`) live in `placed.rs`;
  in Excel they're drawing anchors and tables (the cells a chart spans get explicit
  sizes, as Excel's defaults differ). Formulas parse with a depth limit
  (`formula::MAX_DEPTH`) and `^` works left to right, as in Excel.
- **Present** (`present/`): slides are 960 × 540 (points; PowerPoint's 16:9 page).
  `render.rs` draws a slide for the canvas, thumbnails, slideshow, presenter view and
  PDF alike. Text is edited in an overlay `TextView` styled to the slide's font at the
  canvas scale. `show.rs` is the slideshow (transitions, and builds one click at a
  time); with `presenter_view` on and a second monitor, slides go full screen there and
  the presenter view on this one. `arrange.rs` has multi-select, groups, snap guides
  and align/distribute; `data.rs` the table and chart editors and the transition and
  build menus; `looks.rs` the shapes menu, rotation and borders; `media.rs` video and
  audio (`Kind::Media`, played by `gtk::Video`/`MediaControls` laid over the slide in
  `show.rs`, from files in the cache) and the master slide (`Deck::master`: shown
  through `Deck::composed`, edited through `Deck::editing`/`edit` in master mode). Objects turn about
  their centre (`Object::rotation`; hit-testing goes through `to_local`). Lines
  (`Kind::Line`) run level across the middle of their box and turn with it; their
  colour and width are `Object::outline`; in PowerPoint they're `p:cxnSp` connectors.
  `export.rs` has pictures, handouts and notes pages (the file menu's extra items). Slide charts
  reuse `sheets::chart::draw`. Builds are `Object::build` (in) and `build_out`,
  played in `Slide::steps` order; Magic Move pairs objects with `Object::same_as` and
  is written as PowerPoint's Morph. In the slideshow, digits then Enter go to a slide and
  L toggles the laser pointer (drawn by `Show::paint_extras` over the live slide).
  Layouts are `model::Layout` with fixed frames (`title_frame`, `body_frame`); two
  columns adds `body2_frame` (`Slide.body2`, folded into the body when the layout
  changes away), and picture with caption has `picture_frame`, which the slide's first
  picture fills. In PowerPoint they're placeholders, recognised on the way back by
  position (two bodies side by side; a title low on a slide with a picture).
  The slide sorter (`sorter.rs`) is a `FlowBox` in the `views` stack beside the slide
  view; dragging a tile calls `Deck::move_to`.
- **Office files** are written by hand as XML strings (`write/docx.rs`,
  `present/pptx.rs`, `present/pptx_more.rs` for tables, charts, transitions and
  timing) and read with roxmltree.
- Icons: Lucide, converted from strokes to fills (GTK's symbolic loader fills every
  path) and kept in `data/icons/`. To add one: fetch it from `lucide-static`, replace
  `currentColor` with `#000000`, run it through `picosvg`, save it as
  `data/icons/<name>.svg`, and add the name to `install_icons` in `window.rs`.
- Checks: `cargo clippy --all-targets -- -D warnings`, `cargo test`. The editors'
  offscreen checks need a display and GTK on one thread, so they run in turn from
  `gtk_tests::editors_behave` (`main.rs`); add new GTK checks there, never as their own
  `#[test]` (parallel GTK tests crash). `NXO_SAMPLE_DIR=/tmp/x cargo test -- --ignored
  write_office_samples` writes a .docx, .xlsx and .pptx to check in other software.
  Layout check: `NXO_SNAPSHOT=/tmp/x.png nexus-office --app sheets --new monthly-budget`
  renders the window to a PNG and quits (`NXO_SNAPSHOT_W`/`_H`/`_DELAY`). The window can
  take keyboard focus while it's up (it ignores keys), so keep these runs few. Point
  `XDG_CONFIG_HOME`/`XDG_DATA_HOME`/`XDG_STATE_HOME`/`XDG_CACHE_HOME` at scratch dirs,
  and set `NXO_APP_ID` to run beside an open copy.
