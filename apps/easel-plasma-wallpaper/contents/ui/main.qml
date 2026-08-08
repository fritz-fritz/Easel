import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Qt.labs.platform as Platform
import QtMultimedia
import org.kde.plasma.plasmoid

// Plasma wallpaper host for Easel. Management (library, schedules, spanning) stays
// in easel-desktop; this package renders still frames and live media under
// plasmashell. IPC: desktop writes active.json (still posters + optional live
// session with shared media_time_ms). See docs/adr/0008-plasma-wallpaper-plugin-host.md.

WallpaperItem {
    id: root

    property string stateImageUrl: ""
    property string lastStatePayload: ""
    property var liveDoc: null
    property var liveCrop: null
    property real lastSeekMediaMs: -1

    readonly property bool liveActive: {
        return root.liveDoc
                && root.liveDoc.mode === "live"
                && root.liveDoc.live
                && root.liveCrop
    }

    readonly property bool liveIsGif: {
        if (!root.liveActive)
            return false
        const kind = root.liveDoc.live.media_kind || ""
        if (kind === "animated_image")
            return true
        const src = String(root.liveDoc.live.source || "").toLowerCase()
        return src.endsWith(".gif")
    }

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
        // Linux ProjectDirs("net","fritztech","Easel").data_dir() is
        // $XDG_DATA_HOME/easel (not reverse-DNS). GenericDataLocation is the
        // XDG data root, so this matches the desktop writer path.
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

    function pickLiveCrop(doc) {
        if (!doc || !doc.live || !doc.live.displays || !doc.live.displays.length) {
            return null
        }
        const geom = root.screenGeometry()
        for (let i = 0; i < doc.live.displays.length; ++i) {
            const entry = doc.live.displays[i]
            const g = entry.geometry
            if (!g) {
                continue
            }
            if (g.x === geom.x && g.y === geom.y
                    && g.width === geom.width && g.height === geom.height) {
                return entry
            }
        }
        if (doc.live.displays.length === 1) {
            return doc.live.displays[0]
        }
        return null
    }

    function sourceRectFromUv(uv) {
        if (!uv) {
            return Qt.rect(0, 0, 1, 1)
        }
        return Qt.rect(uv.x || 0, uv.y || 0, uv.width || 1, uv.height || 1)
    }

    function applyLivePlayback() {
        if (!root.liveActive || root.liveIsGif) {
            player.stop()
            player.source = ""
            return
        }
        const live = root.liveDoc.live
        const source = root.fileUrlForPath(live.source || "")
        if (!source || source.length === 0) {
            return
        }
        if (String(player.source) !== String(source)) {
            player.source = source
            root.lastSeekMediaMs = -1
        }
        player.playbackRate = live.rate > 0 ? live.rate : 1.0
        player.loops = (live.loop_mode === "once") ? 1 : MediaPlayer.Infinite
        const targetMs = live.media_time_ms || 0
        // Resync when desktop clock drifts more than ~120ms or on pause edges.
        if (live.paused) {
            if (player.playbackState === MediaPlayer.PlayingState) {
                player.pause()
            }
            if (Math.abs(player.position - targetMs) > 120) {
                player.position = targetMs
                root.lastSeekMediaMs = targetMs
            }
        } else {
            if (Math.abs(targetMs - root.lastSeekMediaMs) > 500
                    || Math.abs(player.position - targetMs) > 250) {
                player.position = targetMs
                root.lastSeekMediaMs = targetMs
            }
            if (player.playbackState !== MediaPlayer.PlayingState) {
                player.play()
            }
        }
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
                // Still refresh live seek even when JSON text is unchanged? Desktop
                // rewrites the file every tick, so payload usually changes. Done.
                return
            }
            root.lastStatePayload = payload
            try {
                const doc = JSON.parse(payload)
                root.liveDoc = doc
                root.liveCrop = root.pickLiveCrop(doc)
                root.stateImageUrl = root.pickImageFromState(payload)
                root.applyLivePlayback()
            } catch (e) {
                root.liveDoc = null
                root.liveCrop = null
                root.stateImageUrl = root.pickImageFromState(payload)
            }
        }
        request.open("GET", root.fileUrlForPath(path))
        request.send()
    }

    Timer {
        interval: 250
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.reloadStateFile()
    }

    // Extra tick so video seek follows media_time_ms even if file mtime coalesces.
    Timer {
        interval: 33
        running: root.liveActive && !root.liveIsGif
        repeat: true
        onTriggered: root.applyLivePlayback()
    }

    Rectangle {
        anchors.fill: parent
        color: "#1a1a1a"

        // Still / poster layer (also shown while live decode is not ready).
        // Raise above live layers until GIF Status.Ready / video Playing so a
        // loading AnimatedImage cannot blank the desktop.
        readonly property bool showPosterFallback: !root.liveActive
                || (root.liveIsGif ? gifPlayer.status !== Image.Ready
                                   : player.playbackState !== MediaPlayer.PlayingState)

        Image {
            id: still
            anchors.fill: parent
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: false
            source: root.imageUrl
            visible: parent.showPosterFallback
            z: parent.showPosterFallback ? 2 : 0
        }

        // GIF live crop using UV window from plan_live_crops.
        Item {
            id: gifCrop
            anchors.fill: parent
            clip: true
            visible: root.liveActive && root.liveIsGif
            z: 1

            readonly property var uv: root.liveCrop ? root.liveCrop.source_uv : null
            readonly property real ux: uv ? (uv.x || 0) : 0
            readonly property real uy: uv ? (uv.y || 0) : 0
            readonly property real uw: uv && uv.width > 0 ? uv.width : 1
            readonly property real uh: uv && uv.height > 0 ? uv.height : 1

            AnimatedImage {
                id: gifPlayer
                width: parent.width / gifCrop.uw
                height: parent.height / gifCrop.uh
                x: -gifCrop.ux * width
                y: -gifCrop.uy * height
                fillMode: Image.Stretch
                asynchronous: true
                cache: false
                // Keep source set while live so decode can finish under the poster.
                source: (root.liveActive && root.liveIsGif)
                        ? root.fileUrlForPath(root.liveDoc.live.source) : ""
                playing: root.liveActive && root.liveIsGif
                        && root.liveDoc && root.liveDoc.live && !root.liveDoc.live.paused
            }
        }

        // Video live crop; sourceRect is normalized UV (Qt Multimedia).
        VideoOutput {
            id: liveVideo
            anchors.fill: parent
            fillMode: VideoOutput.Stretch
            visible: root.liveActive && !root.liveIsGif
            sourceRect: root.sourceRectFromUv(root.liveCrop ? root.liveCrop.source_uv : null)
            z: 1
        }

        MediaPlayer {
            id: player
            videoOutput: liveVideo
            audioOutput: AudioOutput {
                muted: true
                volume: 0
            }
        }

        Label {
            anchors.centerIn: parent
            visible: !root.liveActive && still.status !== Image.Ready
            color: "#cccccc"
            text: still.status === Image.Loading
                ? qsTr("Loading Easel wallpaper…")
                : qsTr("Managed by Easel — apply a wallpaper from the Easel app")
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            width: parent.width * 0.7
            z: 2
        }
    }
}
