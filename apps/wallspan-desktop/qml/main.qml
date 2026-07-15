import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window

import net.fritztech.wallspan
import "components"

ApplicationWindow {
    id: window
    width: 1220
    height: 780
    minimumWidth: 940
    minimumHeight: 640
    visible: true
    title: qsTr("Wallspan")

    AppController {
        id: controller
    }

    ComposeController {
        id: compose
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
            Qt.quit()
            return
        }
        controller.useFixtureDisplays()
        var imagePath = controller.smoke_image_path
        if (imagePath && imagePath.length > 0) {
            compose.setSourcePathFromUrl(imagePath)
        } else {
            console.error("Smoke mode missing smokeImagePath")
            Qt.quit()
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
                    Qt.quit()
                }, Qt.size(Math.max(2, monitorPreview.width), Math.max(2, monitorPreview.height)))
            }
        }
    }

    Component.onCompleted: {
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

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 16
            anchors.rightMargin: 12
            spacing: 12

            Label {
                text: qsTr("Wallspan")
                font.pixelSize: 22
                font.weight: Font.DemiBold
            }

            Label {
                text: pageStack.currentIndex === 0 ? compose.preview_status : controller.status_text
                opacity: 0.65
                Layout.fillWidth: true
                elide: Text.ElideRight
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
                        Layout.preferredHeight: 330
                        Layout.leftMargin: 24
                        Layout.rightMargin: 24
                        previewUrls: compose.display_previews
                        previewReady: compose.preview_ready
                        layoutModel: controller.layout_model
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
                            ComboBox { model: [qsTr("Physical + bezel"), qsTr("Digital only")] }
                            Label { text: qsTr("Schedule") }
                            ComboBox { model: [qsTr("Manual"), qsTr("Every hour"), qsTr("Time of day")] }
                        }
                    }

                    RowLayout {
                        Layout.alignment: Qt.AlignRight
                        Layout.rightMargin: 24
                        Layout.bottomMargin: 20
                        Button { text: qsTr("Save profile") }
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

                    RowLayout {
                        Layout.fillWidth: true
                        TextField {
                            placeholderText: qsTr("Search openly licensed images")
                            Layout.fillWidth: true
                        }
                        ComboBox { model: [qsTr("All licenses"), qsTr("Public domain"), qsTr("Commercial use")] }
                        Button { text: qsTr("Search"); highlighted: true }
                    }

                    Label {
                        visible: !controller.online_sources_available
                        text: qsTr("Online providers are represented in the architecture but are not connected in this scaffold.")
                        wrapMode: Text.WordWrap
                        opacity: 0.7
                        Layout.fillWidth: true
                    }

                    GridLayout {
                        columns: width > 850 ? 3 : 2
                        Layout.fillWidth: true
                        columnSpacing: 14
                        rowSpacing: 14

                        Repeater {
                            model: 6
                            PhotoCard {
                                required property int index
                                Layout.fillWidth: true
                                title: ["Mountain light", "Coastal dusk", "Deep space", "Forest mist", "Desert lines", "City rain"][index]
                                creator: qsTr("Provider preview placeholder")
                                accent: ["#776B5D", "#40566A", "#3B3553", "#48604F", "#9A6D4A", "#3F5260"][index]
                            }
                        }
                    }
                }
            }

            PlaceholderPage { title: qsTr("Library"); description: qsTr("Indexed local media and approved remote still images.") }
            PlaceholderPage { title: qsTr("Profiles"); description: qsTr("Reusable compositions and display groups.") }
            PlaceholderPage { title: qsTr("Automation"); description: qsTr("Intervals, schedules, sunrise, and time-of-day rules.") }
        }
    }

    component PlaceholderPage: Pane {
        id: placeholderPage
        required property string title
        required property string description
        ColumnLayout {
            anchors.centerIn: parent
            spacing: 8
            Label { text: placeholderPage.title; font.pixelSize: 28; font.weight: Font.DemiBold }
            Label { text: placeholderPage.description; opacity: 0.68 }
        }
    }
}
