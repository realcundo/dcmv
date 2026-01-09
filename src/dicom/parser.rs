//! DICOM metadata extraction functions
//!
//! This module provides focused functions for extracting specific metadata
//! from DICOM objects, breaking down the large extract_dicom_data function.

use anyhow::{Context, Result};
use dicom::core::dictionary::UidDictionary;
use dicom::encoding::TransferSyntaxIndex;
use dicom::object::{
    FileDicomObject,
    InMemDicomObject,
    StandardDataDictionary
};
use crate::types::{Dimensions, RescaleParams, PixelAspectRatio, TransferSyntax, SOPClass};

/// Extract basic image dimensions from DICOM object
pub fn extract_dimensions(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<Dimensions> {
    use dicom::dictionary_std::tags;

    let rows = obj
        .get(tags::ROWS)
        .and_then(|e| e.to_int::<u16>().ok())
        .context("Missing or invalid Rows tag")?;

    let cols = obj
        .get(tags::COLUMNS)
        .and_then(|e| e.to_int::<u16>().ok())
        .context("Missing or invalid Columns tag")?;

    Ok(Dimensions::new(rows, cols))
}

/// Extract rescale parameters (slope and intercept) from DICOM object
pub fn extract_rescale_params(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> RescaleParams {
    use dicom::dictionary_std::tags;

    let slope = obj
        .get(tags::RESCALE_SLOPE)
        .and_then(|e| e.to_float64().ok())
        .unwrap_or(1.0);

    let intercept = obj
        .get(tags::RESCALE_INTERCEPT)
        .and_then(|e| e.to_float64().ok())
        .unwrap_or(0.0);

    RescaleParams::new(slope, intercept)
}

/// Extract pixel aspect ratio from DICOM object (if present)
pub fn extract_pixel_aspect_ratio(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Option<PixelAspectRatio> {
    use dicom::dictionary_std::tags;

    obj.get(tags::PIXEL_ASPECT_RATIO)
        .and_then(|e| e.value().to_str().ok())
        .and_then(|s| {
            let (vertical, horizontal) = s.split_once('\\')?;
            let vertical = vertical.trim().parse::<f64>().ok()?;
            let horizontal = horizontal.trim().parse::<f64>().ok()?;
            Some(PixelAspectRatio::new(vertical, horizontal))
        })
}

/// Extract number of frames from DICOM object
pub fn extract_number_of_frames(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> u32 {
    use dicom::dictionary_std::tags;

    obj.get(tags::NUMBER_OF_FRAMES)
        .and_then(|e| e.to_int::<u32>().ok())
        .unwrap_or(1)
}

/// Extract samples per pixel from DICOM object
pub fn extract_samples_per_pixel(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> u16 {
    use dicom::dictionary_std::tags;

    // Default to 1 (grayscale) for backward compatibility
    obj.get(tags::SAMPLES_PER_PIXEL)
        .and_then(|e| e.to_int::<u16>().ok())
        .unwrap_or(1)
}

/// Extract bit depth information from DICOM object
pub fn extract_bit_depth(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<(u16, u16)> {
    use dicom::dictionary_std::tags;

    let bits_allocated = obj
        .get(tags::BITS_ALLOCATED)
        .and_then(|e| e.to_int::<u16>().ok())
        .unwrap_or(16);

    let bits_stored = obj
        .get(tags::BITS_STORED)
        .and_then(|e| e.to_int::<u16>().ok())
        .unwrap_or(bits_allocated);

    Ok((bits_allocated, bits_stored))
}

/// Extract planar configuration from DICOM object (for RGB/YCbCr only)
pub fn extract_planar_configuration(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Option<u16> {
    use dicom::dictionary_std::tags;

    obj.get(tags::PLANAR_CONFIGURATION)
        .and_then(|e| e.to_int::<u16>().ok())
}

/// Extract transfer syntax from DICOM meta header
pub fn extract_transfer_syntax(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> TransferSyntax {
    use dicom::transfer_syntax::TransferSyntaxRegistry;

    let uid = obj.meta().transfer_syntax().to_string();
    let name = TransferSyntaxRegistry
        .get(&uid)
        .map_or_else(|| "Unknown".to_string(), |ts| ts.name().to_string());

    TransferSyntax::new(uid, name)
}

/// Extract SOP class with dictionary lookup from DICOM object
pub fn extract_sop_class(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Option<SOPClass> {
    use dicom::dictionary_std::sop_class;
    use dicom::dictionary_std::tags;

    obj.get(tags::SOP_CLASS_UID)
        .and_then(|e| e.value().to_str().ok())
        .and_then(|uid| {
            sop_class::StandardSopClassDictionary
                .by_uid(&uid)
                .map(|entry| SOPClass::new(uid.to_string(), entry.name.to_string()))
        })
}

/// Extract patient metadata (name, ID, birth date) from DICOM object
pub fn extract_patient_metadata(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> (Option<String>, Option<String>, Option<String>) {
    use dicom::dictionary_std::tags;

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

    (patient_name, patient_id, patient_birth_date)
}

/// Extract study metadata from DICOM object
pub fn extract_study_metadata(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    use dicom::dictionary_std::tags;

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

    (accession_number, study_date, study_description, modality)
}

/// Extract series and image metadata from DICOM object
pub fn extract_series_metadata(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> (Option<String>, Option<f64>) {
    use dicom::dictionary_std::tags;

    let series_description = obj
        .get(tags::SERIES_DESCRIPTION)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());

    let slice_thickness = obj
        .get(tags::SLICE_THICKNESS)
        .and_then(|e| e.to_float64().ok());

    (series_description, slice_thickness)
}
