use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("net.fritztech.wallspan")
            .qml_file("qml/main.qml")
            .qml_files([
                "qml/components/MonitorPreview.qml",
                "qml/components/PhotoCard.qml",
            ]),
    )
    .files(["src/app_controller.rs"])
    .build();
}
