//! The local projection between `[lng, lat]` degrees and ground meters ([`Projection`]), and
//! the map scale in meters per pixel.

use std::f64::consts::PI;

use super::Scale;
use crate::geom::Point;

/// Web Mercator's sphere radius in meters (EPSG:3857).
pub(super) const EARTH_RADIUS_M: f64 = 6_378_137.0;

/// Web Mercator's latitude limit in degrees: beyond it the map has no y.
pub const MAX_LATITUDE: f64 = 85.051_128_779_806_59;

/// Map tiles are 512 pixels wide (MapLibre's zoom levels).
pub(super) const TILE_PX: f64 = 512.0;
/// A local projection around a center: `x = R · cos(lat0) · Δlng`, `y = R · Δlat` (radians),
/// shifted so the center is `(0, 0)`. One unit is one ground meter at the center, as with Web
/// Mercator scaled by `cos(lat0)`; across a 10 km city the two differ by under 0.2%.
/// It is linear, so it needs no platform math per point: native builds and WebAssembly project
/// every point to the same bits (`cos(lat0)` comes from [`crate::det::cos`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    lng0: f64,
    lat0: f64,
    /// Meters per degree east: `R · cos(lat0) · π / 180`.
    m_per_deg_x: f64,
    /// Meters per degree north: `R · π / 180`.
    m_per_deg_y: f64,
    pub(super) cos_lat0: f64,
}

impl Projection {
    /// Centered on the middle of the bounding box of all points. Expects valid coordinates
    /// (see [`side_by_side`](super::side_by_side)); with no points it centers on `(0, 0)`.
    #[must_use]
    pub fn around(lines: &[Vec<Point>]) -> Self {
        let mut min = Point::new(f64::INFINITY, f64::INFINITY);
        let mut max = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in lines.iter().flatten() {
            min = Point::new(min.x.min(p.x), min.y.min(p.y));
            max = Point::new(max.x.max(p.x), max.y.max(p.y));
        }
        if min.x > max.x {
            return Self::at(Point::new(0.0, 0.0));
        }
        Self::at(Point::new(
            f64::midpoint(min.x, max.x),
            f64::midpoint(min.y, max.y),
        ))
    }

    /// Centered on `center` (`[lng, lat]` degrees).
    #[must_use]
    pub fn at(center: Point) -> Self {
        let cos_lat0 = crate::det::cos(center.y.to_radians());
        let m_per_deg_y = EARTH_RADIUS_M * PI / 180.0;
        Self {
            lng0: center.x,
            lat0: center.y,
            m_per_deg_x: m_per_deg_y * cos_lat0,
            m_per_deg_y,
            cos_lat0,
        }
    }

    /// `[lng, lat]` degrees → meters.
    #[must_use]
    pub fn forward(&self, p: Point) -> Point {
        Point::new(
            (p.x - self.lng0) * self.m_per_deg_x,
            (p.y - self.lat0) * self.m_per_deg_y,
        )
    }

    /// Every point of `lines` in meters ([`Projection::forward`]).
    pub(super) fn forward_all(&self, lines: &[Vec<Point>]) -> Vec<Vec<Point>> {
        lines
            .iter()
            .map(|l| l.iter().map(|&p| self.forward(p)).collect())
            .collect()
    }

    /// Meters → `[lng, lat]` degrees.
    #[must_use]
    pub fn inverse(&self, p: Point) -> Point {
        Point::new(
            self.lng0 + p.x / self.m_per_deg_x,
            self.lat0 + p.y / self.m_per_deg_y,
        )
    }

    /// Ground meters per screen pixel at the center (Web Mercator's scale there, 512 px tiles).
    /// 2^zoom comes from [`crate::det::exp2`], so a fractional zoom gives the same bits on every
    /// machine.
    #[must_use]
    pub fn meters_per_pixel(&self, scale: Scale) -> f64 {
        match scale {
            Scale::MetersPerPixel(m) => m,
            Scale::Zoom(z) => {
                2.0 * PI * EARTH_RADIUS_M * self.cos_lat0 / (TILE_PX * crate::det::exp2(z))
            }
        }
    }
}
