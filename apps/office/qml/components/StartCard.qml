import QtQuick
import QtQuick.Layouts

Rectangle {
    id: card
    property string iconName: ""
    property string title: ""
    property string description: ""
    property bool available: true
    signal clicked()
    implicitWidth: 240
    implicitHeight: 145
    radius: 10
    color: pointer.containsMouse && available ? Theme.palette.raised : Theme.palette.surface
    border.color: pointer.containsMouse && available ? Theme.palette.accent : Theme.palette.border
    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 17
        spacing: 0
        IconView { iconName: card.iconName; tint: card.available ? Theme.palette.accent : Theme.palette.muted; Layout.preferredWidth: 28; Layout.preferredHeight: 28 }
        Item { Layout.fillHeight: true }
        Text { text: card.title; color: Theme.palette.text; font.family: "Noto Sans"; font.pixelSize: 14; font.weight: Font.DemiBold }
        Text { text: card.description; color: Theme.palette.muted; font.family: "Noto Sans"; font.pixelSize: 11; Layout.topMargin: 3 }
    }
    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        enabled: card.available
        cursorShape: Qt.PointingHandCursor
        onClicked: card.clicked()
    }
}
