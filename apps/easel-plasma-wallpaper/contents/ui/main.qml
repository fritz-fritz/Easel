import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Qt.labs.platform as Platform
import org.kde.plasma.plasmoid

// Plasma wallpaper host for Easel. Management (library, schedules, spanning) stays
// in easel-desktop; this package renders the active still (and later live media)
// under plasmashell. Still-frame IPC: desktop writes active.json; we poll it so
// dense-solar ticks do not need PlasmaShell.evaluateScript after the first bind.
// See docs/adr/0008-plasma-wallpaper-plugin-host.md.

WallpaperItem {
    id: root

    property string stateImageUrl: ""
    property string lastStatePayload: ""

    readonly property url imageUrl: {
        if (root.stateImageUrl.length > 0) {
            return root.stateImageUrl
        }
        const configured = root.configuration.Image
        if (configured && configured.toString().length > 0) {
            return configured
        }
        return ""
    }

    readonly property string stateFilePath: {
        const configured = root.configuration.StateFile
        if (configured && configured.toString().length > 0) {
            return configured.toString()
        }
        // Match directories::ProjectDirs("net","fritztech","Easel").data_dir()/plasma-wallpaper
        return Platform.StandardPaths.writableLocation(Platform.StandardPaths.GenericDataLocation)
            + "/easel/plasma-wallpaper/active.json"
    }

    function screenGeometry() {
        // Prefer virtual desktop coordinates so they match Easel logical_rect.
        return Qt.rect(Screen.virtualX, Screen.virtualY, Screen.width, Screen.height)
    }

    function fileUrlForPath(path) {
        if (!path || path.length === 0) {
            return ""
        }
        if (path.indexOf("file:") === 0) {
            return path
        }
        return "file://" + path
    }

    function pickImageFromState(payload) {
        try {
            const doc = JSON.parse(payload)
            if (!doc || !doc.displays || !doc.displays.length) {
                return ""
            }
            const geom = root.screenGeometry()
            for (let i = 0; i < doc.displays.length; ++i) {
                const entry = doc.displays[i]
                const g = entry.geometry
                if (!g) {
                    continue
                }
                if (g.x === geom.x && g.y === geom.y
                        && g.width === geom.width && g.height === geom.height) {
                    return entry.image || ""
                }
            }
            // Single-display setups: accept the only frame even if geometry drifts.
            if (doc.displays.length === 1) {
                return doc.displays[0].image || ""
            }
        } catch (e) {
            return ""
        }
        return ""
    }

    function reloadStateFile() {
        const path = root.stateFilePath
        if (!path || path.length === 0) {
            return
        }
        const request = new XMLHttpRequest()
        request.onreadystatechange = function () {
            if (request.readyState !== XMLHttpRequest.DONE) {
                return
            }
            if (request.status !== 200 && request.status !== 0) {
                return
            }
            const payload = request.responseText
            if (!payload || payload === root.lastStatePayload) {
                return
            }
            root.lastStatePayload = payload
            root.stateImageUrl = root.pickImageFromState(payload)
        }
        request.open("GET", root.fileUrlForPath(path))
        request.send()
    }

    Timer {
        interval: 750
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.reloadStateFile()
    }

    Rectangle {
        anchors.fill: parent
        color: "#1a1a1a"

        Image {
            id: still
            anchors.fill: parent
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: false
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
