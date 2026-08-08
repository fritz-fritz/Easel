// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Angular FOV perspective sampling for PhysicalSpan (ADR 0015 / 0016).
//!
//! Panels may carry small tilt/yaw away from the coplanar wall. The image map
//! stays on the z=0 span plane (`place_source_on_span`); destination pixels are
//! lifted onto each panel plane, then projected through the eye with `atan2`.
//! Identity tilt/yaw recovers the Stage 7.5 coplanar map.

use easel_core::ViewerPose;

use crate::plan::PixelRect;

/// Destination→source angular perspective map for one display output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AngularPerspective {
    /// Eye X in arrangement millimeters.
    pub eye_x_mm: f64,
    /// Eye Y in arrangement millimeters.
    pub eye_y_mm: f64,
    /// Positive view distance in millimeters (eye at z = −distance).
    pub distance_mm: f64,
    /// Display content rectangle in millimeters (unrotated bounds).
    pub content_x_mm: f64,
    /// Display content rectangle in millimeters.
    pub content_y_mm: f64,
    /// Display content width in millimeters.
    pub content_w_mm: f64,
    /// Display content height in millimeters.
    pub content_h_mm: f64,
    /// Panel tilt about local X (degrees; positive tips top edge toward viewer).
    pub tilt_deg: f64,
    /// Panel yaw about local Y (degrees; positive turns right edge toward viewer).
    pub yaw_deg: f64,
    /// Left edge of the mapped span/image rectangle in millimeters (z = 0).
    pub map_x_mm: f64,
    /// Top edge of the mapped span/image rectangle in millimeters.
    pub map_y_mm: f64,
    /// Width of the mapped rectangle in millimeters.
    pub map_w_mm: f64,
    /// Height of the mapped rectangle in millimeters.
    pub map_h_mm: f64,
    /// Source crop origin X (continuous pixels).
    pub src_x: f64,
    /// Source crop origin Y (continuous pixels).
    pub src_y: f64,
    /// Source crop width (continuous pixels).
    pub src_w: f64,
    /// Source crop height (continuous pixels).
    pub src_h: f64,
}

impl AngularPerspective {
    /// Builds a map from viewer pose + content/map/source placement.
    #[must_use]
    #[allow(clippy::too_many_arguments, clippy::similar_names)]
    pub fn new(
        pose: ViewerPose,
        eye_x_mm: f64,
        eye_y_mm: f64,
        content_x_mm: f64,
        content_y_mm: f64,
        content_w_mm: f64,
        content_h_mm: f64,
        tilt_deg: f64,
        yaw_deg: f64,
        map_x_mm: f64,
        map_y_mm: f64,
        map_w_mm: f64,
        map_h_mm: f64,
        src_x: f64,
        src_y: f64,
        src_w: f64,
        src_h: f64,
    ) -> Option<Self> {
        if !pose.is_active() || map_w_mm <= 0.0 || map_h_mm <= 0.0 {
            return None;
        }
        if !tilt_deg.is_finite() || !yaw_deg.is_finite() {
            return None;
        }
        Some(Self {
            eye_x_mm,
            eye_y_mm,
            distance_mm: pose.view_distance_mm,
            content_x_mm,
            content_y_mm,
            content_w_mm,
            content_h_mm,
            tilt_deg,
            yaw_deg,
            map_x_mm,
            map_y_mm,
            map_w_mm,
            map_h_mm,
            src_x,
            src_y,
            src_w,
            src_h,
        })
    }

    /// Maps a destination pixel center to continuous source coordinates.
    ///
    /// Returns [`None`] when the wall/map sample lies outside the mapped image
    /// rectangle (Contain letterbox), so the raster path can keep the canvas fill.
    #[must_use]
    pub fn source_xy(
        self,
        dest_x: u32,
        dest_y: u32,
        dest_w: u32,
        dest_h: u32,
    ) -> Option<(f64, f64)> {
        let nx = (f64::from(dest_x) + 0.5) / f64::from(dest_w.max(1));
        let ny = (f64::from(dest_y) + 0.5) / f64::from(dest_h.max(1));
        let (ax, ay) = self.panel_angles(nx, ny);
        let (aleft, aright, atop, abottom) = self.map_angle_bounds();
        let u = normalize_unclamped(ax, aleft, aright);
        let v = normalize_unclamped(ay, atop, abottom);
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return None;
        }
        Some((self.src_x + u * self.src_w, self.src_y + v * self.src_h))
    }

    /// True when a coplanar wall-space point falls inside the mapped image rectangle.
    #[must_use]
    pub fn wall_in_map(self, wall_x: f64, wall_y: f64) -> bool {
        wall_x >= self.map_x_mm
            && wall_x <= self.map_x_mm + self.map_w_mm
            && wall_y >= self.map_y_mm
            && wall_y <= self.map_y_mm + self.map_h_mm
    }

    /// Conservative axis-aligned source crop covering the angular map extremes.
    #[must_use]
    pub fn source_bounds(self, source_w: u32, source_h: u32) -> PixelRect {
        let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let (aleft, aright, atop, abottom) = self.map_angle_bounds();
        for (u, v) in corners {
            let (ax, ay) = self.panel_angles(u, v);
            let su = normalize(ax, aleft, aright);
            let sv = normalize(ay, atop, abottom);
            let sx = self.src_x + su * self.src_w;
            let sy = self.src_y + sv * self.src_h;
            min_x = min_x.min(sx);
            min_y = min_y.min(sy);
            max_x = max_x.max(sx);
            max_y = max_y.max(sy);
        }
        if !min_x.is_finite() {
            return PixelRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            };
        }
        let left = min_x
            .floor()
            .clamp(0.0, f64::from(source_w.saturating_sub(1)));
        let top = min_y
            .floor()
            .clamp(0.0, f64::from(source_h.saturating_sub(1)));
        let right = max_x.ceil().clamp(left + 1.0, f64::from(source_w));
        let bottom = max_y.ceil().clamp(top + 1.0, f64::from(source_h));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        PixelRect {
            x: left as i32,
            y: top as i32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        }
    }

    /// Serializes map fields for Plasma IPC / GPU uniforms (mm + source pixels).
    #[must_use]
    pub fn to_uniform_array(self) -> [f64; 17] {
        [
            self.eye_x_mm,
            self.eye_y_mm,
            self.distance_mm,
            self.content_x_mm,
            self.content_y_mm,
            self.content_w_mm,
            self.content_h_mm,
            self.tilt_deg,
            self.yaw_deg,
            self.map_x_mm,
            self.map_y_mm,
            self.map_w_mm,
            self.map_h_mm,
            self.src_x,
            self.src_y,
            self.src_w,
            self.src_h,
        ]
    }

    fn panel_angles(self, u: f64, v: f64) -> (f64, f64) {
        let (px, py, pz) = self.panel_point(u, v);
        let dx = px - self.eye_x_mm;
        let dy = py - self.eye_y_mm;
        let dz = pz - (-self.distance_mm);
        (angle(dx, dz), angle(dy, dz))
    }

    fn panel_point(self, u: f64, v: f64) -> (f64, f64, f64) {
        let center_x = self.content_x_mm + self.content_w_mm * 0.5;
        let center_y = self.content_y_mm + self.content_h_mm * 0.5;
        let local_x = (u - 0.5) * self.content_w_mm;
        let local_y = (v - 0.5) * self.content_h_mm;
        let (rx, ry, rz) = rotate_tilt_yaw(local_x, local_y, 0.0, self.tilt_deg, self.yaw_deg);
        (center_x + rx, center_y + ry, rz)
    }

    fn map_angle_bounds(self) -> (f64, f64, f64, f64) {
        let corners = [
            (self.map_x_mm, self.map_y_mm),
            (self.map_x_mm + self.map_w_mm, self.map_y_mm),
            (self.map_x_mm, self.map_y_mm + self.map_h_mm),
            (self.map_x_mm + self.map_w_mm, self.map_y_mm + self.map_h_mm),
        ];
        let mut aleft = f64::INFINITY;
        let mut aright = f64::NEG_INFINITY;
        let mut atop = f64::INFINITY;
        let mut abottom = f64::NEG_INFINITY;
        for (x, y) in corners {
            let dx = x - self.eye_x_mm;
            let dy = y - self.eye_y_mm;
            let dz = 0.0 - (-self.distance_mm);
            let ax = angle(dx, dz);
            let ay = angle(dy, dz);
            aleft = aleft.min(ax);
            aright = aright.max(ax);
            atop = atop.min(ay);
            abottom = abottom.max(ay);
        }
        (aleft, aright, atop, abottom)
    }
}

fn rotate_tilt_yaw(x: f64, y: f64, z: f64, tilt_deg: f64, yaw_deg: f64) -> (f64, f64, f64) {
    let tilt = tilt_deg.to_radians();
    let yaw = yaw_deg.to_radians();
    let (st, ct) = (tilt.sin(), tilt.cos());
    // Rx(tilt)
    let y1 = y * ct - z * st;
    let z1 = y * st + z * ct;
    let x1 = x;
    let (sy, cy) = (yaw.sin(), yaw.cos());
    // Ry(yaw)
    let x2 = x1 * cy + z1 * sy;
    let y2 = y1;
    let z2 = -x1 * sy + z1 * cy;
    (x2, y2, z2)
}

fn angle(delta_mm: f64, depth_mm: f64) -> f64 {
    delta_mm.atan2(depth_mm.max(f64::EPSILON))
}

fn normalize(value: f64, min: f64, max: f64) -> f64 {
    if (max - min).abs() < f64::EPSILON {
        0.5
    } else {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    }
}

fn normalize_unclamped(value: f64, min: f64, max: f64) -> f64 {
    if (max - min).abs() < f64::EPSILON {
        0.5
    } else {
        (value - min) / (max - min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::DEFAULT_VIEW_DISTANCE_MM;

    fn active_pose() -> ViewerPose {
        ViewerPose {
            enabled: true,
            view_distance_mm: DEFAULT_VIEW_DISTANCE_MM,
            eye_offset_x_mm: 0.0,
            eye_offset_y_mm: 0.0,
        }
    }

    #[test]
    fn large_distance_approaches_linear_uv() {
        let pose = ViewerPose {
            enabled: true,
            view_distance_mm: 50_000.0,
            eye_offset_x_mm: 0.0,
            eye_offset_y_mm: 0.0,
        };
        let map = AngularPerspective::new(
            pose, 300.0, 150.0, 0.0, 0.0, 600.0, 300.0, 0.0, 0.0, 0.0, 0.0, 600.0, 300.0, 0.0, 0.0,
            100.0, 50.0,
        )
        .expect("map");
        let (sx, sy) = map.source_xy(0, 0, 100, 50).expect("inside map");
        assert!((0.0..2.0).contains(&sx), "sx={sx}");
        assert!((0.0..2.0).contains(&sy), "sy={sy}");
        let (sx2, _) = map.source_xy(99, 0, 100, 50).expect("inside map");
        assert!(sx2 > 90.0, "sx2={sx2}");
    }

    #[test]
    fn wall_outside_map_returns_none() {
        let map = AngularPerspective::new(
            active_pose(),
            300.0,
            150.0,
            0.0,
            0.0,
            600.0,
            300.0,
            0.0,
            0.0,
            150.0,
            0.0,
            300.0,
            300.0,
            0.0,
            0.0,
            100.0,
            50.0,
        )
        .expect("map");
        assert!(map.source_xy(0, 25, 100, 50).is_none());
        assert!(map.source_xy(50, 25, 100, 50).is_some());
    }

    #[test]
    fn zero_angles_match_coplanar_sampling() {
        let with_angles = AngularPerspective::new(
            active_pose(),
            300.0,
            150.0,
            0.0,
            0.0,
            600.0,
            300.0,
            0.0,
            0.0,
            0.0,
            0.0,
            600.0,
            300.0,
            0.0,
            0.0,
            100.0,
            50.0,
        )
        .expect("map");
        let (sx, sy) = with_angles.source_xy(25, 10, 100, 50).expect("sample");
        // Coplanar wall point at the same UV must agree with the 3D path at 0 angles.
        let wall_x = 0.0 + (25.0 + 0.5) / 100.0 * 600.0;
        let wall_y = 0.0 + (10.0 + 0.5) / 50.0 * 300.0;
        let ax = angle(wall_x - 300.0, 600.0);
        let ay = angle(wall_y - 150.0, 600.0);
        let (aleft, aright, atop, abottom) = with_angles.map_angle_bounds();
        let u = normalize(ax, aleft, aright);
        let v = normalize(ay, atop, abottom);
        let expect_x = u * 100.0;
        let expect_y = v * 50.0;
        assert!((sx - expect_x).abs() < 1e-6, "sx={sx} expect={expect_x}");
        assert!((sy - expect_y).abs() < 1e-6, "sy={sy} expect={expect_y}");
    }

    #[test]
    fn nonzero_yaw_changes_sample() {
        let flat = AngularPerspective::new(
            active_pose(),
            300.0,
            150.0,
            0.0,
            0.0,
            600.0,
            300.0,
            0.0,
            0.0,
            0.0,
            0.0,
            600.0,
            300.0,
            0.0,
            0.0,
            100.0,
            50.0,
        )
        .expect("flat");
        let yawed = AngularPerspective::new(
            active_pose(),
            300.0,
            150.0,
            0.0,
            0.0,
            600.0,
            300.0,
            0.0,
            15.0,
            0.0,
            0.0,
            600.0,
            300.0,
            0.0,
            0.0,
            100.0,
            50.0,
        )
        .expect("yawed");
        let a = flat.source_xy(10, 20, 100, 50).expect("flat sample");
        let b = yawed.source_xy(10, 20, 100, 50).expect("yawed sample");
        assert_ne!(a, b);
    }
}
