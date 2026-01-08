use anyhow::{Context, Result};
use image::{DynamicImage, ImageBuffer, RgbImage};
use crate::dicom::DicomMetadata;

/// Convert DICOM pixel data to a DynamicImage
pub fn convert_to_image(metadata: &DicomMetadata) -> Result<DynamicImage> {
    let DicomMetadata {
        rows,
        cols,
        rescale_slope,
        rescale_intercept,
        window_center,
        window_width,
        should_invert,
        pixel_data,
        ..
    } = metadata;

    let rgb_pixels: Vec<u8> = pixel_data.iter().flat_map(|pixel| {
        let rescaled = (*pixel as f64 * rescale_slope) + rescale_intercept;
        let normalized = match (window_center, window_width) {
            (Some(wc), Some(ww)) => {
                let ww_f64 = ww.max(1.0);
                let val = (rescaled - wc) / ww_f64 + 0.5;
                val.clamp(0.0, 1.0)
            }
            _ => (rescaled / 4095.0).clamp(0.0, 1.0)
        };
        let gray = (normalized * 255.0).clamp(0.0, 255.0) as u8;
        // Invert for MONOCHROME1 (min value = white, max value = black)
        let gray = if *should_invert { 255u8.saturating_sub(gray) } else { gray };
        [gray, gray, gray]
    }).collect();

    let rgb_image: RgbImage = ImageBuffer::from_raw(*cols as u32, *rows as u32, rgb_pixels)
        .context("Failed to create RGB image buffer")?;
    Ok(DynamicImage::ImageRgb8(rgb_image))
}
