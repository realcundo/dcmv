//! Photometric interpretation (color space)

use std::fmt::Display;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhotometricInterpretation {
    Monochrome1,
    Monochrome2,
    Rgb,
    YbrFull,
    YbrFull422,
    // TODO: Implement Palette color support
    // Failing files: examples_palette.dcm (2 instances)
    // Requires reading Palette Color Lookup Table (CLUT) data and mapping pixel values through it
    Palette,
    // TODO: Implement YBR_RCT (reversible color transform) support
    // Failing files: GDCMJ2K_TextGBR.dcm, examples_jpeg2k.dcm
    // YBR_RCT is used with JPEG2000 compression, needs conversion similar to YBR_FULL
    YbrRct,
    Unknown(String),
}

impl FromStr for PhotometricInterpretation {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim() {
            "MONOCHROME1" => Self::Monochrome1,
            "MONOCHROME2" => Self::Monochrome2,
            "RGB" => Self::Rgb,
            "YBR_FULL" => Self::YbrFull,
            "YBR_FULL_422" => Self::YbrFull422,
            "PALETTE COLOR" => Self::Palette,
            "YBR_RCT" => Self::YbrRct,
            other => Self::Unknown(other.to_string()),
        })
    }
}

impl PhotometricInterpretation {
    #[inline(always)]
    #[must_use]
    pub fn is_grayscale(&self) -> bool {
        matches!(self, Self::Monochrome1 | Self::Monochrome2)
    }

    #[inline(always)]
    #[must_use]
    pub fn is_rgb(&self) -> bool {
        matches!(self, Self::Rgb)
    }

    #[inline(always)]
    #[must_use]
    pub fn is_ycbcr(&self) -> bool {
        matches!(self, Self::YbrFull | Self::YbrFull422 | Self::YbrRct)
    }

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
            Self::YbrRct => write!(f, "YBR_RCT"),
            Self::Unknown(s) => write!(f, "{s}"),
        }
    }
}
