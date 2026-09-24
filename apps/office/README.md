# Nexus Office Suite

Nexus Office Suite is a home-focused office project for Linux. It consists of Nexus Write, Nexus Sheets, and Nexus Present, with a shared QML interface and Qt/C++ services.

## Current state

The initial seven stages are implemented. The v2 stages add font and color controls in Write and Present, resizable Sheets rows and columns, Write page setup, undo, redo, recovery, nine starter templates, and basic DOCX, XLSX, and PPTX exchange in Write, Sheets, and Present. All three apps share a compact shell, window-size preferences, recent files, automatic Omarchy theme matching, and ten built-in themes. Nexus Write edits rich text, embeds images and tables, saves native `.nwrite` files, and exports PDF. Nexus Sheets edits spreadsheets with formulas, formatting, sorting, filtering, charts, and PDF export. Nexus Present edits slide text, images, movable text boxes, and notes with basic layouts and styles, plays full-screen slideshows, saves native `.npresent` files, and exports PDF.

## Build and run

Requirements: CMake, a C++17 compiler, libzip, and Qt 6.5+ with Core, Gui, Qml, Quick, Quick Controls 2, Dialogs, Svg, and Test modules. Enchant 2 and a spelling dictionary are optional for spelling suggestions.

```sh
cmake -S . -B build -DCMAKE_BUILD_TYPE=Debug
cmake --build build
./build/nexus-office --app write
./build/nexus-office --app write --new-document
./build/nexus-office --app write ~/Documents/notes.nwrite
./build/nexus-office --app write ~/Documents/letter.docx
./build/nexus-office --app sheets
./build/nexus-office --app sheets --new-document
./build/nexus-office --app sheets ~/Documents/budget.xlsx
./build/nexus-office --app present
./build/nexus-office --app present --new-document
./build/nexus-office --app present ~/Documents/trip.pptx
./build/nexus-office --app present ~/Documents/trip.npresent --export-pdf ~/Documents/trip.pdf
ctest --test-dir build --output-on-failure
```

The shell uses `~/.local/state/omarchy/current/theme/colors.toml` when Appearance is set to automatic. It watches for changes and falls back to Catppuccin Mocha if Omarchy is unavailable. A manual theme selection persists across all three apps. `Ctrl+,` opens Appearance; `Ctrl+Q` quits.

In Write, click **Blank document** to start, or choose a personal letter, recipe, or journal template on Home or under File → New from template. The single top bar provides headings, font family, font size, text color, bold, italic, bullets, numbering, undo/redo, image and table insertion, and file actions. Font and color changes apply to selected text or the word at the cursor. Place the cursor on an image and choose **Image size and alignment** from the toolbar or Format menu to set its width and align its paragraph left, center, or right. Height follows the image's proportions. Images remain inline with text; text wrapping around floating images is not available yet. The app icon opens File, Edit, Format, and View menus. `Ctrl+N`, `Ctrl+O`, `Ctrl+S`, `Ctrl+Shift+S`, `Ctrl+B`, and `Ctrl+I` are available. Save creates a ZIP-based `.nwrite` file containing `manifest.json`, `document.html`, and any embedded PNG images. Edits are saved atomically; a recovery snapshot is written periodically while a document is modified, including images. On restart, Write offers to restore an available snapshot. Format → Page setup adjusts each page margin and adds optional repeating header, footer, and page numbers. The preview and letter-size PDF use these settings, which are retained in native `.nwrite` files and recovery snapshots. File → Export PDF creates a letter-size PDF. Right-click a word or use Format → Spelling suggestions to check it when Enchant and a dictionary are installed. Current spelling support provides suggestions on demand, without automatic underlines. File → Import DOCX opens paragraphs, headings, direct bold/italic/font/size/color formatting, simple tables, and common embedded inline images as a new unsaved document. File → Export DOCX writes those features and embedded PNG images to a separate file. Unsupported or missing DOCX images remain visible as “Image omitted” placeholders; floating image layout and advanced Word layout are outside this basic exchange stage. Save the imported document as `.nwrite` to keep editing with all Nexus Write features.

The interface uses bundled [Lucide icons](https://lucide.dev/) with colors derived from the selected theme. The icon license is in `assets/icons/LICENSE` and is installed with the app.

In Sheets, click **Blank spreadsheet** to open a 500-row, 52-column grid per worksheet, or start with a monthly budget, home inventory, or trip budget template. Click a cell to select it; double-click to edit it, or type into the formula field in the top bar and press Enter. Cells support arithmetic, references such as `A1`, ranges, and `SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `COUNTA`, `PRODUCT`, `MEDIAN`, `ABS`, `ROUND`, `SQRT`, and `POWER`. Range summaries ignore blank and text cells where appropriate; errors cover invalid references, division by zero, cycles, and invalid square roots. Format offers two-decimal numbers, currency, percent, and bold cells. Drag the right edge of a column header or the bottom edge of a row header to resize it; native files preserve those sizes. Undo and redo cover cell values, formulas, formatting, dimensions, sort, filter, and charts. Sort keeps the first row in place; filtering shows matching rows. Bar, line, area, and pie charts can use up to 30 rows and two columns; pie charts display positive numeric values. Use the tabs below the grid to add, select, rename, duplicate, or delete worksheets (up to 20). Shift-click extends the selected cell range; Ctrl+C and Ctrl+V copy and paste ranges, including formulas with adjusted relative references. The File menu imports and exports XLSX, and exports PDF or CSV. Sheets keeps up to 50 undo steps and writes a recovery snapshot about every 15 seconds while edited; restart offers to restore an interrupted session.

New spreadsheets save as `.nsheets` JSON files, which preserve formulas, formatting, charts, and view settings. CSV import/export preserves raw cell values and formulas, but CSV cannot store formatting, charts, or row and column sizes. A CSV opened for editing can still be saved back to CSV until formatting or a chart is added; then use Save As to create a native file. CSV input is limited to 5 MB and 500 by 52 cells. XLSX import and export preserve up to 20 worksheets with common cell text, numbers, formulas, bold, number formats, and row/column sizes within the grid limits. Native `.nsheets` files also retain charts and view settings. Older single-sheet `.nsheets` files still open. CSV contains only the active grid, so use `.nsheets` when a workbook has multiple tabs.

In Present, click **Blank presentation** to start, or choose a trip plan, celebration, or story deck. Edit title and body text directly on each slide, and add speaker notes below. The top bar adds, duplicates, and deletes slides; its Layout and Style menus offer three layouts and three slide styles. Add an image, text box, rectangle, or ellipse from the top bar or Format menu. Click an image or shape to select and drag it; click a text box to edit it and use its drag handle to move it. Resize a selected object with its corner handle, align it to a slide edge or center with the alignment menu, and use its × control to remove it. Shapes also have a fill color picker. Up to 30 objects can be placed on a slide. Font family, title/body size, bold, italic, text color, and slide background color are available in the top bar and Format menu. Font and color choices apply to slide text, including text boxes. Undo and redo cover text, notes, formatting, objects, and slide changes. Edit in the app menu can also move slides up or down. Present saves `.npresent` JSON files with up to 200 slides, including embedded images. Press `F5` or click Play for a full-screen slideshow from the beginning; `Shift+F5` starts at the selected slide. Left and right arrows, Space, Page Up/Down, Home, and End navigate; `Esc` exits. Clicking the slide advances, and right-clicking goes back. The next action on the final slide exits. File → Export PDF creates 16:9 pages using the selected theme's accent color and includes images, text boxes, and shapes. File → Import PPTX opens slide titles, body text, images, text boxes, and basic rectangle and ellipse shapes; File → Export PPTX writes these elements to a separate file. Speaker notes stay in the native file and are not shown in the slideshow or exported PDF. Present keeps up to 50 undo steps, groups nearby typing into one step, and offers recovery after an interrupted session.

Install the shared binary, app launchers, desktop entries, and native file type definitions with `cmake --install build --prefix ~/.local`. Then run `update-mime-database ~/.local/share/mime` and `update-desktop-database ~/.local/share/applications` so file managers recognize `.nwrite`, `.nsheets`, and `.npresent` files. This does not change Omarchy configuration.

This release targets home use and keeps full Nexus features in its native formats. Basic Microsoft Office exchange covers DOCX text, tables, and inline images, up to 20 XLSX worksheets with cell values, formulas, simple formatting, and row/column sizes, and PPTX slide text, images, text boxes, rectangles, and ellipses with basic font and color choices. Import creates an unsaved Nexus document; export writes a separate Office file. Advanced Office layout, Write page setup in DOCX, XLSX charts and view settings, speaker notes, transitions, and animations are not exchanged. Apple iWork files are not supported.

## Planned stages

1. Shared shell and themes (complete)
2. Write core editing and native files (complete)
3. Write images, tables, spelling suggestions, and PDF (complete)
4. Sheets grid, formulas, and CSV (complete)
5. Sheets formatting, sort/filter, charts, and PDF (complete)
6. Present editor and native files (complete)
7. Present slideshow, PDF, and suite release (complete)
8. V2 editing controls: Sheets dimensions; Write and Present font and color (complete)
9. V2 reliability: Sheets and Present undo, redo, and recovery (complete)
10. V2 starter templates for all three apps (complete)
11. V2 compatibility: basic DOCX import and export in Write (complete)
12. V2 compatibility: basic XLSX import and export in Sheets (complete)
13. V2 compatibility: basic PPTX import and export in Present (complete)
14. V3 Sheets: multiple worksheet tabs and range clipboard (complete)
15. V3 Write: page view, margins, headers, footers, and page numbers (complete)
16. V3 Present: images and movable text boxes (complete)
17. Present stage 1: basic shapes, shape fill, and slide alignment (complete)
18. Sheets stage 2: more formulas and charts (complete)
19. Write stage 3: image placement and DOCX image exchange (complete)

Each stage is reviewed and tested before the next begins.
