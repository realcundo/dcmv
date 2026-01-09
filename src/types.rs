//! Domain-specific types for DICOM metadata
//!
//! This module provides strongly-typed wrappers for DICOM concepts that were
//! previously represented as tuples or primitive types, improving type safety
//! and code clarity.

use std::fmt;

/// DICOM transfer syntax with proper type instead of (String, String)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSyntax {
    pub uid: String,
    pub name: String,
}

impl TransferSyntax {
    pub fn new(uid: String, name: String) -> Self {
        Self { uid, name }
    }

    /// Check if this is big-endian transfer syntax
    #[inline(always)]
    #[must_use]
    pub fn is_big_endian(&self) -> bool {
        self.uid == "1.2.840.10008.1.2.2" // Explicit VR Big Endian
    }

    /// Check if this uses JPEG compression
    #[inline(always)]
    #[must_use]
    pub fn is_jpeg_compressed(&self) -> bool {
        self.uid.contains("1.2.840.10008.1.2.4.50") || // JPEG Baseline
            self.uid.contains("1.2.840.10008.1.2.4") // JPEG family
    }

    /// Check if this uses JPEG2000 compression
    #[inline(always)]
    #[must_use]
    pub fn is_jpeg2000(&self) -> bool {
        self.uid.contains("JPEG2000")
    }

    /// Check if this uses RLE compression
    #[inline(always)]
    #[must_use]
    pub fn is_rle_compressed(&self) -> bool {
        self.uid.contains("1.2.840.10008.1.2.5") // RLE lossless
    }

    /// Check if this is compressed (any compression type)
    #[inline(always)]
    #[must_use]
    pub fn is_compressed(&self) -> bool {
        self.is_jpeg_compressed() || self.is_jpeg2000() || self.is_rle_compressed()
    }
}

impl fmt::Display for TransferSyntax {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{name} ({uid})", name = self.name, uid = self.uid)
    }
}

/// SOP Class with proper type instead of Option<(String, String)>
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SOPClass {
    pub uid: String,
    pub name: String,
}

impl SOPClass {
    pub fn new(uid: String, name: String) -> Self {
        Self { uid, name }
    }
}

impl fmt::Display for SOPClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{name} ({uid})", name = self.name, uid = self.uid)
    }
}

/// Image dimensions with validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    pub rows: u16,
    pub cols: u16,
}

impl Dimensions {
    pub fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }

    #[inline(always)]
    #[must_use]
    pub fn pixel_count(&self) -> usize {
        usize::from(self.rows) * usize::from(self.cols)
    }

    #[inline(always)]
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.rows > 0 && self.cols > 0
    }
}

impl fmt::Display for Dimensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{cols}x{rows}", cols = self.cols, rows = self.rows)
    }
}

/// Rescale parameters for converting pixel values to real units
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RescaleParams {
    pub slope: f64,
    pub intercept: f64,
}

impl RescaleParams {
    pub fn new(slope: f64, intercept: f64) -> Self {
        Self { slope, intercept }
    }

    /// Default rescale parameters (no transformation)
    #[must_use]
    pub const fn default() -> Self {
        Self {
            slope: 1.0,
            intercept: 0.0,
        }
    }

    /// Apply rescale to a pixel value
    #[inline(always)]
    #[must_use]
    pub fn apply(&self, pixel: u16) -> f64 {
        f64::from(pixel).mul_add(self.slope, self.intercept)
    }
}

impl fmt::Display for RescaleParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "slope={slope}, intercept={intercept}", slope = self.slope, intercept = self.intercept)
    }
}

/// Pixel aspect ratio (vertical:horizontal)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelAspectRatio {
    pub vertical: f64,
    pub horizontal: f64,
}

impl PixelAspectRatio {
    pub fn new(vertical: f64, horizontal: f64) -> Self {
        Self {
            vertical,
            horizontal,
        }
    }

    /// Get the ratio as vertical/horizontal
    #[inline(always)]
    #[must_use]
    pub fn ratio(&self) -> f64 {
        self.vertical / self.horizontal
    }

    /// Check if pixels are square (1:1)
    #[inline(always)]
    #[must_use]
    pub fn is_square(&self) -> bool {
        (self.vertical - self.horizontal).abs() < f64::EPSILON
    }
}

impl fmt::Display for PixelAspectRatio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{vertical}:{horizontal}",
            vertical = self.vertical,
            horizontal = self.horizontal
        )
    }
}
