//! Domain-specific types for DICOM metadata

use dicom::transfer_syntax::entries;
use std::fmt;

/// DICOM transfer syntax (UID, name)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSyntax {
    pub uid: String,
    pub name: String,
}

impl TransferSyntax {
    #[must_use]
    pub fn new(uid: String, name: String) -> Self {
        Self { uid, name }
    }

    #[inline]
    #[must_use]
    pub fn is_big_endian(&self) -> bool {
        self.uid == entries::EXPLICIT_VR_BIG_ENDIAN.uid()
    }
}

impl fmt::Display for TransferSyntax {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{name} ({uid})", name = self.name, uid = self.uid)
    }
}

/// SOP Class (UID, name)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SOPClass {
    pub uid: String,
    pub name: String,
}

impl SOPClass {
    #[must_use]
    pub fn new(uid: String, name: String) -> Self {
        Self { uid, name }
    }
}

impl fmt::Display for SOPClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{name} ({uid})", name = self.name, uid = self.uid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    pub rows: u16,
    pub cols: u16,
}

impl Dimensions {
    #[must_use]
    pub fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }

    #[inline]
    #[must_use]
    pub fn pixel_count(&self) -> usize {
        usize::from(self.rows) * usize::from(self.cols)
    }

    #[inline]
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
    #[must_use]
    pub fn new(slope: f64, intercept: f64) -> Self {
        Self { slope, intercept }
    }

    #[must_use]
    pub const fn default() -> Self {
        Self {
            slope: 1.0,
            intercept: 0.0,
        }
    }

    #[inline(always)]
    #[must_use]
    // Hot path: called for every pixel during conversion
    pub fn apply(&self, pixel: u16) -> f64 {
        f64::from(pixel).mul_add(self.slope, self.intercept)
    }
}

impl fmt::Display for RescaleParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "slope={slope}, intercept={intercept}",
            slope = self.slope,
            intercept = self.intercept
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelAspectRatio {
    pub vertical: f64,
    pub horizontal: f64,
}

impl PixelAspectRatio {
    #[must_use]
    pub fn new(vertical: f64, horizontal: f64) -> Self {
        Self {
            vertical,
            horizontal,
        }
    }

    #[inline]
    #[must_use]
    pub fn ratio(&self) -> f64 {
        self.vertical / self.horizontal
    }

    #[inline]
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

/// Bit depth information for pixel data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitDepth {
    pub allocated: u16,
    pub stored: u16,
}

impl BitDepth {
    #[must_use]
    pub fn new(allocated: u16, stored: u16) -> Self {
        Self { allocated, stored }
    }

    #[inline]
    #[must_use]
    pub fn bytes_per_pixel(&self) -> u16 {
        self.allocated / 8
    }

    #[inline]
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.stored <= self.allocated && self.allocated <= 16
    }
}

impl fmt::Display for BitDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{stored}/{allocated} bits",
            stored = self.stored,
            allocated = self.allocated
        )
    }
}

/// Patient information metadata
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatientInfo {
    pub name: Option<String>,
    pub id: Option<String>,
    pub birth_date: Option<String>,
}

impl PatientInfo {
    #[must_use]
    pub fn new() -> Self {
        Self {
            name: None,
            id: None,
            birth_date: None,
        }
    }

    #[must_use]
    pub fn has_info(&self) -> bool {
        self.name.is_some() || self.id.is_some() || self.birth_date.is_some()
    }
}

impl Default for PatientInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Study information metadata
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudyInfo {
    pub accession_number: Option<String>,
    pub date: Option<String>,
    pub description: Option<String>,
    pub modality: Option<String>,
}

impl StudyInfo {
    #[must_use]
    pub fn new() -> Self {
        Self {
            accession_number: None,
            date: None,
            description: None,
            modality: None,
        }
    }

    #[must_use]
    pub fn has_info(&self) -> bool {
        self.accession_number.is_some()
            || self.date.is_some()
            || self.description.is_some()
            || self.modality.is_some()
    }
}

impl Default for StudyInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Series information metadata
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesInfo {
    pub description: Option<String>,
    pub slice_thickness: Option<f64>,
}

impl SeriesInfo {
    #[must_use]
    pub fn new() -> Self {
        Self {
            description: None,
            slice_thickness: None,
        }
    }

    #[must_use]
    pub fn has_info(&self) -> bool {
        self.description.is_some() || self.slice_thickness.is_some()
    }
}

impl Default for SeriesInfo {
    fn default() -> Self {
        Self::new()
    }
}
