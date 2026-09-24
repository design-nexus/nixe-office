import QtQuick
import QtQuick.Layouts

Rectangle {
    id: card
    property string iconName: ""
    property string title: ""
    property string description: ""
    signal clicked()
    width: 226
    height: 94
    radius: 7
    color: pointer.containsMouse ? Theme.palette.raised : Theme.palette.surface
    border.color: pointer.containsMouse ? Theme.palette.accent : Theme.palette.border
    RowLayout {
        anchors.fill: parent
        anchors.margins: 13
        spacing: 11
        Rectangle {
            Layout.preferredWidth: 34
            Layout.preferredHeight: 34
            radius: 6
            color: Theme.palette.raised
            IconView { anchors.centerIn: parent; iconName: card.iconName; tint: Theme.palette.accent; width: 18; height: 18 }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 3
            Text { text: card.title; color: Theme.palette.text; font.pixelSize: 12; font.weight: Font.DemiBold; elide: Text.ElideRight; Layout.fillWidth: true }
            Text { text: card.description; color: Theme.palette.muted; font.pixelSize: 10; wrapMode: Text.WordWrap; Layout.fillWidth: true; maximumLineCount: 2; elide: Text.ElideRight }
        }
    }
    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: card.clicked()
    }
}
