//! DICOM metadata structure

use crate::types::*;
use super::photometric::PhotometricInterpretation;
use super::pixel_data::DecodedPixelData;

/// DICOM image metadata extracted from the file
#[derive(Debug, Clone)]
pub struct DicomMetadata {
    // Image dimensions and pixel data
    pub dimensions: Dimensions,
    pub rescale: RescaleParams,
    pub pixel_aspect_ratio: Option<PixelAspectRatio>,
    pub number_of_frames: u32, // Number of frames (default 1 for single-frame)

    // Photometric interpretation and color space
    pub photometric_interpretation: PhotometricInterpretation,
    pub samples_per_pixel: u16,        // 1 for grayscale, 3 for RGB
    pub bits_allocated: u16,            // 8, 16, or 32
    pub bits_stored: u16,               // Actual bits used (<= bits_allocated)
    pub planar_configuration: Option<u16>, // 0 = interleaved, 1 = planar (RGB only)

    // Pixel data with format information
    pub(crate) pixel_data_format: DecodedPixelData,

    // Display metadata fields
    pub patient_name: Option<String>,
    pub patient_id: Option<String>,
    pub patient_birth_date: Option<String>,
    pub accession_number: Option<String>,
    pub study_date: Option<String>,
    pub study_description: Option<String>,
    pub modality: Option<String>,
    pub series_description: Option<String>,
    pub slice_thickness: Option<f64>,

    // Technical metadata
    pub sop_class: Option<SOPClass>,
    pub transfer_syntax: TransferSyntax,
}

// Convenience methods for backward compatibility
impl DicomMetadata {
    #[inline(always)]
    #[must_use]
    pub fn rows(&self) -> u16 {
        self.dimensions.rows
    }

    #[inline(always)]
    #[must_use]
    pub fn cols(&self) -> u16 {
        self.dimensions.cols
    }

    #[inline(always)]
    #[must_use]
    pub fn rescale_slope(&self) -> f64 {
        self.rescale.slope
    }

    #[inline(always)]
    #[must_use]
    pub fn rescale_intercept(&self) -> f64 {
        self.rescale.intercept
    }

    /// Returns true if this DICOM file uses big-endian byte order
    #[inline(always)]
    #[must_use]
    #[allow(deprecated)] // Explicit VR Big Endian is retired but still in use
    pub fn is_big_endian(&self) -> bool {
        self.transfer_syntax.is_big_endian()
    }

    /// Get the pixel data as a byte slice
    #[inline(always)]
    #[must_use]
    pub fn pixel_data(&self) -> &[u8] {
        match &self.pixel_data_format {
            DecodedPixelData::YcbCr(data) | DecodedPixelData::Rgb(data) | DecodedPixelData::Native(data) => data,
        }
    }

    /// Returns true if the pixel data is already in RGB format
    /// (e.g., JPEG decoder already converted YCbCr to RGB)
    #[inline(always)]
    #[must_use]
    pub fn is_already_rgb(&self) -> bool {
        matches!(self.pixel_data_format, DecodedPixelData::Rgb(_))
    }
}
