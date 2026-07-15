use std::pin::Pin;

use cxx_qt_lib::QString;

#[cxx_qt::bridge]
mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status_text)]
        #[qproperty(i32, display_count)]
        #[qproperty(bool, online_sources_available)]
        type AppController = super::AppControllerRust;

        #[qinvokable]
        #[rust_name = "refresh_displays"]
        fn refreshDisplays(self: Pin<&mut Self>);
    }
}

/// Presentation state only; application services will replace scaffold values.
pub struct AppControllerRust {
    status_text: QString,
    display_count: i32,
    online_sources_available: bool,
}

impl Default for AppControllerRust {
    fn default() -> Self {
        Self {
            status_text: "Architecture scaffold".into(),
            display_count: 3,
            online_sources_available: false,
        }
    }
}

impl qobject::AppController {
    fn refresh_displays(mut self: Pin<&mut Self>) {
        self.as_mut()
            .set_status_text("Display enumeration is not implemented yet".into());
        self.as_mut().set_display_count(3);
    }
}
