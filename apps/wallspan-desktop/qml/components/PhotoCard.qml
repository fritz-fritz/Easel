import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Frame {
    id: root
    required property string title
    required property string creator
    required property color accent

    padding: 0
    implicitHeight: 190

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

            Label {
                anchors.centerIn: parent
                text: qsTr("Image preview")
                color: "white"
                opacity: 0.82
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: 10
            spacing: 2
            Label { text: root.title; font.weight: Font.DemiBold; Layout.fillWidth: true; elide: Text.ElideRight }
            Label { text: root.creator; opacity: 0.62; font.pixelSize: 12; Layout.fillWidth: true; elide: Text.ElideRight }
        }
    }
}
