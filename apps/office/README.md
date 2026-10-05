# Nexus Office Suite

Three home office apps for [Omarchy](https://omarchy.org): **Nexus Write** for
documents, **Nexus Sheets** for spreadsheets and **Nexus Present** for slides. They
open and save Word, Excel and PowerPoint files, export PDF and print. The window takes
its colors from your Omarchy theme and fits a half-screen tile; the page itself keeps
its own colors, so what you see is what prints.

## What they do

Every app starts on **Home**: a blank file, starter templates, and your recent files.
Then:

- **Write**
  - Paragraph styles (title, three headings, quote), font, size, bold, italic,
    underline, strikethrough, text color and highlight, alignment.
  - Bulleted and numbered lists with nested levels (<kbd>Tab</kbd> /
    <kbd>Shift</kbd>+<kbd>Tab</kbd>), and indents.
  - Pictures, sized with a slider; tables (right-click a cell to add or remove rows and
    columns).
  - Footnotes (<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>F</kbd>): a number in the text, the
    note at the foot of the page when printed.
  - Comments on selected text (<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>M</kbd>), shown as
    cards beside the page; edit, resolve or delete them there.
  - A table of contents built from the headings, with page numbers in print and PDF.
  - Page breaks (<kbd>Ctrl</kbd>+<kbd>Enter</kbd>), and two or three text columns.
  - Spelling checked as you type (right-click a word for suggestions).
  - Page setup: Letter or A4, margins, a header, a footer, page numbers and columns.
  - The headings appear in the sidebar as an outline; click one to go there.
  - Starter templates: a personal letter, a recipe and a journal.
- **Sheets**
  - Up to 10,000 rows and 104 columns per sheet, and up to 50 sheets, as tabs along
    the bottom: click to switch, double-click to rename, drag to reorder, right-click to
    copy, move or delete.
  - Formulas with cell and range references, `$` absolute references and references to
    other sheets (`'Sheet 2'!B4`). Click cells while typing a formula to put their
    references in. Nearly 100 functions:
    - Totals and counts: SUM, AVERAGE, MIN, MAX, COUNT, COUNTA, COUNTBLANK, PRODUCT,
      SUMPRODUCT, COUNTIF(S), SUMIF(S), AVERAGEIF(S), MAXIFS, MINIFS.
    - Statistics: MEDIAN, MODE, STDEV, VAR, LARGE, SMALL, RANK.
    - Maths: ABS, SIGN, ROUND, ROUNDUP, ROUNDDOWN, TRUNC, INT, CEILING, FLOOR, MOD,
      POWER, SQRT, EXP, LN, LOG, LOG10, PI, RAND, RANDBETWEEN.
    - Logic: IF, IFS, SWITCH, IFERROR, IFNA, AND, OR, XOR, NOT, ISBLANK, ISNUMBER,
      ISTEXT, ISERROR, ISNA.
    - Lookup: VLOOKUP, HLOOKUP, XLOOKUP, INDEX, MATCH, CHOOSE.
    - Text: CONCAT, TEXTJOIN, TEXT, LEN, LEFT, RIGHT, MID, FIND, SEARCH, SUBSTITUTE,
      REPLACE, REPT, UPPER, LOWER, PROPER, TRIM, VALUE, EXACT.
    - Dates: TODAY, NOW, DATE, YEAR, MONTH, DAY, HOUR, MINUTE, SECOND, DAYS, EDATE,
      EOMONTH, WEEKDAY, WEEKNUM, NETWORKDAYS.
  - Typed dates (`2026-10-05`, `10/5/2026`), money (`$12.50`), percentages and
    thousands separators are understood as numbers.
  - Number formats (two decimals, currency, percent, date), bold, italic, text and fill
    colors, alignment, and column widths and row heights you drag.
  - Merged cells, and comments on cells (a corner mark; rest the pointer on it to read).
  - Highlight rules: color cells that are above or below a number, between two, equal
    to one, contain some text, are blank or an error, are in the top few, or above
    average.
  - Copy, cut and paste (formulas follow the cells), drag the corner handle to fill,
    <kbd>Ctrl</kbd>+<kbd>D</kbd> / <kbd>Ctrl</kbd>+<kbd>R</kbd> to fill down or right.
  - Sort by a column, filter rows by what a column contains, keep the first row as a
    header and in view while scrolling.
  - Bar, line, area and pie charts of up to six series.
  - The status bar shows the sum, average and count of the selected cells.
  - Starter templates: a monthly budget, a home inventory and a trip budget.
- **Present**
  - Slides with four layouts (title, title and content, section, blank); edit the title
    and text right on the slide, and write speaker notes under it.
  - Ready-made looks, background and text colors, font, sizes, bold and italic.
  - Text boxes, pictures, rectangles, ellipses, tables and charts (double-click a table
    or chart to change what's in it): drag them, resize them by their corner, nudge
    them with the arrow keys.
  - <kbd>Shift</kbd>-click to select several; group them (<kbd>Ctrl</kbd>+<kbd>G</kbd>)
    to move them as one; line them up, space them evenly or change which is in front.
    Guides snap what you drag to the slide's middle and edges and to other objects
    (hold <kbd>Alt</kbd> to move freely).
  - Transitions between slides (fade, push, wipe, dissolve), and builds that bring
    objects in one click at a time (appear, fade in, fly in).
  - Thumbnails of every slide in the sidebar; right-click one to copy, move or delete it.
  - **Present** with <kbd>F5</kbd> (or <kbd>Shift</kbd>+<kbd>F5</kbd> from the current
    slide); the slideshow plays the transitions and builds. With a second screen connected, the slides go there and this screen shows
    the current and next slide, your notes and a timer.
  - Starter templates: a trip plan, a celebration and a story.

All three:

- **Files:** their own formats (`.nwrite`, `.nsheets`, `.npresent`) keep everything.
  Word (`.docx`), Excel (`.xlsx`) and PowerPoint (`.pptx`) files open and save in
  place; the first time a file can't hold something (speaker notes in a PowerPoint
  file, for instance), a note says what. Sheets also opens and saves CSV.
- **Find and replace** (<kbd>Ctrl</kbd>+<kbd>F</kbd>, <kbd>Ctrl</kbd>+<kbd>H</kbd>).
- **Export as PDF** (<kbd>Ctrl</kbd>+<kbd>E</kbd>) and **print** (<kbd>Ctrl</kbd>+<kbd>P</kbd>),
  on Letter or A4 paper.
- **Undo and redo**, and a copy of unsaved work every few seconds: if an app closes
  unexpectedly, Home offers to bring the work back.

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
| <kbd>Ctrl</kbd>+<kbd>S</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> | Save / save as (also as a Word, Excel or PowerPoint file) |
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

## Building

```sh
cargo build --release
```

Needs GTK 4, GtkSourceView 5 and libspelling (`pacman -S gtk4 gtksourceview5 libspelling`),
plus a Hunspell dictionary for spelling (`hunspell-en_us`).

Icons are from [Lucide](https://lucide.dev/) (ISC license, see `data/icons/LICENSE`).
