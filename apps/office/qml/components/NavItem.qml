import QtQuick
import QtQuick.Controls

Button {
    id: control
    property bool active: false
    property bool currentApp: false
    property string iconName: ""
    implicitHeight: 39
    hoverEnabled: true
    background: Rectangle {
        color: control.active ? Theme.palette.raised : control.hovered ? Theme.palette.background : "transparent"
        Rectangle { width: 2; height: parent.height; color: Theme.palette.accent; visible: control.active }
    }
    contentItem: Row {
        spacing: 12
        leftPadding: 17
        IconView {
            anchors.verticalCenter: parent.verticalCenter
            iconName: control.iconName
            tint: control.active || control.currentApp ? Theme.palette.accent : Theme.palette.muted
            width: 18
            height: 18
        }
        Text {
            height: control.height
            text: control.text
            color: control.active ? Theme.palette.text : control.currentApp ? Theme.palette.accent : Theme.palette.muted
            font.family: "Noto Sans"
            font.pixelSize: 12
            font.weight: control.active || control.currentApp ? Font.DemiBold : Font.Normal
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
            width: Math.max(0, control.width - 62)
        }
    }
}
