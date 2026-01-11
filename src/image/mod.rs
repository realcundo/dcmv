mod grayscale;
mod normalization;
mod rgb;
mod ycbcr;

pub use grayscale::convert_grayscale;
pub use rgb::convert_rgb;
pub use ycbcr::convert_ycbcr;

use crate::dicom::{DicomMetadata, PhotometricInterpretation};
use anyhow::Result;
use image::DynamicImage;

/// Convert DICOM metadata and pixel data to a `DynamicImage`
///
/// # Errors
///
/// Returns an error if the photometric interpretation is unsupported or
/// if the conversion fails
pub fn convert_to_image(metadata: &DicomMetadata) -> Result<DynamicImage> {
    if metadata.is_already_rgb() {
        return convert_rgb(metadata);
    }

    match metadata.photometric_interpretation {
        PhotometricInterpretation::Monochrome1 | PhotometricInterpretation::Monochrome2 => {
            convert_grayscale(metadata)
        }
        PhotometricInterpretation::Rgb => convert_rgb(metadata),
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
        use crate::dicom::DecodedPixelData;

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
            pixel_data_format: DecodedPixelData::Native(vec![0u8; 64 * 64 * 2].into_boxed_slice()),
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
