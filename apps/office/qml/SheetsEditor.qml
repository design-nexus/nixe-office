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
    property int dimensionRevision: 0
    readonly property int gridWidth: width - (Sheets.chartRange !== "" ? 320 : 0)
    readonly property string selectionName: Sheets.cellName(selectedRow, selectedColumn)
    signal selectionChanged()
    Connections { target: Sheets; function onDimensionsChanged() { root.dimensionRevision++; grid.forceLayout() } }

    function selectCell(row, column, extend) {
        if (!extend) { anchorRow = row; anchorColumn = column }
        selectedRow = row
        selectedColumn = column
        selectionChanged()
    }
    function resetSelection() { anchorRow = 0; anchorColumn = 0; selectedRow = 0; selectedColumn = 0; selectionChanged() }
    function copySelection() { commitEditing(); Sheets.copyRange(anchorRow, anchorColumn, selectedRow, selectedColumn) }
    function pasteSelection() { commitEditing(); Sheets.pasteRange(selectedRow, selectedColumn) }
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
        x: 44; y: 28
        width: root.gridWidth - 44; height: parent.height - 64
        clip: true
        model: Sheets
        reuseItems: true
        rowSpacing: 0
        columnSpacing: 0
        rowHeightProvider: function(row) { return Sheets.rowHeight(Sheets.sourceRow(row)) }
        columnWidthProvider: function(column) { return Sheets.columnWidth(column) }
        ScrollBar.vertical: ScrollBar { }
        ScrollBar.horizontal: ScrollBar { }
        delegate: Rectangle {
            id: cell
            required property int row
            required property int column
            required property string display
            required property string raw
            required property bool bold
            required property int sourceRow
            property bool editing: false
            implicitWidth: 112
            implicitHeight: 30
            color: sourceRow >= Math.min(root.anchorRow, root.selectedRow) && sourceRow <= Math.max(root.anchorRow, root.selectedRow)
                   && column >= Math.min(root.anchorColumn, root.selectedColumn) && column <= Math.max(root.anchorColumn, root.selectedColumn)
                   ? Theme.palette.raised : Theme.palette.background
            border.color: root.selectedRow === sourceRow && root.selectedColumn === column
                          ? Theme.palette.accent : Theme.palette.border
            border.width: root.selectedRow === sourceRow && root.selectedColumn === column ? 2 : 1

            Text {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 5
                verticalAlignment: Text.AlignVCenter
                text: cell.display
                visible: !cell.editing
                color: text.startsWith("#") ? Theme.palette.accent : Theme.palette.text
                font.pixelSize: 12
                font.bold: cell.bold
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
                color: Theme.palette.text
                selectionColor: Theme.palette.accent
                selectByMouse: true
                font.pixelSize: 12
                function commit() {
                    if (!cell.editing) return
                    const row = cell.sourceRow
                    const column = cell.column
                    const value = text
                    cell.editing = false
                    if (root.activeEditor === input) root.activeEditor = null
                    Sheets.setCell(row, column, value)
                    root.selectionChanged()
                }
                onAccepted: commit()
                onActiveFocusChanged: if (!activeFocus) commit()
                Keys.onEscapePressed: {
                    cell.editing = false
                    text = cell.raw
                    if (root.activeEditor === input) root.activeEditor = null
                }
            }
            MouseArea {
                anchors.fill: parent
                enabled: !cell.editing
                onClicked: function(mouse) { root.selectCell(cell.sourceRow, cell.column, Boolean(mouse.modifiers & Qt.ShiftModifier)) }
                onDoubleClicked: {
                    root.selectCell(cell.sourceRow, cell.column)
                    cell.editing = true
                    root.activeEditor = input
                    input.text = cell.raw
                    input.forceActiveFocus()
                    input.selectAll()
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
