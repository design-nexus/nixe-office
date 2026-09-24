import QtQuick
import QtQuick.Window
import QtQuick.Layouts
import "components"

Window {
    id: showWindow
    visible: false
    width: 960
    height: 540
    color: "#0b0d14"
    title: "Nexus Present — Slideshow"
    property int slideIndex: 0
    property var slideData: ({})
    onSlideIndexChanged: slideData = Present.slideAt(slideIndex)

    function start(index) {
        if (Present.slideCount < 1) return
        slideIndex = Math.max(0, Math.min(index, Present.slideCount - 1))
        slideData = Present.slideAt(slideIndex)
        showFullScreen()
        requestActivate()
        inputLayer.forceActiveFocus()
    }
    function next() {
        if (slideIndex < Present.slideCount - 1) slideIndex++
        else close()
    }
    function previous() {
        if (slideIndex > 0) slideIndex--
    }

    Item {
        id: inputLayer
        anchors.fill: parent
        focus: true
        Keys.onPressed: function(event) {
            if (event.key === Qt.Key_Escape) showWindow.close()
            else if (event.key === Qt.Key_Right || event.key === Qt.Key_Down ||
                     event.key === Qt.Key_Space || event.key === Qt.Key_PageDown ||
                     event.key === Qt.Key_Return || event.key === Qt.Key_Enter) showWindow.next()
            else if (event.key === Qt.Key_Left || event.key === Qt.Key_Up ||
                     event.key === Qt.Key_Backspace || event.key === Qt.Key_PageUp) showWindow.previous()
            else if (event.key === Qt.Key_Home) showWindow.slideIndex = 0
            else if (event.key === Qt.Key_End) showWindow.slideIndex = Present.slideCount - 1
            else return
            event.accepted = true
        }
        PresentationSlide {
            id: canvas
            title: showWindow.slideData.title || ""
            body: showWindow.slideData.body || ""
            layout: showWindow.slideData.layout || 0
            style: showWindow.slideData.style || 0
            fontFamily: showWindow.slideData.fontFamily || "Noto Sans"
            fontColor: showWindow.slideData.fontColor || ""
            backgroundColor: showWindow.slideData.backgroundColor || ""
            bold: showWindow.slideData.bold || false
            italic: showWindow.slideData.italic || false
            titleSize: showWindow.slideData.titleSize || 0
            bodySize: showWindow.slideData.bodySize || 0
            elements: showWindow.slideData.elements || []
            scale: Math.min(inputLayer.width / width, inputLayer.height / height)
            x: (inputLayer.width - width) / 2
            y: (inputLayer.height - height) / 2
        }
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: function(mouse) {
                if (mouse.button === Qt.RightButton) showWindow.previous()
                else showWindow.next()
            }
        }
        Rectangle {
            width: controls.implicitWidth + 14
            height: 45
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 18
            radius: 8
            color: "#dd171b28"
            border.color: "#55ffffff"
            RowLayout {
                id: controls
                anchors.centerIn: parent
                spacing: 5
                IconButton { iconName: "arrow-left"; iconTint: "#ffffff"; helpText: "Previous slide · Left arrow"; enabled: showWindow.slideIndex > 0; onClicked: { showWindow.previous(); inputLayer.forceActiveFocus() } }
                Text { text: (showWindow.slideIndex + 1) + " / " + Present.slideCount; color: "#ffffff"; font.pixelSize: 11; horizontalAlignment: Text.AlignHCenter; Layout.preferredWidth: 48 }
                IconButton { iconName: "arrow-right"; iconTint: "#ffffff"; helpText: showWindow.slideIndex === Present.slideCount - 1 ? "End slideshow" : "Next slide · Right arrow"; onClicked: { showWindow.next(); inputLayer.forceActiveFocus() } }
                Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 20; color: "#55ffffff" }
                IconButton { iconName: "x"; iconTint: "#ffffff"; helpText: "End slideshow · Esc"; onClicked: showWindow.close() }
            }
        }
    }
}
