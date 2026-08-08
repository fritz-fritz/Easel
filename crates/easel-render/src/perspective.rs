// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Angular FOV perspective sampling for coplanar PhysicalSpan (ADR 0015).

use easel_core::ViewerPose;

use crate::plan::PixelRect;

/// Destination→source angular perspective map for one display output.
///
/// Wall millimeters project through `atan2(delta, distance)` into the eye's FOV;
/// that FOV is mapped onto the axis-aligned source crop from `place_source_on_span`.
/// As `distance → ∞` the map approaches linear PhysicalSpan sampling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AngularPerspective {
    /// Eye X in arrangement millimeters.
    pub eye_x_mm: f64,
    /// Eye Y in arrangement millimeters.
    pub eye_y_mm: f64,
    /// Positive view distance in millimeters.
    pub distance_mm: f64,
    /// Display content rectangle in millimeters.
    pub content_x_mm: f64,
    /// Display content rectangle in millimeters.
    pub content_y_mm: f64,
    /// Display content width in millimeters.
    pub content_w_mm: f64,
    /// Display content height in millimeters.
    pub content_h_mm: f64,
    /// Left edge of the mapped span/image rectangle in millimeters.
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
        Some(Self {
            eye_x_mm,
            eye_y_mm,
            distance_mm: pose.view_distance_mm,
            content_x_mm,
            content_y_mm,
            content_w_mm,
            content_h_mm,
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
    /// Returns [`None`] when the wall point lies outside the mapped image rectangle
    /// (Contain letterbox), so the raster path can keep the canvas fill color.
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
        let wall_x = self.content_x_mm + nx * self.content_w_mm;
        let wall_y = self.content_y_mm + ny * self.content_h_mm;
        if !self.wall_in_map(wall_x, wall_y) {
            return None;
        }
        let ax = angle(wall_x - self.eye_x_mm, self.distance_mm);
        let ay = angle(wall_y - self.eye_y_mm, self.distance_mm);

        let (aleft, aright, atop, abottom) = self.map_angle_bounds();
        let u = normalize(ax, aleft, aright);
        let v = normalize(ay, atop, abottom);
        Some((self.src_x + u * self.src_w, self.src_y + v * self.src_h))
    }

    /// True when a wall-space point falls inside the placed image map rectangle.
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
        for (u, v) in corners {
            // Sample content corners through the angular map (skip letterboxed corners).
            let wall_x = self.content_x_mm + u * self.content_w_mm;
            let wall_y = self.content_y_mm + v * self.content_h_mm;
            if !self.wall_in_map(wall_x, wall_y) {
                continue;
            }
            let ax = angle(wall_x - self.eye_x_mm, self.distance_mm);
            let ay = angle(wall_y - self.eye_y_mm, self.distance_mm);
            let (aleft, aright, atop, abottom) = self.map_angle_bounds();
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
            // Entire content is outside the mapped image (full letterbox).
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

    fn map_angle_bounds(self) -> (f64, f64, f64, f64) {
        let xs = [self.map_x_mm, self.map_x_mm + self.map_w_mm];
        let ys = [self.map_y_mm, self.map_y_mm + self.map_h_mm];
        let mut aleft = f64::INFINITY;
        let mut aright = f64::NEG_INFINITY;
        let mut atop = f64::INFINITY;
        let mut abottom = f64::NEG_INFINITY;
        for x in xs {
            let a = angle(x - self.eye_x_mm, self.distance_mm);
            aleft = aleft.min(a);
            aright = aright.max(a);
        }
        for y in ys {
            let a = angle(y - self.eye_y_mm, self.distance_mm);
            atop = atop.min(a);
            abottom = abottom.max(a);
        }
        (aleft, aright, atop, abottom)
    }
}

fn angle(delta_mm: f64, distance_mm: f64) -> f64 {
    delta_mm.atan2(distance_mm.max(f64::EPSILON))
}

fn normalize(value: f64, min: f64, max: f64) -> f64 {
    if (max - min).abs() < f64::EPSILON {
        0.5
    } else {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::DEFAULT_VIEW_DISTANCE_MM;

    #[test]
    fn large_distance_approaches_linear_uv() {
        let pose = ViewerPose {
            enabled: true,
            view_distance_mm: 50_000.0,
            eye_offset_x_mm: 0.0,
            eye_offset_y_mm: 0.0,
        };
        let map = AngularPerspective::new(
            pose, 300.0, 150.0, 0.0, 0.0, 600.0, 300.0, 0.0, 0.0, 600.0, 300.0, 0.0, 0.0, 100.0,
            50.0,
        )
        .expect("map");
        let (sx, sy) = map.source_xy(0, 0, 100, 50).expect("inside map");
        assert!((0.0..2.0).contains(&sx), "sx={sx}");
        assert!((0.0..2.0).contains(&sy), "sy={sy}");
        let (sx2, _) = map.source_xy(99, 0, 100, 50).expect("inside map");
        assert!(sx2 > 90.0, "sx2={sx2}");
        let _ = DEFAULT_VIEW_DISTANCE_MM;
    }

    #[test]
    fn wall_outside_map_returns_none() {
        let pose = ViewerPose {
            enabled: true,
            view_distance_mm: DEFAULT_VIEW_DISTANCE_MM,
            eye_offset_x_mm: 0.0,
            eye_offset_y_mm: 0.0,
        };
        // Content spans 0..600; mapped image is only the centered half-width strip.
        let map = AngularPerspective::new(
            pose, 300.0, 150.0, 0.0, 0.0, 600.0, 300.0, 150.0, 0.0, 300.0, 300.0, 0.0, 0.0, 100.0,
            50.0,
        )
        .expect("map");
        assert!(map.source_xy(0, 25, 100, 50).is_none());
        assert!(map.source_xy(50, 25, 100, 50).is_some());
    }
}
