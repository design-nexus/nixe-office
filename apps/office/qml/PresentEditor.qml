import QtQuick
import QtQuick.Layouts
import "components"

Item {
    id: editor
    property bool loadingSlide: false
    property string selectedElementId: ""
    readonly property string selectedElementType: {
        const item = Present.currentElements.find(function(element) { return element.id === selectedElementId })
        return item ? item.type : ""
    }
    readonly property color slideBackground: Present.currentBackgroundColor !== "" ? Present.currentBackgroundColor : Present.currentStyle === 1 ? "#20263a" : Present.currentStyle === 2 ? Theme.palette.accent : "#ffffff"
    readonly property color slideForeground: Present.currentFontColor !== "" ? Present.currentFontColor : Present.currentStyle === 0 ? "#202536" : "#ffffff"

    property string textTarget: "title"
    function setTextSize(size) { if (textTarget === "body") Present.setBodySize(size); else Present.setTitleSize(size) }
    function addTextBox() { const count = Present.currentElements.length; Present.addTextBox(); const items = Present.currentElements; if (items.length > count) selectedElementId = items[items.length - 1].id }
    function addImage(file) { if (Present.addImage(file)) { const items = Present.currentElements; selectedElementId = items[items.length - 1].id } }
    function addShape(kind) { const count = Present.currentElements.length; Present.addShape(kind); const items = Present.currentElements; if (items.length > count) selectedElementId = items[items.length - 1].id }
    function alignSelected(direction) { if (selectedElementId !== "") Present.alignElement(selectedElementId, direction) }
    function deleteSelectedElement() { if (selectedElementId !== "") { Present.removeElement(selectedElementId); selectedElementId = "" } }
    function loadSlide() {
        loadingSlide = true
        selectedElementId = ""
        titleEdit.text = Present.currentTitle
        bodyEdit.text = Present.currentBody
        notesEdit.text = Present.currentNotes
        loadingSlide = false
    }
    Component.onCompleted: loadSlide()
    Connections {
        target: Present
        function onSelectionChanged() { editor.loadSlide() }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0
        Rectangle {
            Layout.preferredWidth: 210
            Layout.fillHeight: true
            color: Theme.palette.surface
            ColumnLayout {
                anchors.fill: parent
                spacing: 0
                RowLayout {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 43
                    Layout.leftMargin: 15
                    Layout.rightMargin: 10
                    Text { text: "SLIDES"; color: Theme.palette.muted; font.pixelSize: 10; font.weight: Font.DemiBold }
                    Item { Layout.fillWidth: true }
                    Text { text: Present.slideCount; color: Theme.palette.muted; font.pixelSize: 10 }
                }
                Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.palette.border }
                ListView {
                    id: slidesList
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: 3
                    topMargin: 11
                    bottomMargin: 12
                    model: Present
                    delegate: Item {
                        required property int index
                        required property string title
                        required property string body
                        required property int layout
                        required property int style
                        required property int slideNumber
                        required property string fontFamily
                        required property string fontColor
                        required property string backgroundColor
                        required property bool bold
                        required property bool italic
                        required property var elements
                        width: slidesList.width
                        height: 108
                        Rectangle {
                            x: 9; y: 2; width: parent.width - 18; height: 103
                            radius: 5
                            color: Present.selectedIndex === index ? Theme.palette.raised : "transparent"
                            border.width: Present.selectedIndex === index ? 1 : 0
                            border.color: Theme.palette.accent
                        }
                        Text { x: 17; y: 45; text: slideNumber; color: Theme.palette.muted; font.pixelSize: 10 }
                        Rectangle {
                            x: 35; y: 11; width: 155; height: 87
                            radius: 2
                            color: backgroundColor !== "" ? backgroundColor : style === 1 ? "#20263a" : style === 2 ? Theme.palette.accent : "#ffffff"
                            border.color: style === 0 ? "#d4d8df" : "transparent"
                            clip: true
                            Text {
                                x: 12; y: layout === 1 ? 13 : 23; width: parent.width - 24
                                text: title || "Title"
                                color: fontColor !== "" ? fontColor : style === 0 ? "#202536" : "#ffffff"
                                font.family: fontFamily
                                font.pixelSize: layout === 1 ? 12 : 14
                                font.weight: bold ? Font.Bold : Font.DemiBold
                                font.italic: italic
                                elide: Text.ElideRight
                            }
                            Repeater {
                                model: elements
                                Item {
                                    required property var modelData
                                    x: modelData.x * 155 / 960
                                    y: modelData.y * 87 / 540
                                    width: modelData.width * 155 / 960
                                    height: modelData.height * 87 / 540
                                    Image { anchors.fill: parent; visible: modelData.type === "image"; source: modelData.image || ""; fillMode: Image.PreserveAspectFit }
                                    PresentShape { anchors.fill: parent; visible: modelData.type === "shape"; shapeKind: modelData.shape || "rectangle"; fillColor: modelData.fill || "#5273cf" }
                                    Text { anchors.fill: parent; visible: modelData.type === "text"; text: modelData.text; color: fontColor !== "" ? fontColor : style === 0 ? "#202536" : "#ffffff"; font.pixelSize: 5; wrapMode: Text.Wrap; clip: true }
                                }
                            }
                            Text {
                                x: 12; y: layout === 1 ? 38 : 50; width: parent.width - 24
                                text: body || (layout === 0 ? "Subtitle" : "Body text")
                                color: style === 0 ? "#677083" : "#e8eaf2"
                                font.family: fontFamily
                                font.pixelSize: 7
                                font.bold: bold
                                font.italic: italic
                                elide: Text.ElideRight
                            }
                        }
                        TapHandler { onTapped: Present.selectSlide(index) }
                    }
                }
                Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.palette.border }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 44
                    Layout.leftMargin: 10
                    IconButton { iconName: "file-plus"; helpText: "Add slide"; onClicked: Present.addSlide() }
                    Text { text: "Add slide"; color: Theme.palette.muted; font.pixelSize: 11 }
                    Item { Layout.fillWidth: true }
                }
            }
        }
        Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: Theme.palette.border }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0
            Item {
                id: workspace
                Layout.fillWidth: true
                Layout.fillHeight: true
                Rectangle {
                    id: slideShadow
                    width: slide.width * slide.scale + 2
                    height: slide.height * slide.scale + 2
                    anchors.centerIn: parent
                    color: "#26000000"
                    radius: 3
                }
                Item {
                    id: slide
                    width: 960
                    height: 540
                    scale: Math.min(1, (workspace.width - 58) / width, (workspace.height - 58) / height)
                    x: (workspace.width - width) / 2
                    y: (workspace.height - height) / 2
                    Rectangle { anchors.fill: parent; color: editor.slideBackground }
                    TextEdit {
                        id: titleEdit
                        x: 85
                        y: Present.currentLayout === 1 ? 76 : Present.currentLayout === 2 ? 182 : 187
                        width: parent.width - 170
                        height: Present.currentLayout === 1 ? 105 : 116
                        color: editor.slideForeground
                        font.family: Present.currentFontFamily
                        font.pixelSize: Present.currentTitleSize || (Present.currentLayout === 1 ? 49 : 58)
                        font.weight: Present.currentBold ? Font.Bold : Font.DemiBold
                        font.italic: Present.currentItalic
                        onActiveFocusChanged: if (activeFocus) editor.textTarget = "title"
                        horizontalAlignment: Present.currentLayout === 1 ? Text.AlignLeft : Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        wrapMode: TextEdit.Wrap
                        textFormat: TextEdit.PlainText
                        selectByMouse: true
                        clip: true
                        selectionColor: Theme.palette.accent
                        onTextChanged: if (!editor.loadingSlide) Present.setTitle(text)
                    }
                    Text {
                        anchors.fill: titleEdit
                        text: "Click to add a title"
                        color: Qt.rgba(editor.slideForeground.r, editor.slideForeground.g, editor.slideForeground.b, 0.45)
                        font.pixelSize: titleEdit.font.pixelSize
                        font.weight: Font.DemiBold
                        horizontalAlignment: titleEdit.horizontalAlignment
                        verticalAlignment: Text.AlignVCenter
                        visible: titleEdit.text.length === 0 && !titleEdit.activeFocus
                        MouseArea { anchors.fill: parent; onClicked: titleEdit.forceActiveFocus() }
                    }
                    TextEdit {
                        id: bodyEdit
                        x: 91
                        y: Present.currentLayout === 1 ? 197 : Present.currentLayout === 2 ? 306 : 326
                        width: parent.width - 182
                        height: Present.currentLayout === 1 ? 255 : 110
                        color: editor.slideForeground
                        font.family: Present.currentFontFamily
                        font.pixelSize: Present.currentBodySize || (Present.currentLayout === 1 ? 26 : 29)
                        font.bold: Present.currentBold
                        font.italic: Present.currentItalic
                        onActiveFocusChanged: if (activeFocus) editor.textTarget = "body"
                        horizontalAlignment: Present.currentLayout === 1 ? Text.AlignLeft : Text.AlignHCenter
                        verticalAlignment: Present.currentLayout === 1 ? Text.AlignTop : Text.AlignVCenter
                        wrapMode: TextEdit.Wrap
                        textFormat: TextEdit.PlainText
                        selectByMouse: true
                        clip: true
                        selectionColor: Theme.palette.accent
                        onTextChanged: if (!editor.loadingSlide) Present.setBody(text)
                    }
                    Text {
                        anchors.fill: bodyEdit
                        text: Present.currentLayout === 0 ? "Subtitle" : Present.currentLayout === 2 ? "Section description" : "Click to add body text"
                        color: Qt.rgba(editor.slideForeground.r, editor.slideForeground.g, editor.slideForeground.b, 0.45)
                        font.pixelSize: bodyEdit.font.pixelSize
                        horizontalAlignment: bodyEdit.horizontalAlignment
                        verticalAlignment: bodyEdit.verticalAlignment
                        visible: bodyEdit.text.length === 0 && !bodyEdit.activeFocus
                        MouseArea { anchors.fill: parent; onClicked: bodyEdit.forceActiveFocus() }
                    }
                    Repeater {
                        model: Present.currentElements
                        Item {
                            id: objectItem
                            required property var modelData
                            property int drawX: modelData.x
                            property int drawY: modelData.y
                            property int drawWidth: modelData.width
                            property int drawHeight: modelData.height
                            readonly property bool chosen: editor.selectedElementId === modelData.id
                            x: drawX; y: drawY; width: drawWidth; height: drawHeight
                            Image {
                                anchors.fill: parent
                                visible: objectItem.modelData.type === "image"
                                source: objectItem.modelData.image || ""
                                fillMode: Image.PreserveAspectFit
                                asynchronous: true
                            }
                            PresentShape {
                                anchors.fill: parent
                                visible: objectItem.modelData.type === "shape"
                                shapeKind: objectItem.modelData.shape || "rectangle"
                                fillColor: objectItem.modelData.fill || "#5273cf"
                            }
                            TextEdit {
                                anchors.fill: parent
                                anchors.margins: 3
                                visible: objectItem.modelData.type === "text"
                                text: objectItem.modelData.text
                                color: editor.slideForeground
                                font.family: Present.currentFontFamily
                                font.pixelSize: Present.currentBodySize || 28
                                font.bold: Present.currentBold
                                font.italic: Present.currentItalic
                                wrapMode: TextEdit.Wrap
                                textFormat: TextEdit.PlainText
                                selectByMouse: true
                                selectionColor: Theme.palette.accent
                                onActiveFocusChanged: if (activeFocus) editor.selectedElementId = objectItem.modelData.id
                                onTextChanged: if (activeFocus) Present.setElementText(objectItem.modelData.id, text)
                            }
                            MouseArea {
                                anchors.fill: parent
                                visible: objectItem.modelData.type === "image" || objectItem.modelData.type === "shape"
                                cursorShape: Qt.SizeAllCursor
                                property real originX: 0
                                property real originY: 0
                                property int startX: 0
                                property int startY: 0
                                onPressed: function(mouse) { editor.selectedElementId = objectItem.modelData.id; const point = mapToItem(slide, mouse.x, mouse.y); originX = point.x; originY = point.y; startX = objectItem.drawX; startY = objectItem.drawY }
                                onPositionChanged: function(mouse) { if (pressed) { const point = mapToItem(slide, mouse.x, mouse.y); objectItem.drawX = Math.max(0, Math.min(960 - objectItem.drawWidth, Math.round(startX + point.x - originX))); objectItem.drawY = Math.max(0, Math.min(540 - objectItem.drawHeight, Math.round(startY + point.y - originY))) } }
                                onReleased: Present.setElementRect(objectItem.modelData.id, objectItem.drawX, objectItem.drawY, objectItem.drawWidth, objectItem.drawHeight)
                            }
                            Rectangle {
                                anchors.fill: parent
                                color: "transparent"
                                border.width: objectItem.chosen ? 2 : 0
                                border.color: Theme.palette.accent
                            }
                            Rectangle {
                                id: moveHandle
                                width: Math.min(parent.width - 22, 110); height: 18
                                x: 0; y: -18
                                visible: objectItem.chosen
                                color: Theme.palette.accent
                                radius: 3
                                Text { anchors.centerIn: parent; text: "Drag to move"; color: "#ffffff"; font.pixelSize: 10 }
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.SizeAllCursor
                                    property real originX: 0
                                    property real originY: 0
                                    property int startX: 0
                                    property int startY: 0
                                    onPressed: function(mouse) { const point = mapToItem(slide, mouse.x, mouse.y); originX = point.x; originY = point.y; startX = objectItem.drawX; startY = objectItem.drawY }
                                    onPositionChanged: function(mouse) { if (pressed) { const point = mapToItem(slide, mouse.x, mouse.y); objectItem.drawX = Math.max(0, Math.min(960 - objectItem.drawWidth, Math.round(startX + point.x - originX))); objectItem.drawY = Math.max(0, Math.min(540 - objectItem.drawHeight, Math.round(startY + point.y - originY))) } }
                                    onReleased: Present.setElementRect(objectItem.modelData.id, objectItem.drawX, objectItem.drawY, objectItem.drawWidth, objectItem.drawHeight)
                                }
                            }
                            Rectangle {
                                width: 20; height: 20; radius: 3
                                x: parent.width - 20; y: -20
                                visible: objectItem.chosen
                                color: Theme.palette.surface; border.color: Theme.palette.accent
                                Text { anchors.centerIn: parent; text: "×"; color: Theme.palette.text; font.pixelSize: 16 }
                                MouseArea { anchors.fill: parent; onClicked: editor.deleteSelectedElement() }
                            }
                            Rectangle {
                                width: 18; height: 18; radius: 3
                                x: parent.width - 9; y: parent.height - 9
                                visible: objectItem.chosen
                                color: Theme.palette.accent
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.SizeFDiagCursor
                                    property real originX: 0
                                    property real originY: 0
                                    property int startWidth: 0
                                    property int startHeight: 0
                                    onPressed: function(mouse) { const point = mapToItem(slide, mouse.x, mouse.y); originX = point.x; originY = point.y; startWidth = objectItem.drawWidth; startHeight = objectItem.drawHeight }
                                    onPositionChanged: function(mouse) { if (pressed) { const point = mapToItem(slide, mouse.x, mouse.y); objectItem.drawWidth = Math.max(48, Math.min(960 - objectItem.drawX, Math.round(startWidth + point.x - originX))); objectItem.drawHeight = Math.max(32, Math.min(540 - objectItem.drawY, Math.round(startHeight + point.y - originY))) } }
                                    onReleased: Present.setElementRect(objectItem.modelData.id, objectItem.drawX, objectItem.drawY, objectItem.drawWidth, objectItem.drawHeight)
                                }
                            }
                        }
                    }
                }
            }
            Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.palette.border }
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 128
                color: Theme.palette.surface
                ColumnLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 20
                    anchors.rightMargin: 20
                    anchors.topMargin: 12
                    anchors.bottomMargin: 12
                    spacing: 6
                    RowLayout {
                        Layout.fillWidth: true
                        IconView { iconName: "file-text"; tint: Theme.palette.muted; Layout.preferredWidth: 14; Layout.preferredHeight: 14 }
                        Text { text: "SPEAKER NOTES"; color: Theme.palette.muted; font.pixelSize: 10; font.weight: Font.DemiBold }
                        Item { Layout.fillWidth: true }
                        Text { text: "Slide " + (Present.selectedIndex + 1) + " of " + Present.slideCount; color: Theme.palette.muted; font.pixelSize: 10 }
                    }
                    Item {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        TextEdit {
                            id: notesEdit
                            anchors.fill: parent
                            color: Theme.palette.text
                            font.pixelSize: 12
                            wrapMode: TextEdit.Wrap
                            textFormat: TextEdit.PlainText
                            selectByMouse: true
                            selectionColor: Theme.palette.accent
                            onTextChanged: if (!editor.loadingSlide) Present.setNotes(text)
                        }
                        Text { text: "Add notes for this slide…"; color: Theme.palette.muted; font.pixelSize: 12; visible: notesEdit.text.length === 0 && !notesEdit.activeFocus; MouseArea { anchors.fill: parent; onClicked: notesEdit.forceActiveFocus() } }
                    }
                }
            }
        }
    }
}
