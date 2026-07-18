// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Frame {
    id: root
    padding: 18

    property var previewUrls: []
    property bool previewReady: false
    // Encoded rows: "id|xFactor|yFactor|wFactor|hFactor|label|ox|oy|wMm|hMm|bezel"
    property var layoutModel: []
    property string selectedDisplayId: ""
    property bool physicalPreview: true

    signal displaySelected(string displayId)
    signal displayMoved(string displayId, real originXmm, real originYmm)

    background: Rectangle {
        radius: 10
        color: root.palette.alternateBase
        border.color: root.palette.mid
        border.width: 1
    }

    Item {
        id: canvas
        anchors.fill: parent

        property var parsedRows: {
            var rows = []
            for (var i = 0; i < root.layoutModel.length; ++i) {
                var parts = String(root.layoutModel[i]).split("|")
                if (parts.length < 6)
                    continue
                rows.push({
                    id: parts[0],
                    xFactor: Number(parts[1]),
                    yFactor: Number(parts[2]),
                    wFactor: Number(parts[3]),
                    hFactor: Number(parts[4]),
                    label: parts[5],
                    originXmm: parts.length > 6 ? Number(parts[6]) : 0,
                    originYmm: parts.length > 7 ? Number(parts[7]) : 0,
                    widthMm: parts.length > 8 ? Number(parts[8]) : 1,
                    heightMm: parts.length > 9 ? Number(parts[9]) : 1,
                    bezelMm: parts.length > 10 ? Number(parts[10]) : 0,
                    index: i
                })
            }
            if (rows.length === 0) {
                rows = [
                    {
                        id: "",
                        xFactor: 0.04,
                        yFactor: 0.22,
                        wFactor: 0.29,
                        hFactor: 0.57,
                        label: qsTr("No displays"),
                        originXmm: 0,
                        originYmm: 0,
                        widthMm: 1,
                        heightMm: 1,
                        bezelMm: 0,
                        index: 0
                    }
                ]
            }
            return rows
        }

        property real minOriginX: {
            var min = Number.POSITIVE_INFINITY
            for (var i = 0; i < parsedRows.length; ++i)
                min = Math.min(min, parsedRows[i].originXmm)
            return isFinite(min) ? min : 0
        }
        property real minOriginY: {
            var min = Number.POSITIVE_INFINITY
            for (var i = 0; i < parsedRows.length; ++i)
                min = Math.min(min, parsedRows[i].originYmm)
            return isFinite(min) ? min : 0
        }
        property real spanXmm: {
            var max = Number.NEGATIVE_INFINITY
            for (var i = 0; i < parsedRows.length; ++i)
                max = Math.max(max, parsedRows[i].originXmm + parsedRows[i].widthMm)
            var span = max - minOriginX
            return span > 0 ? span : 1
        }
        property real spanYmm: {
            var max = Number.NEGATIVE_INFINITY
            for (var i = 0; i < parsedRows.length; ++i)
                max = Math.max(max, parsedRows[i].originYmm + parsedRows[i].heightMm)
            var span = max - minOriginY
            return span > 0 ? span : 1
        }

        Repeater {
            model: canvas.parsedRows

            delegate: Rectangle {
                id: monitor
                required property var modelData

                property real dragOriginX: modelData.originXmm
                property real dragOriginY: modelData.originYmm

                x: parent.width * modelData.xFactor
                y: parent.height * modelData.yFactor
                width: parent.width * modelData.wFactor
                height: parent.height * modelData.hFactor
                radius: 6
                color: Qt.hsla((modelData.index * 0.17) % 1.0, 0.25, 0.35, 1.0)
                border.color: modelData.id === root.selectedDisplayId ? root.palette.highlight : root.palette.mid
                border.width: modelData.id === root.selectedDisplayId ? 3 : 2
                clip: true

                Image {
                    anchors.fill: parent
                    anchors.margins: Math.max(2, modelData.bezelMm > 0 ? 4 : 2)
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

                MouseArea {
                    anchors.fill: parent
                    enabled: root.physicalPreview && modelData.id.length > 0
                    cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                    property real pressX: 0
                    property real pressY: 0
                    onPressed: (mouse) => {
                        pressX = mouse.x
                        pressY = mouse.y
                        monitor.dragOriginX = modelData.originXmm
                        monitor.dragOriginY = modelData.originYmm
                        root.displaySelected(modelData.id)
                    }
                    onReleased: (mouse) => {
                        var usableW = canvas.width * 0.92
                        var usableH = canvas.height * 0.92
                        var dxMm = (mouse.x - pressX) / Math.max(usableW, 1) * canvas.spanXmm
                        var dyMm = (mouse.y - pressY) / Math.max(usableH, 1) * canvas.spanYmm
                        root.displayMoved(modelData.id, monitor.dragOriginX + dxMm, monitor.dragOriginY + dyMm)
                    }
                }
            }
        }

        Label {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            text: root.physicalPreview
                  ? qsTr("Physical layout · drag to snap · edit size and bezel below")
                  : qsTr("Digital layout preview · switch Correction to Physical + bezel")
            opacity: 0.62
        }
    }
}
