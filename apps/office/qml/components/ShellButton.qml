import QtQuick
import QtQuick.Controls

Button {
    id: control
    property bool prominent: false
    property bool quiet: false
    implicitHeight: 29
    implicitWidth: Math.max(52, label.implicitWidth + 18)
    hoverEnabled: true
    background: Rectangle {
        radius: 4
        color: control.prominent ? Theme.palette.accent
              : control.down || control.hovered ? Theme.palette.raised
              : control.quiet ? "transparent" : Theme.palette.surface
        border.width: control.prominent || control.quiet ? 0 : 1
        border.color: Theme.palette.border
    }
    contentItem: Text {
        id: label
        text: control.text
        color: control.prominent ? Theme.palette.accentText : Theme.palette.text
        font.family: "JetBrains Mono"
        font.pixelSize: 11
        font.weight: Font.Medium
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
}
