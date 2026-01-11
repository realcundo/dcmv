use crate::dicom::DicomMetadata;
use std::fmt;

/// Error type that preserves metadata when available
#[derive(Debug)]
pub enum ProcessError {
    /// File is not a valid DICOM - no metadata available
    NotADicomFile(String),

    /// Valid DICOM file but extraction failed - no metadata available
    ExtractionFailed(String),

    /// Metadata extracted successfully, but image conversion failed
    ConversionFailed {
        metadata: DicomMetadata,
        error: String,
    },

    /// Image ready but display failed
    DisplayFailed {
        metadata: DicomMetadata,
        error: String,
    },
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessError::NotADicomFile(msg) => write!(f, "{msg}"),
            ProcessError::ExtractionFailed(msg) => write!(f, "{msg}"),
            ProcessError::ConversionFailed { error, .. } => write!(f, "{error}"),
            ProcessError::DisplayFailed { error, .. } => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ProcessError {}

impl ProcessError {
    /// Returns metadata if available (for verbose display before error)
    pub fn metadata(&self) -> Option<&DicomMetadata> {
        match self {
            ProcessError::ConversionFailed { metadata, .. } => Some(metadata),
            ProcessError::DisplayFailed { metadata, .. } => Some(metadata),
            _ => None,
        }
    }
}
