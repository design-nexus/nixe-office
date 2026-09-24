import QtQuick
import QtQuick.Controls
import "components"

Item {
    id: root
    property int selectedRow: 0
    property int selectedColumn: 0
    property int anchorRow: 0
    property int anchorColumn: 0
    property var activeEditor: null
    property var formulaBarEditor: null
    property var referenceEditor: null
    property int referenceAnchorRow: -1
    property int referenceAnchorColumn: -1
    property int referenceStart: 0
    property int referenceLength: 0
    property string dragMode: ""
    property int dragTargetRow: -1
    property int dragTargetColumn: -1
    property int dragTop: 0
    property int dragBottom: 0
    property int dragLeft: 0
    property int dragRight: 0
    property var dragSourceDisplay: []
    property var dragSourceBold: []
    property int dimensionRevision: 0
    readonly property int gridWidth: width - (Sheets.chartRange !== "" ? 320 : 0)
    readonly property string selectionName: anchorRow === selectedRow && anchorColumn === selectedColumn
                                            ? Sheets.cellName(selectedRow, selectedColumn)
                                            : Sheets.cellName(Math.min(anchorRow, selectedRow), Math.min(anchorColumn, selectedColumn))
                                              + ":" + Sheets.cellName(Math.max(anchorRow, selectedRow), Math.max(anchorColumn, selectedColumn))
    signal selectionChanged()
    Connections { target: Sheets; function onDimensionsChanged() { root.dimensionRevision++; grid.forceLayout() } }

    function viewRowFor(sourceRow) {
        for (let row = 0; row < Sheets.visibleRowCount; row++)
            if (Sheets.sourceRow(row) === sourceRow) return row
        return -1
    }
    function selectCell(row, column, extend) {
        commitEditing()
        if (!extend) { anchorRow = row; anchorColumn = column }
        selectedRow = row
        selectedColumn = column
        selectionChanged()
        const viewRow = viewRowFor(row)
        if (viewRow >= 0)
            grid.positionViewAtCell(Qt.point(column, viewRow), TableView.Contain)
        grid.forceActiveFocus()
    }
    function extendSelection(row, column) {
        if (row === selectedRow && column === selectedColumn) return
        selectedRow = row
        selectedColumn = column
        selectionChanged()
    }
    function formulaEditor() {
        if (activeEditor && activeEditor.activeFocus && activeEditor.text.startsWith("=")) return activeEditor
        if (formulaBarEditor && formulaBarEditor.activeFocus && formulaBarEditor.text.startsWith("=")) return formulaBarEditor
        return null
    }
    function beginReference(row, column) {
        const editor = formulaEditor()
        if (!editor) return false
        let start = editor.selectionStart
        let end = editor.selectionEnd
        if (start < 0 || end < 0 || start === end) start = end = editor.cursorPosition
        if (start === 0 && end === editor.text.length && editor.text.startsWith("=")) start = 1
        editor.remove(start, end)
        const label = Sheets.cellName(row, column)
        editor.insert(start, label)
        editor.cursorPosition = start + label.length
        editor.forceActiveFocus()
        referenceEditor = editor
        referenceAnchorRow = row
        referenceAnchorColumn = column
        referenceStart = start
        referenceLength = label.length
        return true
    }
    function updateReference(row, column) {
        if (!referenceEditor || row < 0) return
        const label = row === referenceAnchorRow && column === referenceAnchorColumn
                      ? Sheets.cellName(row, column)
                      : Sheets.cellName(Math.min(referenceAnchorRow, row), Math.min(referenceAnchorColumn, column))
                        + ":" + Sheets.cellName(Math.max(referenceAnchorRow, row), Math.max(referenceAnchorColumn, column))
        if (referenceEditor.text.substring(referenceStart, referenceStart + referenceLength) === label) return
        referenceEditor.remove(referenceStart, referenceStart + referenceLength)
        referenceEditor.insert(referenceStart, label)
        referenceLength = label.length
        referenceEditor.cursorPosition = referenceStart + referenceLength
        referenceEditor.forceActiveFocus()
    }
    function endReference() { referenceEditor = null }
    function focusGrid() { grid.forceActiveFocus() }
    function cancelEditing() {
        if (!activeEditor || !activeEditor.activeFocus) return false
        activeEditor.cancel()
        return true
    }
    function cellAtGridPoint(point) {
        const x = point.x + grid.contentX
        const y = point.y + grid.contentY
        let offset = 0
        let column = 51
        for (let index = 0; index < 52; index++) {
            offset += Sheets.columnWidth(index)
            if (x < offset) { column = index; break }
        }
        offset = 0
        let viewRow = Math.max(0, Sheets.visibleRowCount - 1)
        for (let index = 0; index < Sheets.visibleRowCount; index++) {
            offset += Sheets.rowHeight(Sheets.sourceRow(index))
            if (y < offset) { viewRow = index; break }
        }
        return { row: Sheets.sourceRow(viewRow), column: column }
    }
    function startDrag(mode) {
        commitEditing()
        dragTop = Math.min(anchorRow, selectedRow)
        dragBottom = Math.max(anchorRow, selectedRow)
        dragLeft = Math.min(anchorColumn, selectedColumn)
        dragRight = Math.max(anchorColumn, selectedColumn)
        const displays = [], bolds = []
        for (let row = dragTop; row <= dragBottom; row++) {
            const displayRow = [], boldRow = []
            for (let column = dragLeft; column <= dragRight; column++) {
                displayRow.push(Sheets.displayAt(row, column))
                boldRow.push(Sheets.boldAt(row, column))
            }
            displays.push(displayRow)
            bolds.push(boldRow)
        }
        dragSourceDisplay = displays
        dragSourceBold = bolds
        dragTargetRow = mode === "move" ? dragTop : dragBottom
        dragTargetColumn = mode === "move" ? dragLeft : dragRight
        dragMode = mode
    }
    function fillBounds() {
        const verticalDistance = dragTargetRow < dragTop ? dragTop - dragTargetRow : Math.max(0, dragTargetRow - dragBottom)
        const horizontalDistance = dragTargetColumn < dragLeft ? dragLeft - dragTargetColumn : Math.max(0, dragTargetColumn - dragRight)
        const vertical = verticalDistance >= horizontalDistance
        return { top: vertical ? Math.min(dragTop, dragTargetRow) : dragTop,
                 bottom: vertical ? Math.max(dragBottom, dragTargetRow) : dragBottom,
                 left: vertical ? dragLeft : Math.min(dragLeft, dragTargetColumn),
                 right: vertical ? dragRight : Math.max(dragRight, dragTargetColumn) }
    }
    function previewFor(row, column) {
        if (dragMode === "") return null
        const height = dragBottom - dragTop + 1, width = dragRight - dragLeft + 1
        if (dragMode === "fill") {
            const bounds = fillBounds()
            if (row < bounds.top || row > bounds.bottom || column < bounds.left || column > bounds.right ||
                (row >= dragTop && row <= dragBottom && column >= dragLeft && column <= dragRight)) return null
            const sourceRow = ((row - dragTop) % height + height) % height
            const sourceColumn = ((column - dragLeft) % width + width) % width
            return { display: dragSourceDisplay[sourceRow][sourceColumn],
                     bold: dragSourceBold[sourceRow][sourceColumn], kind: "fill" }
        }
        const targetRow = Math.max(0, Math.min(500 - height, dragTargetRow))
        const targetColumn = Math.max(0, Math.min(52 - width, dragTargetColumn))
        if (row >= targetRow && row < targetRow + height && column >= targetColumn && column < targetColumn + width)
            return { display: dragSourceDisplay[row - targetRow][column - targetColumn],
                     bold: dragSourceBold[row - targetRow][column - targetColumn], kind: "move" }
        if (row >= dragTop && row <= dragBottom && column >= dragLeft && column <= dragRight)
            return { display: "", bold: false, kind: "cleared" }
        return null
    }
    function cancelDrag() {
        dragMode = ""
        dragSourceDisplay = []
        dragSourceBold = []
    }
    function finishDrag() {
        const top = dragTop, bottom = dragBottom
        const left = dragLeft, right = dragRight
        if (dragMode === "fill") {
            const bounds = fillBounds()
            Sheets.fillRange(top, left, bottom, right, dragTargetRow, dragTargetColumn)
            anchorRow = bounds.top
            selectedRow = bounds.bottom
            anchorColumn = bounds.left
            selectedColumn = bounds.right
            selectionChanged()
        } else if (dragMode === "move") {
            const targetRow = Math.max(0, Math.min(500 - (bottom - top + 1), dragTargetRow))
            const targetColumn = Math.max(0, Math.min(52 - (right - left + 1), dragTargetColumn))
            Sheets.moveRange(top, left, bottom, right, targetRow, targetColumn)
            anchorRow = targetRow
            anchorColumn = targetColumn
            selectedRow = targetRow + bottom - top
            selectedColumn = targetColumn + right - left
            selectionChanged()
        }
        cancelDrag()
        grid.forceActiveFocus()
    }
    function fillSelectionDown() {
        const top = Math.min(anchorRow, selectedRow), bottom = Math.max(anchorRow, selectedRow)
        if (bottom > top) Sheets.fillRange(top, Math.min(anchorColumn, selectedColumn), top,
                                            Math.max(anchorColumn, selectedColumn), bottom, selectedColumn)
    }
    function fillSelectionRight() {
        const left = Math.min(anchorColumn, selectedColumn), right = Math.max(anchorColumn, selectedColumn)
        if (right > left) Sheets.fillRange(Math.min(anchorRow, selectedRow), left,
                                          Math.max(anchorRow, selectedRow), left, selectedRow, right)
    }
    function beginSelectedEdit(replacement) {
        const sourceRow = selectedRow
        const column = selectedColumn
        const viewRow = viewRowFor(selectedRow)
        if (viewRow < 0) return
        grid.positionViewAtCell(Qt.point(column, viewRow), TableView.Contain)
        const visibleCell = grid.itemAtCell(Qt.point(column, viewRow))
        if (visibleCell) { visibleCell.beginEditing(replacement); return }
        Qt.callLater(function() {
            if (selectedRow !== sourceRow || selectedColumn !== column) return
            const cell = grid.itemAtCell(Qt.point(column, viewRow))
            if (cell) cell.beginEditing(replacement)
        })
    }
    function moveSelection(rowDelta, columnDelta, extend) {
        if (Sheets.visibleRowCount === 0) return
        const viewRow = viewRowFor(selectedRow)
        const nextViewRow = Math.max(0, Math.min(Sheets.visibleRowCount - 1, viewRow + rowDelta))
        selectCell(Sheets.sourceRow(nextViewRow), Math.max(0, Math.min(51, selectedColumn + columnDelta)), extend)
    }
    function resetSelection() { selectCell(0, 0, false) }
    function copySelection() { commitEditing(); Sheets.copyRange(anchorRow, anchorColumn, selectedRow, selectedColumn) }
    function pasteSelection() { commitEditing(); Sheets.pasteRangeToSelection(anchorRow, anchorColumn, selectedRow, selectedColumn) }
    function commitEditing() {
        if (activeEditor) activeEditor.commit()
    }

    Rectangle { anchors.fill: parent; color: Theme.palette.background }

    Rectangle {
        x: 0; y: 0; width: 44; height: 28
        color: Theme.palette.surface
        border.color: Theme.palette.border
        IconView { anchors.centerIn: parent; iconName: "table-2"; tint: Theme.palette.muted; width: 15; height: 15 }
    }
    Flickable {
        id: columnHeader
        x: 44; y: 0
        width: root.gridWidth - 44; height: 28
        contentWidth: headerColumns.width
        contentHeight: height
        contentX: grid.contentX
        interactive: false
        clip: true
        Row {
            id: headerColumns
            Repeater {
                model: 52
                Rectangle {
                    width: Sheets.columnWidth(index) + root.dimensionRevision * 0; height: 28
                    color: index === root.selectedColumn ? Theme.palette.raised : Theme.palette.surface
                    border.color: Theme.palette.border
                    Text {
                        anchors.centerIn: parent
                        text: Sheets.columnName(index)
                        color: index === root.selectedColumn ? Theme.palette.accent : Theme.palette.muted
                        font.pixelSize: 11
                        font.weight: Font.DemiBold
                    }
                    MouseArea {
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.bottom: parent.bottom
                        width: 9
                        cursorShape: Qt.SplitHCursor
                        property real pressX: 0
                        property int startWidth: 0
                        onPressed: function(mouse) { pressX = mapToItem(root, mouse.x, 0).x; startWidth = Sheets.columnWidth(index); Sheets.beginDimensionResize(); mouse.accepted = true }
                        onPositionChanged: function(mouse) { if (pressed) Sheets.setColumnWidth(index, startWidth + Math.round(mapToItem(root, mouse.x, 0).x - pressX)) }
                        onReleased: Sheets.endDimensionResize()
                        onCanceled: Sheets.endDimensionResize()
                    }
                    TapHandler { onTapped: root.selectCell(root.selectedRow, index) }
                }
            }
        }
    }
    Flickable {
        id: rowHeader
        x: 0; y: 28
        width: 44; height: parent.height - 64
        contentWidth: width
        contentHeight: headerRows.height
        contentY: grid.contentY
        interactive: false
        clip: true
        Column {
            id: headerRows
            Repeater {
                model: Sheets.visibleRowCount
                Rectangle {
                    readonly property int logicalRow: Sheets.sourceRow(index) + Sheets.revision * 0
                    width: 44; height: Sheets.rowHeight(logicalRow) + root.dimensionRevision * 0
                    color: logicalRow === root.selectedRow ? Theme.palette.raised : Theme.palette.surface
                    border.color: Theme.palette.border
                    Text {
                        anchors.centerIn: parent
                        text: logicalRow + 1
                        color: logicalRow === root.selectedRow ? Theme.palette.accent : Theme.palette.muted
                        font.pixelSize: 11
                    }
                    MouseArea {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        height: 8
                        cursorShape: Qt.SplitVCursor
                        property real pressY: 0
                        property int startHeight: 0
                        onPressed: function(mouse) { pressY = mapToItem(root, 0, mouse.y).y; startHeight = Sheets.rowHeight(logicalRow); Sheets.beginDimensionResize(); mouse.accepted = true }
                        onPositionChanged: function(mouse) { if (pressed) Sheets.setRowHeight(logicalRow, startHeight + Math.round(mapToItem(root, 0, mouse.y).y - pressY)) }
                        onReleased: Sheets.endDimensionResize()
                        onCanceled: Sheets.endDimensionResize()
                    }
                    TapHandler { onTapped: root.selectCell(logicalRow, root.selectedColumn) }
                }
            }
        }
    }
    TableView {
        id: grid
        objectName: "sheetsGrid"
        x: 44; y: 28
        width: root.gridWidth - 44; height: parent.height - 64
        clip: true
        model: Sheets
        reuseItems: true
        rowSpacing: 0
        columnSpacing: 0
        focus: true
        Keys.onPressed: function(event) {
            const extend = Boolean(event.modifiers & Qt.ShiftModifier)
            if (event.key === Qt.Key_Left) root.moveSelection(0, -1, extend)
            else if (event.key === Qt.Key_Right) root.moveSelection(0, 1, extend)
            else if (event.key === Qt.Key_Up) root.moveSelection(-1, 0, extend)
            else if (event.key === Qt.Key_Down || event.key === Qt.Key_Return || event.key === Qt.Key_Enter) root.moveSelection(1, 0, extend)
            else if (event.key === Qt.Key_Tab) root.moveSelection(0, extend ? -1 : 1, false)
            else if (event.key === Qt.Key_F2) root.beginSelectedEdit()
            else if (event.key === Qt.Key_Delete || event.key === Qt.Key_Backspace)
                Sheets.clearRange(root.anchorRow, root.anchorColumn, root.selectedRow, root.selectedColumn)
            else if (event.text.length > 0 && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)))
                root.beginSelectedEdit(event.text)
            else return
            event.accepted = true
        }
        rowHeightProvider: function(row) { return Sheets.rowHeight(Sheets.sourceRow(row)) }
        columnWidthProvider: function(column) { return Sheets.columnWidth(column) }
        ScrollBar.vertical: ScrollBar { }
        ScrollBar.horizontal: ScrollBar { }
        delegate: Rectangle {
            id: cell
            objectName: "sheetCell-" + sourceRow + "-" + column
            required property int row
            required property int column
            required property string display
            required property string raw
            required property bool bold
            required property int sourceRow
            property bool editing: false
            readonly property bool inSelection: sourceRow >= Math.min(root.anchorRow, root.selectedRow)
                                                && sourceRow <= Math.max(root.anchorRow, root.selectedRow)
                                                && column >= Math.min(root.anchorColumn, root.selectedColumn)
                                                && column <= Math.max(root.anchorColumn, root.selectedColumn)
            readonly property string fillColor: { Sheets.revision; return Sheets.fillColorAt(sourceRow, column) }
            readonly property string textColor: { Sheets.revision; return Sheets.textColorAt(sourceRow, column) }
            readonly property int cellAlignment: { Sheets.revision; return Sheets.alignmentAt(sourceRow, column) }
            readonly property var dragPreview: root.previewFor(sourceRow, column)
            readonly property string shownText: dragPreview ? dragPreview.display : display
            function beginEditing(replacement) {
                root.commitEditing()
                editing = true
                root.activeEditor = input
                input.text = replacement === undefined ? raw : replacement
                input.forceActiveFocus()
                if (replacement === undefined) input.selectAll()
                else input.cursorPosition = input.length
            }
            implicitWidth: 112
            implicitHeight: 30
            color: dragPreview && dragPreview.kind === "cleared" ? Theme.palette.background
                   : dragPreview ? Theme.palette.raised
                   : fillColor !== "" ? fillColor
                   : inSelection ? Theme.palette.raised : Theme.palette.background
            border.color: dragPreview && dragPreview.kind !== "cleared" ? Theme.palette.accent
                          : inSelection ? Theme.palette.accent : Theme.palette.border
            border.width: (dragPreview && dragPreview.kind !== "cleared") ||
                          (root.selectedRow === sourceRow && root.selectedColumn === column) ? 2 : 1

            Text {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 5
                verticalAlignment: Text.AlignVCenter
                text: cell.shownText
                visible: !cell.editing
                color: cell.dragPreview && cell.dragPreview.kind !== "cleared" ? Theme.palette.accent
                       : cell.textColor !== "" ? cell.textColor
                       : text.startsWith("#") ? Theme.palette.accent : Theme.palette.text
                font.pixelSize: 12
                font.bold: cell.dragPreview ? cell.dragPreview.bold : cell.bold
                horizontalAlignment: cell.cellAlignment === 1 ? Text.AlignHCenter
                                     : cell.cellAlignment === 2 ? Text.AlignRight : Text.AlignLeft
                elide: Text.ElideRight
            }
            TextInput {
                id: input
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 5
                verticalAlignment: TextInput.AlignVCenter
                visible: cell.editing
                text: cell.raw
                color: cell.textColor !== "" ? cell.textColor : Theme.palette.text
                selectionColor: Theme.palette.accent
                selectByMouse: true
                font.pixelSize: 12
                horizontalAlignment: cell.cellAlignment === 1 ? TextInput.AlignHCenter
                                     : cell.cellAlignment === 2 ? TextInput.AlignRight : TextInput.AlignLeft
                function commit() {
                    if (!cell.editing) return
                    const row = cell.sourceRow
                    const column = cell.column
                    const value = text
                    cell.editing = false
                    focus = false
                    if (root.activeEditor === input) root.activeEditor = null
                    Sheets.setCell(row, column, value)
                    root.selectionChanged()
                }
                function cancel() {
                    if (!cell.editing) return
                    cell.editing = false
                    focus = false
                    text = cell.raw
                    if (root.activeEditor === input) root.activeEditor = null
                    root.endReference()
                    grid.forceActiveFocus()
                }
                onActiveFocusChanged: if (!activeFocus) commit()
                Keys.onReturnPressed: function(event) { event.accepted = true; commit(); root.moveSelection(1, 0, false) }
                Keys.onEnterPressed: function(event) { event.accepted = true; commit(); root.moveSelection(1, 0, false) }
                Keys.onTabPressed: function(event) { event.accepted = true; commit(); root.moveSelection(0, 1, false) }
                Keys.onBacktabPressed: function(event) { event.accepted = true; commit(); root.moveSelection(0, -1, false) }
                Keys.onEscapePressed: function(event) { event.accepted = true; cancel() }
            }
            MouseArea {
                id: cellMouse
                anchors.fill: parent
                enabled: !cell.editing
                preventStealing: true
                cursorShape: Qt.ArrowCursor
                onPressed: function(mouse) {
                    if (!root.beginReference(cell.sourceRow, cell.column))
                        root.selectCell(cell.sourceRow, cell.column, Boolean(mouse.modifiers & Qt.ShiftModifier))
                }
                onPositionChanged: function(mouse) {
                    if (!pressed) return
                    const point = mapToItem(grid, mouse.x, mouse.y)
                    const target = root.cellAtGridPoint(point)
                    if (root.referenceEditor) root.updateReference(target.row, target.column)
                    else if (target.row >= 0) root.extendSelection(target.row, target.column)
                }
                onReleased: root.endReference()
                onCanceled: root.endReference()
                onDoubleClicked: {
                    if (root.formulaEditor()) return
                    root.selectCell(cell.sourceRow, cell.column)
                    root.beginSelectedEdit()
                }
            }
            Rectangle {
                visible: !cell.editing && cell.sourceRow === Math.min(root.anchorRow, root.selectedRow)
                         && cell.column === Math.min(root.anchorColumn, root.selectedColumn)
                x: 0; y: 0; width: 9; height: 9
                color: Theme.palette.accent
                opacity: 0.75
                MouseArea {
                    anchors.fill: parent
                    preventStealing: true
                    cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                    onPressed: root.startDrag("move")
                    onPositionChanged: function(mouse) {
                        if (!pressed) return
                        const target = root.cellAtGridPoint(mapToItem(grid, mouse.x, mouse.y))
                        root.dragTargetRow = target.row
                        root.dragTargetColumn = target.column
                    }
                    onReleased: root.finishDrag()
                    onCanceled: root.cancelDrag()
                }
            }
            Rectangle {
                visible: !cell.editing && cell.sourceRow === Math.max(root.anchorRow, root.selectedRow)
                         && cell.column === Math.max(root.anchorColumn, root.selectedColumn)
                x: parent.width - 8; y: parent.height - 8; width: 8; height: 8
                color: Theme.palette.accent
                border.color: Theme.palette.background
                MouseArea {
                    anchors.fill: parent
                    preventStealing: true
                    cursorShape: Qt.CrossCursor
                    onPressed: root.startDrag("fill")
                    onPositionChanged: function(mouse) {
                        if (!pressed) return
                        const target = root.cellAtGridPoint(mapToItem(grid, mouse.x, mouse.y))
                        root.dragTargetRow = target.row
                        root.dragTargetColumn = target.column
                    }
                    onReleased: root.finishDrag()
                    onCanceled: root.cancelDrag()
                }
            }
            TableView.onPooled: {
                if (editing) input.commit()
                if (root.activeEditor === input) root.activeEditor = null
            }
            TableView.onReused: {
                if (editing) input.commit()
                if (root.activeEditor === input) root.activeEditor = null
                input.text = raw
            }
        }
    }
    Rectangle {
        id: tabBar
        x: 0; y: parent.height - 36; width: parent.width; height: 36
        color: Theme.palette.surface
        border.color: Theme.palette.border
        Row {
            anchors.left: parent.left; anchors.leftMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Repeater {
                model: Sheets.sheetNames
                Rectangle {
                    required property int index
                    required property string modelData
                    width: Math.max(86, label.implicitWidth + 28); height: 27; radius: 4
                    color: index === Sheets.activeSheetIndex ? Theme.palette.raised : Theme.palette.surface
                    border.color: index === Sheets.activeSheetIndex ? Theme.palette.accent : Theme.palette.border
                    Text { id: label; anchors.centerIn: parent; text: modelData; color: index === Sheets.activeSheetIndex ? Theme.palette.text : Theme.palette.muted; font.pixelSize: 11; elide: Text.ElideRight }
                    MouseArea {
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: function(mouse) {
                            root.commitEditing()
                            if (mouse.button === Qt.RightButton) { tabActions.sheetIndex = index; tabActions.popup() }
                            else { Sheets.selectSheet(index); root.resetSelection() }
                        }
                        onDoubleClicked: { tabActions.sheetIndex = index; renameField.text = Sheets.sheetNames[index]; renameDialog.open() }
                    }
                }
            }
            IconButton { iconName: "file-plus"; helpText: "Add sheet"; onClicked: { root.commitEditing(); Sheets.addSheet(); root.resetSelection() } }
        }
        Menu {
            id: tabActions
            property int sheetIndex: 0
            MenuItem { text: "Rename"; onTriggered: { renameField.text = Sheets.sheetNames[tabActions.sheetIndex]; renameDialog.open() } }
            MenuItem { text: "Duplicate"; onTriggered: { Sheets.selectSheet(tabActions.sheetIndex); Sheets.duplicateSheet(); root.resetSelection() } }
            MenuItem { text: "Delete"; enabled: Sheets.sheetNames.length > 1; onTriggered: { Sheets.removeSheet(tabActions.sheetIndex); root.resetSelection() } }
        }
        Dialog {
            id: renameDialog
            title: "Rename sheet"
            modal: true
            anchors.centerIn: Overlay.overlay
            standardButtons: Dialog.Ok | Dialog.Cancel
            onAccepted: Sheets.renameSheet(tabActions.sheetIndex, renameField.text)
            TextField { id: renameField; width: 240; selectByMouse: true; onAccepted: renameDialog.accept() }
        }
    }
    Rectangle {
        id: chartPane
        visible: Sheets.chartRange !== ""
        x: root.gridWidth
        y: 0
        width: 320
        height: parent.height - 36
        color: Theme.palette.surface
        border.color: Theme.palette.border
        Column {
            anchors.fill: parent
            anchors.margins: 16
            spacing: 8
            Row {
                width: parent.width
                Text {
                    text: "Chart"
                    color: Theme.palette.text
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    width: parent.width - 40
                }
                IconButton { iconName: "x"; helpText: "Remove chart"; onClicked: Sheets.setChart("", "bar") }
            }
            Text { text: Sheets.chartRange + " · " + Sheets.chartType; color: Theme.palette.muted; font.pixelSize: 11 }
            Canvas {
                id: chartCanvas
                width: parent.width
                height: Math.min(310, chartPane.height - 85)
                onPaint: {
                    const ctx = getContext("2d")
                    ctx.clearRect(0, 0, width, height)
                    const points = Sheets.chartData
                    if (!points || points.length === 0) return
                    if (Sheets.chartType === "pie") {
                        const slices = points.filter(point => point.value > 0)
                        const total = slices.reduce((sum, point) => sum + point.value, 0)
                        if (total <= 0) return
                        const colors = ["#638be0", "#e47b89", "#71bc9a", "#d6a15e", "#ae91df", "#67b9c5", "#d980bc", "#a5b668"]
                        const centerX = 91, centerY = height / 2 - 8, radius = Math.min(75, height / 2 - 35)
                        let angle = -Math.PI / 2
                        for (let index = 0; index < slices.length; index++) {
                            const sweep = slices[index].value / total * 2 * Math.PI
                            ctx.beginPath()
                            ctx.moveTo(centerX, centerY)
                            ctx.arc(centerX, centerY, radius, angle, angle + sweep)
                            ctx.closePath()
                            ctx.fillStyle = colors[index % colors.length]
                            ctx.fill()
                            angle += sweep
                            if (index < 8) {
                                ctx.fillRect(184, 22 + index * 25, 9, 9)
                                ctx.fillStyle = Theme.palette.text
                                ctx.font = "10px Noto Sans"
                                ctx.textAlign = "left"
                                ctx.fillText(slices[index].label + "  " + Math.round(slices[index].value / total * 100) + "%", 198, 31 + index * 25, width - 202)
                            }
                        }
                        return
                    }
                    const left = 28, right = width - 10, top = 16, bottom = height - 38
                    const maximum = Math.max(0, ...points.map(point => point.value))
                    const minimum = Math.min(0, ...points.map(point => point.value))
                    const span = Math.max(1, maximum - minimum)
                    const zeroY = top + maximum / span * (bottom - top)
                    ctx.strokeStyle = Theme.palette.border
                    ctx.lineWidth = 1
                    ctx.beginPath()
                    ctx.moveTo(left, top)
                    ctx.lineTo(left, bottom)
                    ctx.moveTo(left, zeroY)
                    ctx.lineTo(right, zeroY)
                    ctx.stroke()
                    const slot = (right - left) / points.length
                    if (Sheets.chartType === "area") {
                        ctx.save()
                        ctx.globalAlpha = 0.24
                        ctx.fillStyle = Theme.palette.accent
                        ctx.beginPath()
                        ctx.moveTo(left + slot * 0.5, zeroY)
                        for (let index = 0; index < points.length; index++)
                            ctx.lineTo(left + slot * (index + 0.5), top + (maximum - points[index].value) / span * (bottom - top))
                        ctx.lineTo(left + slot * (points.length - 0.5), zeroY)
                        ctx.closePath()
                        ctx.fill()
                        ctx.restore()
                    }
                    ctx.fillStyle = Theme.palette.accent
                    ctx.strokeStyle = Theme.palette.accent
                    for (let index = 0; index < points.length; index++) {
                        const point = points[index]
                        const center = left + slot * (index + 0.5)
                        const valueY = top + (maximum - point.value) / span * (bottom - top)
                        if (Sheets.chartType === "line" || Sheets.chartType === "area") {
                            if (index === 0) ctx.beginPath()
                            if (index === 0) ctx.moveTo(center, valueY)
                            else ctx.lineTo(center, valueY)
                            if (index === points.length - 1) ctx.stroke()
                            ctx.fillRect(center - 2, valueY - 2, 4, 4)
                        } else {
                            ctx.fillRect(center - slot * 0.3, Math.min(valueY, zeroY),
                                         slot * 0.6, Math.abs(valueY - zeroY))
                        }
                        ctx.fillStyle = Theme.palette.muted
                        ctx.font = "10px Noto Sans"
                        ctx.textAlign = "center"
                        ctx.fillText(point.label, center, bottom + 18, slot - 2)
                        ctx.fillStyle = Theme.palette.accent
                    }
                }
                Connections {
                    target: Sheets
                    function onStateChanged() { chartCanvas.requestPaint() }
                }
            }
            Text {
                text: Sheets.chartData.length === 0 ? "Choose a range with numeric values."
                    : Sheets.chartType === "pie" && !Sheets.chartData.some(point => point.value > 0)
                      ? "Pie charts need positive values." : ""
                color: Theme.palette.muted
                font.pixelSize: 11
            }
        }
    }
}
