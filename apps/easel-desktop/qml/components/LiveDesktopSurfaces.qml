// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import QtQuick
import QtQuick.Window
import Qt.labs.platform as Platform
import QtMultimedia

// App-owned live wallpaper surfaces (ADR 0014). Rust publishes desktop-live/active.json;
// this Instantiator opens one always-on-bottom window per screen and plays muted GIF/video
// crops. Plasma + plugin remains the supported host; these windows are experimental and
// stop when Easel exits.

Item {
    id: root

    property string lastStatePayload: ""
    property var liveDoc: null

    readonly property bool liveActive: {
        return root.liveDoc
                && root.liveDoc.mode === "live"
                && root.liveDoc.live
    }

    readonly property string stateFilePath: {
        return Platform.StandardPaths.writableLocation(Platform.StandardPaths.GenericDataLocation)
            + "/easel/desktop-live/active.json"
    }

    function fileUrlForPath(path) {
        if (!path || path.length === 0)
            return ""
        if (path.indexOf("file:") === 0)
            return path
        return "file://" + path
    }

    function sourceRectFromUv(uv) {
        if (!uv)
            return Qt.rect(0, 0, 1, 1)
        return Qt.rect(uv.x || 0, uv.y || 0, uv.width || 1, uv.height || 1)
    }

    function pickLiveCrop(doc, screen) {
        if (!doc || !doc.live || !doc.live.displays || !doc.live.displays.length)
            return null
        const x = screen.virtualX
        const y = screen.virtualY
        const w = screen.width
        const h = screen.height
        for (let i = 0; i < doc.live.displays.length; ++i) {
            const entry = doc.live.displays[i]
            const g = entry.geometry
            if (!g)
                continue
            if (g.x === x && g.y === y && g.width === w && g.height === h)
                return entry
        }
        if (doc.live.displays.length === 1)
            return doc.live.displays[0]
        return null
    }

    function pickPoster(doc, screen) {
        if (!doc || !doc.displays || !doc.displays.length)
            return ""
        const x = screen.virtualX
        const y = screen.virtualY
        const w = screen.width
        const h = screen.height
        for (let i = 0; i < doc.displays.length; ++i) {
            const entry = doc.displays[i]
            const g = entry.geometry
            if (!g)
                continue
            if (g.x === x && g.y === y && g.width === w && g.height === h)
                return entry.image || ""
        }
        if (doc.displays.length === 1)
            return doc.displays[0].image || ""
        return ""
    }

    function reloadStateFile() {
        const path = root.stateFilePath
        if (!path || path.length === 0)
            return
        const request = new XMLHttpRequest()
        request.onreadystatechange = function () {
            if (request.readyState !== XMLHttpRequest.DONE)
                return
            if (request.status !== 200 && request.status !== 0) {
                root.liveDoc = null
                return
            }
            const payload = request.responseText
            if (!payload) {
                root.liveDoc = null
                return
            }
            if (payload === root.lastStatePayload)
                return
            root.lastStatePayload = payload
            try {
                root.liveDoc = JSON.parse(payload)
            } catch (e) {
                root.liveDoc = null
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

    Instantiator {
        model: Qt.application.screens
        delegate: Window {
            id: surfaceWindow

            required property var modelData

            property var liveCrop: root.pickLiveCrop(root.liveDoc, modelData)
            property string posterUrl: root.fileUrlForPath(root.pickPoster(root.liveDoc, modelData))
            property bool surfaceLive: root.liveActive && liveCrop !== null

            readonly property bool isGif: {
                if (!surfaceLive || !root.liveDoc || !root.liveDoc.live)
                    return false
                const kind = root.liveDoc.live.media_kind || ""
                if (kind === "animated_image")
                    return true
                const src = String(root.liveDoc.live.source || "").toLowerCase()
                return src.endsWith(".gif") || src.endsWith(".webp")
            }

            // Desktop / bottom stacking: Qt.Desktop on X11; otherwise stay-on-bottom
            // tool window (experimental — DE icon layering varies).
            flags: (Qt.platform.pluginName === "xcb" ? Qt.Desktop : Qt.Window)
                    | Qt.FramelessWindowHint | Qt.WindowStaysOnBottomHint | Qt.Tool
            color: "black"
            title: "Easel Live Wallpaper"
            x: modelData.virtualX
            y: modelData.virtualY
            width: modelData.width
            height: modelData.height
            visible: surfaceLive
            // Keep out of task switchers where the platform honors this.
            visibility: surfaceLive ? Window.Windowed : Window.Hidden

            property real lastSeekMediaMs: -1

            function applyLivePlayback() {
                if (!surfaceLive || isGif) {
                    player.stop()
                    player.source = ""
                    return
                }
                const live = root.liveDoc.live
                const source = root.fileUrlForPath(live.source || "")
                if (!source || source.length === 0)
                    return
                if (String(player.source) !== String(source)) {
                    player.source = source
                    lastSeekMediaMs = -1
                }
                player.playbackRate = live.rate > 0 ? live.rate : 1.0
                player.loops = (live.loop_mode === "once") ? 1 : MediaPlayer.Infinite
                const targetMs = live.media_time_ms || 0
                if (live.paused) {
                    if (player.playbackState === MediaPlayer.PlayingState)
                        player.pause()
                    if (Math.abs(player.position - targetMs) > 120) {
                        player.position = targetMs
                        lastSeekMediaMs = targetMs
                    }
                } else {
                    if (Math.abs(targetMs - lastSeekMediaMs) > 500
                            || Math.abs(player.position - targetMs) > 250) {
                        player.position = targetMs
                        lastSeekMediaMs = targetMs
                    }
                    if (player.playbackState !== MediaPlayer.PlayingState)
                        player.play()
                }
            }

            onSurfaceLiveChanged: applyLivePlayback()
            onIsGifChanged: applyLivePlayback()

            Connections {
                target: root
                function onLiveDocChanged() {
                    surfaceWindow.liveCrop = root.pickLiveCrop(root.liveDoc, surfaceWindow.modelData)
                    surfaceWindow.posterUrl = root.fileUrlForPath(
                                root.pickPoster(root.liveDoc, surfaceWindow.modelData))
                    surfaceWindow.applyLivePlayback()
                }
            }

            Timer {
                interval: 33
                running: surfaceWindow.surfaceLive && !surfaceWindow.isGif
                repeat: true
                onTriggered: surfaceWindow.applyLivePlayback()
            }

            Rectangle {
                anchors.fill: parent
                color: "#000000"

                readonly property bool showPosterFallback: !surfaceWindow.surfaceLive
                        || (surfaceWindow.isGif ? gifPlayer.status !== Image.Ready
                                               : player.playbackState !== MediaPlayer.PlayingState)

                Image {
                    anchors.fill: parent
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    cache: false
                    source: surfaceWindow.posterUrl
                    visible: parent.showPosterFallback
                    z: parent.showPosterFallback ? 2 : 0
                }

                Item {
                    id: gifCrop
                    anchors.fill: parent
                    clip: true
                    visible: surfaceWindow.surfaceLive && surfaceWindow.isGif
                    z: 1

                    readonly property var uv: surfaceWindow.liveCrop
                            ? surfaceWindow.liveCrop.source_uv : null
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
                        source: (surfaceWindow.surfaceLive && surfaceWindow.isGif)
                                ? root.fileUrlForPath(root.liveDoc.live.source) : ""
                        playing: surfaceWindow.surfaceLive && surfaceWindow.isGif
                                && root.liveDoc && root.liveDoc.live && !root.liveDoc.live.paused
                    }
                }

                VideoOutput {
                    id: liveVideo
                    anchors.fill: parent
                    fillMode: VideoOutput.Stretch
                    visible: surfaceWindow.surfaceLive && !surfaceWindow.isGif
                    sourceRect: root.sourceRectFromUv(
                                    surfaceWindow.liveCrop ? surfaceWindow.liveCrop.source_uv : null)
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
            }
        }
    }
}
