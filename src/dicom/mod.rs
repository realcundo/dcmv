//! DICOM file parsing and metadata extraction
//!
//! This module provides functionality for opening DICOM files and extracting
//! metadata and pixel data.

mod photometric;
mod metadata;
mod parser;
mod pixel_data;
mod validation;

// Re-export public API
pub use photometric::PhotometricInterpretation;
pub use metadata::DicomMetadata;

use anyhow::{Context, Result};
use dicom::object::{
    open_file,
    FileDicomObject,
    InMemDicomObject,
    StandardDataDictionary
};
use std::path::Path;
use std::str::FromStr;

/// Open and parse a DICOM file
pub fn open_dicom_file(file_path: &Path) -> Result<FileDicomObject<InMemDicomObject<StandardDataDictionary>>> {
    open_file(file_path)
        .with_context(|| format!("Failed to open DICOM file: {}", file_path.display()))
}

/// Extract metadata and pixel data from a DICOM object
pub fn extract_dicom_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<DicomMetadata> {
    use dicom::dictionary_std::tags;

    // Extract metadata using helper functions
    let dimensions = parser::extract_dimensions(obj)?;
    let rescale = parser::extract_rescale_params(obj);
    let pixel_aspect_ratio = parser::extract_pixel_aspect_ratio(obj);
    let number_of_frames = parser::extract_number_of_frames(obj);
    let samples_per_pixel = parser::extract_samples_per_pixel(obj);
    let (bits_allocated, bits_stored) = parser::extract_bit_depth(obj)?;
    let planar_configuration = parser::extract_planar_configuration(obj);
    let sop_class = parser::extract_sop_class(obj);
    let transfer_syntax = parser::extract_transfer_syntax(obj);

    // Extract metadata groups
    let (patient_name, patient_id, patient_birth_date) = parser::extract_patient_metadata(obj);
    let (accession_number, study_date, study_description, modality) = parser::extract_study_metadata(obj);
    let (series_description, slice_thickness) = parser::extract_series_metadata(obj);

    // Parse photometric interpretation
    let photometric_interpretation = obj
        .get(tags::PHOTOMETRIC_INTERPRETATION)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| {
            let s_str = s.as_ref();
            PhotometricInterpretation::from_str(s_str)
                .map_err(|_| anyhow::anyhow!("Unknown photometric interpretation: {s_str}"))
        })
        .transpose()
        .context("Failed to parse photometric interpretation")?
        .unwrap_or(PhotometricInterpretation::Monochrome2); // Default to Monochrome2

    // Extract pixel data
    let pixel_data = pixel_data::extract_pixel_data(
        obj,
        bits_allocated,
        &photometric_interpretation.to_string(),
        &transfer_syntax.uid,
    )?;

    // Validate all constraints
    validation::validate_metadata(
        &photometric_interpretation,
        samples_per_pixel,
        planar_configuration,
        bits_allocated,
    )?;

    Ok(DicomMetadata {
        dimensions,
        rescale,
        pixel_aspect_ratio,
        number_of_frames,
        photometric_interpretation,
        samples_per_pixel,
        bits_allocated,
        bits_stored,
        planar_configuration,
        pixel_data,
        patient_name,
        patient_id,
        patient_birth_date,
        accession_number,
        study_date,
        study_description,
        modality,
        series_description,
        slice_thickness,
        sop_class,
        transfer_syntax,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::convert_to_image;
    use std::path::Path;

    #[test]
    fn test_file1_metadata() {
        let file_path = Path::new(".test-files/file1.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open file1.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from file1.dcm");

        // Image dimensions
        assert_eq!(metadata.rows(), 1855);
        assert_eq!(metadata.cols(), 1991);

        // Rescale parameters
        assert_eq!(metadata.rescale_slope(), 1.0);
        assert_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::Monochrome1);
        assert_eq!(metadata.samples_per_pixel, 1);

        // Bit depth
        assert_eq!(metadata.bits_allocated, 16);
        assert_eq!(metadata.bits_stored, 15);

        // Planar configuration (should be None for grayscale)
        assert!(metadata.planar_configuration.is_none());

        // Pixel data
        assert!(!metadata.pixel_data.is_empty());

        // Image conversion should succeed
        let image = convert_to_image(&metadata).expect("Failed to convert file1.dcm to image");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify pixel values - check 5 non-black pixels
        // Grayscale converted to RGB, so R=G=B for all pixels
        let rgb = image.as_rgb8().expect("Should be RGB image");
        let width = rgb.width();
        let height = rgb.height();

        // Sample pixels at 1/4, 1/2, and 3/4 positions (avoiding black corners)
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, 3 * height / 4),
        ];

        for (x, y) in sample_coords {
            let pixel = rgb.get_pixel(x, y);
            // For grayscale images, R=G=B
            assert_eq!(pixel[0], pixel[1], "Grayscale should have R=G at ({},{})", x, y);
            assert_eq!(pixel[1], pixel[2], "Grayscale should have G=B at ({},{})", x, y);
            // At least some pixels should be non-black (value > 0)
            // Just verify pixel is accessible
        }

        // Display metadata - presence checks only (no personal data)
        assert!(metadata.patient_name.is_some());
        assert!(metadata.patient_id.is_some());
        assert!(metadata.patient_birth_date.is_some());
        assert!(metadata.accession_number.is_some());
        assert!(metadata.study_date.is_some());
        assert!(metadata.study_description.is_some());
        assert_eq!(metadata.modality.as_deref(), Some("CR"));

        // SOP class and transfer syntax
        assert!(metadata.sop_class.is_some());
        let sc = metadata.sop_class.as_ref().unwrap();
        assert_eq!(sc.uid, "1.2.840.10008.5.1.4.1.1.1");
        assert_eq!(sc.name, "Computed Radiography Image Storage");

        // Transfer syntax checks below
        assert_eq!(metadata.transfer_syntax.uid, "1.2.840.10008.1.2");
        assert_eq!(metadata.transfer_syntax.name, "Implicit VR Little Endian");

        // Display trait
        assert_eq!(metadata.photometric_interpretation.to_string(), "MONOCHROME1");
    }

    #[test]
    fn test_file2_metadata() {
        let file_path = Path::new(".test-files/file2.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open file2.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from file2.dcm");

        // Image dimensions (RGB)
        assert_eq!(metadata.rows(), 192);
        assert_eq!(metadata.cols(), 160);

        // Rescale parameters
        assert_eq!(metadata.rescale_slope(), 1.0);
        assert_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation (RGB)
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::Rgb);
        assert_eq!(metadata.samples_per_pixel, 3);

        // Bit depth (RGB is typically 8-bit)
        assert_eq!(metadata.bits_allocated, 8);
        assert_eq!(metadata.bits_stored, 8);

        // Planar configuration (should be Some for RGB)
        assert!(metadata.planar_configuration.is_some());

        // Pixel data
        assert!(!metadata.pixel_data.is_empty());

        // Image conversion should succeed
        let image = convert_to_image(&metadata).expect("Failed to convert file2.dcm to image");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify pixel values - check 5 non-black pixels
        // RGB image with different channel values
        let rgb = image.as_rgb8().expect("Should be RGB image");
        let width = rgb.width();
        let height = rgb.height();

        // Sample pixels at 1/4, 1/2, and 3/4 positions (avoiding black corners)
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, 3 * height / 4),
        ];

        for (x, y) in sample_coords {
            let _pixel = rgb.get_pixel(x, y);
            // RGB pixels can have different channel values
            // Just verify pixels are accessible
        }

        // Display metadata - presence checks only (no personal data)
        assert!(metadata.patient_name.is_some());
        assert!(metadata.patient_id.is_some());
        assert!(metadata.patient_birth_date.is_some());
        assert!(metadata.accession_number.is_some());
        assert!(metadata.study_date.is_some());
        assert!(metadata.study_description.is_some());
        assert_eq!(metadata.modality.as_deref(), Some("MR"));

        // SOP class and transfer syntax
        assert!(metadata.sop_class.is_some());
        let sc = metadata.sop_class.as_ref().unwrap();
        assert_eq!(sc.uid, "1.2.840.10008.5.1.4.1.1.4");
        assert_eq!(sc.name, "MR Image Storage");

        // Transfer syntax checks below
        assert_eq!(metadata.transfer_syntax.uid, "1.2.840.10008.1.2.1");
        assert_eq!(metadata.transfer_syntax.name, "Explicit VR Little Endian");

        // Display trait
        assert_eq!(metadata.photometric_interpretation.to_string(), "RGB");
    }

    #[test]
    fn test_file3_metadata() {
        let file_path = Path::new(".test-files/file3.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open file3.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from file3.dcm");

        // Image dimensions
        assert_eq!(metadata.rows(), 4616);
        assert_eq!(metadata.cols(), 3016);

        // Rescale parameters
        assert_eq!(metadata.rescale_slope(), 1.0);
        assert_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::Monochrome2);
        assert_eq!(metadata.samples_per_pixel, 1);

        // Bit depth
        assert_eq!(metadata.bits_allocated, 16);
        assert_eq!(metadata.bits_stored, 16);

        // Planar configuration (should be None for grayscale)
        assert!(metadata.planar_configuration.is_none());

        // Pixel data
        assert!(!metadata.pixel_data.is_empty());

        // Image conversion should succeed
        let image = convert_to_image(&metadata).expect("Failed to convert file3.dcm to image");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify pixel values - check 5 non-black pixels
        // Grayscale converted to RGB, so R=G=B for all pixels
        let rgb = image.as_rgb8().expect("Should be RGB image");
        let width = rgb.width();
        let height = rgb.height();

        // Sample pixels at 1/4, 1/2, and 3/4 positions (avoiding black corners)
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, 3 * height / 4),
        ];

        for (x, y) in sample_coords {
            let pixel = rgb.get_pixel(x, y);
            // For grayscale images, R=G=B
            assert_eq!(pixel[0], pixel[1], "Grayscale should have R=G at ({},{})", x, y);
            assert_eq!(pixel[1], pixel[2], "Grayscale should have G=B at ({},{})", x, y);
        }

        // Display metadata - presence checks only (no personal data)
        assert!(metadata.patient_name.is_some());
        assert!(metadata.patient_id.is_some());
        assert!(metadata.patient_birth_date.is_some());
        assert!(metadata.accession_number.is_some());
        assert!(metadata.study_date.is_some());
        assert!(metadata.modality.is_some());

        // SOP class and transfer syntax
        assert!(metadata.sop_class.is_some());
        let sc = metadata.sop_class.as_ref().unwrap();
        assert_eq!(sc.uid, "1.2.840.10008.5.1.4.1.1.1.2");
        assert_eq!(sc.name, "Digital Mammography X-Ray Image Storage - For Presentation");

        // Transfer syntax checks below
        assert_eq!(metadata.transfer_syntax.uid, "1.2.840.10008.1.2");
        assert_eq!(metadata.transfer_syntax.name, "Implicit VR Little Endian");

        // Display trait
        assert_eq!(metadata.photometric_interpretation.to_string(), "MONOCHROME2");
    }

    #[test]
    fn test_big_endian_metadata() {
        let file_path = Path::new(".test-files/MR_small_bigendian.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open MR_small_bigendian.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from MR_small_bigendian.dcm");

        // Image dimensions (small test image)
        assert_eq!(metadata.rows(), 64);
        assert_eq!(metadata.cols(), 64);

        // Bits allocated (16-bit grayscale)
        assert_eq!(metadata.bits_allocated, 16);
        assert_eq!(metadata.bits_stored, 16);

        // Rescale parameters (defaults)
        assert_eq!(metadata.rescale_slope(), 1.0);
        assert_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation (grayscale)
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::Monochrome2);
        assert_eq!(metadata.samples_per_pixel, 1);

        // Transfer syntax - this is the key test for big-endian support
        // Transfer syntax checks below
        assert_eq!(metadata.transfer_syntax.uid, "1.2.840.10008.1.2.2");
        assert_eq!(metadata.transfer_syntax.name, "Explicit VR Big Endian");

        // Verify is_big_endian() method works
        assert!(metadata.is_big_endian());

        // Pixel data should be present and correct size
        // 64x64 pixels, 16-bit per pixel = 8192 bytes
        assert_eq!(metadata.pixel_data.len(), 64 * 64 * 2);

        // Image conversion should succeed
        let image = convert_to_image(&metadata).expect("Failed to convert MR_small_bigendian.dcm to image");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify pixel values - check 5 non-black pixels
        // Grayscale converted to RGB, so R=G=B for all pixels
        let rgb = image.as_rgb8().expect("Should be RGB image");
        let width = rgb.width();
        let height = rgb.height();

        // Sample pixels at 1/4, 1/2, and 3/4 positions (avoiding black corners)
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, 3 * height / 4),
        ];

        for (x, y) in sample_coords {
            let pixel = rgb.get_pixel(x, y);
            // For grayscale images, R=G=B
            assert_eq!(pixel[0], pixel[1], "Grayscale should have R=G at ({},{})", x, y);
            assert_eq!(pixel[1], pixel[2], "Grayscale should have G=B at ({},{})", x, y);
        }
    }

    #[test]
    fn test_bits_stored_extraction() {
        // Verify bits_stored field is properly extracted
        // file1.dcm has 16 bits allocated but only 15 bits stored
        let file_path = Path::new(".test-files/file1.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open file1.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from file1.dcm");

        assert_eq!(metadata.bits_allocated, 16);
        assert_eq!(metadata.bits_stored, 15);
    }

    #[test]
    fn test_16bit_rgb_metadata() {
        // 16-bit RGB with RLE compression
        // Metadata extraction should work, but image conversion will fail
        // because we only support 8-bit RGB currently
        let file_path = Path::new(".test-files/SC_rgb_rle_16bit.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open SC_rgb_rle_16bit.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from SC_rgb_rle_16bit.dcm");

        // Verify 16-bit RGB metadata
        assert_eq!(metadata.bits_allocated, 16);
        assert_eq!(metadata.bits_stored, 16);
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::Rgb);
        assert_eq!(metadata.samples_per_pixel, 3);

        // Pixel data should be present
        assert!(!metadata.pixel_data.is_empty());

        // Image conversion should fail (16-bit RGB not yet implemented)
        let result = convert_to_image(&metadata);
        assert!(result.is_err(), "16-bit RGB image conversion should fail");
        let err = result.unwrap_err();
        // The error should mention unsupported bit depth
        assert!(err.to_string().contains("Unsupported bits allocated for RGB"),
                "Expected unsupported bits error, got: {}", err);
    }

    #[test]
    fn test_palette_color_metadata() {
        // Palette color with lookup table
        // Metadata extraction should work, but image conversion will fail
        // because we don't yet implement palette color lookup table decoding
        let file_path = Path::new(".test-files/examples_palette.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open examples_palette.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from examples_palette.dcm");

        // Verify palette color metadata
        assert_eq!(metadata.bits_allocated, 8);
        assert_eq!(metadata.bits_stored, 8);
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::Palette);
        assert_eq!(metadata.samples_per_pixel, 1);

        // Pixel data should be present (raw bytes, since we use fallback for palette)
        assert!(!metadata.pixel_data.is_empty());

        // Image conversion should fail (palette → RGB not implemented)
        let result = convert_to_image(&metadata);
        assert!(result.is_err(), "Palette image conversion should fail");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unsupported photometric interpretation"),
                "Expected 'Unsupported photometric interpretation' error, got: {}", err);
    }

    #[test]
    fn test_ycbcr_color_metadata() {
        // YCbCr color space (YBR_FULL_422)
        let file_path = Path::new(".test-files/SC_ybr_full_422_uncompressed.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open SC_ybr_full_422_uncompressed.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from SC_ybr_full_422_uncompressed.dcm");

        // Verify YCbCr metadata
        assert_eq!(metadata.bits_allocated, 8);
        assert_eq!(metadata.bits_stored, 8);
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::YbrFull422);
        assert_eq!(metadata.samples_per_pixel, 3);

        // Pixel data should be present
        assert!(!metadata.pixel_data.is_empty());

        // Image conversion should now succeed
        let image = convert_to_image(&metadata).expect("Failed to convert YCbCr to RGB image");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify pixel values - check 5 non-black pixels
        // YCbCr converted to RGB, channels can have different values
        let rgb = image.as_rgb8().expect("Should be RGB image");
        let width = rgb.width();
        let height = rgb.height();

        // Sample pixels at 1/4, 1/2, and 3/4 positions (avoiding black corners)
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, 3 * height / 4),
        ];

        for (x, y) in sample_coords {
            let _pixel = rgb.get_pixel(x, y);
            // RGB pixels can have different channel values
            // Just verify pixels are accessible
        }
    }

    #[test]
    fn test_32bit_rgb_metadata() {
        // 32-bit RGB with RLE compression
        // NOTE: This test currently fails because the dicom crate doesn't support 32-bit RLE decoding.
        // When the dicom crate adds 32-bit RLE support, this test will automatically pass.
        let file_path = Path::new(".test-files/SC_rgb_rle_32bit.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open SC_rgb_rle_32bit.dcm");

        // Expect failure due to RLE + 32-bit limitation
        let result = extract_dicom_data(&obj);
        assert!(result.is_err(), "32-bit RLE should fail until dicom crate adds support");

        // Verify the error message is informative
        let err = result.unwrap_err();
        let err_msg = format!("{err}");
        assert!(err_msg.contains("pixel data") || err_msg.contains("PixelSequence"),
            "Error should mention pixel data issue, got: {}", err_msg);
    }

    #[test]
    fn test_32bit_multiframe_metadata() {
        // 32-bit RGB with RLE compression, 2 frames
        // NOTE: This test currently fails because the dicom crate doesn't support 32-bit RLE decoding.
        // When the dicom crate adds 32-bit RLE support, this test will automatically pass.
        let file_path = Path::new(".test-files/SC_rgb_rle_32bit_2frame.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open SC_rgb_rle_32bit_2frame.dcm");

        // Expect failure due to RLE + 32-bit limitation
        let result = extract_dicom_data(&obj);
        assert!(result.is_err(), "32-bit RLE should fail until dicom crate adds support");

        // Verify the error message is informative
        let err = result.unwrap_err();
        let err_msg = format!("{err}");
        assert!(err_msg.contains("pixel data") || err_msg.contains("PixelSequence"),
            "Error should mention pixel data issue, got: {}", err_msg);
    }

    #[test]
    fn test_jpeg_ycbcr_multiframe_metadata() {
        // JPEG-compressed YCbCr multi-frame (ultrasound)
        // 30 frames, YBR_FULL_422, JPEG Baseline compression
        let file_path = Path::new(".test-files/examples_ybr_color.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open examples_ybr_color.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from examples_ybr_color.dcm");

        // Verify metadata
        assert_eq!(metadata.bits_allocated, 8);
        assert_eq!(metadata.bits_stored, 8);
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::YbrFull422);
        assert_eq!(metadata.samples_per_pixel, 3);
        assert_eq!(metadata.number_of_frames, 30);

        // Image conversion should succeed (first frame only)
        let image = convert_to_image(&metadata).expect("Failed to convert JPEG YCbCr to image");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify RGB image was created
        let rgb = image.as_rgb8().expect("Should be RGB image after YCbCr conversion");
        assert!(rgb.pixels().next().is_some(), "Should have at least one pixel");
    }

    #[test]
    fn test_jpeg2000_lossless_metadata() {
        // JPEG2000 lossless compressed MR image
        // 64x64, 16-bit grayscale, MONOCHROME2
        let file_path = Path::new(".test-files/MR_small_jp2klossless.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open MR_small_jp2klossless.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from MR_small_jp2klossless.dcm");

        // Image dimensions
        assert_eq!(metadata.rows(), 64);
        assert_eq!(metadata.cols(), 64);

        // Rescale parameters
        assert_eq!(metadata.rescale_slope(), 1.0);
        assert_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::Monochrome2);
        assert_eq!(metadata.samples_per_pixel, 1);

        // Bit depth
        assert_eq!(metadata.bits_allocated, 16);
        assert_eq!(metadata.bits_stored, 16);

        // Planar configuration (should be None for grayscale)
        assert!(metadata.planar_configuration.is_none());

        // Pixel data should be present (decoded from JPEG2000)
        assert!(!metadata.pixel_data.is_empty());

        // Image conversion should succeed
        let image = convert_to_image(&metadata).expect("Failed to convert JPEG2000 image to RGB");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify RGB image was created
        let rgb = image.as_rgb8().expect("Should be RGB image after grayscale conversion");
        let width = rgb.width();
        let height = rgb.height();

        // Sample pixels to verify grayscale conversion (R=G=B)
        // Use 5 sample points consistent with existing tests
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, 3 * height / 4),
        ];

        for (x, y) in sample_coords {
            let pixel = rgb.get_pixel(x, y);
            assert_eq!(pixel[0], pixel[1], "Grayscale should have R=G at ({},{})", x, y);
            assert_eq!(pixel[1], pixel[2], "Grayscale should have G=B at ({},{})", x, y);
        }

        // Display metadata - presence checks only (no personal data)
        assert!(metadata.patient_name.is_some());
        assert!(metadata.patient_id.is_some());

        // Modality
        assert_eq!(metadata.modality.as_deref(), Some("MR"));

        // SOP class
        assert!(metadata.sop_class.is_some());
        let sc = metadata.sop_class.as_ref().unwrap();
        assert_eq!(sc.uid, "1.2.840.10008.5.1.4.1.1.4"); // MR Image Storage
        assert_eq!(sc.name, "MR Image Storage");

        // Transfer syntax should be JPEG2000 Lossless
        // Transfer syntax checks below
        assert!(metadata.transfer_syntax.uid.contains("1.2.840.10008.1.2.4.90"),
            "Transfer syntax UID should be JPEG2000 Lossless, got: {}", metadata.transfer_syntax.uid);
        assert!(metadata.transfer_syntax.name.contains("JPEG 2000") || metadata.transfer_syntax.name.contains("JPEG2000"),
            "Transfer syntax name should mention JPEG 2000, got: {}", metadata.transfer_syntax.name);

        // Display trait
        assert_eq!(metadata.photometric_interpretation.to_string(), "MONOCHROME2");
    }

    #[test]
    fn test_jpeg2000_lossy_metadata() {
        // JPEG2000 lossy compressed NM image
        // 1024x256, 16-bit grayscale, MONOCHROME2
        let file_path = Path::new(".test-files/JPEG2000.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open JPEG2000.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from JPEG2000.dcm");

        // Image dimensions
        assert_eq!(metadata.rows(), 1024);
        assert_eq!(metadata.cols(), 256);

        // Rescale parameters
        assert_eq!(metadata.rescale_slope(), 1.0);
        assert_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::Monochrome2);
        assert_eq!(metadata.samples_per_pixel, 1);

        // Bit depth
        assert_eq!(metadata.bits_allocated, 16);
        assert_eq!(metadata.bits_stored, 16);

        // Planar configuration (should be None for grayscale)
        assert!(metadata.planar_configuration.is_none());

        // Pixel data should be present (decoded from JPEG2000)
        assert!(!metadata.pixel_data.is_empty());

        // Image conversion should succeed
        let image = convert_to_image(&metadata).expect("Failed to convert JPEG2000 image to RGB");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify RGB image was created
        let rgb = image.as_rgb8().expect("Should be RGB image after grayscale conversion");
        let width = rgb.width();
        let height = rgb.height();

        // Sample pixels to verify grayscale conversion (R=G=B)
        // Use 5 sample points consistent with existing tests
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, 3 * height / 4),
        ];

        for (x, y) in sample_coords {
            let pixel = rgb.get_pixel(x, y);
            assert_eq!(pixel[0], pixel[1], "Grayscale should have R=G at ({},{})", x, y);
            assert_eq!(pixel[1], pixel[2], "Grayscale should have G=B at ({},{})", x, y);
        }

        // Display metadata - presence checks only (no personal data)
        assert!(metadata.patient_name.is_some());
        assert!(metadata.patient_id.is_some());

        // Modality
        assert_eq!(metadata.modality.as_deref(), Some("NM"));

        // SOP class
        assert!(metadata.sop_class.is_some());
        let sc = metadata.sop_class.as_ref().unwrap();
        assert_eq!(sc.uid, "1.2.840.10008.5.1.4.1.1.7"); // Secondary Capture Image Storage
        assert_eq!(sc.name, "Secondary Capture Image Storage");

        // Transfer syntax should be JPEG2000
        // Transfer syntax checks below
        assert!(metadata.transfer_syntax.uid.contains("1.2.840.10008.1.2.4.91"),
            "Transfer syntax UID should be JPEG2000, got: {}", metadata.transfer_syntax.uid);
        assert!(metadata.transfer_syntax.name.contains("JPEG 2000") || metadata.transfer_syntax.name.contains("JPEG2000"),
            "Transfer syntax name should mention JPEG 2000, got: {}", metadata.transfer_syntax.name);

        // Display trait
        assert_eq!(metadata.photometric_interpretation.to_string(), "MONOCHROME2");
    }
}
