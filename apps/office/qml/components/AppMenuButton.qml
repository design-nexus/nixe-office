import QtQuick
import QtQuick.Controls

Button {
    id: control
    implicitHeight: 30
    implicitWidth: label.implicitWidth + 20
    hoverEnabled: true
    background: Rectangle { radius: 5; color: control.down || control.hovered ? Theme.palette.raised : "transparent" }
    contentItem: Text {
        id: label
        text: control.text
        color: Theme.palette.text
        font.family: "Noto Sans"
        font.pixelSize: 12
        font.weight: Font.Medium
        verticalAlignment: Text.AlignVCenter
        horizontalAlignment: Text.AlignHCenter
    }
}
