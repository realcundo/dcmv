use crate::dicom::DicomMetadata;
use thiserror::Error;

/// Error type that preserves metadata when available
/// Metadata is boxed to reduce stack size (`DicomMetadata` contains pixel data Vec)
#[derive(Error, Debug)]
pub enum ProcessError {
    /// File is not a valid DICOM - no metadata available
    #[error("{0}")]
    NotADicomFile(String),

    /// Valid DICOM file but extraction failed - no metadata available
    #[error("{0}")]
    ExtractionFailed(String),

    /// Metadata extracted successfully, but image conversion failed
    #[error("{error}")]
    ConversionFailed {
        metadata: Box<DicomMetadata>,
        error: String,
    },

    /// Image ready but display failed
    #[error("{error}")]
    DisplayFailed {
        metadata: Box<DicomMetadata>,
        error: String,
    },
}

impl ProcessError {
    /// Returns metadata if available (for verbose display before error)
    #[must_use]
    pub fn metadata(&self) -> Option<&DicomMetadata> {
        match self {
            ProcessError::ConversionFailed { metadata, .. } |
            ProcessError::DisplayFailed { metadata, .. } => Some(metadata),
            _ => None,
        }
    }
}
