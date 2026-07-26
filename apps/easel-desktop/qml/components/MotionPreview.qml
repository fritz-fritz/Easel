// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import QtQuick
import QtQuick.Controls
import QtMultimedia

// Compose preview for local GIF / video. Still multi-display crop preview remains
// in MonitorPreview. Stage 6.6 Apply uses poster-fallback rasters; this surface
// remains single-display muted preview + diagnostics until the live host lands.
Frame {
    id: root
    padding: 12

    property url sourceUrl: ""
    // 0 = loop, 1 = play once, 2 = poster / paused first frame
    property int motionMode: 0
    property string diagnostics: ""

    readonly property bool isGif: {
        const s = String(root.sourceUrl).toLowerCase()
        return s.endsWith(".gif")
    }

    readonly property bool hasSource: String(root.sourceUrl).length > 0

    background: Rectangle {
        radius: 10
        color: root.palette.alternateBase
        border.color: root.palette.mid
        border.width: 1
    }

    function setDiagnostics(text) {
        root.diagnostics = text
    }

    function restartPlayback() {
        if (!root.hasSource || root.isGif)
            return
        player.stop()
        player.source = root.sourceUrl
        if (root.motionMode === 2) {
            player.pause()
            // Seek to start so the first frame is the poster-like view.
            player.position = 0
            player.play()
            Qt.callLater(() => player.pause())
        } else {
            player.loops = root.motionMode === 0 ? MediaPlayer.Infinite : 1
            player.play()
        }
    }

    onSourceUrlChanged: {
        if (!root.hasSource) {
            root.setDiagnostics(qsTr("Open a local GIF or video to preview motion."))
            player.stop()
            player.source = ""
            return
        }
        if (root.isGif) {
            // Stop any previous video decode; AnimatedImage owns GIF playback.
            player.stop()
            player.source = ""
            root.setDiagnostics(qsTr("GIF preview (AnimatedImage)"))
            return
        }
        root.setDiagnostics(qsTr("Loading video…"))
        root.restartPlayback()
    }

    onMotionModeChanged: {
        if (!root.hasSource || root.isGif) {
            player.stop()
            player.source = ""
            return
        }
        root.restartPlayback()
    }

    Item {
        anchors.fill: parent
        clip: true

        AnimatedImage {
            id: gifPreview
            anchors.fill: parent
            fillMode: Image.PreserveAspectFit
            visible: root.hasSource && root.isGif
            source: visible ? root.sourceUrl : ""
            playing: visible && root.motionMode !== 2
            asynchronous: true
            onStatusChanged: {
                if (!visible)
                    return
                if (status === Image.Error)
                    root.setDiagnostics(qsTr("GIF decode failed"))
                else if (status === Image.Ready)
                    root.setDiagnostics(qsTr("GIF ready · %1×%2").arg(implicitWidth).arg(implicitHeight))
            }
        }

        VideoOutput {
            id: videoOutput
            anchors.fill: parent
            fillMode: VideoOutput.PreserveAspectFit
            visible: root.hasSource && !root.isGif
        }

        MediaPlayer {
            id: player
            videoOutput: videoOutput
            audioOutput: AudioOutput {
                muted: true
                volume: 0
            }
            onErrorOccurred: function (error, errorString) {
                root.setDiagnostics(qsTr("Decoder error: %1").arg(errorString || error))
            }
            onMetaDataChanged: {
                if (!root.hasSource || root.isGif)
                    return
                const w = metaData.value(MediaMetaData.Resolution)
                const dur = duration
                const codec = metaData.stringValue(MediaMetaData.VideoCodec)
                        || metaData.stringValue(MediaMetaData.MediaType)
                        || ""
                let bits = []
                if (w)
                    bits.push(String(w.width) + "×" + String(w.height))
                if (dur > 0)
                    bits.push(qsTr("%1 ms").arg(dur))
                if (codec && codec.length)
                    bits.push(codec)
                bits.push(qsTr("audio muted"))
                root.setDiagnostics(bits.join(" · "))
            }
            onMediaStatusChanged: {
                if (mediaStatus === MediaPlayer.InvalidMedia)
                    root.setDiagnostics(qsTr("Unsupported or invalid media"))
            }
        }

        Label {
            anchors.centerIn: parent
            visible: !root.hasSource
            opacity: 0.65
            text: qsTr("Open a GIF or video for motion preview")
        }
    }
}
