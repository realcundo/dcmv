use anyhow::{bail, Result};
use crate::dicom::PhotometricInterpretation;

#[inline]
pub fn validate_photometric_samples(
    photometric_interpretation: &PhotometricInterpretation,
    samples_per_pixel: u16,
) -> Result<()> {
    let is_valid = match (photometric_interpretation, samples_per_pixel) {
        (pi, 1) if pi.is_grayscale() || matches!(pi, PhotometricInterpretation::Palette) => true,
        (pi, 3) if pi.is_rgb() || pi.is_ycbcr() => true,
        _ => false,
    };

    if !is_valid {
        bail!(
            "Inconsistent photometric interpretation {:?} with samples per pixel {}",
            photometric_interpretation,
            samples_per_pixel
        );
    }

    Ok(())
}

#[inline]
pub fn validate_planar_configuration(
    planar_configuration: Option<u16>,
    photometric_interpretation: &PhotometricInterpretation,
) -> Result<()> {
    if planar_configuration.is_some()
        && !photometric_interpretation.is_rgb()
        && !photometric_interpretation.is_ycbcr()
    {
        bail!("Planar configuration should only be present for RGB or YCbCr images");
    }

    Ok(())
}

#[inline]
pub fn validate_bits_allocated(bits_allocated: u16) -> Result<()> {
    if !matches!(bits_allocated, 8 | 16 | 32) {
        bail!(
            "Unsupported bits allocated: {bits_allocated} (expected 8, 16, or 32)"
        );
    }

    Ok(())
}

pub fn validate_metadata(
    photometric_interpretation: &PhotometricInterpretation,
    samples_per_pixel: u16,
    planar_configuration: Option<u16>,
    bits_allocated: u16,
) -> Result<()> {
    validate_photometric_samples(photometric_interpretation, samples_per_pixel)?;
    validate_planar_configuration(planar_configuration, photometric_interpretation)?;
    validate_bits_allocated(bits_allocated)?;
    Ok(())
}
