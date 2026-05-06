//! Core geometry types and coordinate conversion for the image engine.

pub mod pixmap;
pub mod transform;
pub mod history;

/// Logical coordinate (DPI-independent).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalPoint {
    pub x: f64,
    pub y: f64,
}

impl LogicalPoint {
    /// Convert to physical pixel coordinate using the given scale factor.
    pub fn to_physical(&self, scale: f64) -> PhysicalPoint {
        PhysicalPoint {
            x: (self.x * scale).round() as i32,
            y: (self.y * scale).round() as i32,
        }
    }
}

/// Logical rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalRect {
    pub min: LogicalPoint,
    pub max: LogicalPoint,
}

impl LogicalRect {
    /// Width of the rectangle in logical units.
    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    /// Height of the rectangle in logical units.
    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    /// Returns `true` if the rectangle has zero or negative area.
    pub fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }

    /// Returns `true` if the point lies inside the rectangle (min-inclusive, max-exclusive).
    pub fn contains(&self, point: LogicalPoint) -> bool {
        point.x >= self.min.x
            && point.x < self.max.x
            && point.y >= self.min.y
            && point.y < self.max.y
    }

    /// Convert to a physical rectangle using the given scale factor.
    pub fn to_physical(&self, scale: f64) -> PhysicalRect {
        PhysicalRect {
            min: self.min.to_physical(scale),
            max: self.max.to_physical(scale),
        }
    }
}

/// Physical pixel coordinate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

/// Physical rectangle in integer pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicalRect {
    pub min: PhysicalPoint,
    pub max: PhysicalPoint,
}

/// Color with 8-bit RGBA channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Self = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    pub const WHITE: Self = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    pub const TRANSPARENT: Self = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// Parse a hex color string.
    ///
    /// Supported formats:
    /// - `#RRGGBB`
    /// - `#RRGGBBAA`
    /// - `RRGGBB`
    /// - `RRGGBBAA`
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);

        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16)
                    .map_err(|e| format!("invalid red component: {e}"))?;
                let g = u8::from_str_radix(&hex[2..4], 16)
                    .map_err(|e| format!("invalid green component: {e}"))?;
                let b = u8::from_str_radix(&hex[4..6], 16)
                    .map_err(|e| format!("invalid blue component: {e}"))?;
                Ok(Color { r, g, b, a: 255 })
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16)
                    .map_err(|e| format!("invalid red component: {e}"))?;
                let g = u8::from_str_radix(&hex[2..4], 16)
                    .map_err(|e| format!("invalid green component: {e}"))?;
                let b = u8::from_str_radix(&hex[4..6], 16)
                    .map_err(|e| format!("invalid blue component: {e}"))?;
                let a = u8::from_str_radix(&hex[6..8], 16)
                    .map_err(|e| format!("invalid alpha component: {e}"))?;
                Ok(Color { r, g, b, a })
            }
            _ => Err(format!(
                "invalid hex length: expected 6 or 8 digits, got {}",
                hex.len()
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_rect_width_and_height() {
        let rect = LogicalRect {
            min: LogicalPoint { x: 10.0, y: 20.0 },
            max: LogicalPoint { x: 50.0, y: 80.0 },
        };
        assert_eq!(rect.width(), 40.0);
        assert_eq!(rect.height(), 60.0);
    }

    #[test]
    fn logical_rect_is_empty() {
        assert!(
            LogicalRect {
                min: LogicalPoint { x: 0.0, y: 0.0 },
                max: LogicalPoint { x: 0.0, y: 10.0 },
            }
            .is_empty()
        );
        assert!(
            LogicalRect {
                min: LogicalPoint { x: 0.0, y: 0.0 },
                max: LogicalPoint { x: 10.0, y: 0.0 },
            }
            .is_empty()
        );
        assert!(
            LogicalRect {
                min: LogicalPoint { x: 5.0, y: 5.0 },
                max: LogicalPoint { x: 3.0, y: 10.0 },
            }
            .is_empty()
        );
        assert!(
            !LogicalRect {
                min: LogicalPoint { x: 0.0, y: 0.0 },
                max: LogicalPoint { x: 1.0, y: 1.0 },
            }
            .is_empty()
        );
    }

    #[test]
    fn logical_rect_contains() {
        let rect = LogicalRect {
            min: LogicalPoint { x: 0.0, y: 0.0 },
            max: LogicalPoint { x: 100.0, y: 100.0 },
        };

        assert!(rect.contains(LogicalPoint { x: 0.0, y: 0.0 }));
        assert!(rect.contains(LogicalPoint { x: 50.0, y: 50.0 }));
        assert!(rect.contains(LogicalPoint { x: 99.9, y: 99.9 }));

        assert!(!rect.contains(LogicalPoint { x: 100.0, y: 50.0 }));
        assert!(!rect.contains(LogicalPoint { x: 50.0, y: 100.0 }));
        assert!(!rect.contains(LogicalPoint { x: -1.0, y: 50.0 }));
    }

    #[test]
    fn logical_rect_to_physical() {
        let logical = LogicalRect {
            min: LogicalPoint { x: 1.5, y: 2.5 },
            max: LogicalPoint { x: 4.5, y: 6.5 },
        };
        let physical = logical.to_physical(2.0);

        assert_eq!(physical.min.x, 3);
        assert_eq!(physical.min.y, 5);
        assert_eq!(physical.max.x, 9);
        assert_eq!(physical.max.y, 13);
    }

    #[test]
    fn color_from_hex_rgb() {
        let c = Color::from_hex("#FF5722").unwrap();
        assert_eq!(c.r, 0xFF);
        assert_eq!(c.g, 0x57);
        assert_eq!(c.b, 0x22);
        assert_eq!(c.a, 255);
    }

    #[test]
    fn color_from_hex_rgba() {
        let c = Color::from_hex("#AABBCCDD").unwrap();
        assert_eq!(c.r, 0xAA);
        assert_eq!(c.g, 0xBB);
        assert_eq!(c.b, 0xCC);
        assert_eq!(c.a, 0xDD);
    }

    #[test]
    fn color_from_hex_without_hash() {
        let c = Color::from_hex("FF5722").unwrap();
        assert_eq!(c, Color::from_hex("#FF5722").unwrap());
    }

    #[test]
    fn color_from_hex_invalid_length() {
        assert!(Color::from_hex("#FFF").is_err());
        assert!(Color::from_hex("#FF5722AA00").is_err());
    }

    #[test]
    fn color_from_hex_invalid_chars() {
        assert!(Color::from_hex("#GGHHII").is_err());
    }

    #[test]
    fn color_constants() {
        assert_eq!(Color::BLACK, Color { r: 0, g: 0, b: 0, a: 255 });
        assert_eq!(
            Color::WHITE,
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255
            }
        );
        assert_eq!(
            Color::TRANSPARENT,
            Color { r: 0, g: 0, b: 0, a: 0 }
        );
    }
}
