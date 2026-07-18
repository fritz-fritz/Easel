// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! CXX-Qt build wiring for the Easel QML module.

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("net.fritztech.easel")
            .qml_file("qml/main.qml")
            .qml_files([
                "qml/components/MonitorPreview.qml",
                "qml/components/PhotoCard.qml",
            ]),
    )
    .files([
        "src/app_controller.rs",
        "src/automation_controller.rs",
        "src/compose_controller.rs",
        "src/discover_controller.rs",
        "src/library_controller.rs",
        "src/profile_controller.rs",
    ])
    .build();
}
