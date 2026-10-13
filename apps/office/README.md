# Nexus Office Suite

Three home office apps for [Omarchy](https://omarchy.org): **Nexus Write** for
documents, **Nexus Sheets** for spreadsheets and **Nexus Present** for slides. They
open and save Word, Excel and PowerPoint files and their OpenDocument counterparts,
export PDF and print. The window takes its colors from your Omarchy theme and fits a
half-screen tile; the page itself keeps its own colors, so what you see is what prints.

## What they do

Every app starts on **Home**: a blank file, starter templates, and your recent files.
Then:

- **Write**
  - Paragraph styles (title, three headings, quote), font, size, bold, italic,
    underline, strikethrough, superscript and subscript, text color and highlight,
    alignment. Change how a style looks (the pen beside the style list): make every
    heading of that kind look like the text at the cursor, or put it back.
  - Format painter: copy the formatting where the cursor is, then select text to give
    it the same look, or click a paragraph to give it the paragraph's style. Double-click
    the brush to paint more than once; <kbd>Esc</kbd> stops.
  - Links (<kbd>Ctrl</kbd>+<kbd>K</kbd>) to web pages, email addresses or files;
    <kbd>Ctrl</kbd>+click opens one. They stay links in Word files and exported PDFs.
  - Line spacing (single, 1.15, 1½, double) and space before and after each
    paragraph.
  - Bulleted and numbered lists with nested levels (<kbd>Tab</kbd> /
    <kbd>Shift</kbd>+<kbd>Tab</kbd>), and indents.
  - Shapes and charts in the text (a chart's numbers are edited in a small table).
  - Pictures, sized with a slider, in the line or at the left or right of a paragraph
    with the text wrapping beside them; tables (right-click a cell to add or remove rows
    and columns).
  - Footnotes (<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>F</kbd>): a number in the text, the
    note at the foot of the page when printed. Endnotes
    (<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>D</kbd>), numbered i, ii, iii, gather under
    "Notes" at the end.
  - Comments on selected text (<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>M</kbd>), shown as
    cards beside the page; edit, resolve or delete them there.
  - A table of contents built from the headings, with page numbers in print and PDF.
  - Page breaks (<kbd>Ctrl</kbd>+<kbd>Enter</kbd>), two or three text columns, and
    section breaks to change the columns partway, on a new page or straight on.
  - Track changes (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd>): added text is
    underlined in green, deleted text stays struck through in red; go through them and
    accept or reject each one, or all at once.
  - Spelling checked as you type (right-click a word for suggestions).
  - Smart quotes and symbols as you type: "quotes" curl, `--` makes a dash —, `...` an
    ellipsis …, `(c)` ©, `(tm)` ™ and `->` →. Turn it off in Settings.
  - Page setup: Letter, A4, Legal or A5, portrait or landscape, margins, a header, a
    footer (or different ones on even pages, or none on the first), page numbers and
    columns.
  - The headings appear in the sidebar as an outline; click one to go there.
  - Starter templates: a personal letter, a recipe and a journal.
- **Sheets**
  - Up to 1,048,576 rows (as in Excel) and 104 columns per sheet, and up to 50 sheets,
    as tabs along the bottom: click to switch, double-click to rename, drag to reorder,
    right-click to copy, move or delete.
  - Insert and delete rows and columns (right-click a cell or a heading, or
    <kbd>Ctrl</kbd>+<kbd>+</kbd> / <kbd>Ctrl</kbd>+<kbd>-</kbd>); formulas on every sheet,
    merged cells, charts, tables and highlight rules follow the cells they point at.
  - Hide rows and columns (right-click their headings, or <kbd>Ctrl</kbd>+<kbd>9</kbd> /
    <kbd>Ctrl</kbd>+<kbd>0</kbd>) and show them again (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>9</kbd>
    / <kbd>0</kbd>, or drag the heading's edge open). Hidden cells still count in
    formulas, but don't print. Excel files keep them hidden.
  - Formulas with cell and range references, `$` absolute references and references to
    other sheets (`'Sheet 2'!B4`), whole columns and rows (`B:B`, `3:3`), the same cell
    on a run of sheets (`Jan:Dec!B4`), names from Excel and OpenDocument files
    (`=TotalIncome`), table references (`Sales[Amount]`, `[@Qty]`) and array constants
    (`{1,2,3}`). Click cells while typing a formula to put their references in. Over 170
    functions:
    - Totals and counts: SUM, AVERAGE, AVERAGEA, MIN, MAX, COUNT, COUNTA, COUNTBLANK,
      PRODUCT, SUMPRODUCT, COUNTIF(S), SUMIF(S), AVERAGEIF(S), MAXIFS, MINIFS.
    - Statistics: MEDIAN, MODE, STDEV, VAR, LARGE, SMALL, RANK.
    - Maths: ABS, SIGN, ROUND, ROUNDUP, ROUNDDOWN, TRUNC, INT, CEILING, FLOOR, MOD,
      POWER, SQRT, EXP, LN, LOG, LOG10, PI, RAND, RANDBETWEEN.
    - Logic: IF, IFS, SWITCH, IFERROR, IFNA, AND, OR, XOR, NOT, ISBLANK, ISNUMBER,
      ISTEXT, ISERROR, ISNA.
    - Lookup and references: VLOOKUP, HLOOKUP, XLOOKUP, XMATCH, LOOKUP, INDEX, MATCH,
      CHOOSE, ROW, ROWS, COLUMN, COLUMNS, OFFSET, INDIRECT, ADDRESS.
    - Maths and statistics beyond the basics: SUBTOTAL, SIN, COS, TAN and their
      inverses, RADIANS, DEGREES, FACT, COMBIN, PERMUT, GCD, LCM, MROUND, ODD, EVEN,
      SUMSQ, VARP, STDEVP, DEVSQ, AVEDEV, MINA, MAXA, MMULT, MINVERSE, MDETERM.
    - Also TIME, WORKDAY, DAYS360, CLEAN, CHAR, CODE, DOLLAR, HYPERLINK, NA,
      ERROR.TYPE and the base converters (HEX2DEC, DEC2BIN…).
    - Lists: UNIQUE, FILTER, SORT, SORTBY, SEQUENCE, TAKE, DROP, CHOOSECOLS,
      CHOOSEROWS, TRANSPOSE, VSTACK, HSTACK, TOCOL, TOROW. Their results spill into the
      cells below and to the right (`#SPILL!` when something is in the way), as do
      ranges and sums over ranges (`=B2:B9*2`, `=FILTER(A2:C9, C2:C9>100)`).
    - Text: CONCAT, TEXTJOIN, TEXTSPLIT, TEXTBEFORE, TEXTAFTER, TEXT, LEN, LEFT, RIGHT,
      MID, FIND, SEARCH, SUBSTITUTE, REPLACE, REPT, UPPER, LOWER, PROPER, TRIM, VALUE,
      EXACT.
    - Money: PMT, IPMT, PPMT, PV, FV, NPER, RATE, NPV, IRR, EFFECT, NOMINAL, SLN.
    - Dates: TODAY, NOW, DATE, YEAR, MONTH, DAY, HOUR, MINUTE, SECOND, DAYS, EDATE,
      EOMONTH, WEEKDAY, WEEKNUM, NETWORKDAYS.
  - Typed dates (`2026-10-05`, `10/5/2026`), money (`$12.50`), percentages and
    thousands separators are understood as numbers.
  - Number formats: two decimals, currency, percent and date on the toolbar; more under
    its arrow (whole numbers, euros, pounds, yen, date styles like "5 Oct 2026", times),
    or type a format code of your own (`#,##0.0 "kg"`). Bold, italic, text and fill
    colors, alignment, and column widths and row heights you drag.
  - Merged cells, cell borders, text that wraps inside its cell (the row grows to fit),
    and comments on cells (a corner mark; rest the pointer on it to read).
  - Summarize: totals, counts, averages, largest or smallest by group (and optionally
    split across a second column) on a new sheet, with formulas that stay up to date.
    Pivot tables in Excel and OpenDocument files open as these summaries, in their
    place.
  - Print setup: print only the selected cells, and shrink a sheet to fit all its
    columns, or all of it, on one page. Excel and OpenDocument files keep both.
  - Highlight rules: color cells that are above or below a number, between two, equal
    to one, contain some text, are blank or an error, are in the top few, or above
    average.
  - Drag the selection by its edge to move it (hold <kbd>Ctrl</kbd> to copy); copy, cut
    and paste (formulas follow the cells), or paste only the values
    (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd>) or only the formatting; drag the corner handle to fill,
    <kbd>Ctrl</kbd>+<kbd>D</kbd> / <kbd>Ctrl</kbd>+<kbd>R</kbd> to fill down or right.
  - Sort by a column, filter rows by what a column contains, keep the first row as a
    header. Keep the top row, the first column, or everything above and left of the
    cursor in view while the rest scrolls.
  - Bar, horizontal bar, stacked bar, line, area, stacked area, pie, doughnut and
    scatter charts of up to six
    series, beside the sheet or placed anywhere on it (drag to move, drag the corner
    to resize, double-click to change).
  - Tables: blocks of cells with a coloured heading row, banded rows and a total row,
    as many as you like on a sheet; they open and save as Excel tables.
  - Checkboxes in cells: click one or press <kbd>Space</kbd> to tick it; formulas see
    TRUE or FALSE.
  - Dropdown lists in cells (right-click → Dropdown list…): choices typed in, or taken
    from other cells. Pick from the cell's arrow or <kbd>Alt</kbd>+<kbd>↓</kbd>; anything
    else typed there is refused. Excel files keep them.
  - The status bar shows the sum, average and count of the selected cells.
  - Starter templates: a monthly budget, a home inventory and a trip budget.
- **Present**
  - Slides with four layouts (title, title and content, section, blank); edit the title
    and text right on the slide, and write speaker notes under it.
  - Ready-made looks, background and text colors, font, sizes, bold and italic.
  - Text boxes, pictures, tables, charts (double-click a table or chart to change
    what's in it) and eight shapes (rectangle, rounded rectangle, ellipse, triangle,
    diamond, hexagon, star, arrow) and lines, with or without arrowheads: drag them,
    resize them by their corner, turn them by the round handle above, nudge them with
    the arrow keys; give them a border (a line's color and thickness) and a drop shadow.
  - Video and audio on slides, played in the slideshow with their own controls.
  - A master slide for what goes on every slide (a logo, a footer, the slide number);
    any slide can leave it out. Drag a placeholder's edge to move a slide's title or
    text (its corner resizes it); on the master, drag them to move them on every slide
    of that layout.
  - <kbd>Shift</kbd>-click to select several; group them (<kbd>Ctrl</kbd>+<kbd>G</kbd>)
    to move them as one; line them up, space them evenly or change which is in front.
    Guides snap what you drag to the slide's middle and edges and to other objects
    (hold <kbd>Alt</kbd> to move freely).
  - Transitions between slides (fade, push, cover, uncover, wipe, split, zoom, dissolve,
    and Magic Move, where what both slides share glides to its new place), and builds
    that bring objects in, or take them away, one click at a time (appear, fade, fly).
  - Thumbnails of every slide in the sidebar; right-click one to copy, move or delete it.
  - **Present** with <kbd>F5</kbd> (or <kbd>Shift</kbd>+<kbd>F5</kbd> from the current
    slide); the slideshow plays the transitions and builds. Type a slide number and press
    <kbd>Enter</kbd> to go there; <kbd>L</kbd> turns on a laser pointer. With a second screen connected, the slides go there and this screen shows
    the current and next slide, your notes and a timer.
  - Export each slide as a picture (1920 × 1080 PNG), handouts with six slides to a page,
    or pages with each slide over its speaker notes.
  - Starter templates: a trip plan, a celebration and a story.

All three:

- **Files:** their own formats (`.nwrite`, `.nsheets`, `.npresent`) keep everything.
  Word (`.docx`), Excel (`.xlsx`) and PowerPoint (`.pptx`) files open and save in
  place, and so do OpenDocument files (`.odt`, `.ods`, `.odp`, as LibreOffice
  writes them); the first time a file can't hold something (a chart in an OpenDocument
  file, for instance), a note says what. Sheets also opens and saves CSV; Write also
  opens and saves Markdown (`.md`, with its pictures in a folder beside it) and plain
  text (`.txt`).
- **Find and replace** (<kbd>Ctrl</kbd>+<kbd>F</kbd>, <kbd>Ctrl</kbd>+<kbd>H</kbd>).
- **Export as PDF** (<kbd>Ctrl</kbd>+<kbd>E</kbd>) and **print** (<kbd>Ctrl</kbd>+<kbd>P</kbd>),
  on the document's paper (Write) or the one chosen in Settings.
- **Zoom** from 50% to 300% (<kbd>Ctrl</kbd>+scroll, <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>=</kbd>
  / <kbd>-</kbd>, or the buttons at the end of the status bar; <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>0</kbd>
  for 100%). Each app remembers its own.
- **Undo and redo**, and a copy of unsaved work every few seconds: if an app closes
  unexpectedly, Home offers to bring the work back.
- **Save automatically** (in Settings, off at first): a file that has a name saves itself
  a few seconds after each change, and on closing, without asking.

Files made by the first release of Nexus Office open as before.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/design-nexus/nexus-office-suite/main/install.sh | bash
```

This installs GTK 4, GtkSourceView 5, libspelling and an English dictionary if they're
missing, builds with Cargo, and installs `nexus-write`, `nexus-sheets` and
`nexus-present` to `~/.local/bin` with launcher entries. Add `-s -- --default` after
`bash` to also make them the default apps for their file types.

To remove them, run the same line with `uninstall.sh` in place of `install.sh`. Add
`-s -- --purge` to also remove their settings, recent file lists and copies of unsaved
work. Your documents are never touched.

## Usage

```
nexus-write [OPTIONS] [FILE]
nexus-sheets [OPTIONS] [FILE]
nexus-present [OPTIONS] [FILE]
```

| Option | What |
| --- | --- |
| `FILE` | Open this file |
| `--new [TEMPLATE]` | Start a blank file, or one from a template (`letter`, `recipe`, `journal`, `monthly-budget`, `home-inventory`, `trip-budget`, `trip-plan`, `celebration`, `story`) |
| `--export-pdf OUT` | Export `FILE` as a PDF to `OUT` and quit, without opening a window |

Launching an app again while it's running brings its window forward.

| Key | What |
| --- | --- |
| <kbd>Ctrl</kbd>+<kbd>N</kbd> / <kbd>Ctrl</kbd>+<kbd>O</kbd> | New blank file / open a file |
| <kbd>Ctrl</kbd>+<kbd>S</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> | Save / save as (also as a Word, Excel, PowerPoint or OpenDocument file) |
| <kbd>Ctrl</kbd>+<kbd>E</kbd> / <kbd>Ctrl</kbd>+<kbd>P</kbd> | Export as PDF / print |
| <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> | Undo / redo |
| <kbd>Ctrl</kbd>+<kbd>F</kbd> / <kbd>Ctrl</kbd>+<kbd>H</kbd> | Find / find and replace |
| <kbd>Ctrl</kbd>+<kbd>B</kbd> / <kbd>Ctrl</kbd>+<kbd>I</kbd> | Bold / italic |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd> | Collapse or expand the sidebar |
| <kbd>Ctrl</kbd>+<kbd>,</kbd> | Settings |
| <kbd>F11</kbd> | Fullscreen |
| <kbd>F1</kbd> | All keyboard shortcuts, including each app's own |
| <kbd>Ctrl</kbd>+<kbd>Q</kbd> | Quit |

## Files

| Path | What |
| --- | --- |
| `~/.config/nexus-office/settings.toml` | Settings, shared by the three apps |
| `~/.config/nexus-office/themes/*.toml` | Your own themes |
| `~/.local/share/nexus-office/recent-*.json` | Each app's recent files |
| `~/.local/state/nexus-office/recovery/` | Copies of unsaved work |
| `~/.local/state/nexus-office/zoom-*` | Each app's zoom |

## Building

```sh
cargo build --release
```

Needs GTK 4, GtkSourceView 5 and libspelling (`pacman -S gtk4 gtksourceview5 libspelling`),
plus a Hunspell dictionary for spelling (`hunspell-en_us`).

Icons are from [Lucide](https://lucide.dev/) (ISC license, see `data/icons/LICENSE`).
