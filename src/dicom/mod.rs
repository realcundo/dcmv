//! DICOM file parsing and metadata extraction

mod error;
mod metadata;
mod parser;
mod photometric;
mod pixel_data;
mod validation;

/// Type alias for a parsed DICOM object
///
/// This hides the complex generic type signature from the public API
/// while allowing us to pass parsed DICOM objects between functions.
pub type DicomObject = FileDicomObject<InMemDicomObject<StandardDataDictionary>>;

// Re-export public API
pub use error::ProcessError;
pub use metadata::DicomMetadata;
pub use photometric::PhotometricInterpretation;
pub use pixel_data::DecodedPixelData;

use crate::types::{
    BitDepth, Dimensions, PatientInfo, PixelAspectRatio, RescaleParams, SOPClass, SeriesInfo,
    StudyInfo, TransferSyntax,
};
use anyhow::{anyhow, Context, Result};
use dicom::object::file::ReadPreamble;
use dicom::object::{
    open_file, FileDicomObject, InMemDicomObject, OpenFileOptions, StandardDataDictionary,
};
use std::io::{self, stdout, IsTerminal, Read, Seek, Write};
use std::path::Path;
use std::str::FromStr;

use crossterm::terminal::{Clear, ClearType};
use crossterm::{cursor::MoveToColumn, execute, style::Print};
use tempfile::SpooledTempFile;

/// Open and parse a DICOM file
///
/// # Errors
///
/// Returns an error if the file cannot be read or is not a valid DICOM file
pub fn open_dicom_file(file_path: &Path) -> Result<DicomObject> {
    open_file(file_path)
        .with_context(|| format!("Failed to open DICOM file: {}", file_path.display()))
}

/// Format byte count for progress display
///
/// Returns a human-readable string representation of the byte count,
/// using MB for values >= 1 MB, otherwise kB.
fn format_size(bytes: usize) -> String {
    let mb = bytes as f64 / 1024.0 / 1024.0;
    if mb >= 1.0 {
        format!("{:.1} MB", mb)
    } else {
        format!("{:.1} kB", bytes as f64 / 1024.0)
    }
}

/// Read and parse a DICOM file from stdin
///
/// This function reads DICOM data from stdin with progress display and early
/// validation of the DICOM preamble. Data is read into a spooled temp file
/// that keeps small files in memory and spills large files to disk.
///
/// # Errors
///
/// Returns an error if:
/// - stdin cannot be read
/// - the input is not a valid DICOM file (missing "DICM" magic bytes)
/// - the DICOM file cannot be parsed
pub fn read_stdin() -> Result<DicomObject> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let is_tty = io::stdout().is_terminal();
    const PREAMBLE_SIZE: usize = 128;
    const MAGIC: &[u8] = b"DICM";
    const HEADER_SIZE: usize = PREAMBLE_SIZE + MAGIC.len();

    let mut temp_file = SpooledTempFile::new(32 * 1024 * 1024);

    if is_tty {
        execute!(stdout(), Print("Reading from stdin..."))?;
        stdout().flush()?;
    }

    // Read and validate preamble first for early rejection
    let mut header = [0u8; HEADER_SIZE];
    handle.read_exact(&mut header).with_context(|| {
        "Input is too short to be a valid DICOM file with preamble (expected at least 132 bytes)"
    })?;

    if &header[PREAMBLE_SIZE..] != MAGIC {
        return Err(ProcessError::NotADicomFile(anyhow!(
            "Input is not a valid DICOM file (missing DICM magic bytes)"
        ))
        .into());
    }

    temp_file.write_all(&header)?;
    let mut bytes_read = header.len();

    // Copy remaining data
    let mut chunk = [0u8; 128 * 1024];
    loop {
        let n = handle.read(&mut chunk)?;
        if n == 0 {
            break;
        }

        temp_file.write_all(&chunk[..n])?;
        bytes_read += n;

        if is_tty {
            let size_str = format_size(bytes_read);
            execute!(
                stdout(),
                MoveToColumn(0),
                Clear(ClearType::UntilNewLine),
                Print("Reading from stdin ["),
                Print(&size_str),
                Print("]")
            )?;
            stdout().flush()?;
        }
    }

    if is_tty {
        execute!(stdout(), MoveToColumn(0), Clear(ClearType::CurrentLine))?;
        stdout().flush()?;
    }

    temp_file.rewind()?;

    let dcm = OpenFileOptions::new()
        .read_preamble(ReadPreamble::Always)
        .from_reader(temp_file)?;

    Ok(dcm)
}

/// Common metadata extracted from a DICOM object
///
/// This struct contains all the metadata that is shared between
/// `extract_dicom_data` and `extract_metadata_tags`, avoiding duplication.
struct CommonMetadata {
    dimensions: Dimensions,
    bit_depth: BitDepth,
    photometric_interpretation: PhotometricInterpretation,
    samples_per_pixel: u16,
    planar_configuration: Option<u16>,
    number_of_frames: u32,
    pixel_aspect_ratio: Option<PixelAspectRatio>,
    rescale: RescaleParams,
    patient: PatientInfo,
    study: StudyInfo,
    series: SeriesInfo,
    sop_class: Option<SOPClass>,
    transfer_syntax: TransferSyntax,
}

/// Extract common metadata from a DICOM object
///
/// This helper function avoids duplication between `extract_dicom_data` and
/// `extract_metadata_tags` by extracting all metadata except pixel data.
fn extract_common_metadata(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<CommonMetadata> {
    use dicom::dictionary_std::tags;

    let error_context: parser::ErrorContext = obj.into();
    let dimensions = parser::extract_dimensions(obj, &error_context)?;

    let rescale = parser::extract_rescale_params(obj);
    let pixel_aspect_ratio = parser::extract_pixel_aspect_ratio(obj);
    let number_of_frames = parser::extract_number_of_frames(obj);
    let samples_per_pixel = parser::extract_samples_per_pixel(obj);
    let bit_depth = parser::extract_bit_depth(obj, &error_context)?;
    let planar_configuration = parser::extract_planar_configuration(obj);
    let transfer_syntax = parser::extract_transfer_syntax(obj);

    let patient = parser::extract_patient_info(obj);
    let study = parser::extract_study_info(obj);
    let series = parser::extract_series_info(obj);

    let photometric_interpretation = obj
        .get(tags::PHOTOMETRIC_INTERPRETATION)
        .and_then(|e| e.value().to_str().ok())
        .ok_or_else(|| anyhow::anyhow!(error_context.format_error("Photometric Interpretation")))
        .and_then(|s| {
            let s_str = s.as_ref();
            PhotometricInterpretation::from_str(s_str)
                .map_err(|()| anyhow::anyhow!("Unknown photometric interpretation: {s_str}"))
        })?;

    Ok(CommonMetadata {
        dimensions,
        bit_depth,
        photometric_interpretation,
        samples_per_pixel,
        planar_configuration,
        number_of_frames,
        pixel_aspect_ratio,
        rescale,
        patient,
        study,
        series,
        sop_class: error_context.sop_class,
        transfer_syntax,
    })
}

/// Extract metadata and pixel data from a DICOM object
///
/// # Errors
///
/// Returns an error if required DICOM tags are missing or if the pixel data
/// cannot be decoded
pub fn extract_dicom_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<DicomMetadata> {
    let common = extract_common_metadata(obj)?;

    let pixel_data = pixel_data::extract_pixel_data(
        obj,
        common.bit_depth.allocated,
        &common.photometric_interpretation.to_string(),
        &common.transfer_syntax.uid,
        common.planar_configuration,
    )?;

    validation::validate_metadata(
        &common.photometric_interpretation,
        common.samples_per_pixel,
        common.planar_configuration,
        common.bit_depth.allocated,
    )?;

    Ok(DicomMetadata {
        dimensions: common.dimensions,
        bit_depth: common.bit_depth,
        photometric_interpretation: common.photometric_interpretation,
        samples_per_pixel: common.samples_per_pixel,
        planar_configuration: common.planar_configuration,
        number_of_frames: common.number_of_frames,
        pixel_aspect_ratio: common.pixel_aspect_ratio,
        pixel_data_format: pixel_data,
        rescale: common.rescale,
        patient: common.patient,
        study: common.study,
        series: common.series,
        sop_class: common.sop_class,
        transfer_syntax: common.transfer_syntax,
    })
}

/// Extract metadata tags from a DICOM object, even if pixel data decoding fails
///
/// This is used for verbose error display - we can show available metadata
/// even when pixel data cannot be decoded.
///
/// # Errors
///
/// Returns an error if required DICOM tags for metadata extraction are missing.
/// Note: This function does NOT attempt pixel data decoding.
pub fn extract_metadata_tags(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<DicomMetadata> {
    let common = extract_common_metadata(obj)?;

    // Note: No pixel data extraction here - metadata only
    // Use an empty placeholder for pixel_data_format
    let pixel_data_format = DecodedPixelData::Native(Box::<[u8]>::default());

    Ok(DicomMetadata {
        dimensions: common.dimensions,
        bit_depth: common.bit_depth,
        photometric_interpretation: common.photometric_interpretation,
        samples_per_pixel: common.samples_per_pixel,
        planar_configuration: common.planar_configuration,
        number_of_frames: common.number_of_frames,
        pixel_aspect_ratio: common.pixel_aspect_ratio,
        pixel_data_format,
        rescale: common.rescale,
        patient: common.patient,
        study: common.study,
        series: common.series,
        sop_class: common.sop_class,
        transfer_syntax: common.transfer_syntax,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::convert_to_image;
    use approx::assert_relative_eq;
    use std::path::Path;

    // Type aliases for test helper functions (simplifies complex types)
    type GrayscalePixelSamples = [((u32, u32), u8); 10];
    type RgbPixelSamples = [((u32, u32), (u8, u8, u8)); 10];

    #[test]
    fn test_file1_metadata() {
        let file_path = Path::new(".test-files/file1.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open file1.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from file1.dcm");

        // Image dimensions
        assert_eq!(metadata.rows(), 1855);
        assert_eq!(metadata.cols(), 1991);

        // Rescale parameters
        assert_relative_eq!(metadata.rescale_slope(), 1.0);
        assert_relative_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation
        assert_eq!(
            metadata.photometric_interpretation,
            PhotometricInterpretation::Monochrome1
        );
        assert_eq!(metadata.samples_per_pixel, 1);

        // Bit depth
        assert_eq!(metadata.bits_allocated(), 16);
        assert_eq!(metadata.bits_stored(), 15);

        // Planar configuration (should be None for grayscale)
        assert!(metadata.planar_configuration.is_none());

        // Pixel data
        assert!(!metadata.pixel_data().is_empty());

        // Image conversion should succeed
        let image = convert_to_image(&metadata).expect("Failed to convert file1.dcm to image");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify pixel values - check grayscale consistency
        // Grayscale converted to RGB, so R=G=B for all pixels
        let rgb = image.as_rgb8().expect("Should be RGB image");
        let width = rgb.width();
        let height = rgb.height();

        // Sample 10 pixels to verify grayscale conversion (R=G=B)
        // and to catch decoding regressions
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (3 * width / 4, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, height / 2),
            (width / 4, 3 * height / 4),
            (width / 2, 3 * height / 4),
            (3 * width / 4, 3 * height / 4),
            (width / 2, height / 2 + 10),
        ];

        for (x, y) in sample_coords {
            let pixel = rgb.get_pixel(x, y);
            // For grayscale images, R=G=B
            assert_eq!(pixel[0], pixel[1], "Grayscale should have R=G at ({x},{y})");
            assert_eq!(pixel[1], pixel[2], "Grayscale should have G=B at ({x},{y})");
        }

        // Sample 10 specific pixel values to catch decoding regressions
        let expected_pixels = [
            ((width / 4, height / 4), 173),
            ((width / 2, height / 4), 225),
            ((3 * width / 4, height / 4), 152),
            ((width / 4, height / 2), 143),
            ((width / 2, height / 2), 231),
            ((3 * width / 4, height / 2), 122),
            ((width / 4, 3 * height / 4), 101),
            ((width / 2, 3 * height / 4), 239),
            ((3 * width / 4, 3 * height / 4), 105),
            ((width / 2, height / 2 + 10), 229),
        ];

        assert_grayscale_pixels(rgb, "test_file1_metadata", &expected_pixels);

        // Display metadata - presence checks only (no personal data)
        assert!(metadata.patient_name().is_some());
        assert!(metadata.patient_id().is_some());
        assert!(metadata.patient_birth_date().is_some());
        assert!(metadata.accession_number().is_some());
        assert!(metadata.study_date().is_some());
        assert!(metadata.study_description().is_some());
        assert_eq!(metadata.modality(), Some("CR"));

        // SOP class and transfer syntax
        assert!(metadata.sop_class.is_some());
        let sc = metadata.sop_class.as_ref().unwrap();
        assert_eq!(sc.uid, "1.2.840.10008.5.1.4.1.1.1");
        assert_eq!(sc.name, "Computed Radiography Image Storage");

        // Transfer syntax checks below
        assert_eq!(metadata.transfer_syntax.uid, "1.2.840.10008.1.2");
        assert_eq!(metadata.transfer_syntax.name, "Implicit VR Little Endian");

        // Display trait
        assert_eq!(
            metadata.photometric_interpretation.to_string(),
            "MONOCHROME1"
        );
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
        assert_relative_eq!(metadata.rescale_slope(), 1.0);
        assert_relative_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation (RGB)
        assert_eq!(
            metadata.photometric_interpretation,
            PhotometricInterpretation::Rgb
        );
        assert_eq!(metadata.samples_per_pixel, 3);

        // Bit depth (RGB is typically 8-bit)
        assert_eq!(metadata.bits_allocated(), 8);
        assert_eq!(metadata.bits_stored(), 8);

        // Planar configuration (should be Some for RGB)
        assert!(metadata.planar_configuration.is_some());

        // Pixel data
        assert!(!metadata.pixel_data().is_empty());

        // Image conversion should succeed
        let image = convert_to_image(&metadata).expect("Failed to convert file2.dcm to image");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify pixel values - check 10 specific pixels to catch decoding regressions
        let rgb = image.as_rgb8().expect("Should be RGB image");
        let width = rgb.width();
        let height = rgb.height();

        // Sample 10 specific pixel values (RGB image with different channel values)
        let expected_pixels = [
            ((width / 4, height / 4), (63, 80, 157)),
            ((width / 2, height / 4), (14, 14, 141)),
            ((3 * width / 4, height / 4), (7, 7, 135)),
            ((width / 4, height / 2), (7, 7, 135)),
            ((width / 2, height / 2), (3, 3, 130)),
            ((3 * width / 4, height / 2), (86, 127, 166)),
            ((width / 4, 3 * height / 4), (42, 42, 42)),
            ((width / 2, 3 * height / 4), (56, 56, 56)),
            ((3 * width / 4, 3 * height / 4), (65, 65, 65)),
            ((width / 2, height / 2 + 10), (13, 13, 140)),
        ];

        assert_rgb_pixels(rgb, "test_file2_metadata", &expected_pixels);

        // Display metadata - presence checks only (no personal data)
        assert!(metadata.patient_name().is_some());
        assert!(metadata.patient_id().is_some());
        assert!(metadata.patient_birth_date().is_some());
        assert!(metadata.accession_number().is_some());
        assert!(metadata.study_date().is_some());
        assert!(metadata.study_description().is_some());
        assert_eq!(metadata.modality(), Some("MR"));

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
        assert_relative_eq!(metadata.rescale_slope(), 1.0);
        assert_relative_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation
        assert_eq!(
            metadata.photometric_interpretation,
            PhotometricInterpretation::Monochrome2
        );
        assert_eq!(metadata.samples_per_pixel, 1);

        // Bit depth
        assert_eq!(metadata.bits_allocated(), 16);
        assert_eq!(metadata.bits_stored(), 16);

        // Planar configuration (should be None for grayscale)
        assert!(metadata.planar_configuration.is_none());

        // Pixel data
        assert!(!metadata.pixel_data().is_empty());

        // Image conversion should succeed
        let image = convert_to_image(&metadata).expect("Failed to convert file3.dcm to image");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify pixel values - check grayscale consistency
        // Grayscale converted to RGB, so R=G=B for all pixels
        let rgb = image.as_rgb8().expect("Should be RGB image");
        let width = rgb.width();
        let height = rgb.height();

        // Sample 10 pixels to verify grayscale conversion (R=G=B)
        // and to catch decoding regressions
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (3 * width / 4, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, height / 2),
            (width / 4, 3 * height / 4),
            (width / 2, 3 * height / 4),
            (3 * width / 4, 3 * height / 4),
            (width / 2, height / 2 + 10),
        ];

        for (x, y) in sample_coords {
            let pixel = rgb.get_pixel(x, y);
            // For grayscale images, R=G=B
            assert_eq!(pixel[0], pixel[1], "Grayscale should have R=G at ({x},{y})");
            assert_eq!(pixel[1], pixel[2], "Grayscale should have G=B at ({x},{y})");
        }

        // Sample 10 specific pixel values to catch decoding regressions
        let expected_pixels = [
            ((width / 4, height / 4), 74),
            ((width / 2, height / 4), 0),
            ((3 * width / 4, height / 4), 0),
            ((width / 4, height / 2), 79),
            ((width / 2, height / 2), 0),
            ((3 * width / 4, height / 2), 0),
            ((width / 4, 3 * height / 4), 40),
            ((width / 2, 3 * height / 4), 0),
            ((3 * width / 4, 3 * height / 4), 0),
            ((width / 2, height / 2 + 10), 0),
        ];

        assert_grayscale_pixels(rgb, "test_file3_metadata", &expected_pixels);

        // Display metadata - presence checks only (no personal data)
        assert!(metadata.patient_name().is_some());
        assert!(metadata.patient_id().is_some());
        assert!(metadata.patient_birth_date().is_some());
        assert!(metadata.accession_number().is_some());
        assert!(metadata.study_date().is_some());
        assert!(metadata.modality().is_some());

        // SOP class and transfer syntax
        assert!(metadata.sop_class.is_some());
        let sc = metadata.sop_class.as_ref().unwrap();
        assert_eq!(sc.uid, "1.2.840.10008.5.1.4.1.1.1.2");
        assert_eq!(
            sc.name,
            "Digital Mammography X-Ray Image Storage - For Presentation"
        );

        // Transfer syntax checks below
        assert_eq!(metadata.transfer_syntax.uid, "1.2.840.10008.1.2");
        assert_eq!(metadata.transfer_syntax.name, "Implicit VR Little Endian");

        // Display trait
        assert_eq!(
            metadata.photometric_interpretation.to_string(),
            "MONOCHROME2"
        );
    }

    #[test]
    fn test_big_endian_metadata() {
        let file_path = Path::new(".test-files/MR_small_bigendian.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open MR_small_bigendian.dcm");
        let metadata =
            extract_dicom_data(&obj).expect("Failed to extract data from MR_small_bigendian.dcm");

        // Image dimensions (small test image)
        assert_eq!(metadata.rows(), 64);
        assert_eq!(metadata.cols(), 64);

        // Bits allocated (16-bit grayscale)
        assert_eq!(metadata.bits_allocated(), 16);
        assert_eq!(metadata.bits_stored(), 16);

        // Rescale parameters (defaults)
        assert_relative_eq!(metadata.rescale_slope(), 1.0);
        assert_relative_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation (grayscale)
        assert_eq!(
            metadata.photometric_interpretation,
            PhotometricInterpretation::Monochrome2
        );
        assert_eq!(metadata.samples_per_pixel, 1);

        // Transfer syntax - this is the key test for big-endian support
        // Transfer syntax checks below
        assert_eq!(metadata.transfer_syntax.uid, "1.2.840.10008.1.2.2");
        assert_eq!(metadata.transfer_syntax.name, "Explicit VR Big Endian");

        // Verify is_big_endian() method works
        assert!(metadata.is_big_endian());

        // Pixel data should be present and correct size
        // 64x64 pixels, 16-bit per pixel = 8192 bytes
        assert_eq!(metadata.pixel_data().len(), 64 * 64 * 2);

        // Image conversion should succeed
        let image =
            convert_to_image(&metadata).expect("Failed to convert MR_small_bigendian.dcm to image");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify pixel values - check grayscale consistency
        // Grayscale converted to RGB, so R=G=B for all pixels
        let rgb = image.as_rgb8().expect("Should be RGB image");
        let width = rgb.width();
        let height = rgb.height();

        // Sample 10 pixels to verify grayscale conversion (R=G=B)
        // and to catch decoding regressions
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (3 * width / 4, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, height / 2),
            (width / 4, 3 * height / 4),
            (width / 2, 3 * height / 4),
            (3 * width / 4, 3 * height / 4),
            (width / 2, height / 2 + 10),
        ];

        for (x, y) in sample_coords {
            let pixel = rgb.get_pixel(x, y);
            // For grayscale images, R=G=B
            assert_eq!(pixel[0], pixel[1], "Grayscale should have R=G at ({x},{y})");
            assert_eq!(pixel[1], pixel[2], "Grayscale should have G=B at ({x},{y})");
        }

        // Sample 10 specific pixel values to catch decoding regressions
        let expected_pixels = [
            ((width / 4, height / 4), 101),
            ((width / 2, height / 4), 20),
            ((3 * width / 4, height / 4), 21),
            ((width / 4, height / 2), 16),
            ((width / 2, height / 2), 6),
            ((3 * width / 4, height / 2), 153),
            ((width / 4, 3 * height / 4), 18),
            ((width / 2, 3 * height / 4), 4),
            ((3 * width / 4, 3 * height / 4), 59),
            ((width / 2, height / 2 + 10), 8),
        ];

        assert_grayscale_pixels(rgb, "test_big_endian_metadata", &expected_pixels);
    }

    #[test]
    fn test_bits_stored_extraction() {
        // Verify bits_stored field is properly extracted
        // file1.dcm has 16 bits allocated but only 15 bits stored
        let file_path = Path::new(".test-files/file1.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open file1.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from file1.dcm");

        assert_eq!(metadata.bits_allocated(), 16);
        assert_eq!(metadata.bits_stored(), 15);
    }

    #[test]
    fn test_16bit_rgb_metadata() {
        // 16-bit RGB with RLE compression
        // Metadata extraction should work, but image conversion will fail
        // because we only support 8-bit RGB currently
        let file_path = Path::new(".test-files/SC_rgb_rle_16bit.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open SC_rgb_rle_16bit.dcm");
        let metadata =
            extract_dicom_data(&obj).expect("Failed to extract data from SC_rgb_rle_16bit.dcm");

        // Verify 16-bit RGB metadata
        assert_eq!(metadata.bits_allocated(), 16);
        assert_eq!(metadata.bits_stored(), 16);
        assert_eq!(
            metadata.photometric_interpretation,
            PhotometricInterpretation::Rgb
        );
        assert_eq!(metadata.samples_per_pixel, 3);

        // Pixel data should be present
        assert!(!metadata.pixel_data().is_empty());

        // Image conversion should fail (16-bit RGB not yet implemented)
        let result = convert_to_image(&metadata);
        assert!(result.is_err(), "16-bit RGB image conversion should fail");
        let err = result.unwrap_err();
        // The error should mention unsupported bit depth
        assert!(
            err.to_string()
                .contains("Unsupported bits allocated for RGB"),
            "Expected unsupported bits error, got: {err}"
        );
    }

    #[test]
    fn test_palette_color_metadata() {
        // Palette color with lookup table
        // Metadata extraction should work, but image conversion will fail
        // because we don't yet implement palette color lookup table decoding
        let file_path = Path::new(".test-files/examples_palette.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open examples_palette.dcm");
        let metadata =
            extract_dicom_data(&obj).expect("Failed to extract data from examples_palette.dcm");

        // Verify palette color metadata
        assert_eq!(metadata.bits_allocated(), 8);
        assert_eq!(metadata.bits_stored(), 8);
        assert_eq!(
            metadata.photometric_interpretation,
            PhotometricInterpretation::Palette
        );
        assert_eq!(metadata.samples_per_pixel, 1);

        // Pixel data should be present (raw bytes, since we use fallback for palette)
        assert!(!metadata.pixel_data().is_empty());

        // Image conversion should fail (palette → RGB not implemented)
        let result = convert_to_image(&metadata);
        assert!(result.is_err(), "Palette image conversion should fail");
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("Unsupported photometric interpretation"),
            "Expected 'Unsupported photometric interpretation' error, got: {err}"
        );
    }

    #[test]
    fn test_ycbcr_color_metadata() {
        // YCbCr color space (YBR_FULL_422)
        let file_path = Path::new(".test-files/SC_ybr_full_422_uncompressed.dcm");
        let obj =
            open_dicom_file(file_path).expect("Failed to open SC_ybr_full_422_uncompressed.dcm");
        let metadata = extract_dicom_data(&obj)
            .expect("Failed to extract data from SC_ybr_full_422_uncompressed.dcm");

        // Verify YCbCr metadata
        assert_eq!(metadata.bits_allocated(), 8);
        assert_eq!(metadata.bits_stored(), 8);
        assert_eq!(
            metadata.photometric_interpretation,
            PhotometricInterpretation::YbrFull422
        );
        assert_eq!(metadata.samples_per_pixel, 3);

        // Pixel data should be present
        assert!(!metadata.pixel_data().is_empty());

        // Image conversion should now succeed
        let image = convert_to_image(&metadata).expect("Failed to convert YCbCr to RGB image");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify pixel values - check 10 specific pixels to catch decoding regressions
        // YCbCr converted to RGB, channels can have different values
        let rgb = image.as_rgb8().expect("Should be RGB image");
        let width = rgb.width();
        let height = rgb.height();

        // Sample 10 specific pixel values
        let expected_pixels = [
            ((width / 4, height / 4), (0, 255, 4)),
            ((width / 2, height / 4), (0, 255, 4)),
            ((3 * width / 4, height / 4), (0, 255, 4)),
            ((width / 4, height / 2), (124, 130, 255)),
            ((width / 2, height / 2), (124, 130, 255)),
            ((3 * width / 4, height / 2), (124, 130, 255)),
            ((width / 4, 3 * height / 4), (64, 64, 64)),
            ((width / 2, 3 * height / 4), (64, 64, 64)),
            ((3 * width / 4, 3 * height / 4), (64, 64, 64)),
            ((width / 2, height / 2 + 10), (0, 3, 1)),
        ];

        assert_rgb_pixels(rgb, "test_ycbcr_color_metadata", &expected_pixels);
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
        assert!(
            result.is_err(),
            "32-bit RLE should fail until dicom crate adds support"
        );

        // Verify the error message is informative
        let err = result.unwrap_err();
        let err_msg = format!("{err}");
        assert!(
            err_msg.contains("pixel data") || err_msg.contains("PixelSequence"),
            "Error should mention pixel data issue, got: {err_msg}"
        );
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
        assert!(
            result.is_err(),
            "32-bit RLE should fail until dicom crate adds support"
        );

        // Verify the error message is informative
        let err = result.unwrap_err();
        let err_msg = format!("{err}");
        assert!(
            err_msg.contains("pixel data") || err_msg.contains("PixelSequence"),
            "Error should mention pixel data issue, got: {err_msg}"
        );
    }

    #[test]
    fn test_jpeg_ycbcr_multiframe_metadata() {
        // JPEG-compressed YCbCr multi-frame (ultrasound)
        // 30 frames, YBR_FULL_422, JPEG Baseline compression
        let file_path = Path::new(".test-files/examples_ybr_color.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open examples_ybr_color.dcm");
        let metadata =
            extract_dicom_data(&obj).expect("Failed to extract data from examples_ybr_color.dcm");

        // Verify metadata
        assert_eq!(metadata.bits_allocated(), 8);
        assert_eq!(metadata.bits_stored(), 8);
        assert_eq!(
            metadata.photometric_interpretation,
            PhotometricInterpretation::YbrFull422
        );
        assert_eq!(metadata.samples_per_pixel, 3);
        assert_eq!(metadata.number_of_frames, 30);

        // Image conversion should succeed (first frame only)
        let image = convert_to_image(&metadata).expect("Failed to convert JPEG YCbCr to image");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify pixel values - check 10 specific pixels to catch decoding regressions
        let rgb = image
            .as_rgb8()
            .expect("Should be RGB image after YCbCr conversion");
        let width = rgb.width();
        let height = rgb.height();

        // Sample 10 specific pixel values
        let expected_pixels = [
            ((width / 4, height / 4), (0, 0, 0)),
            ((width / 2, height / 4), (49, 49, 49)),
            ((3 * width / 4, height / 4), (0, 0, 0)),
            ((width / 4, height / 2), (1, 1, 1)),
            ((width / 2, height / 2), (7, 7, 7)),
            ((3 * width / 4, height / 2), (0, 0, 0)),
            ((width / 4, 3 * height / 4), (1, 1, 1)),
            ((width / 2, 3 * height / 4), (135, 135, 135)),
            ((3 * width / 4, 3 * height / 4), (2, 2, 2)),
            ((width / 2, height / 2 + 10), (25, 25, 25)),
        ];

        assert_rgb_pixels(rgb, "test_jpeg_ycbcr_multiframe_metadata", &expected_pixels);
    }

    #[test]
    fn test_jpeg2000_lossless_metadata() {
        // JPEG2000 lossless compressed MR image
        // 64x64, 16-bit grayscale, MONOCHROME2
        let file_path = Path::new(".test-files/MR_small_jp2klossless.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open MR_small_jp2klossless.dcm");
        let metadata = extract_dicom_data(&obj)
            .expect("Failed to extract data from MR_small_jp2klossless.dcm");

        // Image dimensions
        assert_eq!(metadata.rows(), 64);
        assert_eq!(metadata.cols(), 64);

        // Rescale parameters
        assert_relative_eq!(metadata.rescale_slope(), 1.0);
        assert_relative_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation
        assert_eq!(
            metadata.photometric_interpretation,
            PhotometricInterpretation::Monochrome2
        );
        assert_eq!(metadata.samples_per_pixel, 1);

        // Bit depth
        assert_eq!(metadata.bits_allocated(), 16);
        assert_eq!(metadata.bits_stored(), 16);

        // Planar configuration (should be None for grayscale)
        assert!(metadata.planar_configuration.is_none());

        // Pixel data should be present (decoded from JPEG2000)
        assert!(!metadata.pixel_data().is_empty());

        // Image conversion should succeed
        let image = convert_to_image(&metadata).expect("Failed to convert JPEG2000 image to RGB");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify RGB image was created
        let rgb = image
            .as_rgb8()
            .expect("Should be RGB image after grayscale conversion");
        let width = rgb.width();
        let height = rgb.height();

        // Sample 10 pixels to verify grayscale conversion (R=G=B)
        // and to catch decoding regressions
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (3 * width / 4, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, height / 2),
            (width / 4, 3 * height / 4),
            (width / 2, 3 * height / 4),
            (3 * width / 4, 3 * height / 4),
            (width / 2, height / 2 + 10),
        ];

        for (x, y) in sample_coords {
            let pixel = rgb.get_pixel(x, y);
            assert_eq!(pixel[0], pixel[1], "Grayscale should have R=G at ({x},{y})");
            assert_eq!(pixel[1], pixel[2], "Grayscale should have G=B at ({x},{y})");
        }

        // Sample 10 specific pixel values to catch decoding regressions
        let expected_pixels = [
            ((width / 4, height / 4), 101),
            ((width / 2, height / 4), 20),
            ((3 * width / 4, height / 4), 21),
            ((width / 4, height / 2), 16),
            ((width / 2, height / 2), 6),
            ((3 * width / 4, height / 2), 153),
            ((width / 4, 3 * height / 4), 18),
            ((width / 2, 3 * height / 4), 4),
            ((3 * width / 4, 3 * height / 4), 59),
            ((width / 2, height / 2 + 10), 8),
        ];

        assert_grayscale_pixels(rgb, "test_jpeg2000_lossless_metadata", &expected_pixels);

        // Display metadata - presence checks only (no personal data)
        assert!(metadata.patient_name().is_some());
        assert!(metadata.patient_id().is_some());

        // Modality
        assert_eq!(metadata.modality(), Some("MR"));

        // SOP class
        assert!(metadata.sop_class.is_some());
        let sc = metadata.sop_class.as_ref().unwrap();
        assert_eq!(sc.uid, "1.2.840.10008.5.1.4.1.1.4"); // MR Image Storage
        assert_eq!(sc.name, "MR Image Storage");

        // Transfer syntax should be JPEG2000 Lossless
        // Transfer syntax checks below
        assert!(
            metadata
                .transfer_syntax
                .uid
                .contains("1.2.840.10008.1.2.4.90"),
            "Transfer syntax UID should be JPEG2000 Lossless, got: {}",
            metadata.transfer_syntax.uid
        );
        assert!(
            metadata.transfer_syntax.name.contains("JPEG 2000")
                || metadata.transfer_syntax.name.contains("JPEG2000"),
            "Transfer syntax name should mention JPEG 2000, got: {}",
            metadata.transfer_syntax.name
        );

        // Display trait
        assert_eq!(
            metadata.photometric_interpretation.to_string(),
            "MONOCHROME2"
        );
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
        assert_relative_eq!(metadata.rescale_slope(), 1.0);
        assert_relative_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation
        assert_eq!(
            metadata.photometric_interpretation,
            PhotometricInterpretation::Monochrome2
        );
        assert_eq!(metadata.samples_per_pixel, 1);

        // Bit depth
        assert_eq!(metadata.bits_allocated(), 16);
        assert_eq!(metadata.bits_stored(), 16);

        // Planar configuration (should be None for grayscale)
        assert!(metadata.planar_configuration.is_none());

        // Pixel data should be present (decoded from JPEG2000)
        assert!(!metadata.pixel_data().is_empty());

        // Image conversion should succeed
        let image = convert_to_image(&metadata).expect("Failed to convert JPEG2000 image to RGB");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify RGB image was created
        let rgb = image
            .as_rgb8()
            .expect("Should be RGB image after grayscale conversion");
        let width = rgb.width();
        let height = rgb.height();

        // Sample 10 pixels to verify grayscale conversion (R=G=B)
        // and to catch decoding regressions
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (3 * width / 4, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, height / 2),
            (width / 4, 3 * height / 4),
            (width / 2, 3 * height / 4),
            (3 * width / 4, 3 * height / 4),
            (width / 2, height / 2 + 10),
        ];

        for (x, y) in sample_coords {
            let pixel = rgb.get_pixel(x, y);
            assert_eq!(pixel[0], pixel[1], "Grayscale should have R=G at ({x},{y})");
            assert_eq!(pixel[1], pixel[2], "Grayscale should have G=B at ({x},{y})");
        }

        // Sample 10 specific pixel values to catch decoding regressions
        let expected_pixels = [
            ((width / 4, height / 4), 0),
            ((width / 2, height / 4), 0),
            ((3 * width / 4, height / 4), 0),
            ((width / 4, height / 2), 0),
            ((width / 2, height / 2), 0),
            ((3 * width / 4, height / 2), 0),
            ((width / 4, 3 * height / 4), 254),
            ((width / 2, 3 * height / 4), 254),
            ((3 * width / 4, 3 * height / 4), 254),
            ((width / 2, height / 2 + 10), 0),
        ];

        assert_grayscale_pixels(rgb, "test_jpeg2000_lossy_metadata", &expected_pixels);

        // Display metadata - presence checks only (no personal data)
        assert!(metadata.patient_name().is_some());
        assert!(metadata.patient_id().is_some());

        // Modality
        assert_eq!(metadata.modality(), Some("NM"));

        // SOP class
        assert!(metadata.sop_class.is_some());
        let sc = metadata.sop_class.as_ref().unwrap();
        assert_eq!(sc.uid, "1.2.840.10008.5.1.4.1.1.7"); // Secondary Capture Image Storage
        assert_eq!(sc.name, "Secondary Capture Image Storage");

        // Transfer syntax should be JPEG2000
        // Transfer syntax checks below
        assert!(
            metadata
                .transfer_syntax
                .uid
                .contains("1.2.840.10008.1.2.4.91"),
            "Transfer syntax UID should be JPEG2000, got: {}",
            metadata.transfer_syntax.uid
        );
        assert!(
            metadata.transfer_syntax.name.contains("JPEG 2000")
                || metadata.transfer_syntax.name.contains("JPEG2000"),
            "Transfer syntax name should mention JPEG 2000, got: {}",
            metadata.transfer_syntax.name
        );

        // Display trait
        assert_eq!(
            metadata.photometric_interpretation.to_string(),
            "MONOCHROME2"
        );
    }

    #[test]
    fn test_ybr_full_to_dynamic_image() {
        // Test that dicom-pixeldata's to_dynamic_image() works for YBR_FULL (non-422) files
        // This is a regression test for the frame indexing bug we discovered in YBR_FULL_422
        use dicom::pixeldata::{ConvertOptions, PixelDecoder};

        let ybr_full_files = [
            ".test-files/SC_rgb_dcmtk_+eb+cy+n1.dcm",
            ".test-files/SC_rgb_dcmtk_+eb+cy+n2.dcm",
            ".test-files/SC_rgb_dcmtk_+eb+cy+s4.dcm",
        ];

        for file_path in ybr_full_files {
            eprintln!("Testing to_dynamic_image() on: {file_path}");
            let path = Path::new(file_path);

            let obj =
                open_dicom_file(path).unwrap_or_else(|e| panic!("Failed to open {file_path}: {e}"));

            // Decode pixel data
            let decoded_pixel_data = obj
                .decode_pixel_data()
                .unwrap_or_else(|e| panic!("Failed to decode pixel data for {file_path}: {e}"));

            // Check number of frames
            let num_frames = decoded_pixel_data.number_of_frames();
            eprintln!("  Frames: {num_frames}");

            // Try to convert to dynamic image
            let options =
                ConvertOptions::new().with_modality_lut(dicom::pixeldata::ModalityLutOption::None);

            let result = decoded_pixel_data.to_dynamic_image_with_options(0, &options);

            match result {
                Ok(dynamic_image) => {
                    eprintln!("  SUCCESS: to_dynamic_image() worked");
                    // Verify we got an RGB image
                    let _rgb = dynamic_image.to_rgb8();
                }
                Err(e) => {
                    eprintln!("  ERROR: to_dynamic_image() failed: {e}");
                    panic!("to_dynamic_image() failed for {file_path} (frames: {num_frames}): {e}");
                }
            }
        }
    }

    #[test]
    fn test_ybr_full_422_to_dynamic_image_bug() {
        // Test that documents the frame indexing bug in dicom-pixeldata
        // For YBR_FULL_422 files, to_dynamic_image() incorrectly reports "frame #0 is out of range"
        // even when number_of_frames() returns 1
        use dicom::pixeldata::{ConvertOptions, PixelDecoder};

        let file_path = ".test-files/SC_ybr_full_422_uncompressed.dcm";
        let path = Path::new(file_path);

        // Skip test if file doesn't exist
        if !path.exists() {
            eprintln!("SKIP: {file_path} not found");
            return;
        }

        let obj = open_dicom_file(path).expect("Failed to open file");

        // Decode pixel data
        let decoded_pixel_data = obj
            .decode_pixel_data()
            .expect("Failed to decode pixel data");

        // Check number of frames
        let num_frames = decoded_pixel_data.number_of_frames();
        eprintln!("YBR_FULL_422 file has {num_frames} frames");
        assert_eq!(num_frames, 1, "Expected 1 frame");

        // Try to convert to dynamic image - this should fail due to the bug
        let options =
            ConvertOptions::new().with_modality_lut(dicom::pixeldata::ModalityLutOption::None);

        let result = decoded_pixel_data.to_dynamic_image_with_options(0, &options);

        // This test documents the known bug - we expect it to fail
        assert!(
            result.is_err(),
            "Expected to_dynamic_image() to fail for YBR_FULL_422 due to frame indexing bug, \
            but it succeeded. This might mean the bug was fixed in dicom-pixeldata!"
        );

        let err = result.unwrap_err();
        let err_msg = format!("{err}");
        eprintln!("Expected error (bug confirmed): {err_msg}");

        // The error should mention "out of range" or "frame"
        assert!(
            err_msg.to_lowercase().contains("out of range")
                || err_msg.to_lowercase().contains("frame"),
            "Expected 'out of range' error, got: {err_msg}"
        );
    }

    /// Helper to verify grayscale pixel values at specific coordinates
    ///
    /// # Arguments
    /// * `rgb` - The RGB image buffer (grayscale images are converted to RGB format)
    /// * `test_name` - Name of the test (for debug output)
    /// * `expected_pixels` - Array of (coordinates, `expected_value`) pairs
    fn assert_grayscale_pixels(
        rgb: &image::RgbImage,
        test_name: &str,
        expected_pixels: &GrayscalePixelSamples,
    ) {
        // Collect actual values
        let actual_pixels: Vec<_> = expected_pixels
            .iter()
            .map(|((x, y), _)| (*x, *y, rgb.get_pixel(*x, *y)[0]))
            .collect();

        let expected_values: Vec<_> = expected_pixels
            .iter()
            .map(|((x, y), v)| (*x, *y, *v))
            .collect();

        // Print debug output
        println!("\n{test_name} pixel values:");
        for (i, ((x, y), expected)) in expected_pixels.iter().enumerate() {
            let actual = actual_pixels[i].2;
            println!("  [{i}] ({x}, {y}): expected={expected}, actual={actual}");
        }

        // Assert
        assert_eq!(
            actual_pixels, expected_values,
            "Pixel values mismatch! See output above for details."
        );
    }

    /// Helper to verify RGB pixel values at specific coordinates
    ///
    /// # Arguments
    /// * `rgb` - The RGB image buffer
    /// * `test_name` - Name of the test (for debug output)
    /// * `expected_pixels` - Array of (coordinates, `expected_rgb`) pairs
    fn assert_rgb_pixels(
        rgb: &image::RgbImage,
        test_name: &str,
        expected_pixels: &RgbPixelSamples,
    ) {
        // Collect actual values (all 3 channels)
        let actual_pixels: Vec<_> = expected_pixels
            .iter()
            .map(|((x, y), _)| {
                (
                    *x,
                    *y,
                    (
                        rgb.get_pixel(*x, *y)[0],
                        rgb.get_pixel(*x, *y)[1],
                        rgb.get_pixel(*x, *y)[2],
                    ),
                )
            })
            .collect();

        let expected_values: Vec<_> = expected_pixels
            .iter()
            .map(|((x, y), v)| (*x, *y, *v))
            .collect();

        // Print debug output
        println!("\n{test_name} pixel values:");
        for (i, ((x, y), expected)) in expected_pixels.iter().enumerate() {
            let actual = actual_pixels[i].2;
            println!("  [{i}] ({x}, {y}): expected={expected:?}, actual={actual:?}");
        }

        assert_eq!(
            actual_pixels, expected_values,
            "Pixel values mismatch! See output above for details."
        );
    }

    #[test]
    fn test_non_image_rtplan_error_message() {
        let file_path = Path::new(".test-files/RTPLAN.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open RTPLAN.dcm");
        let result = extract_dicom_data(&obj);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("non-image DICOM file"));
        assert!(err.contains("Modality: RTPLAN"));
        assert!(err.contains("SOP Class: RT Plan Storage"));
        assert!(err.contains("1.2.840.10008.5.1.4.1.1.481.5")); // UID from pydicom test file
    }

    #[test]
    #[ignore = "RTSTRUCT.dcm file is missing DICOM meta header - need a valid RTSTRUCT file"]
    fn test_non_image_rtstruct_error_message() {
        let file_path = Path::new(".test-files/RTSTRUCT.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open RTSTRUCT.dcm");
        let result = extract_dicom_data(&obj);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("non-image DICOM file"));
        assert!(err.contains("Modality: RTSTRUCT"));
        assert!(err.contains("SOP Class: RT Structure Set Storage"));
    }

    #[test]
    fn test_non_image_sr_error_message() {
        let file_path = Path::new(".test-files/SR.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open SR.dcm");
        let result = extract_dicom_data(&obj);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("non-image DICOM file"));
        assert!(err.contains("Modality: SR"));
        assert!(err.contains("SOP Class: Comprehensive SR Storage"));
        assert!(err.contains("1.2.840.10008.5.1.4.1.1.88.33")); // UID from pydicom test file
    }

    #[test]
    fn test_jpeg_ls_lossless_metadata() {
        // JPEG-LS Lossless compressed MR image
        // 64x64, 16-bit grayscale, MONOCHROME2
        let file_path = Path::new(".test-files/MR_small_jpeg_ls_lossless.dcm");

        // Skip test if file doesn't exist
        if !file_path.exists() {
            eprintln!("SKIP: {file_path:?} not found");
            return;
        }

        let obj = open_dicom_file(file_path).expect("Failed to open MR_small_jpeg_ls_lossless.dcm");
        let metadata = extract_dicom_data(&obj)
            .expect("Failed to extract data from MR_small_jpeg_ls_lossless.dcm");

        // Image dimensions
        assert_eq!(metadata.rows(), 64);
        assert_eq!(metadata.cols(), 64);

        // Rescale parameters
        assert_relative_eq!(metadata.rescale_slope(), 1.0);
        assert_relative_eq!(metadata.rescale_intercept(), 0.0);

        // Photometric interpretation
        assert_eq!(
            metadata.photometric_interpretation,
            PhotometricInterpretation::Monochrome2
        );
        assert_eq!(metadata.samples_per_pixel, 1);

        // Bit depth
        assert_eq!(metadata.bits_allocated(), 16);
        assert_eq!(metadata.bits_stored(), 16);

        // Planar configuration (should be None for grayscale)
        assert!(metadata.planar_configuration.is_none());

        // Pixel data should be present (decoded from JPEG-LS)
        assert!(!metadata.pixel_data().is_empty());

        // Image conversion should succeed
        let image = convert_to_image(&metadata).expect("Failed to convert JPEG-LS image to RGB");
        assert_eq!(image.width(), u32::from(metadata.cols()));
        assert_eq!(image.height(), u32::from(metadata.rows()));

        // Verify RGB image was created
        let rgb = image
            .as_rgb8()
            .expect("Should be RGB image after grayscale conversion");
        let width = rgb.width();
        let height = rgb.height();

        // Sample 10 pixels to verify grayscale conversion (R=G=B)
        // and to catch decoding regressions
        let sample_coords = [
            (width / 4, height / 4),
            (width / 2, height / 4),
            (3 * width / 4, height / 4),
            (width / 4, height / 2),
            (width / 2, height / 2),
            (3 * width / 4, height / 2),
            (width / 4, 3 * height / 4),
            (width / 2, 3 * height / 4),
            (3 * width / 4, 3 * height / 4),
            (width / 2, height / 2 + 10),
        ];

        for (x, y) in sample_coords {
            let pixel = rgb.get_pixel(x, y);
            assert_eq!(pixel[0], pixel[1], "Grayscale should have R=G at ({x},{y})");
            assert_eq!(pixel[1], pixel[2], "Grayscale should have G=B at ({x},{y})");
        }

        // Sample 10 specific pixel values to catch decoding regressions
        let expected_pixels = [
            ((width / 4, height / 4), 101),
            ((width / 2, height / 4), 20),
            ((3 * width / 4, height / 4), 21),
            ((width / 4, height / 2), 16),
            ((width / 2, height / 2), 6),
            ((3 * width / 4, height / 2), 153),
            ((width / 4, 3 * height / 4), 18),
            ((width / 2, 3 * height / 4), 4),
            ((3 * width / 4, 3 * height / 4), 59),
            ((width / 2, height / 2 + 10), 8),
        ];

        assert_grayscale_pixels(rgb, "test_jpeg_ls_lossless_metadata", &expected_pixels);

        // Modality
        assert_eq!(metadata.modality(), Some("MR"));

        // SOP class
        assert!(metadata.sop_class.is_some());
        let sc = metadata.sop_class.as_ref().unwrap();
        assert_eq!(sc.uid, "1.2.840.10008.5.1.4.1.1.4"); // MR Image Storage
        assert_eq!(sc.name, "MR Image Storage");

        // Transfer syntax should be JPEG-LS Lossless
        assert!(
            metadata
                .transfer_syntax
                .uid
                .contains("1.2.840.10008.1.2.4.80"),
            "Transfer syntax UID should be JPEG-LS Lossless, got: {}",
            metadata.transfer_syntax.uid
        );
        assert!(
            metadata.transfer_syntax.name.contains("JPEG-LS")
                || metadata.transfer_syntax.name.contains("JPEGLS"),
            "Transfer syntax name should mention JPEG-LS, got: {}",
            metadata.transfer_syntax.name
        );

        // Display trait
        assert_eq!(
            metadata.photometric_interpretation.to_string(),
            "MONOCHROME2"
        );
    }

    #[test]
    fn test_non_image_dicomdir_error_message() {
        let file_path = Path::new(".test-files/DICOMDIR.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open DICOMDIR.dcm");
        let result = extract_dicom_data(&obj);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        eprintln!("DICOMDIR error: {err}");
        // DICOMDIR might not have SOP Class or Modality, check for generic error
        assert!(err.contains("Missing or invalid Rows tag"));
    }

    #[test]
    fn test_extract_metadata_tags_on_jpeg2000() {
        // Test that extract_metadata_tags() works even when pixel data decoding fails
        // examples_jpeg2k.dcm has JPEG2000 compressed pixel data that gdcm can't decode
        let file_path = Path::new(".test-files/examples_jpeg2k.dcm");
        assert!(file_path.exists());

        // Opening the file should succeed (it's a valid DICOM)
        let obj = open_dicom_file(file_path).expect("Failed to open JPEG2000 file");

        // Full extraction should fail (pixel decode fails)
        let full_result = extract_dicom_data(&obj);
        assert!(
            full_result.is_err(),
            "extract_dicom_data should fail for JPEG2000"
        );

        // Partial metadata extraction should succeed
        let metadata = extract_metadata_tags(&obj)
            .expect("extract_metadata_tags should succeed even when pixel decode fails");

        // Verify key metadata fields are extracted correctly
        assert_eq!(metadata.rows(), 480);
        assert_eq!(metadata.cols(), 640);
        assert_eq!(metadata.samples_per_pixel, 3);
        assert_eq!(metadata.bits_allocated(), 8);
        assert_eq!(metadata.bits_stored(), 8);
    }
}
