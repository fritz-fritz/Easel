// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Apple `apple_desktop` XMP + binary plist parsing.

use std::collections::HashMap;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use easel_core::{AppearanceMode, DynamicScheduleKind, DynamicStillKey, LocalTimeOfDay};
use plist::Value;
use serde::Deserialize;
use thiserror::Error;

/// Which Apple XMP tag supplied the schedule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppleMetadataFlavor {
    /// `apple_desktop:solar` — altitude/azimuth samples.
    Solar,
    /// `apple_desktop:apr` — light/dark indices.
    Appearance,
    /// `apple_desktop:h24` — 24-hour time samples.
    H24,
}

/// Parsed Apple dynamic desktop metadata mapped toward Easel keys.
#[derive(Clone, Debug, PartialEq)]
pub struct AppleDesktopMetadata {
    /// Schedule flavor.
    pub flavor: AppleMetadataFlavor,
    /// Corresponding Easel schedule kind.
    pub schedule_kind: DynamicScheduleKind,
    /// Image index → key for each sample.
    pub keys_by_index: HashMap<u32, DynamicStillKey>,
}

/// Locates an XMP packet inside arbitrary file bytes (HEIC friendly).
#[must_use]
pub fn scrape_xmp_packet(bytes: &[u8]) -> Option<String> {
    const BEGIN: &[u8] = b"<?xpacket begin=";
    const BEGIN_ALT: &[u8] = b"<x:xmpmeta";
    const END: &[u8] = b"<?xpacket end=";
    let start = find_subslice(bytes, BEGIN).or_else(|| find_subslice(bytes, BEGIN_ALT))?;
    let end = find_subslice(&bytes[start..], END)? + start;
    let end = (end + END.len()).min(bytes.len());
    // Include a little trailing content for `end="w"?>`.
    let end = bytes[end..]
        .iter()
        .position(|&b| b == b'>')
        .map_or(end, |rel| end + rel + 1)
        .min(bytes.len());
    String::from_utf8(bytes[start..end].to_vec()).ok()
}

/// Parses `apple_desktop:{solar,apr,h24}` from an XMP packet string.
pub fn parse_apple_desktop_from_xmp(xmp: &str) -> Result<AppleDesktopMetadata, MetadataError> {
    for (attr, flavor) in [
        ("apple_desktop:solar=\"", AppleMetadataFlavor::Solar),
        ("apple_desktop:apr=\"", AppleMetadataFlavor::Appearance),
        ("apple_desktop:h24=\"", AppleMetadataFlavor::H24),
    ] {
        if let Some(rest) = xmp.split(attr).nth(1) {
            let encoded = rest
                .split('"')
                .next()
                .ok_or(MetadataError::MalformedXmpAttribute)?;
            return parse_apple_desktop_plist(encoded, flavor);
        }
    }
    // Also accept element form: <apple_desktop:solar>...</apple_desktop:solar>
    for (tag, flavor) in [
        ("apple_desktop:solar", AppleMetadataFlavor::Solar),
        ("apple_desktop:apr", AppleMetadataFlavor::Appearance),
        ("apple_desktop:h24", AppleMetadataFlavor::H24),
    ] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        if let (Some(start), Some(end_rel)) =
            (xmp.find(&open).map(|i| i + open.len()), xmp.find(&close))
        {
            if end_rel > start {
                let encoded = xmp[start..end_rel].trim();
                return parse_apple_desktop_plist(encoded, flavor);
            }
        }
    }
    Err(MetadataError::NoAppleDesktopTag)
}

/// Decodes a base64 Apple desktop binary plist into keyed frame metadata.
pub fn parse_apple_desktop_plist(
    base64_plist: &str,
    flavor: AppleMetadataFlavor,
) -> Result<AppleDesktopMetadata, MetadataError> {
    let bytes = BASE64
        .decode(base64_plist.trim())
        .map_err(|error| MetadataError::Base64(error.to_string()))?;
    let value = Value::from_reader(std::io::Cursor::new(bytes))
        .map_err(|error| MetadataError::Plist(error.to_string()))?;
    match flavor {
        AppleMetadataFlavor::Solar => parse_solar_plist(&value),
        AppleMetadataFlavor::Appearance => parse_appearance_plist(&value),
        AppleMetadataFlavor::H24 => parse_h24_plist(&value),
    }
}

fn parse_solar_plist(value: &Value) -> Result<AppleDesktopMetadata, MetadataError> {
    #[derive(Debug, Deserialize)]
    struct SolarRoot {
        #[serde(default)]
        si: Vec<SolarItem>,
    }
    #[derive(Debug, Deserialize)]
    struct SolarItem {
        i: u32,
        a: f64,
        z: f64,
    }
    let root: SolarRoot =
        plist::from_value(value).map_err(|error| MetadataError::Plist(error.to_string()))?;
    if root.si.is_empty() {
        return Err(MetadataError::EmptySchedule);
    }
    let mut keys_by_index = HashMap::new();
    for item in root.si {
        let mut azimuth = item.z % 360.0;
        if azimuth < 0.0 {
            azimuth += 360.0;
        }
        keys_by_index.insert(
            item.i,
            DynamicStillKey::SolarPosition {
                altitude_deg: item.a,
                azimuth_deg: azimuth,
            },
        );
    }
    Ok(AppleDesktopMetadata {
        flavor: AppleMetadataFlavor::Solar,
        schedule_kind: DynamicScheduleKind::SolarPosition,
        keys_by_index,
    })
}

fn parse_appearance_plist(value: &Value) -> Result<AppleDesktopMetadata, MetadataError> {
    #[derive(Debug, Deserialize)]
    struct AprRoot {
        l: u32,
        d: u32,
    }
    let root: AprRoot =
        plist::from_value(value).map_err(|error| MetadataError::Plist(error.to_string()))?;
    let mut keys_by_index = HashMap::new();
    keys_by_index.insert(
        root.l,
        DynamicStillKey::Appearance {
            mode: AppearanceMode::Light,
        },
    );
    keys_by_index.insert(
        root.d,
        DynamicStillKey::Appearance {
            mode: AppearanceMode::Dark,
        },
    );
    Ok(AppleDesktopMetadata {
        flavor: AppleMetadataFlavor::Appearance,
        schedule_kind: DynamicScheduleKind::Appearance,
        keys_by_index,
    })
}

fn parse_h24_plist(value: &Value) -> Result<AppleDesktopMetadata, MetadataError> {
    // wallpapper / Apple h24 uses `ti` array with `i` index and `t` time fraction of day,
    // or nested dicts. Accept both `ti` items with `t` in [0,1) and hour/minute fields.
    #[derive(Debug, Deserialize)]
    struct H24Root {
        #[serde(default)]
        ti: Vec<H24Item>,
    }
    #[derive(Debug, Deserialize)]
    struct H24Item {
        i: u32,
        #[serde(default)]
        t: Option<f64>,
        #[serde(default)]
        hour: Option<u8>,
        #[serde(default)]
        minute: Option<u8>,
    }
    let root: H24Root =
        plist::from_value(value).map_err(|error| MetadataError::Plist(error.to_string()))?;
    if root.ti.is_empty() {
        return Err(MetadataError::EmptySchedule);
    }
    let mut keys_by_index = HashMap::new();
    for item in root.ti {
        let time = if let (Some(hour), Some(minute)) = (item.hour, item.minute) {
            LocalTimeOfDay::new(hour, minute)
                .map_err(|error| MetadataError::Plist(error.to_string()))?
        } else if let Some(fraction) = item.t {
            let clamped = fraction.clamp(0.0, 0.999_999);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let minutes = (clamped * 24.0 * 60.0).floor() as u32;
            #[allow(clippy::cast_possible_truncation)]
            let hour = (minutes / 60) as u8;
            #[allow(clippy::cast_possible_truncation)]
            let minute = (minutes % 60) as u8;
            LocalTimeOfDay::new(hour, minute)
                .map_err(|error| MetadataError::Plist(error.to_string()))?
        } else {
            return Err(MetadataError::Plist(format!(
                "h24 item {} missing time fields",
                item.i
            )));
        };
        keys_by_index.insert(item.i, DynamicStillKey::TimeOfDay { time });
    }
    Ok(AppleDesktopMetadata {
        flavor: AppleMetadataFlavor::H24,
        schedule_kind: DynamicScheduleKind::TimeOfDay,
        keys_by_index,
    })
}

/// Builds a base64 binary plist for the given Apple metadata flavor and keyed indices.
pub fn build_apple_desktop_plist<S: std::hash::BuildHasher>(
    flavor: AppleMetadataFlavor,
    keys_by_index: &HashMap<u32, DynamicStillKey, S>,
) -> Result<String, MetadataError> {
    let value = match flavor {
        AppleMetadataFlavor::Solar => build_solar_plist_values(keys_by_index)?,
        AppleMetadataFlavor::Appearance => build_appearance_plist_values(keys_by_index)?,
        AppleMetadataFlavor::H24 => build_h24_plist_values(keys_by_index)?,
    };
    let mut bytes = Vec::new();
    value
        .to_writer_binary(&mut bytes)
        .map_err(|error| MetadataError::Plist(error.to_string()))?;
    Ok(BASE64.encode(bytes))
}

/// Builds a minimal XMP packet carrying one `apple_desktop:{solar,apr,h24}` attribute.
pub fn build_apple_xmp<S: std::hash::BuildHasher>(
    flavor: AppleMetadataFlavor,
    keys_by_index: &HashMap<u32, DynamicStillKey, S>,
) -> Result<String, MetadataError> {
    let encoded = build_apple_desktop_plist(flavor, keys_by_index)?;
    let attr = match flavor {
        AppleMetadataFlavor::Solar => "solar",
        AppleMetadataFlavor::Appearance => "apr",
        AppleMetadataFlavor::H24 => "h24",
    };
    Ok(format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:apple_desktop="http://ns.apple.com/namespace/1.0/" apple_desktop:{attr}="{encoded}"/></rdf:RDF></x:xmpmeta><?xpacket end="w"?>"#
    ))
}

/// Infers Apple flavor + keyed indices from a domain still set for export.
pub fn apple_keys_from_still_set(
    set: &easel_core::DynamicStillSet,
) -> Result<(AppleMetadataFlavor, HashMap<u32, DynamicStillKey>), MetadataError> {
    use easel_core::DynamicScheduleKind;

    let flavor = match set.schedule_kind {
        DynamicScheduleKind::SolarPosition => AppleMetadataFlavor::Solar,
        DynamicScheduleKind::Appearance => AppleMetadataFlavor::Appearance,
        DynamicScheduleKind::TimeOfDay => AppleMetadataFlavor::H24,
    };
    let mut keys_by_index = HashMap::new();
    for (order, frame) in set.frames.iter().enumerate() {
        let index = frame
            .source_index
            .unwrap_or_else(|| u32::try_from(order).unwrap_or(u32::MAX));
        match (flavor, frame.key) {
            (AppleMetadataFlavor::Solar, DynamicStillKey::SolarPosition { .. })
            | (AppleMetadataFlavor::Appearance, DynamicStillKey::Appearance { .. })
            | (AppleMetadataFlavor::H24, DynamicStillKey::TimeOfDay { .. }) => {
                keys_by_index.insert(index, frame.key);
            }
            _ => {
                return Err(MetadataError::Plist(format!(
                    "frame {index} key {} is incompatible with {:?} export",
                    frame.key.label(),
                    flavor
                )));
            }
        }
    }
    if keys_by_index.is_empty() {
        return Err(MetadataError::EmptySchedule);
    }
    Ok((flavor, keys_by_index))
}

fn build_solar_plist_values<S: std::hash::BuildHasher>(
    keys_by_index: &HashMap<u32, DynamicStillKey, S>,
) -> Result<Value, MetadataError> {
    let mut items = Vec::new();
    let mut indices: Vec<_> = keys_by_index.keys().copied().collect();
    indices.sort_unstable();
    for index in indices {
        let Some(DynamicStillKey::SolarPosition {
            altitude_deg,
            azimuth_deg,
        }) = keys_by_index.get(&index).copied()
        else {
            return Err(MetadataError::Plist(format!(
                "solar export missing SolarPosition for index {index}"
            )));
        };
        let mut item = plist::Dictionary::new();
        item.insert("i".into(), Value::Integer(i64::from(index).into()));
        item.insert("a".into(), Value::Real(altitude_deg));
        item.insert("z".into(), Value::Real(azimuth_deg));
        items.push(Value::Dictionary(item));
    }
    let mut root = plist::Dictionary::new();
    root.insert("si".into(), Value::Array(items));
    Ok(Value::Dictionary(root))
}

fn build_appearance_plist_values<S: std::hash::BuildHasher>(
    keys_by_index: &HashMap<u32, DynamicStillKey, S>,
) -> Result<Value, MetadataError> {
    let mut light = None;
    let mut dark = None;
    for (index, key) in keys_by_index {
        match key {
            DynamicStillKey::Appearance {
                mode: AppearanceMode::Light,
            } => light = Some(*index),
            DynamicStillKey::Appearance {
                mode: AppearanceMode::Dark,
            } => dark = Some(*index),
            _ => {
                return Err(MetadataError::Plist(format!(
                    "appearance export got non-appearance key at {index}"
                )));
            }
        }
    }
    let (Some(l), Some(d)) = (light, dark) else {
        return Err(MetadataError::Plist(
            "appearance export requires both light and dark indices".into(),
        ));
    };
    let mut root = plist::Dictionary::new();
    root.insert("l".into(), Value::Integer(i64::from(l).into()));
    root.insert("d".into(), Value::Integer(i64::from(d).into()));
    Ok(Value::Dictionary(root))
}

fn build_h24_plist_values<S: std::hash::BuildHasher>(
    keys_by_index: &HashMap<u32, DynamicStillKey, S>,
) -> Result<Value, MetadataError> {
    let mut items = Vec::new();
    let mut indices: Vec<_> = keys_by_index.keys().copied().collect();
    indices.sort_unstable();
    for index in indices {
        let Some(DynamicStillKey::TimeOfDay { time }) = keys_by_index.get(&index).copied() else {
            return Err(MetadataError::Plist(format!(
                "h24 export missing TimeOfDay for index {index}"
            )));
        };
        let fraction = (f64::from(time.hour) * 60.0 + f64::from(time.minute)) / (24.0 * 60.0);
        let mut item = plist::Dictionary::new();
        item.insert("i".into(), Value::Integer(i64::from(index).into()));
        item.insert("t".into(), Value::Real(fraction));
        items.push(Value::Dictionary(item));
    }
    let mut root = plist::Dictionary::new();
    root.insert("ti".into(), Value::Array(items));
    Ok(Value::Dictionary(root))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Metadata parse failures.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum MetadataError {
    /// No Apple desktop XMP attribute was found.
    #[error("no apple_desktop solar/apr/h24 metadata in XMP")]
    NoAppleDesktopTag,
    /// Attribute quoting was malformed.
    #[error("malformed apple_desktop XMP attribute")]
    MalformedXmpAttribute,
    /// Base64 decode failed.
    #[error("apple_desktop base64 decode failed: {0}")]
    Base64(String),
    /// Binary plist parse failed.
    #[error("apple_desktop plist parse failed: {0}")]
    Plist(String),
    /// Schedule contained no samples.
    #[error("apple_desktop schedule is empty")]
    EmptySchedule,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal solar plist (two samples) encoded as binary via plist crate in the test.
    fn solar_base64() -> String {
        let value = Value::Dictionary({
            let mut root = plist::Dictionary::new();
            let items = vec![
                Value::Dictionary({
                    let mut item = plist::Dictionary::new();
                    item.insert("i".into(), Value::Integer(0.into()));
                    item.insert("a".into(), Value::Real(-0.34));
                    item.insert("z".into(), Value::Real(270.9));
                    item
                }),
                Value::Dictionary({
                    let mut item = plist::Dictionary::new();
                    item.insert("i".into(), Value::Integer(1.into()));
                    item.insert("a".into(), Value::Real(53.4));
                    item.insert("z".into(), Value::Real(182.2));
                    item
                }),
            ];
            root.insert("si".into(), Value::Array(items));
            root
        });
        let mut bytes = Vec::new();
        value.to_writer_binary(&mut bytes).unwrap();
        BASE64.encode(bytes)
    }

    #[test]
    fn parses_solar_plist_into_position_keys() {
        let meta = parse_apple_desktop_plist(&solar_base64(), AppleMetadataFlavor::Solar).unwrap();
        assert_eq!(meta.flavor, AppleMetadataFlavor::Solar);
        assert_eq!(meta.schedule_kind, DynamicScheduleKind::SolarPosition);
        assert_eq!(meta.keys_by_index.len(), 2);
        assert!(matches!(
            meta.keys_by_index.get(&1).unwrap(),
            DynamicStillKey::SolarPosition {
                altitude_deg,
                ..
            } if (*altitude_deg - 53.4).abs() < 0.01
        ));
    }

    #[test]
    fn parses_appearance_plist() {
        let value = Value::Dictionary({
            let mut root = plist::Dictionary::new();
            root.insert("l".into(), Value::Integer(0.into()));
            root.insert("d".into(), Value::Integer(1.into()));
            root
        });
        let mut bytes = Vec::new();
        value.to_writer_binary(&mut bytes).unwrap();
        let encoded = BASE64.encode(bytes);
        let meta = parse_apple_desktop_plist(&encoded, AppleMetadataFlavor::Appearance).unwrap();
        assert_eq!(
            meta.keys_by_index.get(&0),
            Some(&DynamicStillKey::Appearance {
                mode: AppearanceMode::Light
            })
        );
        assert_eq!(
            meta.keys_by_index.get(&1),
            Some(&DynamicStillKey::Appearance {
                mode: AppearanceMode::Dark
            })
        );
    }

    #[test]
    fn scrapes_and_parses_xmp_attribute() {
        let encoded = solar_base64();
        let xmp = format!(
            r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:apple_desktop="http://ns.apple.com/namespace/1.0/" apple_desktop:solar="{encoded}"/></rdf:RDF></x:xmpmeta><?xpacket end="w"?>"#
        );
        let bytes = xmp.as_bytes();
        let scraped = scrape_xmp_packet(bytes).unwrap();
        let meta = parse_apple_desktop_from_xmp(&scraped).unwrap();
        assert_eq!(meta.keys_by_index.len(), 2);
    }
}
