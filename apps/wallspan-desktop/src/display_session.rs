//! Shared display session state, Qt probe ingestion, and arrangement persistence.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use wallspan_core::{
    Display, DisplayArrangement, DisplayEvidence, LogicalRect, Millimeters, NativePixelSize,
    ObservedDisplay, PhysicalSize, ScaleFactor, approximate_physical_origin, match_displays,
};

use crate::fixtures::dev_displays;

static SESSION: OnceLock<Mutex<DisplaySession>> = OnceLock::new();
static SMOKE: OnceLock<SmokePaths> = OnceLock::new();

/// Smoke screenshot output paths configured before the Qt event loop starts.
#[derive(Clone, Debug)]
pub struct SmokePaths {
    /// Directory that receives `gui-*.png`.
    pub out_dir: PathBuf,
    /// Local still image loaded into Compose for the screenshot.
    pub image_path: PathBuf,
}

/// Records smoke screenshot paths for the QML controllers.
pub fn configure_smoke(out_dir: PathBuf, image_path: PathBuf) {
    let _ = SMOKE.set(SmokePaths {
        out_dir,
        image_path,
    });
}

/// Returns configured smoke screenshot paths, if any.
#[must_use]
pub fn smoke_paths() -> Option<&'static SmokePaths> {
    SMOKE.get()
}

/// In-memory display arrangement used by Compose and App controllers.
#[derive(Clone, Debug)]
pub struct DisplaySession {
    /// Current matched arrangement.
    pub arrangement: DisplayArrangement,
    /// Whether values came from live Qt probing this session.
    pub from_probe: bool,
}

fn session() -> &'static Mutex<DisplaySession> {
    SESSION.get_or_init(|| {
        Mutex::new(DisplaySession {
            arrangement: load_or_default(),
            from_probe: false,
        })
    })
}

/// Returns the active arrangement, falling back to fixture displays when empty.
#[must_use]
pub fn current_displays() -> Vec<Display> {
    let guard = session().lock().expect("display session lock");
    if guard.arrangement.displays.is_empty() {
        dev_displays()
    } else {
        guard.arrangement.displays.clone()
    }
}

/// Returns preview-scaled copies of [`current_displays`].
#[must_use]
pub fn current_preview_displays() -> Vec<Display> {
    const SCALE: u32 = 8;
    current_displays()
        .into_iter()
        .map(|mut display| {
            display.native_pixels.width = (display.native_pixels.width / SCALE).max(1);
            display.native_pixels.height = (display.native_pixels.height / SCALE).max(1);
            display.logical_rect.width = (display.logical_rect.width / SCALE).max(1);
            display.logical_rect.height = (display.logical_rect.height / SCALE).max(1);
            display.logical_rect.x /= i32::try_from(SCALE).unwrap_or(1);
            display.logical_rect.y /= i32::try_from(SCALE).unwrap_or(1);
            display
        })
        .collect()
}

/// Normalized layout rows for MonitorPreview: `xFactor|yFactor|wFactor|hFactor|label`.
#[must_use]
pub fn layout_preview_model() -> Vec<String> {
    let displays = current_displays();
    if displays.is_empty() {
        return Vec::new();
    }

    let min_x = displays
        .iter()
        .map(|display| display.logical_rect.x)
        .min()
        .unwrap_or(0);
    let min_y = displays
        .iter()
        .map(|display| display.logical_rect.y)
        .min()
        .unwrap_or(0);
    let max_x = displays
        .iter()
        .map(|display| {
            display.logical_rect.x + i32::try_from(display.logical_rect.width).unwrap_or(0)
        })
        .max()
        .unwrap_or(1);
    let max_y = displays
        .iter()
        .map(|display| {
            display.logical_rect.y + i32::try_from(display.logical_rect.height).unwrap_or(0)
        })
        .max()
        .unwrap_or(1);
    let span_x = f64::from((max_x - min_x).max(1));
    let span_y = f64::from((max_y - min_y).max(1));
    let margin = 0.04;
    let usable_w = 1.0 - margin * 2.0;
    let usable_h = 1.0 - margin * 2.0;

    displays
        .iter()
        .map(|display| {
            let x = margin + usable_w * f64::from(display.logical_rect.x - min_x) / span_x;
            let y = margin + usable_h * f64::from(display.logical_rect.y - min_y) / span_y;
            let w = usable_w * f64::from(display.logical_rect.width) / span_x;
            let h = usable_h * f64::from(display.logical_rect.height) / span_y;
            let name = display
                .connector_name
                .clone()
                .or_else(|| display.model.clone())
                .unwrap_or_else(|| "Display".into());
            let label = format!(
                "{name} · {}×{}",
                display.native_pixels.width, display.native_pixels.height
            );
            format!("{x:.5}|{y:.5}|{w:.5}|{h:.5}|{label}")
        })
        .collect()
}

/// One screen observation reported from Qt Quick.
#[derive(Clone, Debug)]
pub struct ScreenProbe {
    /// Platform screen name / connector.
    pub name: String,
    /// Manufacturer when available.
    pub manufacturer: String,
    /// Model when available.
    pub model: String,
    /// Serial when available.
    pub serial: String,
    /// Logical x.
    pub x: i32,
    /// Logical y.
    pub y: i32,
    /// Logical width.
    pub width: u32,
    /// Logical height.
    pub height: u32,
    /// Device pixel ratio.
    pub device_pixel_ratio: f64,
    /// Physical width in millimeters.
    pub physical_width_mm: f64,
    /// Physical height in millimeters.
    pub physical_height_mm: f64,
}

impl ScreenProbe {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn into_observed(self) -> Option<ObservedDisplay> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let ratio = if self.device_pixel_ratio.is_finite() && self.device_pixel_ratio > 0.0 {
            self.device_pixel_ratio
        } else {
            1.0
        };
        let native_width = ((f64::from(self.width) * ratio).round().max(1.0) as u32).max(1);
        let native_height = ((f64::from(self.height) * ratio).round().max(1.0) as u32).max(1);
        let scale = scale_from_ratio(ratio).unwrap_or_default();
        let physical_size = PhysicalSize {
            width: Millimeters(
                if self.physical_width_mm.is_finite() && self.physical_width_mm > 0.0 {
                    self.physical_width_mm
                } else {
                    f64::from(native_width) / 96.0 * 25.4
                },
            ),
            height: Millimeters(
                if self.physical_height_mm.is_finite() && self.physical_height_mm > 0.0 {
                    self.physical_height_mm
                } else {
                    f64::from(native_height) / 96.0 * 25.4
                },
            ),
        };
        let logical_rect = LogicalRect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        };
        let physical_origin = approximate_physical_origin(logical_rect, physical_size);
        Some(ObservedDisplay {
            evidence: DisplayEvidence {
                connector_name: non_empty(self.name),
                manufacturer: non_empty(self.manufacturer),
                model: non_empty(self.model),
                serial: non_empty(self.serial),
                native_pixels: NativePixelSize {
                    width: native_width,
                    height: native_height,
                },
            },
            logical_rect,
            scale_factor: scale,
            physical_size,
            physical_origin,
            rotation_degrees: 0,
        })
    }
}

/// Replaces the session arrangement from Qt-reported screens and persists it.
pub fn replace_from_probes(probes: Vec<ScreenProbe>) -> Result<DisplayArrangement, String> {
    let observed: Vec<ObservedDisplay> = probes
        .into_iter()
        .filter_map(ScreenProbe::into_observed)
        .collect();

    let mut guard = session().lock().expect("display session lock");
    let previous = if guard.arrangement.displays.is_empty() {
        load_or_default()
    } else {
        guard.arrangement.clone()
    };
    let matched = match_displays(&previous, observed);
    matched.validate().map_err(|error| error.to_string())?;
    save_arrangement(&matched).map_err(|error| error.to_string())?;
    guard.arrangement = matched.clone();
    guard.from_probe = true;
    Ok(matched)
}

/// Forces the fixture three-monitor layout into the session (smoke screenshots).
pub fn use_fixture_arrangement() {
    let arrangement =
        DisplayArrangement::from_displays(dev_displays()).expect("fixture displays are valid");
    let mut guard = session().lock().expect("display session lock");
    guard.arrangement = arrangement;
    guard.from_probe = false;
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn scale_from_ratio(ratio: f64) -> Result<ScaleFactor, wallspan_core::DisplayValidationError> {
    let numerator = ((ratio * 1000.0).round().max(1.0) as u32).max(1);
    ScaleFactor::new(numerator, 1000)
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("net", "fritztech", "wallspan").map_or_else(
        || PathBuf::from(".").join("wallspan-config"),
        |dirs| dirs.config_dir().to_path_buf(),
    )
}

fn arrangement_path() -> PathBuf {
    config_dir().join("arrangement.toml")
}

fn load_or_default() -> DisplayArrangement {
    load_arrangement(&arrangement_path()).unwrap_or_else(|_| DisplayArrangement::empty())
}

fn load_arrangement(path: &Path) -> Result<DisplayArrangement, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let arrangement: DisplayArrangement =
        toml::from_str(&text).map_err(|error| error.to_string())?;
    arrangement.validate().map_err(|error| error.to_string())?;
    Ok(arrangement)
}

fn save_arrangement(arrangement: &DisplayArrangement) -> Result<(), String> {
    fs::create_dir_all(config_dir()).map_err(|error| error.to_string())?;
    let text = toml::to_string_pretty(arrangement).map_err(|error| error.to_string())?;
    let path = arrangement_path();
    let temp = path.with_extension("toml.part");
    let stash = path.with_extension("toml.bak");
    {
        let mut file = fs::File::create(&temp).map_err(|error| error.to_string())?;
        file.write_all(text.as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }

    // Windows cannot rename over an existing destination. Stash the previous
    // file, move the new one into place, then remove the stash.
    let had_existing = path.exists();
    if had_existing {
        let _ = fs::remove_file(&stash);
        fs::rename(&path, &stash).map_err(|error| {
            let _ = fs::remove_file(&temp);
            error.to_string()
        })?;
    }

    match fs::rename(&temp, &path) {
        Ok(()) => {
            let _ = fs::remove_file(&stash);
            Ok(())
        }
        Err(error) => {
            if had_existing {
                let _ = fs::rename(&stash, &path);
            }
            let _ = fs::remove_file(&temp);
            Err(error.to_string())
        }
    }
}
