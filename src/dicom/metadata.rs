use super::photometric::PhotometricInterpretation;
use super::pixel_data::DecodedPixelData;
use crate::types::{Dimensions, PixelAspectRatio, RescaleParams, SOPClass, TransferSyntax};

#[derive(Debug, Clone)]
pub struct DicomMetadata {
    pub dimensions: Dimensions,
    pub rescale: RescaleParams,
    pub pixel_aspect_ratio: Option<PixelAspectRatio>,
    pub number_of_frames: u32,

    pub photometric_interpretation: PhotometricInterpretation,
    pub samples_per_pixel: u16,
    pub bits_allocated: u16,
    pub bits_stored: u16,
    pub planar_configuration: Option<u16>,

    pub(crate) pixel_data_format: DecodedPixelData,

    pub patient_name: Option<String>,
    pub patient_id: Option<String>,
    pub patient_birth_date: Option<String>,
    pub accession_number: Option<String>,
    pub study_date: Option<String>,
    pub study_description: Option<String>,
    pub modality: Option<String>,
    pub series_description: Option<String>,
    pub slice_thickness: Option<f64>,

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
}
