//! Image conversion module
//!
//! This module handles conversion of DICOM pixel data to RGB images,
//! supporting various photometric interpretations and bit depths.

mod normalization;
mod grayscale;
mod rgb;
mod ycbcr;

// Re-export public API for backward compatibility
pub use grayscale::convert_grayscale;
pub use rgb::convert_rgb;
pub use ycbcr::convert_ycbcr;

use anyhow::Result;
use image::DynamicImage;
use crate::dicom::{DicomMetadata, PhotometricInterpretation};

/// Convert DICOM pixel data to a DynamicImage
///
/// This is the main entry point for image conversion, dispatching to the
/// appropriate conversion function based on the photometric interpretation.
pub fn convert_to_image(metadata: &DicomMetadata) -> Result<DynamicImage> {
    match metadata.photometric_interpretation {
        PhotometricInterpretation::Monochrome1 | PhotometricInterpretation::Monochrome2 => {
            convert_grayscale(metadata)
        }
        PhotometricInterpretation::Rgb => {
            convert_rgb(metadata)
        }
        PhotometricInterpretation::YbrFull | PhotometricInterpretation::YbrFull422 => {
            convert_ycbcr(metadata)
        }
        _ => {
            anyhow::bail!(
                "Unsupported photometric interpretation: {:?}",
                metadata.photometric_interpretation
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_grayscale_dispatch() {
        // Test that grayscale photometric interpretations dispatch correctly
        // This is a compile-time check that the module structure is correct
        let metadata = DicomMetadata {
            dimensions: crate::types::Dimensions::new(64, 64),
            rescale: crate::types::RescaleParams::new(1.0, 0.0),
            pixel_aspect_ratio: None,
            number_of_frames: 1,
            photometric_interpretation: PhotometricInterpretation::Monochrome2,
            samples_per_pixel: 1,
            bits_allocated: 16,
            bits_stored: 16,
            planar_configuration: None,
            pixel_data: vec![0u8; 64 * 64 * 2],
            patient_name: None,
            patient_id: None,
            patient_birth_date: None,
            accession_number: None,
            study_date: None,
            study_description: None,
            modality: None,
            series_description: None,
            slice_thickness: None,
            sop_class: None,
            transfer_syntax: crate::types::TransferSyntax::new(
                "1.2.840.10008.1.2".to_string(),
                "Implicit VR Little Endian".to_string(),
            ),
        };

        // This should not compile if the dispatch is broken
        let _ = convert_grayscale(&metadata);
    }
}
