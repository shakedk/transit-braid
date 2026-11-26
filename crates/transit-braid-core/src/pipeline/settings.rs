//! The settings of a call ([`Settings`]): the map scale it draws at ([`Scale`]) and the line
//! widths in pixels.

use super::PipelineError;

/// How big a screen pixel is on the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scale {
    /// Map zoom level (512-pixel tiles), 0 to 24.
    Zoom(f64),
    /// Ground meters per screen pixel, above 0.
    MetersPerPixel(f64),
}

/// Settings for [`side_by_side`](super::side_by_side). Widths are in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// The map scale the result is drawn at.
    pub scale: Scale,
    /// Width of each line in pixels, above 0.
    pub line_width_px: f64,
    /// Gap between neighboring lines in pixels, 0 or more.
    pub spacing_px: f64,
}

impl Settings {
    /// Checks every setting is a finite number in its range.
    ///
    /// # Errors
    /// [`PipelineError::InvalidSettings`] naming the setting.
    pub fn validate(&self) -> Result<(), PipelineError> {
        let bad = |why: &str| Err(PipelineError::InvalidSettings(why.to_owned()));
        match self.scale {
            Scale::Zoom(z) if !(z.is_finite() && (0.0..=24.0).contains(&z)) => {
                return bad("zoom must be a number from 0 to 24");
            }
            Scale::MetersPerPixel(m) if !(m.is_finite() && m > 0.0) => {
                return bad("meters per pixel must be a number above 0");
            }
            _ => {}
        }
        if !(self.line_width_px.is_finite() && self.line_width_px > 0.0) {
            return bad("line width must be a number above 0");
        }
        if !(self.spacing_px.is_finite() && self.spacing_px >= 0.0) {
            return bad("spacing must be a number, 0 or more");
        }
        Ok(())
    }
}
