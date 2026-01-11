use super::photometric::PhotometricInterpretation;
use super::pixel_data::DecodedPixelData;
use crate::types::{
    BitDepth, Dimensions, PatientInfo, PixelAspectRatio, RescaleParams, SOPClass, SeriesInfo,
    StudyInfo, TransferSyntax,
};

#[derive(Debug, Clone)]
pub struct DicomMetadata {
    // Image pixel properties
    pub dimensions: Dimensions,
    pub bit_depth: BitDepth,
    pub photometric_interpretation: PhotometricInterpretation,
    pub samples_per_pixel: u16,
    pub planar_configuration: Option<u16>,
    pub number_of_frames: u32,
    pub pixel_aspect_ratio: Option<PixelAspectRatio>,
    pub(crate) pixel_data_format: DecodedPixelData,

    // Rescaling parameters
    pub rescale: RescaleParams,

    // Grouped metadata
    pub patient: PatientInfo,
    pub study: StudyInfo,
    pub series: SeriesInfo,

    // DICOM header
    pub sop_class: Option<SOPClass>,
    pub transfer_syntax: TransferSyntax,
}

impl DicomMetadata {
    #[inline]
    #[must_use]
    pub fn rows(&self) -> u16 {
        self.dimensions.rows
    }

    #[inline]
    #[must_use]
    pub fn cols(&self) -> u16 {
        self.dimensions.cols
    }

    #[inline]
    #[must_use]
    pub fn rescale_slope(&self) -> f64 {
        self.rescale.slope
    }

    #[inline]
    #[must_use]
    pub fn rescale_intercept(&self) -> f64 {
        self.rescale.intercept
    }

    /// Returns true if this DICOM file uses big-endian byte order
    #[inline]
    #[must_use]
    #[allow(deprecated)] // Explicit VR Big Endian is retired but still in use
    pub fn is_big_endian(&self) -> bool {
        self.transfer_syntax.is_big_endian()
    }

    #[inline(always)]
    #[must_use]
    // Hot path: called once per pixel during conversion
    pub fn pixel_data(&self) -> &[u8] {
        match &self.pixel_data_format {
            DecodedPixelData::YcbCr(data)
            | DecodedPixelData::Rgb(data)
            | DecodedPixelData::Native(data) => data,
        }
    }

    #[inline]
    #[must_use]
    pub fn is_already_rgb(&self) -> bool {
        matches!(self.pixel_data_format, DecodedPixelData::Rgb(_))
    }

    // Backward-compatible accessors for bit_depth
    #[inline]
    #[must_use]
    pub fn bits_allocated(&self) -> u16 {
        self.bit_depth.allocated
    }

    #[inline]
    #[must_use]
    pub fn bits_stored(&self) -> u16 {
        self.bit_depth.stored
    }

    // Backward-compatible accessors for patient info
    #[inline]
    #[must_use]
    pub fn patient_name(&self) -> Option<&str> {
        self.patient.name.as_deref()
    }

    #[inline]
    #[must_use]
    pub fn patient_id(&self) -> Option<&str> {
        self.patient.id.as_deref()
    }

    #[inline]
    #[must_use]
    pub fn patient_birth_date(&self) -> Option<&str> {
        self.patient.birth_date.as_deref()
    }

    // Backward-compatible accessors for study info
    #[inline]
    #[must_use]
    pub fn accession_number(&self) -> Option<&str> {
        self.study.accession_number.as_deref()
    }

    #[inline]
    #[must_use]
    pub fn study_date(&self) -> Option<&str> {
        self.study.date.as_deref()
    }

    #[inline]
    #[must_use]
    pub fn study_description(&self) -> Option<&str> {
        self.study.description.as_deref()
    }

    #[inline]
    #[must_use]
    pub fn modality(&self) -> Option<&str> {
        self.study.modality.as_deref()
    }

    // Backward-compatible accessors for series info
    #[inline]
    #[must_use]
    pub fn series_description(&self) -> Option<&str> {
        self.series.description.as_deref()
    }

    #[inline]
    #[must_use]
    pub fn slice_thickness(&self) -> Option<f64> {
        self.series.slice_thickness
    }
}
