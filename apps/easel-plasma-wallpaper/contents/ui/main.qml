import QtQuick
import QtQuick.Controls
import org.kde.plasma.plasmoid

// Plasma wallpaper host for Easel. Management (library, schedules, spanning) stays
// in easel-desktop; this package renders the active still (and later live media)
// under plasmashell. See docs/adr/0008-plasma-wallpaper-plugin-host.md.

WallpaperItem {
    id: root

    readonly property url imageUrl: {
        const configured = root.configuration.Image
        if (configured && configured.toString().length > 0) {
            return configured
        }
        return ""
    }

    Rectangle {
        anchors.fill: parent
        color: "#1a1a1a"

        Image {
            id: still
            anchors.fill: parent
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            source: root.imageUrl
            visible: status === Image.Ready
        }

        Label {
            anchors.centerIn: parent
            visible: still.status !== Image.Ready
            color: "#cccccc"
            text: still.status === Image.Loading
                ? qsTr("Loading Easel wallpaper…")
                : qsTr("Managed by Easel — apply a wallpaper from the Easel app")
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            width: parent.width * 0.7
        }
    }
}
