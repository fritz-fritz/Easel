import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Frame {
    id: root
    padding: 18

    property var previewUrls: []
    property bool previewReady: false

    background: Rectangle {
        radius: 10
        color: root.palette.alternateBase
        border.color: root.palette.mid
        border.width: 1
    }

    Item {
        anchors.fill: parent

        Repeater {
            model: [
                {
                    xFactor: 0.04,
                    yFactor: 0.22,
                    wFactor: 0.29,
                    hFactor: 0.57,
                    label: qsTr("Left · 2560×1440"),
                    fallback: "#40566A",
                    index: 0
                },
                {
                    xFactor: 0.35,
                    yFactor: 0.09,
                    wFactor: 0.34,
                    hFactor: 0.70,
                    label: qsTr("Center · 3840×2160"),
                    fallback: "#776B5D",
                    index: 1
                },
                {
                    xFactor: 0.71,
                    yFactor: 0.28,
                    wFactor: 0.25,
                    hFactor: 0.51,
                    label: qsTr("Right · 1920×1080"),
                    fallback: "#48604F",
                    index: 2
                }
            ]

            delegate: Rectangle {
                required property var modelData
                x: parent.width * modelData.xFactor
                y: parent.height * modelData.yFactor
                width: parent.width * modelData.wFactor
                height: parent.height * modelData.hFactor
                radius: 6
                color: modelData.fallback
                border.color: root.palette.highlight
                border.width: 2
                clip: true

                Image {
                    anchors.fill: parent
                    anchors.margins: 2
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    cache: false
                    visible: root.previewReady && root.previewUrls.length > modelData.index
                    source: visible ? root.previewUrls[modelData.index] : ""
                }

                Label {
                    anchors.centerIn: parent
                    text: modelData.label
                    color: "white"
                    style: Text.Outline
                    styleColor: "#80000000"
                    opacity: root.previewReady ? 0.85 : 1.0
                }
            }
        }

        Label {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            text: qsTr("Physical layout preview · drag and calibration are planned")
            opacity: 0.62
        }
    }
}
