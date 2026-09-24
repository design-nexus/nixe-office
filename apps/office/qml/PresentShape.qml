import QtQuick

Item {
    id: shapeItem
    property string shapeKind: "rectangle"
    property color fillColor: "#5273cf"
    Rectangle {
        anchors.fill: parent
        visible: shapeItem.shapeKind === "rectangle"
        color: shapeItem.fillColor
    }
    Canvas {
        id: ellipseCanvas
        anchors.fill: parent
        visible: shapeItem.shapeKind === "ellipse"
        onPaint: {
            const ctx = getContext("2d")
            ctx.clearRect(0, 0, width, height)
            ctx.save()
            ctx.translate(width / 2, height / 2)
            ctx.scale(width / 2, height / 2)
            ctx.beginPath()
            ctx.arc(0, 0, 1, 0, 2 * Math.PI)
            ctx.fillStyle = shapeItem.fillColor
            ctx.fill()
            ctx.restore()
        }
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
    }
    onFillColorChanged: ellipseCanvas.requestPaint()
    onShapeKindChanged: ellipseCanvas.requestPaint()
}
