// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import QtQuick
import QtMultimedia

// Headless Qt Multimedia probe for library video indexing (Stage 6.5).
// Processes library.video_probe_queue one path at a time, grabs a poster frame,
// and completes via library.completeVideoProbe / skipVideoProbe.
Item {
    id: root
    width: 64
    height: 64
    visible: false

    property var libraryController: null
    property bool busy: false

    function pathFromUrl(urlString) {
        const raw = String(urlString || "")
        if (raw.indexOf("file:") === 0)
            return decodeURIComponent(raw.replace(/^file:\/\//, "").replace(/^\/([A-Za-z]:)/, "$1"))
        return raw
    }

    function maybeStart() {
        if (busy || !libraryController)
            return
        const queue = libraryController.video_probe_queue
        if (!queue || queue.length === 0)
            return
        const source = String(queue[0])
        if (!source.length)
            return
        busy = true
        grabImage.source = ""
        player.stop()
        player.source = source.indexOf("file:") === 0 ? source : ("file://" + source)
        player.play()
    }

    Connections {
        target: root.libraryController
        function onVideo_probe_queueChanged() {
            root.maybeStart()
        }
    }

    Timer {
        id: settleTimer
        interval: 350
        repeat: false
        onTriggered: root.capturePoster()
    }

    function fail(path, message) {
        if (libraryController)
            libraryController.skipVideoProbe(path, message)
        busy = false
        Qt.callLater(root.maybeStart)
    }

    function capturePoster() {
        const path = root.pathFromUrl(player.source)
        if (!videoOutput.videoSink || !videoOutput.videoSink.videoSize
                || videoOutput.videoSink.videoSize.width <= 0) {
            // Metadata-only success without a frame still indexes the file.
            finishWithoutPoster(path)
            return
        }
        videoOutput.grabToImage(function (result) {
            if (!result) {
                finishWithoutPoster(path)
                return
            }
            const dest = libraryController.videoProbeTempPath(path)
            if (!result.saveToFile(dest)) {
                root.fail(path, qsTr("Could not write poster grab"))
                return
            }
            const size = videoOutput.videoSink.videoSize
            libraryController.completeVideoProbe(
                        path,
                        root.probePayloadJson(size.width, size.height),
                        dest)
            player.stop()
            busy = false
            Qt.callLater(root.maybeStart)
        })
    }

    function probePayloadJson(width, height) {
        return JSON.stringify({
            width: width,
            height: height,
            durationMs: player.duration > 0 ? player.duration : 0,
            container: player.metaData.stringValue(MediaMetaData.FileFormat) || "",
            videoCodec: player.metaData.stringValue(MediaMetaData.VideoCodec) || "",
            hasAudio: !!player.metaData.value(MediaMetaData.AudioBitRate)
                    || !!player.metaData.value(MediaMetaData.AudioCodec)
        })
    }

    function finishWithoutPoster(path) {
        const res = player.metaData.value(MediaMetaData.Resolution)
        const width = res ? res.width : 0
        const height = res ? res.height : 0
        if (width <= 0 || height <= 0) {
            root.fail(path, qsTr("No video resolution from decoder"))
            return
        }
        libraryController.completeVideoProbe(path, root.probePayloadJson(width, height), "")
        player.stop()
        busy = false
        Qt.callLater(root.maybeStart)
    }

    VideoOutput {
        id: videoOutput
        anchors.fill: parent
    }

    MediaPlayer {
        id: player
        videoOutput: videoOutput
        audioOutput: AudioOutput {
            muted: true
            volume: 0
        }
        onErrorOccurred: function (error, errorString) {
            root.fail(root.pathFromUrl(source), errorString || String(error))
        }
        onMediaStatusChanged: {
            if (mediaStatus === MediaPlayer.LoadedMedia
                    || mediaStatus === MediaPlayer.BufferedMedia) {
                player.pause()
                settleTimer.restart()
            } else if (mediaStatus === MediaPlayer.InvalidMedia) {
                root.fail(root.pathFromUrl(source), qsTr("Invalid media"))
            }
        }
    }

    // Keep a dummy Image id reserved for future poster decode checks.
    Image {
        id: grabImage
        visible: false
    }

    Component.onCompleted: maybeStart()
}
