import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import "components"

ApplicationWindow {
    id: window
    visible: true
    width: App.windowWidth
    height: App.windowHeight
    minimumWidth: 760
    minimumHeight: 520
    title: section === "editor" ? activeDocument.displayName + " — " + App.appTitle : App.appTitle
    color: Theme.palette.background
    font.family: "Noto Sans"
    font.pixelSize: 12

    property string section: "home"
    property string pendingAction: ""
    property var pendingFile: null
    property bool allowClose: false
    property string colorTarget: ""
    readonly property var activeDocument: App.appId === "sheets" ? Sheets : App.appId === "present" ? Present : Write
    readonly property string appGlyph: App.appId === "write" ? "W" : App.appId === "sheets" ? "S" : "P"
    readonly property string appIconName: App.appId === "write" ? "file-text" : App.appId === "sheets" ? "file-spreadsheet" : "presentation"
    readonly property string documentName: App.appId === "write" ? "document" : App.appId === "sheets" ? "spreadsheet" : "presentation"
    readonly property var templateCatalog: App.appId === "write"
        ? [{ id: "letter", title: "Personal letter", description: "A warm letter with address and sign-off", icon: "file-text" },
           { id: "recipe", title: "Recipe", description: "Ingredients, steps, and family notes", icon: "list" },
           { id: "journal", title: "Journal", description: "Capture today and what comes next", icon: "type" }]
        : App.appId === "sheets"
        ? [{ id: "monthly-budget", title: "Monthly budget", description: "Plan, spend, and see the difference", icon: "file-spreadsheet" },
           { id: "home-inventory", title: "Home inventory", description: "List belongings and estimated value", icon: "table-2" },
           { id: "trip-budget", title: "Trip budget", description: "Track travel costs in one place", icon: "chart-column" }]
        : [{ id: "trip-plan", title: "Trip plan", description: "Route, activities, and reminders", icon: "presentation" },
           { id: "celebration", title: "Celebration", description: "A simple event or family gathering", icon: "palette" },
           { id: "story", title: "Our story", description: "Share memories across four slides", icon: "file-text" }]

    function continueStartupAfterRecovery() {
        if (initialDocumentPath !== "") openDocument(initialDocumentPath)
        else if (startBlankDocument) newDocument()
    }
    function newTemplate(templateId) {
        if (!activeDocument.createTemplate(templateId)) return
        section = "editor"
        if (App.appId === "write") writeEditor.focusEditor()
    }
    function newDocument() {
        activeDocument.newDocument()
        section = "editor"
        if (App.appId === "write") writeEditor.focusEditor()
    }
    function openDocument(file) {
        const isPath = typeof file === "string" && !file.startsWith("file:")
        const lower = file.toString().toLowerCase()
        const isDocx = App.appId === "write" && lower.endsWith(".docx")
        const isXlsx = App.appId === "sheets" && lower.endsWith(".xlsx")
        const isPptx = App.appId === "present" && lower.endsWith(".pptx")
        const opened = isDocx ? (isPath ? Write.importDocxPath(file) : Write.importDocx(file))
            : isXlsx ? (isPath ? Sheets.importXlsxPath(file) : Sheets.importXlsx(file))
            : isPptx ? (isPath ? Present.importPptxPath(file) : Present.importPptx(file))
            : (isPath ? activeDocument.openPath(file) : activeDocument.open(file))
        if (opened) {
            if (isDocx || isXlsx || isPptx) {
                if (isPath) App.recordRecentFile(file)
                else App.recordRecentUrl(file)
            } else if (activeDocument.path !== "") App.recordRecentFile(activeDocument.path)
            section = "editor"
            if (App.appId === "write") writeEditor.focusEditor()
        }
    }
    function requestAction(action, file) {
        commitSheetEdits()
        pendingAction = action
        pendingFile = file
        if (activeDocument.dirty) unsavedPopup.open()
        else finishPending()
    }
    function finishPending() {
        const action = pendingAction
        const file = pendingFile
        pendingAction = ""
        pendingFile = null
        if (action === "new") newDocument()
        else if (action === "template") newTemplate(file)
        else if (action === "open") openDocument(file)
        else if (action === "importDocx") { if (Write.importDocx(file)) { App.recordRecentUrl(file); section = "editor"; writeEditor.focusEditor() } }
        else if (action === "importXlsx") { if (Sheets.importXlsx(file)) { App.recordRecentUrl(file); section = "editor" } }
        else if (action === "importPptx") { if (Present.importPptx(file)) { App.recordRecentUrl(file); section = "editor" } }
        else if (action === "quit") { allowClose = true; window.close() }
    }
    function requestSave() {
        commitSheetEdits()
        if (activeDocument.path === "" || (App.appId === "sheets" && Sheets.sheetNames.length > 1 && Sheets.path.toLowerCase().endsWith(".csv"))) saveDialog.open()
        else if (activeDocument.save()) {
            App.recordRecentFile(activeDocument.path)
            if (pendingAction !== "") finishPending()
        }
    }
    function commitSheetEdits() {
        if (App.appId !== "sheets") return
        if (formulaInput.editingRow >= 0) formulaInput.commit()
        sheetsEditor.commitEditing()
    }
    function showToolbarMenu(menu, button) {
        const point = button.mapToItem(window.contentItem, 0, button.height)
        menu.x = point.x
        menu.y = point.y
        menu.open()
    }
    function openSpelling() {
        spellingPopup.position = writeEditor.cursorPosition()
        spellingPopup.result = Write.spellingAt(spellingPopup.position)
        spellingPopup.open()
    }
    function chooseColor(target) {
        colorTarget = target
        const selected = Present.currentElements.find(function(item) { return item.id === presentEditor.selectedElementId })
        colorDialog.selectedColor = target === "presentBackground" ? (Present.currentBackgroundColor || "#ffffff")
            : target === "presentText" ? (Present.currentFontColor || "#202536")
            : target === "presentShape" ? (selected ? selected.fill : "#5273cf")
            : "#202329"
        colorDialog.open()
    }
    function chooseSheetColor(target, button) {
        sheetColorMenu.target = target
        sheetColorMenu.currentColor = target === "fill"
            ? (Sheets.fillColorAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn) || "")
            : (Sheets.textColorAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn) || "")
        sheetColorInput.text = sheetColorMenu.currentColor
        showToolbarMenu(sheetColorMenu, button)
    }
    function applySheetColor(color) {
        const firstRow = sheetsEditor.anchorRow
        const firstColumn = sheetsEditor.anchorColumn
        const lastRow = sheetsEditor.selectedRow
        const lastColumn = sheetsEditor.selectedColumn
        if (sheetColorMenu.target === "fill") Sheets.setRangeFillColor(firstRow, firstColumn, lastRow, lastColumn, color)
        else Sheets.setRangeTextColor(firstRow, firstColumn, lastRow, lastColumn, color)
        sheetColorMenu.close()
    }
    function applyFontFamily(family) {
        if (App.appId === "present") Present.setFontFamily(family)
        else writeEditor.setFontFamily(family)
    }
    function applyFontSize(size) {
        if (App.appId === "present") presentEditor.setTextSize(size)
        else writeEditor.setFontSize(size)
    }
    function runSheetFormatAction(action) {
        const row = sheetsEditor.selectedRow
        const column = sheetsEditor.selectedColumn
        const firstRow = sheetsEditor.anchorRow
        const firstColumn = sheetsEditor.anchorColumn
        if (action === "general") Sheets.setRangeNumberFormat(firstRow, firstColumn, row, column, 0)
        else if (action === "number") Sheets.setRangeNumberFormat(firstRow, firstColumn, row, column, 1)
        else if (action === "currency") Sheets.setRangeNumberFormat(firstRow, firstColumn, row, column, 2)
        else if (action === "percent") Sheets.setRangeNumberFormat(firstRow, firstColumn, row, column, 3)
        else if (action === "bold") Sheets.toggleRangeBold(firstRow, firstColumn, row, column)
        else if (action === "align_left") Sheets.setRangeAlignment(firstRow, firstColumn, row, column, 0)
        else if (action === "align_center") Sheets.setRangeAlignment(firstRow, firstColumn, row, column, 1)
        else if (action === "align_right") Sheets.setRangeAlignment(firstRow, firstColumn, row, column, 2)
        else if (action === "sort_asc") Sheets.sortBy(column, true)
        else if (action === "sort_desc") Sheets.sortBy(column, false)
        else if (action === "clear_sort") Sheets.clearSort()
        else if (action === "filter") filterPopup.open()
        else if (action === "clear_filter") Sheets.setFilter(column, "")
        else if (action === "chart") chartPopup.open()
    }
    function runWriteFormatAction(action) {
        if (action === "body") writeEditor.setHeading(0)
        else if (action === "heading1") writeEditor.setHeading(1)
        else if (action === "heading2") writeEditor.setHeading(2)
        else if (action === "bold") writeEditor.toggleBold()
        else if (action === "italic") writeEditor.toggleItalic()
        else if (action === "bullets") writeEditor.toggleList(false)
        else if (action === "numbers") writeEditor.toggleList(true)
        else if (action === "image") imageDialog.open()
        else if (action === "imageLayout") imageLayoutPopup.open()
        else if (action === "table") writeEditor.insertTable(3, 3)
        else if (action === "spelling") openSpelling()
        else if (action === "font") window.showToolbarMenu(fontMenu, appIconButton)
        else if (action === "size") window.showToolbarMenu(fontSizeMenu, appIconButton)
        else if (action === "color") chooseColor("writeText")
        else if (action === "page") pageSetupPopup.open()
    }
    function runPresentFormatAction(action) {
        if (action === "title") Present.setLayout(0)
        else if (action === "body") Present.setLayout(1)
        else if (action === "section") Present.setLayout(2)
        else if (action === "light") Present.setStyle(0)
        else if (action === "dark") Present.setStyle(1)
        else if (action === "accent") Present.setStyle(2)
        else if (action === "font") window.showToolbarMenu(fontMenu, appIconButton)
        else if (action === "size") window.showToolbarMenu(fontSizeMenu, appIconButton)
        else if (action === "color") chooseColor("presentText")
        else if (action === "background") chooseColor("presentBackground")
        else if (action === "bold") Present.setBold(!Present.currentBold)
        else if (action === "italic") Present.setItalic(!Present.currentItalic)
        else if (action === "image") presentImageDialog.open()
        else if (action === "textbox") presentEditor.addTextBox()
        else if (action === "rectangle" || action === "ellipse") presentEditor.addShape(action)
        else if (action === "shapeColor" && presentEditor.selectedElementType === "shape") chooseColor("presentShape")
        else if (action.startsWith("align:") ) presentEditor.alignSelected(action.slice(6))
    }
    function startSlideshow(fromCurrent) {
        if (App.appId !== "present" || !Present.hasDocument) return
        slideshow.start(fromCurrent ? Present.selectedIndex : 0)
    }
    onClosing: function(close) {
        commitSheetEdits()
        if (activeDocument.dirty && !allowClose) {
            close.accepted = false
            requestAction("quit")
            return
        }
        App.setWindowSize(width, height)
    }
    Component.onCompleted: {
        if (App.appId === "write") {
            if (Write.recoveryAvailable) recoveryPopup.open()
            else if (initialDocumentPath !== "") openDocument(initialDocumentPath)
            else if (startBlankDocument) newDocument()
        }
        if (App.appId === "sheets") {
            if (Sheets.recoveryAvailable) recoveryPopup.open()
            else if (initialDocumentPath !== "") openDocument(initialDocumentPath)
            else if (startBlankDocument) newDocument()
        }
        if (App.appId === "present") {
            if (Present.recoveryAvailable) recoveryPopup.open()
            else if (initialDocumentPath !== "") openDocument(initialDocumentPath)
            else if (startBlankDocument) newDocument()
        }
        if (previewFileMenu) Qt.callLater(function() { appMenu.open(); fileMenu.open() })
        if (previewFormatMenu) Qt.callLater(function() { appMenu.open(); formatMenu.open() })
        if (previewSlideshow) Qt.callLater(function() { window.startSlideshow(false) })
        if (previewTemplatePicker) Qt.callLater(function() { templatePopup.open() })
    }
    onWidthChanged: sizeSave.restart()
    onHeightChanged: sizeSave.restart()
    Timer { id: sizeSave; interval: 350; onTriggered: App.setWindowSize(window.width, window.height) }
    Shortcut { sequence: "Ctrl+Q"; onActivated: window.close() }
    Shortcut { sequence: "Ctrl+,"; onActivated: themePopup.open() }
    Shortcut {
        sequence: "Escape"
        onActivated: {
            if (App.appId === "sheets" && window.section === "editor") {
                if (formulaInput.activeFocus && formulaInput.editingRow >= 0) { formulaInput.cancel(); return }
                if (sheetsEditor.cancelEditing()) return
            }
            themePopup.close()
            appPopup.close()
        }
    }
    Shortcut { sequence: "Ctrl+N"; onActivated: window.requestAction("new") }
    Shortcut { sequence: "Ctrl+O"; onActivated: openDialog.open() }
    Shortcut { sequence: "Ctrl+S"; enabled: window.section === "editor"; onActivated: window.requestSave() }
    Shortcut { sequence: "Ctrl+Shift+S"; enabled: window.section === "editor"; onActivated: { window.commitSheetEdits(); saveDialog.open() } }
    Shortcut { sequence: "Ctrl+Z"; enabled: App.appId !== "write" && window.section === "editor"; onActivated: { window.commitSheetEdits(); activeDocument.undo() } }
    Shortcut { sequence: "Ctrl+Shift+Z"; enabled: App.appId !== "write" && window.section === "editor"; onActivated: { window.commitSheetEdits(); activeDocument.redo() } }
    Shortcut { sequence: "Ctrl+Y"; enabled: App.appId !== "write" && window.section === "editor"; onActivated: { window.commitSheetEdits(); activeDocument.redo() } }
    Shortcut { sequence: "Ctrl+C"; enabled: App.appId === "sheets" && window.section === "editor" && !formulaInput.activeFocus && sheetsEditor.activeEditor === null; onActivated: sheetsEditor.copySelection() }
    Shortcut { sequence: "Ctrl+V"; enabled: App.appId === "sheets" && window.section === "editor" && !formulaInput.activeFocus && sheetsEditor.activeEditor === null; onActivated: sheetsEditor.pasteSelection() }
    Shortcut { sequence: "Ctrl+D"; enabled: App.appId === "sheets" && window.section === "editor" && sheetsEditor.activeEditor === null; onActivated: sheetsEditor.fillSelectionDown() }
    Shortcut { sequence: "Ctrl+R"; enabled: App.appId === "sheets" && window.section === "editor" && sheetsEditor.activeEditor === null; onActivated: sheetsEditor.fillSelectionRight() }
    Shortcut { sequence: "Ctrl+B"; enabled: App.appId === "write" && window.section === "editor"; onActivated: writeEditor.toggleBold() }
    Shortcut { sequence: "Ctrl+I"; enabled: App.appId === "write" && window.section === "editor"; onActivated: writeEditor.toggleItalic() }
    Shortcut { sequence: "F5"; enabled: App.appId === "present" && window.section === "editor"; onActivated: window.startSlideshow(false) }
    Shortcut { sequence: "Shift+F5"; enabled: App.appId === "present" && window.section === "editor"; onActivated: window.startSlideshow(true) }

    SlideshowWindow { id: slideshow }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 52
            color: Theme.palette.surface
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 16
                anchors.rightMargin: 16
                spacing: 11
                IconButton {
                    id: appIconButton
                    iconName: window.appIconName
                    helpText: App.appTitle + " menu"
                    selected: appMenu.visible
                    onClicked: appMenu.open()
                }
                Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; color: Theme.palette.border; visible: window.section === "editor" }
                RowLayout {
                    visible: App.appId === "write" && window.section === "editor"
                    spacing: 2
                    IconButton { iconName: "arrow-left"; helpText: "Back to Home"; onClicked: window.section = "home" }
                    IconButton { iconName: "file-plus"; helpText: "New document · Ctrl+N"; visible: window.width >= 940; onClicked: window.requestAction("new") }
                    IconButton { iconName: "folder-open"; helpText: "Open · Ctrl+O"; visible: window.width >= 940; onClicked: openDialog.open() }
                    IconButton { iconName: "save"; helpText: "Save · Ctrl+S"; onClicked: window.requestSave() }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 5; Layout.rightMargin: 5; color: Theme.palette.border }
                    IconButton { iconName: "undo-2"; helpText: "Undo · Ctrl+Z"; onClicked: { Write.undo(); writeEditor.focusEditor() } }
                    IconButton { iconName: "redo-2"; helpText: "Redo · Ctrl+Shift+Z"; onClicked: { Write.redo(); writeEditor.focusEditor() } }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 5; Layout.rightMargin: 5; color: Theme.palette.border }
                    AppMenuButton { id: styleButton; text: "Paragraph  ▾"; onClicked: window.showToolbarMenu(styleMenu, styleButton) }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 5; Layout.rightMargin: 5; color: Theme.palette.border }
                    IconButton { iconName: "bold"; helpText: "Bold · Ctrl+B"; onClicked: writeEditor.toggleBold() }
                    IconButton { iconName: "italic"; helpText: "Italic · Ctrl+I"; onClicked: writeEditor.toggleItalic() }
                    AppMenuButton { id: writeFontButton; text: "Font  ▾"; visible: window.width >= 1160; onClicked: window.showToolbarMenu(fontMenu, writeFontButton) }
                    AppMenuButton { id: writeSizeButton; text: "Size  ▾"; visible: window.width >= 1260; onClicked: window.showToolbarMenu(fontSizeMenu, writeSizeButton) }
                    IconButton { iconName: "palette"; helpText: "Text color"; visible: window.width >= 1160; onClicked: window.chooseColor("writeText") }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 5; Layout.rightMargin: 5; color: Theme.palette.border }
                    IconButton { iconName: "list"; helpText: "Bulleted list"; onClicked: writeEditor.toggleList(false) }
                    IconButton { iconName: "list-ordered"; helpText: "Numbered list"; onClicked: writeEditor.toggleList(true) }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 5; Layout.rightMargin: 5; color: Theme.palette.border; visible: window.width >= 1000 }
                    IconButton { iconName: "image"; helpText: "Insert image"; visible: window.width >= 1000; onClicked: imageDialog.open() }
                    IconButton { iconName: "sliders-horizontal"; helpText: "Image size and alignment"; visible: window.width >= 1100; enabled: Write.imageAt(writeEditor.cursorPosition()).found; onClicked: imageLayoutPopup.open() }
                    IconButton { id: tableButton; iconName: "table-2"; helpText: "Insert table"; visible: window.width >= 1000; onClicked: window.showToolbarMenu(tableMenu, tableButton) }
                    IconButton { iconName: "settings-2"; helpText: "Page setup"; visible: window.width >= 1100; onClicked: pageSetupPopup.open() }
                }
                Flickable {
                    id: sheetToolbarScroll
                    visible: App.appId === "sheets" && window.section === "editor"
                    Layout.fillWidth: visible
                    Layout.preferredWidth: window.width - (window.width >= 1450 ? 365 : 165)
                    Layout.minimumWidth: 0
                    Layout.preferredHeight: 36
                    clip: true
                    contentWidth: sheetToolbar.implicitWidth
                    contentHeight: height
                    flickableDirection: Flickable.HorizontalFlick
                    boundsBehavior: Flickable.StopAtBounds
                    RowLayout {
                    id: sheetToolbar
                    spacing: 2
                    IconButton { iconName: "arrow-left"; helpText: "Back to Home"; onClicked: window.section = "home" }
                    IconButton { iconName: "file-plus"; helpText: "New spreadsheet · Ctrl+N"; visible: window.width >= 940; onClicked: window.requestAction("new") }
                    IconButton { iconName: "folder-open"; helpText: "Open CSV · Ctrl+O"; visible: window.width >= 940; onClicked: openDialog.open() }
                    IconButton { iconName: "save"; helpText: "Save spreadsheet · Ctrl+S"; onClicked: window.requestSave() }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 6; Layout.rightMargin: 6; color: Theme.palette.border }
                    IconButton { iconName: "undo-2"; helpText: "Undo · Ctrl+Z"; enabled: Sheets.canUndo; onClicked: { window.commitSheetEdits(); Sheets.undo() } }
                    IconButton { iconName: "redo-2"; helpText: "Redo · Ctrl+Shift+Z"; enabled: Sheets.canRedo; onClicked: { window.commitSheetEdits(); Sheets.redo() } }
                    Rectangle {
                        Layout.preferredWidth: sheetsEditor.anchorRow === sheetsEditor.selectedRow && sheetsEditor.anchorColumn === sheetsEditor.selectedColumn ? 48 : 88
                        Layout.preferredHeight: 32
                        radius: 5; color: Theme.palette.raised
                        Text { anchors.centerIn: parent; text: sheetsEditor.selectionName; color: Theme.palette.accent; font.pixelSize: 11; font.weight: Font.DemiBold }
                    }
                    Text { text: "fx"; color: Theme.palette.accent; font.pixelSize: 13; font.italic: true; Layout.leftMargin: 6; Layout.rightMargin: 4 }
                    Rectangle {
                        Layout.preferredWidth: Math.max(170, Math.min(280, window.width - 1000))
                        Layout.preferredHeight: 32
                        radius: 5
                        color: Theme.palette.raised
                        TextInput {
                            id: formulaInput
                            property int editingRow: -1
                            property int editingColumn: -1
                            anchors.fill: parent
                            anchors.leftMargin: 9; anchors.rightMargin: 9
                            verticalAlignment: TextInput.AlignVCenter
                            color: Theme.palette.text
                            selectionColor: Theme.palette.accent
                            font.pixelSize: 12
                            selectByMouse: true
                            function commit() {
                                if (editingRow < 0) return
                                const row = editingRow
                                const column = editingColumn
                                editingRow = -1
                                editingColumn = -1
                                Sheets.setCell(row, column, text)
                                focus = false
                            }
                            function cancel() {
                                if (editingRow < 0) return
                                const row = editingRow
                                const column = editingColumn
                                editingRow = -1
                                editingColumn = -1
                                text = Sheets.rawAt(row, column)
                                focus = false
                                sheetsEditor.endReference()
                                sheetsEditor.focusGrid()
                            }
                            onAccepted: commit()
                            Keys.onEscapePressed: function(event) { event.accepted = true; cancel() }
                            onActiveFocusChanged: {
                                if (activeFocus) {
                                    editingRow = sheetsEditor.selectedRow
                                    editingColumn = sheetsEditor.selectedColumn
                                } else commit()
                            }
                        }
                    }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 5; Layout.rightMargin: 5; color: Theme.palette.border }
                    IconButton { iconName: "type"; helpText: "General format"; selected: Sheets.numberFormatAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn) === 0 && Sheets.revision >= 0; onClicked: window.runSheetFormatAction("general") }
                    IconButton { iconName: "decimal"; helpText: "Number · 2 decimals"; selected: Sheets.numberFormatAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn) === 1 && Sheets.revision >= 0; onClicked: window.runSheetFormatAction("number") }
                    IconButton { iconName: "currency"; helpText: "Currency"; selected: Sheets.numberFormatAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn) === 2 && Sheets.revision >= 0; onClicked: window.runSheetFormatAction("currency") }
                    IconButton { iconName: "percent"; helpText: "Percent"; selected: Sheets.numberFormatAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn) === 3 && Sheets.revision >= 0; onClicked: window.runSheetFormatAction("percent") }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 5; Layout.rightMargin: 5; color: Theme.palette.border }
                    IconButton { iconName: "bold"; helpText: "Bold selected cells"; selected: Sheets.boldAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn) && Sheets.revision >= 0; onClicked: window.runSheetFormatAction("bold") }
                    IconButton {
                        id: sheetFillButton
                        iconName: "palette"; helpText: "Cell fill color"
                        onClicked: window.chooseSheetColor("fill", sheetFillButton)
                        Rectangle {
                            anchors.horizontalCenter: parent.horizontalCenter
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 3
                            width: 17; height: 3; radius: 1
                            color: { Sheets.revision; return Sheets.fillColorAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn) || Theme.palette.background }
                            border.width: 1; border.color: Theme.palette.border
                        }
                    }
                    IconButton {
                        id: sheetTextButton
                        iconName: "type"; helpText: "Cell text color"
                        onClicked: window.chooseSheetColor("text", sheetTextButton)
                        Rectangle {
                            anchors.horizontalCenter: parent.horizontalCenter
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 3
                            width: 17; height: 3; radius: 1
                            color: { Sheets.revision; return Sheets.textColorAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn) || Theme.palette.text }
                            border.width: 1; border.color: Theme.palette.border
                        }
                    }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 5; Layout.rightMargin: 5; color: Theme.palette.border }
                    IconButton { iconName: "align-left"; helpText: "Align left"; selected: Sheets.alignmentAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn) === 0 && Sheets.revision >= 0; onClicked: window.runSheetFormatAction("align_left") }
                    IconButton { iconName: "align-center"; helpText: "Align center"; selected: Sheets.alignmentAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn) === 1 && Sheets.revision >= 0; onClicked: window.runSheetFormatAction("align_center") }
                    IconButton { iconName: "align-right"; helpText: "Align right"; selected: Sheets.alignmentAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn) === 2 && Sheets.revision >= 0; onClicked: window.runSheetFormatAction("align_right") }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 5; Layout.rightMargin: 5; color: Theme.palette.border }
                    IconButton { iconName: "arrow-up-narrow-wide"; helpText: "Sort selected column"; onClicked: Sheets.sortBy(sheetsEditor.selectedColumn, true) }
                    IconButton { iconName: "filter"; helpText: "Filter selected column"; onClicked: filterPopup.open() }
                    IconButton { iconName: "chart-column"; helpText: "Create chart"; onClicked: chartPopup.open() }
                    }
                }
                IconButton {
                    iconName: "arrow-left"
                    helpText: "Previous toolbar tools"
                    visible: sheetToolbarScroll.visible && sheetToolbarScroll.contentX > 1
                    onClicked: sheetToolbarScroll.contentX = Math.max(0, sheetToolbarScroll.contentX - 250)
                }
                IconButton {
                    iconName: "arrow-right"
                    helpText: "More toolbar tools"
                    visible: sheetToolbarScroll.visible && sheetToolbarScroll.contentX < sheetToolbarScroll.contentWidth - sheetToolbarScroll.width - 1
                    onClicked: sheetToolbarScroll.contentX = Math.min(sheetToolbarScroll.contentWidth - sheetToolbarScroll.width, sheetToolbarScroll.contentX + 250)
                }
                RowLayout {
                    visible: App.appId === "present" && window.section === "editor"
                    spacing: 2
                    IconButton { iconName: "arrow-left"; helpText: "Back to Home"; onClicked: window.section = "home" }
                    IconButton { iconName: "file-plus"; helpText: "New presentation · Ctrl+N"; visible: window.width >= 940; onClicked: window.requestAction("new") }
                    IconButton { iconName: "folder-open"; helpText: "Open presentation · Ctrl+O"; visible: window.width >= 940; onClicked: openDialog.open() }
                    IconButton { iconName: "save"; helpText: "Save presentation · Ctrl+S"; onClicked: window.requestSave() }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 5; Layout.rightMargin: 5; color: Theme.palette.border }
                    IconButton { iconName: "undo-2"; helpText: "Undo · Ctrl+Z"; enabled: Present.canUndo; onClicked: Present.undo() }
                    IconButton { iconName: "redo-2"; helpText: "Redo · Ctrl+Shift+Z"; enabled: Present.canRedo; onClicked: Present.redo() }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 5; Layout.rightMargin: 5; color: Theme.palette.border }
                    IconButton { iconName: "file-plus"; helpText: "Add slide"; onClicked: Present.addSlide() }
                    IconButton { iconName: "copy"; helpText: "Duplicate slide"; onClicked: Present.duplicateSlide() }
                    IconButton { iconName: "x"; helpText: "Delete slide"; onClicked: Present.deleteSlide() }
                    IconButton { iconName: "image"; helpText: "Add image"; visible: window.width >= 940; onClicked: presentImageDialog.open() }
                    IconButton { iconName: "type"; helpText: "Add text box"; visible: window.width >= 940; onClicked: presentEditor.addTextBox() }
                    IconButton { id: presentShapeButton; iconName: "shapes"; helpText: "Add shape"; visible: window.width >= 1080; onClicked: window.showToolbarMenu(presentShapeMenu, presentShapeButton) }
                    IconButton { id: presentAlignButton; iconName: "align-center"; helpText: "Align selected object"; visible: window.width >= 1080; enabled: presentEditor.selectedElementId !== ""; onClicked: window.showToolbarMenu(presentAlignMenu, presentAlignButton) }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; Layout.leftMargin: 5; Layout.rightMargin: 5; color: Theme.palette.border }
                    IconButton { iconName: "play"; helpText: "Play slideshow · F5"; onClicked: window.startSlideshow(false) }
                    AppMenuButton { id: slideLayoutButton; text: "Layout  ▾"; onClicked: window.showToolbarMenu(slideLayoutMenu, slideLayoutButton) }
                    AppMenuButton { id: slideStyleButton; text: "Style  ▾"; visible: window.width >= 1000; onClicked: window.showToolbarMenu(slideStyleMenu, slideStyleButton) }
                    AppMenuButton { id: presentFontButton; text: "Font  ▾"; visible: window.width >= 1130; onClicked: window.showToolbarMenu(fontMenu, presentFontButton) }
                    AppMenuButton { id: presentSizeButton; text: "Size  ▾"; visible: window.width >= 1240; onClicked: window.showToolbarMenu(fontSizeMenu, presentSizeButton) }
                    IconButton { iconName: "bold"; helpText: "Bold slide text"; visible: window.width >= 1130; selected: Present.currentBold; onClicked: Present.setBold(!Present.currentBold) }
                    IconButton { iconName: "italic"; helpText: "Italic slide text"; visible: window.width >= 1130; selected: Present.currentItalic; onClicked: Present.setItalic(!Present.currentItalic) }
                    IconButton { iconName: "palette"; helpText: "Text color"; visible: window.width >= 1130; onClicked: window.chooseColor("presentText") }
                    IconButton { iconName: "sliders-horizontal"; helpText: "Slide background color"; visible: window.width >= 1240; onClicked: window.chooseColor("presentBackground") }
                }
                Item { Layout.fillWidth: true }
                Text {
                    text: activeDocument.displayName
                    color: Theme.palette.muted
                    font.pixelSize: 11
                    elide: Text.ElideMiddle
                    Layout.maximumWidth: 180
                    visible: window.section === "editor" && window.width >= (App.appId === "sheets" ? 1450 : 1090)
                }
                IconButton { iconName: "layout-grid"; helpText: "Switch apps"; onClicked: appPopup.open() }
                IconButton { iconName: "palette"; helpText: "Appearance · Ctrl+,"; onClicked: themePopup.open() }
            }
        }
        Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.palette.border }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0
            Rectangle {
                Layout.preferredWidth: 202
                Layout.fillHeight: true
                visible: window.section !== "editor"
                color: Theme.palette.surface
                ColumnLayout {
                    anchors.fill: parent
                    spacing: 0
                    Text { text: "WORKSPACE"; color: Theme.palette.muted; font.pixelSize: 10; font.weight: Font.DemiBold; Layout.leftMargin: 16; Layout.topMargin: 13; Layout.bottomMargin: 5 }
                    NavItem { text: "Home"; iconName: "house"; active: window.section === "home"; Layout.fillWidth: true; onClicked: window.section = "home" }
                    NavItem { text: "Recent"; iconName: "clock-3"; active: window.section === "recent"; Layout.fillWidth: true; onClicked: window.section = "recent" }
                    NavItem { text: activeDocument.displayName; iconName: window.appIconName; active: window.section === "editor"; visible: activeDocument.hasDocument; Layout.fillWidth: true; onClicked: window.section = "editor" }
                    Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; Layout.topMargin: 12; Layout.bottomMargin: 10; color: Theme.palette.border }
                    Text { text: "NEXUS OFFICE"; color: Theme.palette.muted; font.pixelSize: 10; font.weight: Font.DemiBold; Layout.leftMargin: 16; Layout.bottomMargin: 5 }
                    NavItem { text: "Write"; iconName: "file-text"; currentApp: App.appId === "write"; Layout.fillWidth: true; onClicked: App.launchApp("write") }
                    NavItem { text: "Sheets"; iconName: "file-spreadsheet"; currentApp: App.appId === "sheets"; Layout.fillWidth: true; onClicked: App.launchApp("sheets") }
                    NavItem { text: "Present"; iconName: "presentation"; currentApp: App.appId === "present"; Layout.fillWidth: true; onClicked: App.launchApp("present") }
                    Item { Layout.fillHeight: true }
                    Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.palette.border }
                    Text {
                        text: Theme.activeThemeName
                        color: Theme.palette.muted
                        font.pixelSize: 10
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                        Layout.leftMargin: 16
                        Layout.rightMargin: 12
                        Layout.topMargin: 9
                        Layout.bottomMargin: 11
                    }
                }
            }
            Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: Theme.palette.border; visible: window.section !== "editor" }

            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 0
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 43
                    visible: window.section !== "editor"
                    color: Theme.palette.background
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 17
                        anchors.rightMargin: 17
                        spacing: 8
                        IconView { iconName: window.section === "home" ? "house" : "clock-3"; tint: Theme.palette.accent; Layout.preferredWidth: 17; Layout.preferredHeight: 17 }
                        Text { text: window.section === "home" ? "Home" : "Recent"; color: Theme.palette.text; font.pixelSize: 13; font.weight: Font.DemiBold }
                        Item { Layout.fillWidth: true }
                    }
                }
                Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.palette.border; visible: window.section !== "editor" }

                WriteEditor {
                    id: writeEditor
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: App.appId === "write" && window.section === "editor"
                    onRequestSpelling: window.openSpelling()
                }
                SheetsEditor {
                    id: sheetsEditor
                    formulaBarEditor: formulaInput
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: App.appId === "sheets" && window.section === "editor"
                    onSelectionChanged: {
                        if (formulaInput.activeFocus) formulaInput.commit()
                        formulaInput.text = Sheets.rawAt(selectedRow, selectedColumn)
                    }
                }
                PresentEditor {
                    id: presentEditor
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: App.appId === "present" && window.section === "editor"
                }
                Connections {
                    target: Sheets
                    function onStateChanged() {
                        if (!formulaInput.activeFocus)
                            formulaInput.text = Sheets.rawAt(sheetsEditor.selectedRow, sheetsEditor.selectedColumn)
                    }
                }

                Flickable {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: window.section !== "editor"
                    contentWidth: width
                    contentHeight: body.implicitHeight + 32
                    clip: true
                    ScrollBar.vertical: ScrollBar { }
                    ColumnLayout {
                        id: body
                        x: 20
                        y: 18
                        width: parent.width - 40
                        spacing: 0
                        ColumnLayout {
                            Layout.fillWidth: true
                            visible: window.section === "home"
                            spacing: 0
                            Text { text: "Create"; color: Theme.palette.text; font.pixelSize: 21; font.weight: Font.DemiBold; Layout.bottomMargin: 3 }
                            Text { text: App.appDescription; color: Theme.palette.muted; font.pixelSize: 12; Layout.bottomMargin: 18 }
                            RowLayout {
                                spacing: 14
                                StartCard {
                                    iconName: App.appId === "write" ? "file-plus" : window.appIconName
                                    title: "Blank " + window.documentName
                                    description: App.appId === "write" ? "Start writing" : App.appId === "sheets" ? "Start a spreadsheet" : "Start a presentation"
                                    available: true
                                    onClicked: window.requestAction("new")
                                }
                                StartCard {
                                    iconName: "folder-open"
                                    title: "Open a file"
                                    description: App.appId === "write" ? "Continue your work" : App.appId === "sheets" ? "Open a CSV file" : "Open a presentation"
                                    available: true
                                    onClicked: openDialog.open()
                                }
                            }
                            Text { text: "Templates"; color: Theme.palette.text; font.pixelSize: 16; font.weight: Font.DemiBold; Layout.topMargin: 28; Layout.bottomMargin: 10 }
                            Flow {
                                Layout.fillWidth: true
                                spacing: 10
                                Repeater {
                                    model: window.templateCatalog
                                    TemplateCard {
                                        iconName: modelData.icon
                                        title: modelData.title
                                        description: modelData.description
                                        onClicked: window.requestAction("template", modelData.id)
                                    }
                                }
                            }
                            Text { text: "Recent files"; color: Theme.palette.text; font.pixelSize: 16; font.weight: Font.DemiBold; Layout.topMargin: 28; Layout.bottomMargin: 10 }
                            Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.palette.border }
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.topMargin: 14
                                IconView { iconName: "clock-3"; tint: Theme.palette.muted; Layout.preferredWidth: 17; Layout.preferredHeight: 17 }
                                Text { text: App.recentFiles.length === 0 ? "No recent files yet" : App.recentFiles.length + " recent files"; color: Theme.palette.muted; font.pixelSize: 12; Layout.leftMargin: 5 }
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            visible: window.section === "recent"
                            spacing: 0
                            RowLayout {
                                Layout.fillWidth: true
                                Text { text: "Recent files"; color: Theme.palette.text; font.pixelSize: 15; font.weight: Font.DemiBold }
                                Item { Layout.fillWidth: true }
                                ShellButton { text: "Clear"; visible: App.recentFiles.length > 0; onClicked: App.clearRecentFiles() }
                            }
                            Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; Layout.topMargin: 13; color: Theme.palette.border }
                            Text { text: "No recent files yet. Files you open will appear here."; color: Theme.palette.muted; font.pixelSize: 12; visible: App.recentFiles.length === 0; Layout.topMargin: 18 }
                            Repeater {
                                model: App.recentFiles
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 0
                                    RowLayout {
                                        Layout.fillWidth: true
                                        Layout.preferredHeight: 50
                                        IconView { iconName: window.appIconName; tint: Theme.palette.accent; Layout.preferredWidth: 20; Layout.preferredHeight: 20 }
                                        ColumnLayout {
                                            Layout.leftMargin: 8
                                            spacing: 3
                                            Text { text: modelData.split("/").pop(); color: Theme.palette.text; font.pixelSize: 12; font.weight: Font.DemiBold }
                                            Text { text: modelData; color: Theme.palette.muted; font.pixelSize: 10; elide: Text.ElideMiddle; Layout.maximumWidth: body.width - 90 }
                                        }
                                        Item { Layout.fillWidth: true }
                                    }
                                    TapHandler {
                                        enabled: true
                                        onTapped: window.requestAction("open", modelData)
                                    }
                                    Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.palette.border }
                                }
                            }
                        }
                    }
                }
            }
        }
        Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.palette.border; visible: window.section !== "editor" }
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 25
            color: Theme.palette.surface
            visible: window.section !== "editor"
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 14
                Text { text: "Nexus Office Suite"; color: Theme.palette.muted; font.pixelSize: 10 }
                Item { Layout.fillWidth: true }
                Text { text: window.section === "home" ? "Home" : window.section === "editor" ? activeDocument.displayName : App.recentFiles.length + " files"; color: Theme.palette.muted; font.pixelSize: 10 }
            }
        }
    }

    FileDialog {
        id: openDialog
        title: App.appId === "sheets" ? "Open spreadsheet" : App.appId === "present" ? "Open presentation" : "Open Nexus Write document"
        fileMode: FileDialog.OpenFile
        nameFilters: App.appId === "sheets" ? ["Spreadsheets (*.nsheets *.csv *.xlsx)", "Nexus Sheets (*.nsheets)", "Excel workbooks (*.xlsx)", "CSV spreadsheets (*.csv)"] : App.appId === "present" ? ["Presentations (*.npresent *.pptx)", "Nexus Present (*.npresent)", "PowerPoint presentations (*.pptx)"] : ["Write documents (*.nwrite *.docx)", "Nexus Write (*.nwrite)", "Word documents (*.docx)"]
        onAccepted: window.requestAction("open", selectedFile)
    }
    FileDialog {
        id: saveDialog
        title: App.appId === "sheets" ? "Save spreadsheet" : App.appId === "present" ? "Save presentation" : "Save Nexus Write document"
        fileMode: FileDialog.SaveFile
        defaultSuffix: App.appId === "sheets" ? "nsheets" : App.appId === "present" ? "npresent" : "nwrite"
        nameFilters: App.appId === "sheets" ? ["Nexus Sheets (*.nsheets)", "CSV spreadsheets (*.csv)"] : App.appId === "present" ? ["Nexus Present (*.npresent)"] : ["Nexus Write documents (*.nwrite)"]
        onAccepted: {
            if (activeDocument.saveAs(selectedFile)) {
                App.recordRecentFile(activeDocument.path)
                if (window.pendingAction !== "") window.finishPending()
            }
        }
        onRejected: window.pendingAction = ""
    }
    FileDialog {
        id: imageDialog
        title: "Insert image"
        fileMode: FileDialog.OpenFile
        nameFilters: ["Images (*.png *.jpg *.jpeg *.webp *.bmp *.gif)"]
        onAccepted: writeEditor.insertImage(selectedFile)
    }
    FileDialog {
        id: presentImageDialog
        title: "Add image to slide"
        fileMode: FileDialog.OpenFile
        nameFilters: ["Images (*.png *.jpg *.jpeg *.webp *.bmp *.gif)"]
        onAccepted: presentEditor.addImage(selectedFile)
    }
    FileDialog {
        id: docxImportDialog
        title: "Import Word document"
        fileMode: FileDialog.OpenFile
        nameFilters: ["Word documents (*.docx)"]
        onAccepted: window.requestAction("importDocx", selectedFile)
    }
    FileDialog {
        id: docxExportDialog
        title: "Export Word document"
        fileMode: FileDialog.SaveFile
        defaultSuffix: "docx"
        nameFilters: ["Word documents (*.docx)"]
        onAccepted: Write.exportDocx(selectedFile)
    }
    FileDialog {
        id: xlsxImportDialog
        title: "Import Excel workbook"
        fileMode: FileDialog.OpenFile
        nameFilters: ["Excel workbooks (*.xlsx)"]
        onAccepted: window.requestAction("importXlsx", selectedFile)
    }
    FileDialog {
        id: xlsxExportDialog
        title: "Export Excel workbook"
        fileMode: FileDialog.SaveFile
        defaultSuffix: "xlsx"
        nameFilters: ["Excel workbooks (*.xlsx)"]
        onAccepted: { window.commitSheetEdits(); Sheets.exportXlsx(selectedFile) }
    }
    FileDialog {
        id: pptxImportDialog
        title: "Import PowerPoint presentation"
        fileMode: FileDialog.OpenFile
        nameFilters: ["PowerPoint presentations (*.pptx)"]
        onAccepted: window.requestAction("importPptx", selectedFile)
    }
    FileDialog {
        id: pptxExportDialog
        title: "Export PowerPoint presentation"
        fileMode: FileDialog.SaveFile
        defaultSuffix: "pptx"
        nameFilters: ["PowerPoint presentations (*.pptx)"]
        onAccepted: Present.exportPptx(selectedFile)
    }
    FileDialog {
        id: pdfDialog
        title: "Export PDF"
        fileMode: FileDialog.SaveFile
        defaultSuffix: "pdf"
        nameFilters: ["PDF documents (*.pdf)"]
        onAccepted: {
            if (App.appId === "sheets") Sheets.exportPdf(selectedFile)
            else if (App.appId === "present") Present.exportPdf(selectedFile, Theme.palette.accent)
            else Write.exportPdf(selectedFile)
        }
    }
    FileDialog {
        id: csvExportDialog
        title: "Export CSV"
        fileMode: FileDialog.SaveFile
        defaultSuffix: "csv"
        nameFilters: ["CSV spreadsheets (*.csv)"]
        onAccepted: Sheets.exportCsv(selectedFile)
    }
    Connections {
        target: Write
        function onErrorChanged() { if (Write.errorString !== "") errorPopup.open() }
    }
    Connections {
        target: Sheets
        function onErrorChanged() { if (Sheets.errorString !== "") errorPopup.open() }
    }
    Connections {
        target: Present
        function onErrorChanged() { if (Present.errorString !== "") errorPopup.open() }
    }

    Popup {
        id: templatePopup
        x: (window.width - width) / 2
        y: Math.max(62, (window.height - height) / 2)
        width: Math.min(window.width - 32, 520)
        padding: 15
        modal: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { radius: 8; color: Theme.palette.surface; border.color: Theme.palette.border }
        contentItem: ColumnLayout {
            spacing: 12
            RowLayout {
                Layout.fillWidth: true
                IconView { iconName: "layout-grid"; tint: Theme.palette.accent; Layout.preferredWidth: 18; Layout.preferredHeight: 18 }
                Text { text: "Choose a template"; color: Theme.palette.text; font.pixelSize: 15; font.weight: Font.DemiBold }
                Item { Layout.fillWidth: true }
                IconButton { iconName: "x"; helpText: "Close"; onClicked: templatePopup.close() }
            }
            Flow {
                Layout.fillWidth: true
                Layout.preferredHeight: childrenRect.height
                spacing: 10
                Repeater {
                    model: window.templateCatalog
                    TemplateCard {
                        iconName: modelData.icon
                        title: modelData.title
                        description: modelData.description
                        onClicked: { templatePopup.close(); window.requestAction("template", modelData.id) }
                    }
                }
            }
        }
    }
    Menu {
        id: presentShapeMenu
        width: 205
        padding: 6
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        AppMenuItem { text: "Rectangle"; iconName: "shapes"; onTriggered: presentEditor.addShape("rectangle") }
        AppMenuItem { text: "Ellipse"; iconName: "shapes"; onTriggered: presentEditor.addShape("ellipse") }
        AppMenuItem { text: "Shape fill…"; iconName: "palette"; enabled: presentEditor.selectedElementType === "shape"; onTriggered: window.chooseColor("presentShape") }
    }
    Menu {
        id: presentAlignMenu
        width: 205
        padding: 6
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        AppMenuItem { text: "Left"; iconName: "align-center"; onTriggered: presentEditor.alignSelected("left") }
        AppMenuItem { text: "Center"; iconName: "align-center"; onTriggered: presentEditor.alignSelected("center") }
        AppMenuItem { text: "Right"; iconName: "align-center"; onTriggered: presentEditor.alignSelected("right") }
        MenuSeparator { }
        AppMenuItem { text: "Top"; iconName: "align-center"; onTriggered: presentEditor.alignSelected("top") }
        AppMenuItem { text: "Middle"; iconName: "align-center"; onTriggered: presentEditor.alignSelected("middle") }
        AppMenuItem { text: "Bottom"; iconName: "align-center"; onTriggered: presentEditor.alignSelected("bottom") }
    }
    Menu {
        id: appMenu
        x: 16
        y: 46
        width: 205
        padding: 6
        delegate: AppMenuItem {
            iconName: text === "File" ? "file-text" : text === "Edit" ? "scissors" : text === "Format" ? "type" : text === "Data" ? "table-2" : "panel-left"
        }
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        Menu {
            id: fileMenu
            title: "File"
            width: 270
            padding: 6
            background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
            AppMenuItem { text: App.appId === "sheets" ? "New spreadsheet" : App.appId === "present" ? "New presentation" : "New document"; iconName: "file-plus"; shortcutLabel: "Ctrl+N"; onTriggered: window.requestAction("new") }
            AppMenuItem { text: "New from template…"; iconName: "layout-grid"; onTriggered: templatePopup.open() }
            AppMenuItem { text: App.appId === "sheets" ? "Open spreadsheet…" : "Open…"; iconName: "folder-open"; shortcutLabel: "Ctrl+O"; onTriggered: openDialog.open() }
            Instantiator {
                model: App.appId === "write" ? 1 : 0
                delegate: AppMenuItem { text: "Import DOCX…"; iconName: "file-text"; onTriggered: docxImportDialog.open() }
                onObjectAdded: function(index, object) { fileMenu.insertItem(3, object) }
                onObjectRemoved: function(index, object) { fileMenu.removeItem(object) }
            }
            Instantiator {
                model: App.appId === "sheets" ? 1 : 0
                delegate: AppMenuItem { text: "Import XLSX…"; iconName: "file-spreadsheet"; onTriggered: xlsxImportDialog.open() }
                onObjectAdded: function(index, object) { fileMenu.insertItem(3, object) }
                onObjectRemoved: function(index, object) { fileMenu.removeItem(object) }
            }
            Instantiator {
                model: App.appId === "present" ? 1 : 0
                delegate: AppMenuItem { text: "Import PPTX…"; iconName: "presentation"; onTriggered: pptxImportDialog.open() }
                onObjectAdded: function(index, object) { fileMenu.insertItem(3, object) }
                onObjectRemoved: function(index, object) { fileMenu.removeItem(object) }
            }
            MenuSeparator { }
            AppMenuItem { text: "Save"; iconName: "save"; shortcutLabel: "Ctrl+S"; enabled: window.section === "editor"; onTriggered: window.requestSave() }
            AppMenuItem { text: "Save As…"; iconName: "save"; shortcutLabel: "Ctrl+Shift+S"; enabled: window.section === "editor"; onTriggered: { window.commitSheetEdits(); saveDialog.open() } }
            AppMenuItem { text: "Export PDF…"; iconName: "file-down"; enabled: window.section === "editor"; onTriggered: { window.commitSheetEdits(); pdfDialog.open() } }
            Instantiator {
                model: App.appId === "write" ? 1 : 0
                delegate: AppMenuItem { text: "Export DOCX (text and tables)…"; iconName: "file-down"; enabled: window.section === "editor"; onTriggered: docxExportDialog.open() }
                onObjectAdded: function(index, object) { fileMenu.insertItem(8, object) }
                onObjectRemoved: function(index, object) { fileMenu.removeItem(object) }
            }
            Instantiator {
                model: App.appId === "sheets" ? 1 : 0
                delegate: AppMenuItem {
                    text: "Export CSV…"
                    iconName: "file-down"
                    enabled: window.section === "editor"
                    onTriggered: { window.commitSheetEdits(); csvExportDialog.open() }
                }
                onObjectAdded: function(index, object) { fileMenu.insertItem(7, object) }
                onObjectRemoved: function(index, object) { fileMenu.removeItem(object) }
            }
            Instantiator {
                model: App.appId === "sheets" ? 1 : 0
                delegate: AppMenuItem { text: "Export XLSX…"; iconName: "file-down"; enabled: window.section === "editor"; onTriggered: xlsxExportDialog.open() }
                onObjectAdded: function(index, object) { fileMenu.insertItem(8, object) }
                onObjectRemoved: function(index, object) { fileMenu.removeItem(object) }
            }
            Instantiator {
                model: App.appId === "present" ? 1 : 0
                delegate: AppMenuItem { text: "Export PPTX…"; iconName: "file-down"; enabled: window.section === "editor"; onTriggered: pptxExportDialog.open() }
                onObjectAdded: function(index, object) { fileMenu.insertItem(7, object) }
                onObjectRemoved: function(index, object) { fileMenu.removeItem(object) }
            }
            MenuSeparator { }
            AppMenuItem { text: "Back to Home"; iconName: "house"; onTriggered: window.section = "home" }
            AppMenuItem { text: "Quit"; iconName: "x"; shortcutLabel: "Ctrl+Q"; onTriggered: window.close() }
        }
        Menu {
            id: editMenu
            title: "Edit"
            width: 270
            padding: 6
            background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
            Instantiator {
                model: App.appId === "present"
                    ? [{ label: "Undo", icon: "undo-2", action: "undo", shortcut: "Ctrl+Z" },
                       { label: "Redo", icon: "redo-2", action: "redo", shortcut: "Ctrl+Shift+Z" },
                       { label: "Add slide", icon: "file-plus", action: "add" },
                       { label: "Duplicate slide", icon: "copy", action: "duplicate" },
                       { label: "Delete slide", icon: "x", action: "delete" },
                       { label: "Move slide up", icon: "arrow-up-narrow-wide", action: "up" },
                       { label: "Move slide down", icon: "arrow-up-narrow-wide", action: "down" }]
                    : App.appId === "sheets"
                    ? [{ label: "Undo", icon: "undo-2", action: "undo", shortcut: "Ctrl+Z" },
                       { label: "Redo", icon: "redo-2", action: "redo", shortcut: "Ctrl+Shift+Z" },
                       { label: "Copy range", icon: "copy", action: "copy", shortcut: "Ctrl+C" },
                       { label: "Paste range", icon: "clipboard-paste", action: "paste", shortcut: "Ctrl+V" },
                       { label: "Clear cell", icon: "x", action: "clear" }]
                    : [{ label: "Undo", icon: "undo-2", action: "undo", shortcut: "Ctrl+Z" },
                       { label: "Redo", icon: "redo-2", action: "redo", shortcut: "Ctrl+Shift+Z" },
                       { label: "Cut", icon: "scissors", action: "cut", shortcut: "Ctrl+X" },
                       { label: "Copy", icon: "copy", action: "copy", shortcut: "Ctrl+C" },
                       { label: "Paste", icon: "clipboard-paste", action: "paste", shortcut: "Ctrl+V" },
                       { label: "Select All", icon: "type", action: "select_all", shortcut: "Ctrl+A" }]
                delegate: AppMenuItem {
                    text: modelData.label
                    iconName: modelData.icon
                    shortcutLabel: modelData.shortcut || ""
                    enabled: window.section === "editor" && (modelData.action !== "undo" || (App.appId === "write" || activeDocument.canUndo)) && (modelData.action !== "redo" || (App.appId === "write" || activeDocument.canRedo))
                    onTriggered: {
                        if (App.appId === "present") {
                            if (modelData.action === "undo") Present.undo()
                            else if (modelData.action === "redo") Present.redo()
                            else if (modelData.action === "add") Present.addSlide()
                            else if (modelData.action === "duplicate") Present.duplicateSlide()
                            else if (modelData.action === "delete") Present.deleteSlide()
                            else if (modelData.action === "up") Present.moveSlide(-1)
                            else if (modelData.action === "down") Present.moveSlide(1)
                        }
                        else if (App.appId === "sheets") {
                            window.commitSheetEdits()
                            if (modelData.action === "undo") Sheets.undo()
                            else if (modelData.action === "redo") Sheets.redo()
                            else if (modelData.action === "copy") sheetsEditor.copySelection()
                            else if (modelData.action === "paste") sheetsEditor.pasteSelection()
                            else Sheets.clearRange(sheetsEditor.anchorRow, sheetsEditor.anchorColumn,
                                                   sheetsEditor.selectedRow, sheetsEditor.selectedColumn)
                        }
                        else if (modelData.action === "undo") { Write.undo(); writeEditor.focusEditor() }
                        else if (modelData.action === "redo") { Write.redo(); writeEditor.focusEditor() }
                        else if (modelData.action === "cut") writeEditor.cut()
                        else if (modelData.action === "copy") writeEditor.copy()
                        else if (modelData.action === "paste") writeEditor.paste()
                        else writeEditor.selectAll()
                    }
                }
                onObjectAdded: function(index, object) { editMenu.insertItem(index, object) }
                onObjectRemoved: function(index, object) { editMenu.removeItem(object) }
            }
        }
        Menu {
            id: formatMenu
            title: App.appId === "sheets" ? "Data" : "Format"
            width: 270
            padding: 6
            background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
            Instantiator {
                model: App.appId === "present"
                    ? [{ label: "Title slide", icon: "presentation", action: "title" },
                       { label: "Title and body", icon: "presentation", action: "body" },
                       { label: "Section slide", icon: "presentation", action: "section" },
                       { label: "Light slide", icon: "palette", action: "light" },
                       { label: "Dark slide", icon: "palette", action: "dark" },
                       { label: "Accent slide", icon: "palette", action: "accent" },
                       { label: "Font family…", icon: "type", action: "font" },
                       { label: "Font size…", icon: "type", action: "size" },
                       { label: "Bold text", icon: "bold", action: "bold" },
                       { label: "Italic text", icon: "italic", action: "italic" },
                       { label: "Text color…", icon: "palette", action: "color" },
                       { label: "Background color…", icon: "palette", action: "background" },
                       { label: "Add image…", icon: "image", action: "image" },
                       { label: "Add text box", icon: "type", action: "textbox" },
                       { label: "Add rectangle", icon: "shapes", action: "rectangle" },
                       { label: "Add ellipse", icon: "shapes", action: "ellipse" },
                       { label: "Shape fill…", icon: "palette", action: "shapeColor" },
                       { label: "Align left", icon: "align-center", action: "align:left" },
                       { label: "Align center", icon: "align-center", action: "align:center" },
                       { label: "Align right", icon: "align-center", action: "align:right" },
                       { label: "Align top", icon: "align-center", action: "align:top" },
                       { label: "Align middle", icon: "align-center", action: "align:middle" },
                       { label: "Align bottom", icon: "align-center", action: "align:bottom" }]
                    : App.appId === "sheets"
                    ? [{ label: "Sort column ascending", icon: "arrow-up-narrow-wide", action: "sort_asc" },
                       { label: "Sort column descending", icon: "arrow-up-narrow-wide", action: "sort_desc" },
                       { label: "Clear sort", icon: "x", action: "clear_sort" },
                       { label: "Filter column…", icon: "filter", action: "filter" },
                       { label: "Show all rows", icon: "list", action: "clear_filter" },
                       { label: "Create chart…", icon: "chart-column", action: "chart" }]
                    : [{ label: "Body text", icon: "type", action: "body" },
                       { label: "Heading 1", icon: "type", action: "heading1" },
                       { label: "Heading 2", icon: "type", action: "heading2" },
                       { label: "Bold", icon: "bold", action: "bold", shortcut: "Ctrl+B" },
                       { label: "Italic", icon: "italic", action: "italic", shortcut: "Ctrl+I" },
                       { label: "Font family…", icon: "type", action: "font" },
                       { label: "Font size…", icon: "type", action: "size" },
                       { label: "Text color…", icon: "palette", action: "color" },
                       { label: "Page setup…", icon: "settings-2", action: "page" },
                       { label: "Bulleted list", icon: "list", action: "bullets" },
                       { label: "Numbered list", icon: "list-ordered", action: "numbers" },
                       { label: "Insert image…", icon: "image", action: "image" },
                       { label: "Image size and alignment…", icon: "sliders-horizontal", action: "imageLayout" },
                       { label: "Insert 3 × 3 table", icon: "table-2", action: "table" },
                       { label: "Spelling suggestions…", icon: "type", action: "spelling" }]
                delegate: AppMenuItem {
                    text: modelData.label
                    iconName: modelData.icon
                    shortcutLabel: modelData.shortcut || ""
                    enabled: window.section === "editor" &&
                             (modelData.action !== "spelling" || Write.spellcheckAvailable) &&
                             (modelData.action !== "imageLayout" || Write.imageAt(writeEditor.cursorPosition()).found) &&
                             (modelData.action !== "shapeColor" || presentEditor.selectedElementType === "shape") &&
                             (!modelData.action.startsWith("align:") || presentEditor.selectedElementId !== "")
                    onTriggered: {
                        if (App.appId === "present") window.runPresentFormatAction(modelData.action)
                        else if (App.appId === "sheets") window.runSheetFormatAction(modelData.action)
                        else window.runWriteFormatAction(modelData.action)
                    }
                }
                onObjectAdded: function(index, object) { formatMenu.insertItem(index, object) }
                onObjectRemoved: function(index, object) { formatMenu.removeItem(object) }
            }
        }
        Menu {
            id: viewMenu
            title: "View"
            width: 270
            padding: 6
            background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
            Instantiator {
                model: App.appId === "present" ? 2 : 0
                delegate: AppMenuItem {
                    text: index === 0 ? "Play from beginning" : "Play from this slide"
                    iconName: "play"
                    shortcutLabel: index === 0 ? "F5" : "Shift+F5"
                    enabled: window.section === "editor"
                    onTriggered: window.startSlideshow(index === 1)
                }
                onObjectAdded: function(index, object) { viewMenu.insertItem(index, object) }
                onObjectRemoved: function(index, object) { viewMenu.removeItem(object) }
            }
            AppMenuItem { text: "Appearance"; iconName: "palette"; shortcutLabel: "Ctrl+,"; onTriggered: themePopup.open() }
            AppMenuItem { text: "Home"; iconName: "house"; onTriggered: window.section = "home" }
        }
    }
    ColorDialog {
        id: colorDialog
        title: colorTarget === "presentBackground" ? "Slide background" : colorTarget === "presentShape" ? "Shape fill"
               : "Text color"
        onAccepted: {
            if (colorTarget === "presentBackground") Present.setBackgroundColor(selectedColor.toString())
            else if (colorTarget === "presentText") Present.setFontColor(selectedColor.toString())
            else if (colorTarget === "presentShape") Present.setElementFill(presentEditor.selectedElementId, selectedColor.toString())
            else writeEditor.setFontColor(selectedColor)
        }
    }
    Popup {
        id: imageLayoutPopup
        property int imagePosition: -1
        property string imageAlignment: "left"
        x: (window.width - width) / 2
        y: Math.max(60, (window.height - height) / 2)
        width: 350
        padding: 16
        modal: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { radius: 8; color: Theme.palette.surface; border.color: Theme.palette.border }
        onOpened: {
            imagePosition = writeEditor.cursorPosition()
            const info = Write.imageAt(imagePosition)
            if (!info.found) { close(); return }
            imageWidthBox.value = info.width
            imageAlignment = info.alignment
        }
        contentItem: ColumnLayout {
            spacing: 12
            RowLayout {
                Layout.fillWidth: true
                IconView { iconName: "image"; tint: Theme.palette.accent; Layout.preferredWidth: 18; Layout.preferredHeight: 18 }
                Text { text: "Image layout"; color: Theme.palette.text; font.pixelSize: 15; font.weight: Font.DemiBold }
                Item { Layout.fillWidth: true }
                IconButton { iconName: "x"; helpText: "Close"; onClicked: imageLayoutPopup.close() }
            }
            Text { text: "Width · height adjusts automatically"; color: Theme.palette.muted; font.pixelSize: 11 }
            SpinBox { id: imageWidthBox; from: 48; to: 610; stepSize: 10; editable: true; Layout.preferredWidth: 130 }
            Text { text: "Align within paragraph"; color: Theme.palette.muted; font.pixelSize: 11 }
            RowLayout {
                spacing: 6
                IconButton { iconName: "align-left"; helpText: "Align left"; selected: imageLayoutPopup.imageAlignment === "left"; onClicked: imageLayoutPopup.imageAlignment = "left" }
                IconButton { iconName: "align-center"; helpText: "Align center"; selected: imageLayoutPopup.imageAlignment === "center"; onClicked: imageLayoutPopup.imageAlignment = "center" }
                IconButton { iconName: "align-right"; helpText: "Align right"; selected: imageLayoutPopup.imageAlignment === "right"; onClicked: imageLayoutPopup.imageAlignment = "right" }
                Item { Layout.fillWidth: true }
                ShellButton { text: "Cancel"; onClicked: imageLayoutPopup.close() }
                ShellButton { text: "Apply"; prominent: true; onClicked: {
                    if (Write.setImageWidth(imageLayoutPopup.imagePosition, imageWidthBox.value) &&
                        Write.setImageAlignment(imageLayoutPopup.imagePosition, imageLayoutPopup.imageAlignment))
                        imageLayoutPopup.close()
                } }
            }
        }
    }
    Popup {
        id: pageSetupPopup
        x: (window.width - width) / 2
        y: Math.max(60, (window.height - height) / 2)
        width: 410
        padding: 16
        modal: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { radius: 8; color: Theme.palette.surface; border.color: Theme.palette.border }
        onOpened: {
            topMarginBox.value = Write.topMargin
            bottomMarginBox.value = Write.bottomMargin
            leftMarginBox.value = Write.leftMargin
            rightMarginBox.value = Write.rightMargin
            headerField.text = Write.headerText
            footerField.text = Write.footerText
            pageNumberCheck.checked = Write.showPageNumbers
        }
        contentItem: ColumnLayout {
            spacing: 11
            RowLayout {
                Layout.fillWidth: true
                IconView { iconName: "settings-2"; tint: Theme.palette.accent; Layout.preferredWidth: 18; Layout.preferredHeight: 18 }
                Text { text: "Page setup"; color: Theme.palette.text; font.pixelSize: 15; font.weight: Font.DemiBold }
                Item { Layout.fillWidth: true }
                IconButton { iconName: "x"; helpText: "Close"; onClicked: pageSetupPopup.close() }
            }
            Text { text: "Margins · 96 pixels = 1 inch"; color: Theme.palette.muted; font.pixelSize: 11 }
            GridLayout {
                columns: 4
                columnSpacing: 7
                rowSpacing: 7
                Text { text: "Top"; color: Theme.palette.text }
                SpinBox { id: topMarginBox; from: 36; to: 144; stepSize: 6; editable: true; Layout.preferredWidth: 104 }
                Text { text: "Bottom"; color: Theme.palette.text }
                SpinBox { id: bottomMarginBox; from: 36; to: 144; stepSize: 6; editable: true; Layout.preferredWidth: 104 }
                Text { text: "Left"; color: Theme.palette.text }
                SpinBox { id: leftMarginBox; from: 36; to: 144; stepSize: 6; editable: true; Layout.preferredWidth: 104 }
                Text { text: "Right"; color: Theme.palette.text }
                SpinBox { id: rightMarginBox; from: 36; to: 144; stepSize: 6; editable: true; Layout.preferredWidth: 104 }
            }
            Text { text: "Header"; color: Theme.palette.muted; font.pixelSize: 11 }
            TextField { id: headerField; Layout.fillWidth: true; placeholderText: "Optional text on every page"; maximumLength: 120 }
            Text { text: "Footer"; color: Theme.palette.muted; font.pixelSize: 11 }
            TextField { id: footerField; Layout.fillWidth: true; placeholderText: "Optional text on every page"; maximumLength: 120 }
            CheckBox { id: pageNumberCheck; text: "Show page numbers" }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                ShellButton { text: "Cancel"; onClicked: pageSetupPopup.close() }
                ShellButton { text: "Apply"; prominent: true; onClicked: {
                    Write.setPageLayout(topMarginBox.value, bottomMarginBox.value, leftMarginBox.value, rightMarginBox.value,
                                        headerField.text, footerField.text, pageNumberCheck.checked)
                    if (Write.errorString === "") pageSetupPopup.close()
                } }
            }
        }
    }
    Menu {
        id: fontMenu
        width: 210; padding: 6
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        Instantiator {
            model: ["Noto Sans", "Noto Serif", "DejaVu Sans", "Liberation Sans", "Liberation Serif", "Monospace"]
            delegate: AppMenuItem { text: modelData; iconName: "type"; onTriggered: window.applyFontFamily(modelData) }
            onObjectAdded: function(index, object) { fontMenu.insertItem(index, object) }
            onObjectRemoved: function(index, object) { fontMenu.removeItem(object) }
        }
    }
    Menu {
        id: fontSizeMenu
        width: 150; padding: 6
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        Instantiator {
            model: App.appId === "present" ? (presentEditor.textTarget === "title" ? [24, 32, 40, 48, 58, 72, 96] : [14, 18, 24, 29, 36, 48, 64]) : [10, 12, 14, 18, 24, 32, 48, 64]
            delegate: AppMenuItem { text: modelData + " pt"; iconName: "type"; onTriggered: window.applyFontSize(modelData) }
            onObjectAdded: function(index, object) { fontSizeMenu.insertItem(index, object) }
            onObjectRemoved: function(index, object) { fontSizeMenu.removeItem(object) }
        }
    }
    Menu {
        id: slideLayoutMenu
        width: 210
        padding: 6
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        AppMenuItem { text: "Title slide"; iconName: "presentation"; onTriggered: Present.setLayout(0) }
        AppMenuItem { text: "Title and body"; iconName: "presentation"; onTriggered: Present.setLayout(1) }
        AppMenuItem { text: "Section slide"; iconName: "presentation"; onTriggered: Present.setLayout(2) }
    }
    Menu {
        id: slideStyleMenu
        width: 190
        padding: 6
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        AppMenuItem { text: "Light"; iconName: "palette"; onTriggered: Present.setStyle(0) }
        AppMenuItem { text: "Dark"; iconName: "palette"; onTriggered: Present.setStyle(1) }
        AppMenuItem { text: "Accent"; iconName: "palette"; onTriggered: Present.setStyle(2) }
    }
    Menu {
        id: styleMenu
        width: 270
        padding: 6
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        AppMenuItem { text: "Body text"; iconName: "type"; onTriggered: writeEditor.setHeading(0) }
        AppMenuItem { text: "Heading 1"; iconName: "type"; onTriggered: writeEditor.setHeading(1) }
        AppMenuItem { text: "Heading 2"; iconName: "type"; onTriggered: writeEditor.setHeading(2) }
    }
    Menu {
        id: tableMenu
        width: 225
        padding: 6
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        AppMenuItem { text: "2 × 2 table"; iconName: "table-2"; onTriggered: writeEditor.insertTable(2, 2) }
        AppMenuItem { text: "3 × 3 table"; iconName: "table-2"; onTriggered: writeEditor.insertTable(3, 3) }
        AppMenuItem { text: "4 × 4 table"; iconName: "table-2"; onTriggered: writeEditor.insertTable(4, 4) }
    }
    Menu {
        id: sheetColorMenu
        property string target: "fill"
        property string currentColor: ""
        width: 234
        padding: 10
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        contentItem: ColumnLayout {
            spacing: 8
            Text {
                text: sheetColorMenu.target === "fill" ? "Cell fill" : "Text color"
                color: Theme.palette.text
                font.pixelSize: 12
                font.weight: Font.DemiBold
            }
            GridLayout {
                columns: 6
                rowSpacing: 5
                columnSpacing: 5
                Repeater {
                    model: ["#ffffff", "#e2e8f0", "#94a3b8", "#475569", "#1e293b", "#111827",
                            "#fecaca", "#fed7aa", "#fef08a", "#bbf7d0", "#bfdbfe", "#ddd6fe",
                            "#ef4444", "#f97316", "#eab308", "#22c55e", "#3b82f6", "#a855f7"]
                    delegate: Rectangle {
                        required property string modelData
                        Layout.preferredWidth: 29
                        Layout.preferredHeight: 25
                        radius: 4
                        color: modelData
                        border.width: sheetColorMenu.currentColor.toLowerCase() === modelData ? 2 : 1
                        border.color: sheetColorMenu.currentColor.toLowerCase() === modelData ? Theme.palette.accent : Theme.palette.border
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: window.applySheetColor(parent.modelData)
                        }
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 5
                TextField {
                    id: sheetColorInput
                    Layout.fillWidth: true
                    Layout.preferredHeight: 30
                    placeholderText: "#RRGGBB"
                    color: Theme.palette.text
                    selectionColor: Theme.palette.accent
                    validator: RegularExpressionValidator { regularExpression: /^#[0-9a-fA-F]{6}$/ }
                    onAccepted: if (acceptableInput) window.applySheetColor(text)
                    background: Rectangle { radius: 4; color: Theme.palette.raised; border.color: Theme.palette.border }
                }
                IconButton {
                    iconName: "check"
                    helpText: "Apply custom color"
                    enabled: sheetColorInput.acceptableInput
                    onClicked: window.applySheetColor(sheetColorInput.text)
                }
            }
            AppMenuButton {
                text: sheetColorMenu.target === "fill" ? "Clear fill" : "Default text color"
                onClicked: window.applySheetColor("")
            }
        }
    }
    Popup {
        id: filterPopup
        x: (window.width - width) / 2
        y: 62
        width: 320
        padding: 16
        modal: true
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        onOpened: { filterInput.text = Sheets.filterQuery; filterInput.forceActiveFocus() }
        contentItem: ColumnLayout {
            spacing: 10
            Text { text: "Filter column " + Sheets.columnName(sheetsEditor.selectedColumn); color: Theme.palette.text; font.pixelSize: 14; font.weight: Font.DemiBold }
            Text { text: "Show rows containing"; color: Theme.palette.muted; font.pixelSize: 11 }
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 34
                radius: 5
                color: Theme.palette.raised
                TextInput {
                    id: filterInput
                    anchors.fill: parent
                    anchors.leftMargin: 9; anchors.rightMargin: 9
                    verticalAlignment: TextInput.AlignVCenter
                    color: Theme.palette.text
                    selectByMouse: true
                    onAccepted: { Sheets.setFilter(sheetsEditor.selectedColumn, text); filterPopup.close() }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                ShellButton { text: "Clear"; onClicked: { Sheets.setFilter(sheetsEditor.selectedColumn, ""); filterPopup.close() } }
                ShellButton { text: "Apply"; prominent: true; onClicked: { Sheets.setFilter(sheetsEditor.selectedColumn, filterInput.text); filterPopup.close() } }
            }
        }
    }
    Popup {
        id: chartPopup
        property string chartKind: "bar"
        x: (window.width - width) / 2
        y: 62
        width: 340
        padding: 16
        modal: true
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        onOpened: {
            chartRangeInput.text = Sheets.chartRange === "" ? "A1:B8" : Sheets.chartRange
            chartKind = Sheets.chartType
        }
        contentItem: ColumnLayout {
            spacing: 11
            Text { text: "Create chart"; color: Theme.palette.text; font.pixelSize: 14; font.weight: Font.DemiBold }
            Text { text: "Choose up to 30 rows in one or two columns."; color: Theme.palette.muted; font.pixelSize: 11 }
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 34
                radius: 5
                color: Theme.palette.raised
                TextInput {
                    id: chartRangeInput
                    anchors.fill: parent
                    anchors.leftMargin: 9; anchors.rightMargin: 9
                    verticalAlignment: TextInput.AlignVCenter
                    color: Theme.palette.text
                    selectByMouse: true
                }
            }
            RowLayout {
                spacing: 6
                IconButton { iconName: "chart-column"; helpText: "Bar chart"; selected: chartPopup.chartKind === "bar"; onClicked: chartPopup.chartKind = "bar" }
                IconButton { iconName: "chart-line"; helpText: "Line chart"; selected: chartPopup.chartKind === "line"; onClicked: chartPopup.chartKind = "line" }
                IconButton { iconName: "chart-area"; helpText: "Area chart"; selected: chartPopup.chartKind === "area"; onClicked: chartPopup.chartKind = "area" }
                IconButton { iconName: "chart-pie"; helpText: "Pie chart"; selected: chartPopup.chartKind === "pie"; onClicked: chartPopup.chartKind = "pie" }
                Item { Layout.fillWidth: true }
                ShellButton { text: "Cancel"; onClicked: chartPopup.close() }
                ShellButton { text: "Create"; prominent: true; onClicked: { Sheets.setChart(chartRangeInput.text, chartPopup.chartKind); if (Sheets.errorString === "") chartPopup.close() } }
            }
        }
    }

    Popup {
        id: spellingPopup
        property int position: 0
        property var result: ({ word: "", correct: true, suggestions: [] })
        x: (window.width - width) / 2
        y: 58
        width: 280
        padding: 12
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        contentItem: Column {
            spacing: 6
            Text { text: spellingPopup.result.word || "Spelling"; color: Theme.palette.text; font.pixelSize: 13; font.weight: Font.DemiBold }
            Text {
                text: !spellingPopup.result.available ? "No spelling dictionary is installed."
                    : !spellingPopup.result.word ? "Place the cursor inside a word."
                    : spellingPopup.result.correct ? "Spelling looks correct."
                    : spellingPopup.result.suggestions.length === 0 ? "No suggestions found."
                    : "Suggested corrections"
                color: Theme.palette.muted
                font.pixelSize: 11
            }
            Repeater {
                model: spellingPopup.result.suggestions || []
                NavItem {
                    width: 256
                    text: modelData
                    iconName: "check"
                    onClicked: {
                        writeEditor.replaceWordAt(spellingPopup.position, modelData)
                        spellingPopup.close()
                    }
                }
            }
        }
    }

    Popup {
        id: unsavedPopup
        x: (window.width - width) / 2
        y: (window.height - height) / 2
        width: 370
        padding: 17
        modal: true
        closePolicy: Popup.NoAutoClose
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        contentItem: ColumnLayout {
            spacing: 12
            Text { text: "Save changes to " + activeDocument.displayName + "?"; color: Theme.palette.text; font.pixelSize: 15; font.weight: Font.DemiBold }
            Text { text: "Your edits will be lost if you discard them."; color: Theme.palette.muted; font.pixelSize: 11 }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                ShellButton { text: "Cancel"; onClicked: { window.pendingAction = ""; unsavedPopup.close() } }
                ShellButton { text: "Discard"; onClicked: { activeDocument.discardSessionRecovery(); unsavedPopup.close(); window.finishPending() } }
                ShellButton { text: "Save"; prominent: true; onClicked: { unsavedPopup.close(); window.requestSave() } }
            }
        }
    }
    Popup {
        id: recoveryPopup
        x: (window.width - width) / 2
        y: (window.height - height) / 2
        width: 370
        padding: 17
        modal: true
        closePolicy: Popup.NoAutoClose
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        contentItem: ColumnLayout {
            spacing: 12
            Text { text: "Recover your document?"; color: Theme.palette.text; font.pixelSize: 15; font.weight: Font.DemiBold }
            Text { text: App.appTitle + " found edits from an earlier session."; color: Theme.palette.muted; font.pixelSize: 11 }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                ShellButton { text: "Discard"; onClicked: { activeDocument.discardRecovery(); recoveryPopup.close(); window.continueStartupAfterRecovery() } }
                ShellButton { text: "Recover"; prominent: true; onClicked: { if (activeDocument.restoreRecovery()) { window.section = "editor"; if (App.appId === "write") writeEditor.focusEditor(); recoveryPopup.close() } } }
            }
        }
    }
    Popup {
        id: errorPopup
        x: (window.width - width) / 2
        y: (window.height - height) / 2
        width: 370
        padding: 17
        modal: true
        background: Rectangle { radius: 7; color: Theme.palette.surface; border.color: Theme.palette.border }
        contentItem: ColumnLayout {
            spacing: 12
            Text { text: "Could not complete the action"; color: Theme.palette.text; font.pixelSize: 15; font.weight: Font.DemiBold }
            Text { text: activeDocument.errorString; color: Theme.palette.muted; font.pixelSize: 11; wrapMode: Text.Wrap; Layout.fillWidth: true }
            ShellButton { text: "OK"; Layout.alignment: Qt.AlignRight; onClicked: errorPopup.close() }
        }
    }

    Popup {
        id: appPopup
        x: window.width - width - 54
        y: 48
        width: 176
        padding: 5
        modal: false
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { radius: 5; color: Theme.palette.surface; border.color: Theme.palette.border }
        contentItem: Column {
            spacing: 0
            Repeater {
                model: [ { id: "write", title: "Nexus Write", icon: "file-text" },
                         { id: "sheets", title: "Nexus Sheets", icon: "file-spreadsheet" },
                         { id: "present", title: "Nexus Present", icon: "presentation" } ]
                NavItem {
                    width: 164
                    text: modelData.title
                    iconName: modelData.icon
                    active: App.appId === modelData.id
                    onClicked: { App.launchApp(modelData.id); appPopup.close() }
                }
            }
            Rectangle { width: parent.width; height: 1; color: Theme.palette.border }
            NavItem { width: 164; text: "Quit"; iconName: "x"; onClicked: window.close() }
        }
    }

    Popup {
        id: themePopup
        x: window.width - width - 12
        y: 48
        width: 342
        padding: 9
        modal: false
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { radius: 5; color: Theme.palette.surface; border.color: Theme.palette.border }
        contentItem: Column {
            spacing: 5
            Text { text: "APPEARANCE"; color: Theme.palette.muted; font.pixelSize: 10; font.weight: Font.DemiBold }
            ThemeTile {
                width: parent.width
                themeId: "auto"
                themeName: Theme.omarchyAvailable ? "Match Omarchy system" : "Automatic · Catppuccin"
                swatch: Theme.palette
                onClicked: { Theme.selectedTheme = themeId; themePopup.close() }
            }
            Rectangle { width: parent.width; height: 1; color: Theme.palette.border }
            Grid {
                columns: 2
                columnSpacing: 3
                rowSpacing: 2
                Repeater {
                    model: Theme.themes
                    ThemeTile {
                        width: 160
                        themeId: modelData.id
                        themeName: modelData.name
                        swatch: modelData.palette
                        onClicked: { Theme.selectedTheme = themeId; themePopup.close() }
                    }
                }
            }
        }
    }
}
