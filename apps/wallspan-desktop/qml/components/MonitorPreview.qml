import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Frame {
    id: root
    padding: 18

    property var previewUrls: []
    property bool previewReady: false
    // Encoded rows: "xFactor|yFactor|wFactor|hFactor|label"
    property var layoutModel: []

    background: Rectangle {
        radius: 10
        color: root.palette.alternateBase
        border.color: root.palette.mid
        border.width: 1
    }

    Item {
        anchors.fill: parent

        Repeater {
            model: {
                var rows = []
                for (var i = 0; i < root.layoutModel.length; ++i) {
                    var parts = String(root.layoutModel[i]).split("|")
                    if (parts.length < 5)
                        continue
                    rows.push({
                        xFactor: Number(parts[0]),
                        yFactor: Number(parts[1]),
                        wFactor: Number(parts[2]),
                        hFactor: Number(parts[3]),
                        label: parts.slice(4).join("|"),
                        index: i
                    })
                }
                if (rows.length === 0) {
                    rows = [
                        { xFactor: 0.04, yFactor: 0.22, wFactor: 0.29, hFactor: 0.57, label: qsTr("No displays"), index: 0 }
                    ]
                }
                return rows
            }

            delegate: Rectangle {
                required property var modelData
                x: parent.width * modelData.xFactor
                y: parent.height * modelData.yFactor
                width: parent.width * modelData.wFactor
                height: parent.height * modelData.hFactor
                radius: 6
                color: Qt.hsla((modelData.index * 0.17) % 1.0, 0.25, 0.35, 1.0)
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
                    wrapMode: Text.WordWrap
                    horizontalAlignment: Text.AlignHCenter
                    width: parent.width - 12
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
