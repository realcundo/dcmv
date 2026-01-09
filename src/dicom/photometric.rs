//! Photometric interpretation (color space) definitions

use std::fmt::Display;
use std::str::FromStr;

/// Photometric interpretation describes the color space of pixel data
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhotometricInterpretation {
    /// Grayscale where min value = white, max value = black
    Monochrome1,
    /// Grayscale where min value = black, max value = white
    Monochrome2,
    /// RGB color space (interleaved or planar)
    Rgb,
    /// RGB stored in YCbCr color space
    YbrFull,
    /// YCbCr for JPEG with 4:2:2 subsampling
    YbrFull422,
    /// Palette color (requires lookup table)
    Palette,
    /// Unknown photometric interpretation
    Unknown(String),
}

impl FromStr for PhotometricInterpretation {
    type Err = ();

    /// Parse photometric interpretation from DICOM string
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim() {
            "MONOCHROME1" => Self::Monochrome1,
            "MONOCHROME2" => Self::Monochrome2,
            "RGB" => Self::Rgb,
            "YBR_FULL" => Self::YbrFull,
            "YBR_FULL_422" => Self::YbrFull422,
            "PALETTE COLOR" => Self::Palette,
            other => Self::Unknown(other.to_string()),
        })
    }
}

impl PhotometricInterpretation {
    /// Check if this is a grayscale interpretation
    #[inline(always)]
    #[must_use]
    pub fn is_grayscale(&self) -> bool {
        matches!(self, Self::Monochrome1 | Self::Monochrome2)
    }

    /// Check if this is an RGB interpretation
    #[inline(always)]
    #[must_use]
    pub fn is_rgb(&self) -> bool {
        matches!(self, Self::Rgb)
    }

    /// Check if this is a YCbCr interpretation
    #[inline(always)]
    #[must_use]
    pub fn is_ycbcr(&self) -> bool {
        matches!(self, Self::YbrFull | Self::YbrFull422)
    }

    /// Check if pixel values should be inverted (MONOCHROME1)
    #[inline(always)]
    #[must_use]
    pub fn should_invert(&self) -> bool {
        matches!(self, Self::Monochrome1)
    }
}

impl Display for PhotometricInterpretation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Monochrome1 => write!(f, "MONOCHROME1"),
            Self::Monochrome2 => write!(f, "MONOCHROME2"),
            Self::Rgb => write!(f, "RGB"),
            Self::YbrFull => write!(f, "YBR_FULL"),
            Self::YbrFull422 => write!(f, "YBR_FULL_422"),
            Self::Palette => write!(f, "PALETTE COLOR"),
            Self::Unknown(s) => write!(f, "{s}"),
        }
    }
}
