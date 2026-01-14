use crate::types::{BitDepth, Dimensions, PatientInfo, PixelAspectRatio, RescaleParams, SeriesInfo, SOPClass, StudyInfo, TransferSyntax};
use anyhow::{Context, Result};
use dicom::core::dictionary::UidDictionary;
use dicom::dictionary_std::sop_class;
use dicom::dictionary_std::tags;
use dicom::encoding::TransferSyntaxIndex;
use dicom::object::{FileDicomObject, InMemDicomObject, StandardDataDictionary};
use dicom::transfer_syntax::TransferSyntaxRegistry;

/// Partial metadata for error message context
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub modality: Option<String>,
    pub sop_class: Option<SOPClass>,
}

impl ErrorContext {
    #[must_use]
    pub fn new() -> Self {
        Self {
            modality: None,
            sop_class: None,
        }
    }

    pub fn format_error(&self, tag_name: &str) -> String {
        let mut parts = Vec::new();

        if let Some(modality) = &self.modality {
            parts.push(format!("Modality: {modality}"));
        }

        if let Some(sc) = &self.sop_class {
            parts.push(format!("SOP Class: {sc}")); // Uses Display: "Name (UID)"
        }

        if parts.is_empty() {
            // Generic error when no context available
            format!("Missing or invalid {tag_name} tag")
        } else {
            format!(
                "Missing or invalid {tag_name} tag - this may be a non-image DICOM file ({})",
                parts.join(", ")
            )
        }
    }
}

impl Default for ErrorContext {
    fn default() -> Self {
        Self::new()
    }
}

// From DicomObject
impl From<&FileDicomObject<InMemDicomObject<StandardDataDictionary>>> for ErrorContext {
    fn from(obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>) -> Self {
        ErrorContext {
            modality: obj
                .get(tags::MODALITY)
                .and_then(|e| e.value().to_str().ok())
                .map(|s| s.to_string()),
            sop_class: extract_sop_class(obj),
        }
    }
}

pub fn extract_dimensions(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
    error_context: &ErrorContext,
) -> Result<Dimensions> {
    let rows = obj
        .get(tags::ROWS)
        .and_then(|e| e.to_int::<u16>().ok())
        .with_context(|| error_context.format_error("Rows"))?;

    let cols = obj
        .get(tags::COLUMNS)
        .and_then(|e| e.to_int::<u16>().ok())
        .with_context(|| error_context.format_error("Columns"))?;

    Ok(Dimensions::new(rows, cols))
}

pub fn extract_rescale_params(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> RescaleParams {
    // RESCALE_SLOPE and RESCALE_INTERCEPT are optional DICOM tags
    // They are primarily used for CT/PET scans to convert pixel values to Hounsfield units
    // For other modalities (CR, MR, etc.), these tags may not be present
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

pub fn extract_pixel_aspect_ratio(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Option<PixelAspectRatio> {
    obj.get(tags::PIXEL_ASPECT_RATIO)
        .and_then(|e| e.value().to_str().ok())
        .and_then(|s| {
            let (vertical, horizontal) = s.split_once('\\')?;
            let vertical = vertical.trim().parse::<f64>().ok()?;
            let horizontal = horizontal.trim().parse::<f64>().ok()?;
            Some(PixelAspectRatio::new(vertical, horizontal))
        })
}

#[inline]
pub fn extract_number_of_frames(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> u32 {
    obj.get(tags::NUMBER_OF_FRAMES)
        .and_then(|e| e.to_int::<u32>().ok())
        .unwrap_or(1)
}

#[inline]
pub fn extract_samples_per_pixel(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> u16 {
    obj.get(tags::SAMPLES_PER_PIXEL)
        .and_then(|e| e.to_int::<u16>().ok())
        .unwrap_or(1)
}

pub fn extract_bit_depth(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
    error_context: &ErrorContext,
) -> Result<BitDepth, anyhow::Error> {
    let allocated = obj
        .get(tags::BITS_ALLOCATED)
        .and_then(|e| e.to_int::<u16>().ok())
        .ok_or_else(|| anyhow::anyhow!(error_context.format_error("Bits Allocated")))?;

    let stored = obj
        .get(tags::BITS_STORED)
        .and_then(|e| e.to_int::<u16>().ok())
        .ok_or_else(|| anyhow::anyhow!(error_context.format_error("Bits Stored")))?;

    Ok(BitDepth::new(allocated, stored))
}

#[inline]
pub fn extract_planar_configuration(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Option<u16> {
    obj.get(tags::PLANAR_CONFIGURATION)
        .and_then(|e| e.to_int::<u16>().ok())
}

pub fn extract_transfer_syntax(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> TransferSyntax {
    let uid = obj.meta().transfer_syntax().to_string();
    let name = TransferSyntaxRegistry
        .get(&uid)
        .map_or_else(|| "Unknown".to_string(), |ts| ts.name().to_string());

    TransferSyntax::new(uid, name)
}

pub fn extract_sop_class(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Option<SOPClass> {
    obj.get(tags::SOP_CLASS_UID)
        .and_then(|e| e.value().to_str().ok())
        .and_then(|uid| {
            sop_class::StandardSopClassDictionary
                .by_uid(&uid)
                .map(|entry| SOPClass::new(uid.to_string(), entry.name.to_string()))
        })
}

pub fn extract_patient_info(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> PatientInfo {
    let name = obj
        .get(tags::PATIENT_NAME)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());

    let id = obj
        .get(tags::PATIENT_ID)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());

    let birth_date = obj
        .get(tags::PATIENT_BIRTH_DATE)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());

    PatientInfo { name, id, birth_date }
}

pub fn extract_study_info(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> StudyInfo {
    let accession_number = obj
        .get(tags::ACCESSION_NUMBER)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());

    let date = obj
        .get(tags::STUDY_DATE)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());

    let description = obj
        .get(tags::STUDY_DESCRIPTION)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());

    let modality = obj
        .get(tags::MODALITY)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());

    StudyInfo {
        accession_number,
        date,
        description,
        modality,
    }
}

pub fn extract_series_info(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> SeriesInfo {
    let description = obj
        .get(tags::SERIES_DESCRIPTION)
        .and_then(|e| e.value().to_str().ok())
        .map(|s| s.to_string());

    let slice_thickness = obj
        .get(tags::SLICE_THICKNESS)
        .and_then(|e| e.to_float64().ok());

    SeriesInfo {
        description,
        slice_thickness,
    }
}
