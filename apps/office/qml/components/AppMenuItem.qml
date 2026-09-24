import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

MenuItem {
    id: control
    property string iconName: ""
    property string shortcutLabel: ""
    implicitWidth: 226
    implicitHeight: 34
    background: Rectangle { radius: 5; color: control.highlighted ? Theme.palette.raised : "transparent" }
    contentItem: RowLayout {
        spacing: 10
        IconView {
            iconName: control.iconName
            tint: control.enabled ? Theme.palette.muted : Theme.palette.border
            Layout.preferredWidth: 16
            Layout.preferredHeight: 16
        }
        Text {
            text: control.text
            color: control.enabled ? Theme.palette.text : Theme.palette.muted
            font.family: "Noto Sans"
            font.pixelSize: 12
            Layout.fillWidth: true
        }
        Text {
            text: control.shortcutLabel
            color: Theme.palette.muted
            font.family: "JetBrains Mono"
            font.pixelSize: 10
            visible: text !== ""
        }
        Text {
            text: "›"
            color: Theme.palette.muted
            font.pixelSize: 19
            visible: control.subMenu !== null
        }
    }
}
