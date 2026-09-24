import QtQuick
import QtQuick.Controls

Button {
    id: control
    property string iconName: ""
    property string helpText: ""
    property bool selected: false
    property bool emphasized: false
    property color iconTint: Theme.palette.text
    implicitWidth: 35
    implicitHeight: 34
    hoverEnabled: true
    Accessible.name: helpText
    ToolTip.visible: hovered && helpText !== ""
    ToolTip.text: helpText
    ToolTip.delay: 650
    background: Rectangle {
        radius: 6
        color: control.emphasized ? Theme.palette.accent
             : control.down || control.selected ? Theme.palette.raised
             : control.hovered ? Theme.palette.raised : "transparent"
        border.width: control.selected && !control.emphasized ? 1 : 0
        border.color: Theme.palette.border
    }
    contentItem: Item {
        IconView {
            anchors.centerIn: parent
            iconName: control.iconName
            tint: control.emphasized ? Theme.palette.accentText : control.enabled ? control.iconTint : Theme.palette.muted
        }
    }
}
