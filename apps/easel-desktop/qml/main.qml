// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.platform as Labs

import net.fritztech.easel
import "components"

ApplicationWindow {
    id: window
    width: 1220
    height: 780
    minimumWidth: 940
    minimumHeight: 640
    visible: true
    title: qsTr("Easel")

    AppController {
        id: controller
    }

    ComposeController {
        id: compose
    }

    DiscoverController {
        id: discover

        onAcquired_file_urlChanged: {
            if (acquired_file_url && acquired_file_url.length > 0) {
                compose.setSourcePathFromUrl(acquired_file_url)
                pageStack.currentIndex = 0
            }
        }
    }

    LibraryController {
        id: library

        onSelected_file_urlChanged: {
            if (selected_file_url && selected_file_url.length > 0) {
                compose.setSourcePathFromUrl(selected_file_url)
                pageStack.currentIndex = 0
            }
        }
    }

    ProfileController {
        id: profiles
    }

    AutomationController {
        id: automation
    }

    Timer {
        interval: 1500
        running: pageStack.currentIndex === 2
        repeat: true
        onTriggered: library.pollWatch()
    }

    Timer {
        interval: 30000
        running: controller.smoke_out_dir.length === 0
        repeat: true
        onTriggered: automation.runTick()
    }

    Labs.SystemTrayIcon {
        id: tray
        visible: available && controller.smoke_out_dir.length === 0
        tooltip: qsTr("Easel")
        onActivated: window.show()

        menu: Labs.Menu {
            Labs.MenuItem {
                text: automation.paused ? qsTr("Resume rotation") : qsTr("Pause rotation")
                onTriggered: automation.setAutomationPaused(!automation.paused)
            }
            Labs.MenuItem {
                text: qsTr("Skip to next")
                onTriggered: automation.skipNext()
            }
            Labs.MenuItem {
                text: qsTr("Run schedule tick")
                onTriggered: automation.runTick()
            }
            Labs.MenuSeparator {}
            Labs.MenuItem {
                text: qsTr("Show Easel")
                onTriggered: {
                    window.show()
                    window.raise()
                    window.requestActivate()
                }
            }
            Labs.MenuItem {
                text: qsTr("Quit")
                onTriggered: Qt.quit()
            }
        }
    }

    function probeScreens() {
        controller.beginScreenProbe()
        var screens = Qt.application.screens
        for (var i = 0; i < screens.length; ++i) {
            var screen = screens[i]
            var physical = screen.physicalSize
            controller.addScreenProbe(
                        screen.name || "",
                        screen.manufacturer || "",
                        screen.model || "",
                        screen.serialNumber || "",
                        screen.virtualX,
                        screen.virtualY,
                        screen.width,
                        screen.height,
                        screen.devicePixelRatio,
                        physical ? physical.width : 0,
                        physical ? physical.height : 0)
        }
        controller.commitScreenProbe()
        compose.refreshPreview()
    }

    function runSmokeScreenshot() {
        console.log("smokeOutDir=", controller.smoke_out_dir, "image=", controller.smoke_image_path)
        if (!controller.smoke_out_dir || controller.smoke_out_dir.length === 0) {
            console.error("Smoke mode requested but smokeOutDir is empty")
            controller.forceSmokeExit(1)
            return
        }
        controller.useFixtureDisplays()
        var imagePath = controller.smoke_image_path
        if (imagePath && imagePath.length > 0) {
            compose.setSourcePathFromUrl(imagePath)
        } else {
            console.error("Smoke mode missing smokeImagePath")
            controller.forceSmokeExit(1)
            return
        }
        smokeTimer.start()
    }

    Timer {
        id: smokeTimer
        interval: 250
        repeat: true
        property int ticks: 0
        onTriggered: {
            ticks += 1
            var ready = compose.preview_ready
            if (ready || ticks > 80) {
                stop()
                if (!ready)
                    console.error("Smoke screenshot timed out waiting for preview; status=", compose.preview_status)
                var os = Qt.platform.os
                var path = controller.smoke_out_dir + "/gui-" + os + ".png"
                monitorPreview.grabToImage(function(result) {
                    var ok = result.saveToFile(path)
                    console.log("Smoke grabToImage save", path, "ok=", ok, "size=", result.width, "x", result.height)
                    controller.forceSmokeExit(ok ? 0 : 1)
                }, Qt.size(Math.max(2, monitorPreview.width), Math.max(2, monitorPreview.height)))
            }
        }
    }

    Component.onCompleted: {
        profiles.refresh()
        automation.refresh()
        if (controller.smoke_out_dir && controller.smoke_out_dir.length > 0) {
            runSmokeScreenshot()
        } else {
            probeScreens()
        }
    }

    FileDialog {
        id: imageDialog
        title: qsTr("Open local image")
        nameFilters: [qsTr("Images (*.png *.jpg *.jpeg *.webp *.bmp *.tif *.tiff *.gif)"), qsTr("All files (*)")]
        onAccepted: compose.setSourcePathFromUrl(selectedFile)
    }

    FolderDialog {
        id: folderDialog
        title: qsTr("Add library folder")
        onAccepted: library.addFolderFromUrl(selectedFolder)
    }

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 16
            anchors.rightMargin: 12
            spacing: 12

            Label {
                text: qsTr("Easel")
                font.pixelSize: 22
                font.weight: Font.DemiBold
            }

            Label {
                text: {
                    if (pageStack.currentIndex === 0)
                        return compose.preview_status
                    if (pageStack.currentIndex === 1)
                        return discover.status_text
                    if (pageStack.currentIndex === 2)
                        return library.status_text
                    if (pageStack.currentIndex === 3)
                        return profiles.status_text
                    if (pageStack.currentIndex === 4)
                        return automation.status_text
                    return controller.status_text
                }
                opacity: 0.65
                Layout.fillWidth: true
                elide: Text.ElideRight
            }

            ToolButton {
                text: automation.paused ? qsTr("Resume") : qsTr("Pause")
                Accessible.name: text
                onClicked: automation.setAutomationPaused(!automation.paused)
            }

            ToolButton {
                text: qsTr("Skip")
                Accessible.name: text
                onClicked: automation.skipNext()
            }

            ToolButton {
                text: qsTr("Refresh displays")
                Accessible.name: text
                onClicked: probeScreens()
            }

            ToolButton {
                text: qsTr("Settings")
                Accessible.name: text
            }
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Pane {
            Layout.fillHeight: true
            Layout.preferredWidth: 210
            padding: 12

            ButtonGroup { id: navigationGroup }

            ColumnLayout {
                anchors.fill: parent
                spacing: 6

                Label {
                    text: qsTr("WORKSPACE")
                    opacity: 0.55
                    font.pixelSize: 11
                    leftPadding: 10
                    bottomPadding: 4
                }

                Repeater {
                    model: [qsTr("Compose"), qsTr("Discover"), qsTr("Library"), qsTr("Profiles"), qsTr("Automation")]

                    delegate: ToolButton {
                        required property string modelData
                        required property int index
                        text: modelData
                        checkable: true
                        checked: index === 0
                        autoExclusive: true
                        ButtonGroup.group: navigationGroup
                        Layout.fillWidth: true
                        display: AbstractButton.TextOnly
                        onClicked: pageStack.currentIndex = index
                    }
                }

                Item { Layout.fillHeight: true }

                Label {
                    text: qsTr("%1 displays detected").arg(controller.display_count)
                    opacity: 0.65
                    leftPadding: 10
                }
            }
        }

        Rectangle {
            Layout.fillHeight: true
            width: 1
            color: window.palette.mid
            opacity: 0.35
        }

        StackLayout {
            id: pageStack
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: 0

            ScrollView {
                contentWidth: availableWidth

                ColumnLayout {
                    width: parent.width
                    spacing: 18

                    Item { Layout.preferredHeight: 8 }

                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: 24
                        Layout.rightMargin: 24

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 4
                            Label {
                                text: qsTr("Compose wallpaper")
                                font.pixelSize: 28
                                font.weight: Font.DemiBold
                            }
                            Label {
                                text: qsTr("Preview the physical continuity across your displays.")
                                opacity: 0.68
                            }
                        }

                        Button {
                            text: qsTr("Open image")
                            Accessible.name: text
                            onClicked: imageDialog.open()
                        }

                        ComboBox {
                            model: [qsTr("Desk — all displays"), qsTr("Center display"), qsTr("Side displays")]
                            Layout.preferredWidth: 220
                        }
                    }

                    MonitorPreview {
                        id: monitorPreview
                        Layout.fillWidth: true
                        // Fixed height keeps smoke grab aspect stable across CI window chrome.
                        Layout.preferredHeight: 330
                        Layout.preferredWidth: 900
                        Layout.maximumWidth: 900
                        Layout.alignment: Qt.AlignHCenter
                        Layout.leftMargin: 24
                        Layout.rightMargin: 24
                        previewUrls: compose.display_previews
                        previewReady: compose.preview_ready
                        layoutModel: controller.layout_model
                        selectedDisplayId: controller.selected_display_id
                        physicalPreview: controller.physical_preview
                        onDisplaySelected: (displayId) => controller.selectDisplay(displayId)
                        onDisplayMoved: (displayId, originXmm, originYmm) => {
                            controller.selectDisplay(displayId)
                            controller.moveSelectedDisplay(originXmm, originYmm)
                            compose.refreshPreview()
                        }
                    }

                    GroupBox {
                        title: qsTr("Physical calibration")
                        Layout.fillWidth: true
                        Layout.leftMargin: 24
                        Layout.rightMargin: 24
                        enabled: controller.selected_display_id.length > 0

                        GridLayout {
                            columns: 6
                            width: parent.width
                            columnSpacing: 10
                            rowSpacing: 10

                            Label { text: qsTr("Origin X mm") }
                            SpinBox {
                                id: originXSpin
                                from: -10000
                                to: 10000
                                value: Math.round(controller.selected_origin_x_mm)
                                editable: true
                                Layout.fillWidth: true
                                onValueModified: {
                                    controller.moveSelectedDisplay(value, controller.selected_origin_y_mm)
                                    compose.refreshPreview()
                                }
                            }
                            Label { text: qsTr("Origin Y mm") }
                            SpinBox {
                                id: originYSpin
                                from: -10000
                                to: 10000
                                value: Math.round(controller.selected_origin_y_mm)
                                editable: true
                                Layout.fillWidth: true
                                onValueModified: {
                                    controller.moveSelectedDisplay(controller.selected_origin_x_mm, value)
                                    compose.refreshPreview()
                                }
                            }
                            Label { text: qsTr("Bezel mm") }
                            SpinBox {
                                id: bezelSpin
                                from: 0
                                to: 100
                                value: Math.round(controller.selected_bezel_mm)
                                editable: true
                                Layout.fillWidth: true
                                onValueModified: {
                                    controller.applySelectedBezel(value)
                                    compose.refreshPreview()
                                }
                            }

                            Label { text: qsTr("Width mm") }
                            SpinBox {
                                id: widthSpin
                                from: 50
                                to: 5000
                                value: Math.round(controller.selected_width_mm)
                                editable: true
                                Layout.fillWidth: true
                                onValueModified: {
                                    controller.applySelectedSize(value, controller.selected_height_mm)
                                    compose.refreshPreview()
                                }
                            }
                            Label { text: qsTr("Height mm") }
                            SpinBox {
                                id: heightSpin
                                from: 50
                                to: 5000
                                value: Math.round(controller.selected_height_mm)
                                editable: true
                                Layout.fillWidth: true
                                onValueModified: {
                                    controller.applySelectedSize(controller.selected_width_mm, value)
                                    compose.refreshPreview()
                                }
                            }
                            Label { text: qsTr("Preview") }
                            ComboBox {
                                model: [qsTr("Physical"), qsTr("Digital (before)")]
                                currentIndex: controller.physical_preview ? 0 : 1
                                Layout.fillWidth: true
                                onActivated: {
                                    controller.setPhysicalPreviewEnabled(currentIndex === 0)
                                }
                            }
                        }
                    }

                    GroupBox {
                        title: qsTr("Composition")
                        Layout.fillWidth: true
                        Layout.leftMargin: 24
                        Layout.rightMargin: 24

                        GridLayout {
                            columns: 4
                            width: parent.width
                            columnSpacing: 12
                            rowSpacing: 12

                            Label { text: qsTr("Media") }
                            ComboBox {
                                id: mediaMode
                                model: [qsTr("Still image"), qsTr("Dynamic stills"), qsTr("Animated / video")]
                            }
                            Label { text: qsTr("Motion") }
                            ComboBox {
                                model: [qsTr("Loop at 30 fps"), qsTr("Play once"), qsTr("Poster frame only")]
                                enabled: mediaMode.currentIndex === 2
                                Layout.fillWidth: true
                            }

                            Label { text: qsTr("Fit") }
                            ComboBox {
                                id: fitMode
                                model: [qsTr("Cover"), qsTr("Contain"), qsTr("Stretch"), qsTr("Native")]
                                currentIndex: compose.fit_mode_index
                                onActivated: {
                                    compose.fit_mode_index = currentIndex
                                    compose.refreshPreview()
                                }
                            }
                            Label { text: qsTr("Zoom") }
                            Slider {
                                id: zoomSlider
                                from: 1.0
                                to: 3.0
                                value: compose.zoom
                                Layout.fillWidth: true
                                onMoved: {
                                    compose.zoom = value
                                    compose.refreshPreview()
                                }
                            }

                            Label { text: qsTr("Focal X") }
                            Slider {
                                id: focalXSlider
                                from: 0.0
                                to: 1.0
                                value: compose.focal_x
                                Layout.fillWidth: true
                                onMoved: {
                                    compose.focal_x = value
                                    compose.refreshPreview()
                                }
                            }
                            Label { text: qsTr("Focal Y") }
                            Slider {
                                id: focalYSlider
                                from: 0.0
                                to: 1.0
                                value: compose.focal_y
                                Layout.fillWidth: true
                                onMoved: {
                                    compose.focal_y = value
                                    compose.refreshPreview()
                                }
                            }

                            Label { text: qsTr("Correction") }
                            ComboBox {
                                id: correctionMode
                                model: [qsTr("Physical + bezel"), qsTr("Digital only")]
                                currentIndex: compose.layout_mode_index
                                onActivated: {
                                    compose.layout_mode_index = currentIndex
                                    controller.setPhysicalPreviewEnabled(currentIndex === 0)
                                    compose.refreshPreview()
                                }
                            }
                            Label { text: qsTr("Schedule") }
                            ComboBox { model: [qsTr("Manual"), qsTr("Every hour"), qsTr("Time of day")] }
                        }
                    }

                    RowLayout {
                        Layout.alignment: Qt.AlignRight
                        Layout.rightMargin: 24
                        Layout.bottomMargin: 20
                        Button {
                            text: qsTr("Save profile")
                            onClicked: {
                                profiles.saveFromCompose(
                                            qsTr("Compose profile"),
                                            compose.source_path,
                                            compose.fit_mode_index,
                                            compose.layout_mode_index,
                                            compose.zoom,
                                            compose.focal_x,
                                            compose.focal_y)
                                if (compose.source_path && compose.source_path.length > 0)
                                    automation.enqueuePath(compose.source_path)
                            }
                        }
                        Button {
                            text: qsTr("Apply")
                            highlighted: true
                            enabled: compose.preview_ready && !compose.apply_busy
                            onClicked: compose.applyWallpaper()
                        }
                    }
                }
            }

            ScrollView {
                contentWidth: availableWidth

                ColumnLayout {
                    x: 24
                    y: 20
                    width: parent.width - 48
                    spacing: 18

                    Label {
                        text: qsTr("Discover")
                        font.pixelSize: 28
                        font.weight: Font.DemiBold
                    }

                    Label {
                        text: qsTr("Openverse results keep creator, license, and source links. Metadata accuracy is not guaranteed — open the work page before applying.")
                        wrapMode: Text.WordWrap
                        opacity: 0.7
                        Layout.fillWidth: true
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        TextField {
                            id: discoverQuery
                            placeholderText: qsTr("Search openly licensed images")
                            Layout.fillWidth: true
                            onAccepted: {
                                discover.query_text = text
                                discover.search()
                            }
                        }
                        ComboBox {
                            id: licenseFilter
                            model: [qsTr("All licenses"), qsTr("Public domain"), qsTr("Commercial use")]
                            currentIndex: discover.license_filter_index
                            onActivated: discover.license_filter_index = currentIndex
                        }
                        Button {
                            text: discover.busy ? qsTr("Searching…") : qsTr("Search")
                            highlighted: true
                            enabled: !discover.busy
                            onClicked: {
                                discover.query_text = discoverQuery.text
                                discover.search()
                            }
                        }
                    }

                    GridLayout {
                        columns: width > 850 ? 3 : 2
                        Layout.fillWidth: true
                        columnSpacing: 14
                        rowSpacing: 14

                        Repeater {
                            model: discover.result_model
                            PhotoCard {
                                required property int index
                                required property string modelData
                                readonly property var payload: JSON.parse(modelData)
                                Layout.fillWidth: true
                                title: payload.title
                                creator: payload.creator
                                subtitle: payload.license + " · " + payload.width + "×" + payload.height
                                            + (payload.meetsMinimum ? "" : qsTr(" · may upscale"))
                                imageSource: payload.preview
                                meetsMinimum: payload.meetsMinimum
                                accent: ["#776B5D", "#40566A", "#3B3553", "#48604F", "#9A6D4A", "#3F5260"][index % 6]
                                onActivated: discover.useResult(index)
                                onFavoriteRequested: discover.favoriteResult(index)
                            }
                        }
                    }

                    Button {
                        visible: discover.has_more
                        text: qsTr("Load more")
                        enabled: !discover.busy
                        Layout.alignment: Qt.AlignHCenter
                        onClicked: discover.loadMore()
                    }
                }
            }

            ScrollView {
                contentWidth: availableWidth

                ColumnLayout {
                    x: 24
                    y: 20
                    width: parent.width - 48
                    spacing: 18

                    Label {
                        text: qsTr("Library")
                        font.pixelSize: 28
                        font.weight: Font.DemiBold
                    }

                    Label {
                        text: library.status_text
                        wrapMode: Text.WordWrap
                        opacity: 0.7
                        Layout.fillWidth: true
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        Button {
                            text: qsTr("Add folder")
                            highlighted: true
                            onClicked: folderDialog.open()
                        }
                        Button {
                            text: qsTr("Rescan")
                            onClicked: library.rescan()
                        }
                        Button {
                            text: qsTr("Refresh")
                            onClicked: library.refresh()
                        }
                    }

                    Label {
                        text: qsTr("Indexed folders")
                        font.weight: Font.DemiBold
                    }

                    Repeater {
                        model: library.folder_model
                        Label {
                            required property string modelData
                            text: "• " + modelData
                            opacity: 0.8
                        }
                    }

                    Label {
                        text: qsTr("Favorites")
                        font.weight: Font.DemiBold
                        visible: library.favorite_model.length > 0
                    }

                    GridLayout {
                        columns: width > 850 ? 3 : 2
                        Layout.fillWidth: true
                        columnSpacing: 14
                        rowSpacing: 14
                        visible: library.favorite_model.length > 0

                        Repeater {
                            model: library.favorite_model
                            PhotoCard {
                                required property int index
                                required property string modelData
                                readonly property var payload: JSON.parse(modelData)
                                Layout.fillWidth: true
                                title: payload.title
                                creator: payload.creator
                                subtitle: payload.license
                                imageSource: payload.preview
                                meetsMinimum: payload.meetsMinimum
                                accent: "#40566A"
                            }
                        }
                    }

                    Label {
                        text: qsTr("Recent indexed media")
                        font.weight: Font.DemiBold
                    }

                    GridLayout {
                        columns: width > 850 ? 3 : 2
                        Layout.fillWidth: true
                        columnSpacing: 14
                        rowSpacing: 14

                        Repeater {
                            model: library.asset_model
                            PhotoCard {
                                required property int index
                                required property string modelData
                                readonly property var payload: JSON.parse(modelData)
                                Layout.fillWidth: true
                                title: payload.title
                                creator: payload.creator
                                subtitle: payload.source + " · score " + payload.score
                                imageSource: payload.preview
                                meetsMinimum: payload.meetsMinimum
                                accent: ["#776B5D", "#48604F", "#9A6D4A", "#3F5260"][index % 4]
                                onActivated: library.useAsset(index)
                            }
                        }
                    }
                }
            }
            ScrollView {
                contentWidth: availableWidth

                ColumnLayout {
                    x: 24
                    y: 20
                    width: parent.width - 48
                    spacing: 18

                    Label {
                        text: qsTr("Profiles")
                        font.pixelSize: 28
                        font.weight: Font.DemiBold
                    }

                    Label {
                        text: qsTr("Reusable compositions and display-group assignments. Save from Compose, then activate here for automation.")
                        wrapMode: Text.WordWrap
                        opacity: 0.7
                        Layout.fillWidth: true
                    }

                    RowLayout {
                        Button {
                            text: qsTr("Refresh")
                            onClicked: profiles.refresh()
                        }
                        Button {
                            text: qsTr("Ensure default group")
                            onClicked: profiles.ensureDefaultGroup()
                        }
                        Label {
                            text: profiles.status_text
                            opacity: 0.7
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                        }
                    }

                    Label {
                        text: qsTr("Saved profiles")
                        font.weight: Font.DemiBold
                    }

                    Repeater {
                        model: profiles.profile_model
                        delegate: RowLayout {
                            required property int index
                            required property string modelData
                            readonly property var payload: JSON.parse(modelData)
                            Layout.fillWidth: true
                            spacing: 12

                            Label {
                                text: (payload.active ? "● " : "○ ") + payload.name
                                Layout.fillWidth: true
                            }
                            Label {
                                text: qsTr("%1 displays · %2 · %3").arg(payload.displays).arg(payload.fit).arg(payload.layout)
                                opacity: 0.65
                            }
                            Button {
                                text: qsTr("Activate")
                                onClicked: profiles.activateProfile(index)
                            }
                        }
                    }

                    Label {
                        visible: profiles.profile_model.length === 0
                        text: qsTr("No profiles yet. Use Save profile on Compose.")
                        opacity: 0.65
                    }

                    Label {
                        text: qsTr("Display groups")
                        font.weight: Font.DemiBold
                    }

                    Repeater {
                        model: profiles.group_model
                        delegate: Label {
                            required property string modelData
                            readonly property var payload: JSON.parse(modelData)
                            text: qsTr("%1 — %2 display(s)").arg(payload.name).arg(payload.displays)
                            opacity: 0.8
                        }
                    }
                }
            }

            ScrollView {
                contentWidth: availableWidth

                ColumnLayout {
                    x: 24
                    y: 20
                    width: parent.width - 48
                    spacing: 18

                    Label {
                        text: qsTr("Automation")
                        font.pixelSize: 28
                        font.weight: Font.DemiBold
                    }

                    Label {
                        text: qsTr("Interval schedules, rotation queues, pause/skip, and hotplug policy. CLI: easel-desktop --cli status|pause|resume|skip|tick")
                        wrapMode: Text.WordWrap
                        opacity: 0.7
                        Layout.fillWidth: true
                    }

                    RowLayout {
                        Button {
                            text: automation.paused ? qsTr("Resume") : qsTr("Pause")
                            onClicked: automation.setAutomationPaused(!automation.paused)
                        }
                        Button {
                            text: qsTr("Skip")
                            highlighted: true
                            onClicked: automation.skipNext()
                        }
                        Button {
                            text: qsTr("Run tick")
                            onClicked: automation.runTick()
                        }
                        Button {
                            text: qsTr("Queue from favorites")
                            onClicked: automation.buildQueueFromFavorites()
                        }
                    }

                    GroupBox {
                        title: qsTr("Interval schedule")
                        Layout.fillWidth: true

                        RowLayout {
                            width: parent.width
                            Label { text: qsTr("Seconds") }
                            SpinBox {
                                id: intervalSpin
                                from: 60
                                to: 86400
                                stepSize: 60
                                value: automation.interval_seconds
                                editable: true
                                Layout.preferredWidth: 160
                            }
                            Button {
                                text: qsTr("Save interval")
                                onClicked: automation.saveInterval(intervalSpin.value)
                            }
                        }
                    }

                    GroupBox {
                        title: qsTr("Missing display policy")
                        Layout.fillWidth: true

                        ComboBox {
                            model: [
                                qsTr("Skip missing outputs"),
                                qsTr("Pause until restored"),
                                qsTr("Require any expected display")
                            ]
                            currentIndex: automation.policy_index
                            onActivated: automation.applyPolicyIndex(currentIndex)
                            Layout.fillWidth: true
                            width: parent.width
                        }
                    }

                    Label {
                        text: qsTr("Last decision")
                        font.weight: Font.DemiBold
                    }
                    Label {
                        text: automation.last_decision.length > 0 ? automation.last_decision : qsTr("No automation decisions yet.")
                        wrapMode: Text.WordWrap
                        opacity: 0.75
                        Layout.fillWidth: true
                    }

                    Label {
                        text: qsTr("Schedules")
                        font.weight: Font.DemiBold
                    }
                    Repeater {
                        model: automation.schedule_model
                        delegate: Label {
                            required property string modelData
                            readonly property var payload: JSON.parse(modelData)
                            text: (payload.active ? "● " : "○ ") + payload.name + " — " + payload.kind
                            opacity: 0.8
                        }
                    }

                    Label {
                        text: qsTr("Rotation queues")
                        font.weight: Font.DemiBold
                    }
                    Repeater {
                        model: automation.queue_model
                        delegate: Label {
                            required property string modelData
                            readonly property var payload: JSON.parse(modelData)
                            text: (payload.active ? "● " : "○ ") + payload.name + qsTr(" — %1 assets (avoid %2)").arg(payload.assets).arg(payload.avoidRepeat)
                            opacity: 0.8
                        }
                    }
                }
            }
        }
    }
}
