import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "components"

Item {
    id: root
    signal requestSpelling()
    function focusEditor() { editor.forceActiveFocus() }
    function cut() { editor.cut(); focusEditor() }
    function copy() { editor.copy(); focusEditor() }
    function paste() { editor.paste(); focusEditor() }
    function selectAll() { editor.selectAll(); focusEditor() }
    function toggleBold() { Write.toggleBold(editor.selectedText.length ? editor.selectionStart : editor.cursorPosition,
                                              editor.selectedText.length ? editor.selectionEnd : editor.cursorPosition); focusEditor() }
    function toggleItalic() { Write.toggleItalic(editor.selectedText.length ? editor.selectionStart : editor.cursorPosition,
                                                  editor.selectedText.length ? editor.selectionEnd : editor.cursorPosition); focusEditor() }
    function setFontFamily(family) { Write.setFontFamily(editor.selectionStart, editor.selectionEnd, family); focusEditor() }
    function setFontSize(points) { Write.setFontSize(editor.selectionStart, editor.selectionEnd, points); focusEditor() }
    function setFontColor(color) { Write.setFontColor(editor.selectionStart, editor.selectionEnd, color); focusEditor() }
    function setHeading(level) { Write.setHeading(editor.cursorPosition, level); focusEditor() }
    function toggleList(ordered) { Write.toggleList(editor.cursorPosition, ordered); focusEditor() }
    function insertImage(file) { if (Write.insertImage(editor.cursorPosition, file)) focusEditor() }
    function insertTable(rows, columns) { Write.insertTable(editor.cursorPosition, rows, columns); focusEditor() }
    function cursorPosition() { return editor.cursorPosition }
    function replaceWordAt(position, replacement) { Write.replaceWordAt(position, replacement); focusEditor() }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Flickable {
            id: pageView
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: Math.max(width, 780)
            contentHeight: paper.height + 64
            clip: true
            ScrollBar.vertical: ScrollBar { }
            ScrollBar.horizontal: ScrollBar { }
            Rectangle {
                x: paper.x + 2
                y: paper.y + 5
                width: paper.width
                height: paper.height
                color: Theme.palette.dark ? "#33000000" : "#18000000"
                radius: 2
            }
            Rectangle {
                id: paper
                x: Math.max(30, (pageView.contentWidth - width) / 2)
                y: 30
                width: 720
                height: Math.max(930, Write.pageCount * 930)
                color: "#ffffff"
                border.color: "#d5d9df"
                Repeater {
                    model: Math.max(0, Write.pageCount - 1)
                    Rectangle { x: 0; y: (index + 1) * 930 - 1; width: paper.width; height: 1; color: "#e5e8ec" }
                }
                TextEdit {
                    id: editor
                    anchors.fill: parent
                    textFormat: TextEdit.RichText
                    wrapMode: TextEdit.Wrap
                    color: "#202329"
                    selectionColor: "#a9c8ff"
                    selectedTextColor: "#172337"
                    selectByMouse: true
                    persistentSelection: true
                    font.family: "Noto Sans"
                    font.pointSize: 11
                    Component.onCompleted: Write.attachEditor(textDocument)
                    MouseArea {
                        anchors.fill: parent
                        acceptedButtons: Qt.RightButton
                        onClicked: function(mouse) {
                            editor.cursorPosition = editor.positionAt(mouse.x, mouse.y)
                            root.requestSpelling()
                        }
                    }
                }
                Repeater {
                    model: Write.pageCount
                    Item {
                        x: 0; y: index * 930; width: paper.width; height: 930
                        Text {
                            x: Write.leftMargin; y: 12
                            width: paper.width - Write.leftMargin - Write.rightMargin
                            height: Math.max(18, Write.topMargin - 18)
                            text: Write.headerText
                            color: "#536071"; font.pixelSize: 11
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                        }
                        Text {
                            x: Write.leftMargin; y: 930 - Write.bottomMargin + 6
                            width: paper.width - Write.leftMargin - Write.rightMargin - (Write.showPageNumbers ? 50 : 0)
                            height: Math.max(18, Write.bottomMargin - 12)
                            text: Write.footerText
                            color: "#536071"; font.pixelSize: 11
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                        }
                        Text {
                            visible: Write.showPageNumbers
                            x: paper.width - Write.rightMargin - 45; y: 930 - Write.bottomMargin + 6
                            width: 45; height: Math.max(18, Write.bottomMargin - 12)
                            text: String(index + 1)
                            color: "#536071"; font.pixelSize: 11
                            horizontalAlignment: Text.AlignRight
                            verticalAlignment: Text.AlignVCenter
                        }
                    }
                }
                Repeater {
                    model: Math.max(0, Write.pageCount - 1)
                    Rectangle {
                        x: 0; y: (index + 1) * 930 - 6
                        width: paper.width; height: 12
                        color: Theme.palette.background
                    }
                }
            }
        }
        Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.palette.border }
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 28
            color: Theme.palette.surface
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 17
                anchors.rightMargin: 17
                spacing: 6
                IconView { iconName: "file-text"; tint: Theme.palette.muted; Layout.preferredWidth: 14; Layout.preferredHeight: 14 }
                Text { text: Write.displayName; color: Theme.palette.muted; font.family: "Noto Sans"; font.pixelSize: 11 }
                Item { Layout.fillWidth: true }
                Text { text: "Nexus Write"; color: Theme.palette.muted; font.family: "Noto Sans"; font.pixelSize: 10 }
            }
        }
    }

}
