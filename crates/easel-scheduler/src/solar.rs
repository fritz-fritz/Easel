// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Approximate sunrise/sunset using the NOAA solar equations.

use easel_core::GeoLocation;

/// Sunrise or sunset instant for a civil day.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolarEvent {
    /// Unix timestamp (UTC seconds) of sunrise.
    pub sunrise_unix: u64,
    /// Unix timestamp (UTC seconds) of sunset.
    pub sunset_unix: u64,
}

/// Computes sunrise and sunset for the UTC day containing `day_unix`.
///
/// Uses a compact NOAA-style approximation suitable for wallpaper scheduling.
/// Polar day/night returns the day's noon for both events so callers can still
/// schedule a deterministic fallback frame.
#[must_use]
pub fn solar_events_for_day(location: GeoLocation, day_unix: u64) -> SolarEvent {
    let day_start = day_unix - (day_unix % 86_400);
    let julian = f64::from(u32::try_from(day_start / 86_400).unwrap_or(0)) + 2_440_587.5;
    let n = libm::floor(julian - 2_450_545.0 + 0.000_8);
    let j_star = n - location.longitude_deg / 360.0;
    let m = (357.5291 + 0.985_600_28 * j_star).rem_euclid(360.0);
    let m_rad = m.to_radians();
    let c =
        1.9148 * libm::sin(m_rad) + 0.02 * libm::sin(2.0 * m_rad) + 0.0003 * libm::sin(3.0 * m_rad);
    let lambda = (m + c + 180.0 + 102.9372).rem_euclid(360.0).to_radians();
    let j_transit =
        2_450_545.0 + j_star + 0.0053 * libm::sin(m_rad) - 0.0069 * libm::sin(2.0 * lambda);
    let sin_dec = libm::sin(lambda) * libm::sin(23.4397_f64.to_radians());
    let dec = libm::asin(sin_dec);
    let lat = location.latitude_deg.to_radians();
    let cos_h = (libm::sin((-0.833_f64).to_radians()) - libm::sin(lat) * libm::sin(dec))
        / (libm::cos(lat) * libm::cos(dec));
    let noon = julian_to_unix(j_transit);
    if !(-1.0..=1.0).contains(&cos_h) {
        return SolarEvent {
            sunrise_unix: noon,
            sunset_unix: noon,
        };
    }
    let hour_angle = libm::acos(cos_h);
    let j_rise = j_transit - hour_angle / (2.0 * std::f64::consts::PI);
    let j_set = j_transit + hour_angle / (2.0 * std::f64::consts::PI);
    SolarEvent {
        sunrise_unix: julian_to_unix(j_rise),
        sunset_unix: julian_to_unix(j_set),
    }
}

fn julian_to_unix(julian: f64) -> u64 {
    let seconds = (julian - 2_440_587.5) * 86_400.0;
    if !seconds.is_finite() || seconds <= 0.0 {
        0
    } else {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        {
            seconds.round() as u64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equator_equinox_has_sunrise_before_sunset() {
        let location = GeoLocation {
            latitude_deg: 0.0,
            longitude_deg: 0.0,
        };
        // 2024-03-20 00:00:00 UTC
        let events = solar_events_for_day(location, 1_711_894_400);
        assert!(events.sunrise_unix < events.sunset_unix);
        let day_len = events.sunset_unix - events.sunrise_unix;
        assert!(day_len > 40_000 && day_len < 50_000);
    }
}
