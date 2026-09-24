import QtQuick

Item {
    id: slide
    property string title: ""
    property string body: ""
    property int layout: 0
    property int style: 0
    property string fontFamily: "Noto Sans"
    property string fontColor: ""
    property string backgroundColor: ""
    property bool bold: false
    property bool italic: false
    property int titleSize: 0
    property int bodySize: 0
    property var elements: []
    width: 960
    height: 540
    readonly property color foreground: fontColor !== "" ? fontColor : style === 0 ? "#202536" : "#ffffff"

    Rectangle {
        anchors.fill: parent
        color: slide.backgroundColor !== "" ? slide.backgroundColor : slide.style === 1 ? "#20263a" : slide.style === 2 ? Theme.palette.accent : "#ffffff"
    }
    Text {
        x: 85
        y: slide.layout === 1 ? 76 : slide.layout === 2 ? 182 : 187
        width: parent.width - 170
        height: slide.layout === 1 ? 105 : 116
        text: slide.title
        color: slide.foreground
        font.family: slide.fontFamily
        font.pixelSize: slide.titleSize || (slide.layout === 1 ? 49 : 58)
        font.weight: slide.bold ? Font.Bold : Font.DemiBold
        font.italic: slide.italic
        horizontalAlignment: slide.layout === 1 ? Text.AlignLeft : Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        wrapMode: Text.Wrap
        clip: true
    }
    Text {
        x: 91
        y: slide.layout === 1 ? 197 : slide.layout === 2 ? 306 : 326
        width: parent.width - 182
        height: slide.layout === 1 ? 255 : 110
        text: slide.body
        color: slide.foreground
        font.family: slide.fontFamily
        font.pixelSize: slide.bodySize || (slide.layout === 1 ? 26 : 29)
        font.bold: slide.bold
        font.italic: slide.italic
        horizontalAlignment: slide.layout === 1 ? Text.AlignLeft : Text.AlignHCenter
        verticalAlignment: slide.layout === 1 ? Text.AlignTop : Text.AlignVCenter
        wrapMode: Text.Wrap
        clip: true
    }
    Repeater {
        model: slide.elements
        Item {
            required property var modelData
            x: modelData.x; y: modelData.y
            width: modelData.width; height: modelData.height
            Image { anchors.fill: parent; visible: modelData.type === "image"; source: modelData.image || ""; fillMode: Image.PreserveAspectFit }
            PresentShape { anchors.fill: parent; visible: modelData.type === "shape"; shapeKind: modelData.shape || "rectangle"; fillColor: modelData.fill || "#5273cf" }
            Text { anchors.fill: parent; visible: modelData.type === "text"; text: modelData.text; color: slide.foreground; font.family: slide.fontFamily; font.pixelSize: slide.bodySize || 28; font.bold: slide.bold; font.italic: slide.italic; wrapMode: Text.Wrap; verticalAlignment: Text.AlignVCenter; clip: true }
        }
    }
}
