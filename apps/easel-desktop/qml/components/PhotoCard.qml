// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Frame {
    id: root
    required property string title
    required property string creator
    property string subtitle: ""
    property string imageSource: ""
    property color accent: "#48604F"
    property bool meetsMinimum: true
    signal activated()
    signal favoriteRequested()

    padding: 0
    implicitHeight: 210

    background: Rectangle {
        radius: 9
        color: root.palette.base
        border.color: root.palette.mid
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            color: root.accent
            radius: 9
            clip: true

            Image {
                id: previewImage
                anchors.fill: parent
                source: root.imageSource
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                visible: root.imageSource.length > 0 && status === Image.Ready
            }

            Label {
                anchors.centerIn: parent
                visible: root.imageSource.length === 0 || previewImage.status !== Image.Ready
                text: qsTr("Image preview")
                color: "white"
                opacity: 0.82
            }

            Label {
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 8
                visible: !root.meetsMinimum
                text: qsTr("Low res")
                color: "white"
                padding: 4
                background: Rectangle {
                    color: "#AA000000"
                    radius: 4
                }
                font.pixelSize: 11
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: 10
            spacing: 2
            Label {
                text: root.title
                font.weight: Font.DemiBold
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
            Label {
                text: root.creator
                opacity: 0.62
                font.pixelSize: 12
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
            Label {
                visible: root.subtitle.length > 0
                text: root.subtitle
                opacity: 0.55
                font.pixelSize: 11
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
        }
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: (mouse) => {
            if (mouse.button === Qt.RightButton)
                root.favoriteRequested()
            else
                root.activated()
        }
    }
}
