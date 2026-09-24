import QtQuick

Image {
    id: icon
    property string iconName: ""
    property color tint: "#ffffff"
    width: 18
    height: 18
    sourceSize.width: 48
    sourceSize.height: 48
    fillMode: Image.PreserveAspectFit
    source: iconName === "" ? "" : "image://nexus-icons/" + iconName + "~" + tint.toString().replace("#", "")
}
