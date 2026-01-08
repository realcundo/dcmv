use anyhow::{Context, Result};
use dicom::core::dictionary::UidDictionary;
use dicom::object::{
    open_file,
    FileDicomObject,
    InMemDicomObject,
    StandardDataDictionary
};
use dicom::encoding::TransferSyntaxIndex;
use dicom_transfer_syntax_registry::TransferSyntaxRegistry;
use dicom_object::Tag;
use dicom_pixeldata::PixelDecoder;

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

impl PhotometricInterpretation {
    /// Parse photometric interpretation from DICOM string
    pub fn from_str(s: &str) -> Self {
        match s.trim() {
            "MONOCHROME1" => Self::Monochrome1,
            "MONOCHROME2" => Self::Monochrome2,
            "RGB" => Self::Rgb,
            "YBR_FULL" => Self::YbrFull,
            "YBR_FULL_422" => Self::YbrFull422,
            "PALETTE COLOR" => Self::Palette,
            other => Self::Unknown(other.to_string()),
        }
    }

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

/// DICOM image metadata extracted from the file
#[derive(Debug, Clone)]
pub struct DicomMetadata {
    pub rows: u16,
    pub cols: u16,
    pub rescale_slope: f64,
    pub rescale_intercept: f64,
    pub window_center: Option<f64>,
    pub window_width: Option<f64>,
    pub pixel_aspect_ratio: Option<(f64, f64)>, // (vertical, horizontal)

    // Photometric interpretation and color space
    pub photometric_interpretation: PhotometricInterpretation,
    #[allow(dead_code)]
    pub samples_per_pixel: u16,        // 1 for grayscale, 3 for RGB
    pub bits_allocated: u16,            // 8 or 16
    #[allow(dead_code)]
    pub bits_stored: u16,               // Significant bits
    #[allow(dead_code)]
    pub pixel_representation: u16,      // 0 = unsigned, 1 = signed
    pub planar_configuration: Option<u16>, // 0 = interleaved, 1 = planar (RGB only)

    // Raw pixel data as bytes (supports both 8-bit RGB and 16-bit grayscale)
    pub pixel_data: Vec<u8>,
}

/// Open and parse a DICOM file
pub fn open_dicom_file(file_path: &std::path::Path) -> Result<FileDicomObject<InMemDicomObject<StandardDataDictionary>>> {
    open_file(file_path)
        .with_context(|| format!("Failed to open DICOM file: {}", file_path.display()))
}

/// Extract metadata and pixel data from a DICOM object
pub fn extract_dicom_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
    user_window_center: Option<f64>,
    user_window_width: Option<f64>,
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

    // Get window parameters from DICOM if not provided by user
    let window_center = user_window_center.or_else(|| {
        obj.get(tags::WINDOW_CENTER)
            .and_then(|e| e.to_float64().ok())
    });

    let window_width = user_window_width.or_else(|| {
        obj.get(tags::WINDOW_WIDTH)
            .and_then(|e| e.to_float64().ok())
    });

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
        .map(|s| PhotometricInterpretation::from_str(&s))
        .unwrap_or(PhotometricInterpretation::Monochrome2); // Default

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

    // Get bits stored (significant bits)
    let bits_stored = obj
        .get(tags::BITS_STORED)
        .and_then(|e| e.to_int::<u16>().ok())
        .unwrap_or(bits_allocated); // Default to bits_allocated

    // Get pixel representation (0=unsigned, 1=signed)
    let pixel_representation = obj
        .get(tags::PIXEL_REPRESENTATION)
        .and_then(|e| e.to_int::<u16>().ok())
        .unwrap_or(0); // Default to unsigned

    // Get planar configuration (for RGB only)
    let planar_configuration = obj
        .get(tags::PLANAR_CONFIGURATION)
        .and_then(|e| e.to_int::<u16>().ok());

    // Decode pixel data (handles both compressed and uncompressed)
    let decoded_pixel_data = obj.decode_pixel_data()
        .context("Failed to decode pixel data")?;

    // Get raw pixel data as bytes (supports both 8-bit RGB and 16-bit grayscale)
    let pixel_data = decoded_pixel_data.to_vec::<u8>()
        .context("Failed to convert pixel data to bytes")?;

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
        anyhow::bail!("Unsupported bits allocated: {}", bits_allocated);
    }

    Ok(DicomMetadata {
        rows,
        cols,
        rescale_slope,
        rescale_intercept,
        window_center,
        window_width,
        pixel_aspect_ratio,
        photometric_interpretation,
        samples_per_pixel,
        bits_allocated,
        bits_stored,
        pixel_representation,
        planar_configuration,
        pixel_data,
    })
}

/// Print DICOM metadata to stdout
pub fn print_metadata(obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>) {
    use dicom::dictionary_std::tags;

    // Patient info
    print_tag(obj, tags::PATIENT_NAME, "Patient Name");
    print_tag(obj, tags::PATIENT_ID, "Patient ID");
    print_tag(obj, tags::PATIENT_BIRTH_DATE, "Birth Date");

    // Study info
    print_tag(obj, tags::ACCESSION_NUMBER, "Accession Number");
    print_tag(obj, tags::STUDY_DATE, "Study Date");
    print_tag(obj, tags::STUDY_DESCRIPTION, "Study Description");
    print_tag(obj, tags::MODALITY, "Modality");

    // Series info
    print_tag(obj, tags::SERIES_DESCRIPTION, "Series Description");

    // Image info
    print_dimensions(obj);
    print_pixel_aspect_ratio(obj);
    print_sop_class_uid(obj);
    print_transfer_syntax_uid(obj);
    print_tag(obj, tags::SLICE_THICKNESS, "Slice Thickness");

    println!();
}

fn print_tag(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
    tag: Tag,
    name: &str,
) {
    if let Some(elem) = obj.get(tag) {
        // Try to get string representation
        match elem.value().to_str() {
            Ok(s) => println!("{:20}: {}", name, s),
            Err(_) => println!("{:20}: {:?}", name, elem.value()),
        }
    }
}

/// Print dimensions as "WIDTHxHEIGHT" combining rows and columns
fn print_dimensions(obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>) {
    use dicom::dictionary_std::tags;

    let cols = obj.get(tags::COLUMNS).and_then(|e| e.value().to_str().ok());
    let rows = obj.get(tags::ROWS).and_then(|e| e.value().to_str().ok());

    match (cols, rows) {
        (Some(c), Some(r)) => println!("{:20}: {}x{}", "Dimensions", c, r),
        (Some(c), None) => println!("{:20}: {}", "Dimensions", c),
        (None, Some(r)) => println!("{:20}: {}", "Dimensions", r),
        (None, None) => {},
    }
}

/// Print pixel aspect ratio in a readable format (e.g., "1:1" instead of "1\1")
/// DICOM tag (0028,0034) stores two integers: vertical\horizontal
fn print_pixel_aspect_ratio(obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>) {
    use dicom::dictionary_std::tags;

    if let Some(elem) = obj.get(tags::PIXEL_ASPECT_RATIO) {
        match elem.value().to_str() {
            Ok(s) => {
                // Parse the two values separated by backslash
                if let Some((vertical, horizontal)) = s.split_once('\\') {
                    println!("{:20}: {}:{}", "Pixel Aspect Ratio", vertical.trim(), horizontal.trim());
                } else {
                    println!("{:20}: {}", "Pixel Aspect Ratio", s);
                }
            }
            Err(_) => println!("{:20}: {:?}", "Pixel Aspect Ratio", elem.value()),
        }
    }
}

/// Print SOP Class UID with human-readable name from the DICOM dictionary
fn print_sop_class_uid(obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>) {
    use dicom::dictionary_std::tags;
    use dicom_dictionary_std::sop_class;

    if let Some(elem) = obj.get(tags::SOP_CLASS_UID) {
        match elem.value().to_str() {
            Ok(uid) => {
                // Look up human-readable name in SOP class dictionary
                let name = sop_class::StandardSopClassDictionary
                    .by_uid(&uid)
                    .map(|entry| entry.name)
                    .unwrap_or("Unknown");

                println!("{:20}: {} ({})", "SOP Class UID", name, uid);
            }
            Err(_) => println!("{:20}: {:?}", "SOP Class UID", elem.value()),
        }
    }
}

/// Print Transfer Syntax UID with human-readable name from the DICOM dictionary
fn print_transfer_syntax_uid(obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>) {
    // Get transfer syntax from meta header
    let uid = obj.meta().transfer_syntax();

    // Look up human-readable name in transfer syntax registry
    let name = TransferSyntaxRegistry
        .get(uid)
        .map(|ts| ts.name())
        .unwrap_or("Unknown");

    println!("{:20}: {} ({})", "Transfer Syntax", name, uid);
}
