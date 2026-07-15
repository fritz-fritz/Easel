import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Frame {
    id: root
    padding: 18

    background: Rectangle {
        radius: 10
        color: root.palette.alternateBase
        border.color: root.palette.mid
        border.width: 1
    }

    Item {
        anchors.fill: parent

        Rectangle {
            id: leftDisplay
            x: parent.width * 0.04
            y: parent.height * 0.22
            width: parent.width * 0.29
            height: parent.height * 0.57
            radius: 6
            color: "#40566A"
            border.color: root.palette.highlight
            border.width: 2
            Label { anchors.centerIn: parent; text: qsTr("Left · 2560×1440"); color: "white" }
        }

        Rectangle {
            x: parent.width * 0.35
            y: parent.height * 0.09
            width: parent.width * 0.34
            height: parent.height * 0.70
            radius: 6
            color: "#776B5D"
            border.color: root.palette.highlight
            border.width: 2
            Label { anchors.centerIn: parent; text: qsTr("Center · 3840×2160"); color: "white" }
        }

        Rectangle {
            x: parent.width * 0.71
            y: parent.height * 0.28
            width: parent.width * 0.25
            height: parent.height * 0.51
            radius: 6
            color: "#48604F"
            border.color: root.palette.highlight
            border.width: 2
            Label { anchors.centerIn: parent; text: qsTr("Right · 1920×1080"); color: "white" }
        }

        Label {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            text: qsTr("Physical layout preview · drag and calibration are planned")
            opacity: 0.62
        }
    }
}
