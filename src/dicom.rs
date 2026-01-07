use anyhow::{Context, Result};
use dicom::core::dictionary::UidDictionary;
use dicom::object::{open_file, InMemDicomObject, StandardDataDictionary};
use dicom_object::Tag;

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
    pub pixel_data: Vec<u16>,
}

/// Open and parse a DICOM file
pub fn open_dicom_file(file_path: &std::path::Path) -> Result<InMemDicomObject<StandardDataDictionary>> {
    let file_obj = open_file(file_path)
        .with_context(|| format!("Failed to open DICOM file: {}", file_path.display()))?;
    Ok(file_obj.into_inner())
}

/// Extract metadata and pixel data from a DICOM object
pub fn extract_dicom_data(
    obj: &InMemDicomObject<StandardDataDictionary>,
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

    // Decode pixel data
    // Get the raw pixel data from the DICOM object
    let pixel_data = obj.get(tags::PIXEL_DATA)
        .and_then(|e| {
            // Get the value as a byte vector
            e.value().to_bytes().ok()
        })
        .and_then(|bytes| {
            // Convert bytes to u16 pixels (little-endian)
            if bytes.len() % 2 != 0 {
                return None;
            }
            Some(bytes.chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>())
        })
        .context("Failed to get pixel data")?;

    Ok(DicomMetadata {
        rows,
        cols,
        rescale_slope,
        rescale_intercept,
        window_center,
        window_width,
        pixel_aspect_ratio,
        pixel_data,
    })
}

/// Print DICOM metadata to stdout
pub fn print_metadata(obj: &InMemDicomObject<StandardDataDictionary>) {
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
    print_tag(obj, tags::SLICE_THICKNESS, "Slice Thickness");

    println!();
}

fn print_tag(
    obj: &InMemDicomObject<StandardDataDictionary>,
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
fn print_dimensions(obj: &InMemDicomObject<StandardDataDictionary>) {
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
fn print_pixel_aspect_ratio(obj: &InMemDicomObject<StandardDataDictionary>) {
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
fn print_sop_class_uid(obj: &InMemDicomObject<StandardDataDictionary>) {
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
