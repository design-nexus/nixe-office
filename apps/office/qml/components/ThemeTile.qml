import QtQuick
import QtQuick.Controls

Button {
    id: control
    property string themeId: ""
    property string themeName: ""
    property var swatch: ({})
    property bool active: Theme.selectedTheme === themeId
    implicitHeight: 34
    hoverEnabled: true
    background: Rectangle {
        color: control.hovered || control.active ? Theme.palette.raised : "transparent"
        Rectangle { width: 2; height: parent.height; visible: control.active; color: Theme.palette.accent }
    }
    contentItem: Row {
        spacing: 8
        leftPadding: 7
        Rectangle {
            width: 20; height: 20; radius: 3
            anchors.verticalCenter: parent.verticalCenter
            color: control.swatch.background || Theme.palette.background
            border.color: control.swatch.border || Theme.palette.border
            Rectangle { width: 7; height: 7; radius: 4; anchors.centerIn: parent; color: control.swatch.accent || Theme.palette.accent }
        }
        Text {
            text: control.themeName
            color: Theme.palette.text
            font.family: "JetBrains Mono"
            font.pixelSize: 10
            verticalAlignment: Text.AlignVCenter
            height: control.height
            elide: Text.ElideRight
            width: Math.max(0, control.width - 43)
        }
    }
}
