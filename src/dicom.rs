use anyhow::{Context, Result};
use dicom::core::dictionary::UidDictionary;
use dicom::dictionary_std::sop_class;
use dicom::encoding::TransferSyntaxIndex;
use dicom::object::{
    open_file,
    FileDicomObject,
    InMemDicomObject,
    StandardDataDictionary
};
use dicom::pixeldata::PixelDecoder;
use dicom::transfer_syntax::TransferSyntaxRegistry;
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
    /// RGB stored in YCbCr color space (future)
    YbrFull,
    /// YCbCr for JPEG (future)
    YbrFull422,
    /// Palette color (future)
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
    pub fn is_grayscale(&self) -> bool {
        matches!(self, Self::Monochrome1 | Self::Monochrome2)
    }

    /// Check if this is an RGB interpretation
    pub fn is_rgb(&self) -> bool {
        matches!(self, Self::Rgb)
    }

    /// Check if pixel values should be inverted (MONOCHROME1)
    pub fn should_invert(&self) -> bool {
        matches!(self, Self::Monochrome1)
    }
}

impl std::fmt::Display for PhotometricInterpretation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Monochrome1 => write!(f, "MONOCHROME1"),
            Self::Monochrome2 => write!(f, "MONOCHROME2"),
            Self::Rgb => write!(f, "RGB"),
            Self::YbrFull => write!(f, "YBR_FULL"),
            Self::YbrFull422 => write!(f, "YBR_FULL_422"),
            Self::Palette => write!(f, "PALETTE COLOR"),
            Self::Unknown(s) => write!(f, "{}", s),
        }
    }
}

/// DICOM image metadata extracted from the file
#[derive(Debug, Clone)]
pub struct DicomMetadata {
    pub rows: u16,
    pub cols: u16,
    pub rescale_slope: f64,
    pub rescale_intercept: f64,
    pub pixel_aspect_ratio: Option<(f64, f64)>, // (vertical, horizontal)

    // Photometric interpretation and color space
    pub photometric_interpretation: PhotometricInterpretation,
    pub samples_per_pixel: u16,        // 1 for grayscale, 3 for RGB
    pub bits_allocated: u16,            // 8 or 16
    pub planar_configuration: Option<u16>, // 0 = interleaved, 1 = planar (RGB only)

    // Raw pixel data as bytes (supports both 8-bit RGB and 16-bit grayscale)
    pub pixel_data: Vec<u8>,

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
    pub sop_class: Option<(String, String)>,     // (uid, name)
    pub transfer_syntax: (String, String),       // (uid, name) - always present
}

/// Open and parse a DICOM file
pub fn open_dicom_file(file_path: &std::path::Path) -> Result<FileDicomObject<InMemDicomObject<StandardDataDictionary>>> {
    open_file(file_path)
        .with_context(|| format!("Failed to open DICOM file: {}", file_path.display()))
}

/// Extract metadata and pixel data from a DICOM object
pub fn extract_dicom_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<DicomMetadata> {
    use dicom::dictionary_std::tags;

    // Get image dimensions
    let rows = obj
        .get(tags::ROWS)
        .and_then(|e| e.to_int::<u16>().ok())
        .context("Missing or invalid Rows tag")?;

    let cols = obj
        .get(tags::COLUMNS)
        .and_then(|e| e.to_int::<u16>().ok())
        .context("Missing or invalid Columns tag")?;

    // Get rescale parameters
    let rescale_slope = obj
        .get(tags::RESCALE_SLOPE)
        .and_then(|e| e.to_float64().ok())
        .unwrap_or(1.0);

    let rescale_intercept = obj
        .get(tags::RESCALE_INTERCEPT)
        .and_then(|e| e.to_float64().ok())
        .unwrap_or(0.0);

    // Get pixel aspect ratio (vertical\horizontal)
    let pixel_aspect_ratio = obj
        .get(tags::PIXEL_ASPECT_RATIO)
        .and_then(|e| e.value().to_str().ok())
        .and_then(|s| {
            let (vertical, horizontal) = s.split_once('\\')?;
            let vertical = vertical.trim().parse::<f64>().ok()?;
            let horizontal = horizontal.trim().parse::<f64>().ok()?;
            Some((vertical, horizontal))
        });

    // Parse photometric interpretation
    let photometric_interpretation = obj
        .get(tags::PHOTOMETRIC_INTERPRETATION)
        .and_then(|e| e.value().to_str().ok())
        .map_or(PhotometricInterpretation::Monochrome2, |s| PhotometricInterpretation::from_str(&s).unwrap()); // Default

    // Get samples per pixel (1 for grayscale, 3 for RGB)
    let samples_per_pixel = obj
        .get(tags::SAMPLES_PER_PIXEL)
        .and_then(|e| e.to_int::<u16>().ok())
        .unwrap_or(1); // Default to 1 (grayscale)

    // Get bits allocated (8 or 16)
    let bits_allocated = obj
        .get(tags::BITS_ALLOCATED)
        .and_then(|e| e.to_int::<u16>().ok())
        .unwrap_or(16); // Default to 16

    // Get planar configuration (for RGB only)
    let planar_configuration = obj
        .get(tags::PLANAR_CONFIGURATION)
        .and_then(|e| e.to_int::<u16>().ok());

    // Extract patient info
    let patient_name = obj
        .get(tags::PATIENT_NAME)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());
    let patient_id = obj
        .get(tags::PATIENT_ID)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());
    let patient_birth_date = obj
        .get(tags::PATIENT_BIRTH_DATE)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());

    // Extract study info
    let accession_number = obj
        .get(tags::ACCESSION_NUMBER)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());
    let study_date = obj
        .get(tags::STUDY_DATE)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());
    let study_description = obj
        .get(tags::STUDY_DESCRIPTION)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());
    let modality = obj
        .get(tags::MODALITY)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());

    // Extract series info
    let series_description = obj
        .get(tags::SERIES_DESCRIPTION)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());

    // Extract image info
    let slice_thickness = obj
        .get(tags::SLICE_THICKNESS)
        .and_then(|e| e.to_float64().ok());

    // Extract SOP Class UID with lookup
    let sop_class = obj
        .get(tags::SOP_CLASS_UID)
        .and_then(|e| e.value().to_str().ok())
        .and_then(|uid| {
            sop_class::StandardSopClassDictionary
                .by_uid(&uid)
                .map(|entry| (uid.to_string(), entry.name.to_string()))
        });

    // Extract Transfer Syntax from meta header (always present)
    let transfer_syntax_uid = obj.meta().transfer_syntax().to_string();
    let transfer_syntax_name = TransferSyntaxRegistry
        .get(&transfer_syntax_uid)
        .map_or_else(|| "Unknown".to_string(), |ts| ts.name().to_string());
    let transfer_syntax = (transfer_syntax_uid, transfer_syntax_name);

    // Decode pixel data (handles both compressed and uncompressed)
    let decoded_pixel_data = obj.decode_pixel_data()
        .context("Failed to decode pixel data")?;

    // Get raw pixel data as bytes (supports both 8-bit RGB and 16-bit grayscale)
    // For 16-bit grayscale, we need to use u16, then convert to bytes
    let pixel_data = if bits_allocated == 16 {
        decoded_pixel_data.to_vec::<u16>()
            .context("Failed to convert 16-bit pixel data")?
            .iter()
            .flat_map(|&v| v.to_le_bytes())
            .collect()
    } else {
        decoded_pixel_data.to_vec::<u8>()
            .context("Failed to convert pixel data to bytes")?
    };

    // Validate photometric interpretation matches samples per pixel
    let is_valid = match (&photometric_interpretation, samples_per_pixel) {
        (pi, 1) if pi.is_grayscale() => true,
        (pi, 3) if pi.is_rgb() => true,
        _ => false,
    };

    if !is_valid {
        anyhow::bail!(
            "Inconsistent photometric interpretation {:?} with samples per pixel {}",
            photometric_interpretation, samples_per_pixel
        );
    }

    // Validate planar configuration only for RGB
    if planar_configuration.is_some() && !photometric_interpretation.is_rgb() {
        anyhow::bail!("Planar configuration should only be present for RGB images");
    }

    // Validate bits allocated
    if !matches!(bits_allocated, 8 | 16) {
        anyhow::bail!("Unsupported bits allocated: {bits_allocated}");
    }

    Ok(DicomMetadata {
        rows,
        cols,
        rescale_slope,
        rescale_intercept,
        pixel_aspect_ratio,
        photometric_interpretation,
        samples_per_pixel,
        bits_allocated,
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
    use std::path::Path;

    #[test]
    fn test_file1_metadata() {
        let file_path = Path::new(".test-files/file1.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open file1.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from file1.dcm");

        // Image dimensions
        assert_eq!(metadata.rows, 1855);
        assert_eq!(metadata.cols, 1991);

        // Rescale parameters
        assert_eq!(metadata.rescale_slope, 1.0);
        assert_eq!(metadata.rescale_intercept, 0.0);

        // Window parameters (may be present or absent)
        // Just verify they were extracted without checking specific values

        // Photometric interpretation
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::Monochrome1);
        assert_eq!(metadata.samples_per_pixel, 1);

        // Bit depth
        assert_eq!(metadata.bits_allocated, 16);

        // Planar configuration (should be None for grayscale)
        assert!(metadata.planar_configuration.is_none());

        // Pixel data
        assert!(!metadata.pixel_data.is_empty());

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
        let (uid, name) = metadata.sop_class.as_ref().unwrap();
        assert_eq!(uid, "1.2.840.10008.5.1.4.1.1.1");
        assert_eq!(name, "Computed Radiography Image Storage");

        let (ts_uid, ts_name) = &metadata.transfer_syntax;
        assert_eq!(ts_uid, "1.2.840.10008.1.2");
        assert_eq!(ts_name, "Implicit VR Little Endian");

        // Display trait
        assert_eq!(metadata.photometric_interpretation.to_string(), "MONOCHROME1");
    }

    #[test]
    fn test_file2_metadata() {
        let file_path = Path::new(".test-files/file2.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open file2.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from file2.dcm");

        // Image dimensions (RGB)
        assert_eq!(metadata.rows, 192);
        assert_eq!(metadata.cols, 160);

        // Rescale parameters
        assert_eq!(metadata.rescale_slope, 1.0);
        assert_eq!(metadata.rescale_intercept, 0.0);

        // Photometric interpretation (RGB)
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::Rgb);
        assert_eq!(metadata.samples_per_pixel, 3);

        // Bit depth (RGB is typically 8-bit)
        assert_eq!(metadata.bits_allocated, 8);

        // Planar configuration (should be Some for RGB)
        assert!(metadata.planar_configuration.is_some());

        // Pixel data
        assert!(!metadata.pixel_data.is_empty());

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
        let (uid, name) = metadata.sop_class.as_ref().unwrap();
        assert_eq!(uid, "1.2.840.10008.5.1.4.1.1.4");
        assert_eq!(name, "MR Image Storage");

        let (ts_uid, ts_name) = &metadata.transfer_syntax;
        assert_eq!(ts_uid, "1.2.840.10008.1.2.1");
        assert_eq!(ts_name, "Explicit VR Little Endian");

        // Display trait
        assert_eq!(metadata.photometric_interpretation.to_string(), "RGB");
    }

    #[test]
    fn test_file3_metadata() {
        let file_path = Path::new(".test-files/file3.dcm");
        let obj = open_dicom_file(file_path).expect("Failed to open file3.dcm");
        let metadata = extract_dicom_data(&obj).expect("Failed to extract data from file3.dcm");

        // Image dimensions
        assert_eq!(metadata.rows, 4616);
        assert_eq!(metadata.cols, 3016);

        // Rescale parameters
        assert_eq!(metadata.rescale_slope, 1.0);
        assert_eq!(metadata.rescale_intercept, 0.0);

        // Photometric interpretation
        assert_eq!(metadata.photometric_interpretation, PhotometricInterpretation::Monochrome2);
        assert_eq!(metadata.samples_per_pixel, 1);

        // Bit depth
        assert_eq!(metadata.bits_allocated, 16);

        // Planar configuration (should be None for grayscale)
        assert!(metadata.planar_configuration.is_none());

        // Pixel data
        assert!(!metadata.pixel_data.is_empty());

        // Display metadata - presence checks only (no personal data)
        assert!(metadata.patient_name.is_some());
        assert!(metadata.patient_id.is_some());
        assert!(metadata.patient_birth_date.is_some());
        assert!(metadata.accession_number.is_some());
        assert!(metadata.study_date.is_some());
        assert!(metadata.modality.is_some());

        // SOP class and transfer syntax
        assert!(metadata.sop_class.is_some());
        let (uid, name) = metadata.sop_class.as_ref().unwrap();
        assert_eq!(uid, "1.2.840.10008.5.1.4.1.1.1.2");
        assert_eq!(name, "Digital Mammography X-Ray Image Storage - For Presentation");

        let (ts_uid, ts_name) = &metadata.transfer_syntax;
        assert_eq!(ts_uid, "1.2.840.10008.1.2");
        assert_eq!(ts_name, "Implicit VR Little Endian");

        // Display trait
        assert_eq!(metadata.photometric_interpretation.to_string(), "MONOCHROME2");
    }
}
