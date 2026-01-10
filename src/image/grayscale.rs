use super::normalization::find_min_max;
use crate::dicom::DicomMetadata;
use anyhow::{Context, Result};
use image::{DynamicImage, ImageBuffer, RgbImage};

// f32: better SIMD (8 floats/AVX2 reg vs 4)
/// Convert grayscale DICOM pixel data to a `DynamicImage`
///
/// # Errors
///
/// Returns an error if pixel data extraction or conversion fails
pub fn convert_grayscale(metadata: &DicomMetadata) -> Result<DynamicImage> {
    let pixel_data = extract_grayscale_pixels(metadata)?;

    let slope = metadata.rescale_slope() as f32;
    let intercept = metadata.rescale_intercept() as f32;

    let (min_val, max_val) = pixel_data
        .iter()
        .map(|&pixel| f32::from(pixel).mul_add(slope, intercept))
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), val| {
            (min.min(val), max.max(val))
        });

    let range = if max_val > min_val {
        max_val - min_val
    } else {
        1.0_f32
    };

    let should_invert = metadata.photometric_interpretation.should_invert();

    let rgb_pixels: Vec<u8> = pixel_data
        .iter()
        .flat_map(|&pixel| {
            let rescaled = f32::from(pixel).mul_add(slope, intercept);

            let normalized = (rescaled - min_val) / range;
            let gray = (normalized * 255.0_f32) as u8;

            let gray = if should_invert {
                255u8.saturating_sub(gray)
            } else {
                gray
            };

            [gray, gray, gray]
        })
        .collect();

    let rgb_image: RgbImage = ImageBuffer::from_raw(
        u32::from(metadata.cols()),
        u32::from(metadata.rows()),
        rgb_pixels,
    )
    .context("Failed to create RGB image buffer")?;

    Ok(DynamicImage::ImageRgb8(rgb_image))
}

fn extract_grayscale_pixels(metadata: &DicomMetadata) -> Result<Vec<u16>> {
    let pixel_data = metadata.pixel_data();

    match metadata.bits_allocated {
        8 => Ok(pixel_data.iter().map(|&b| u16::from(b)).collect()),
        16 => {
            if !pixel_data.len().is_multiple_of(2) {
                anyhow::bail!("Invalid 16-bit pixel data length");
            }

            Ok(pixel_data
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect())
        }
        32 => {
            if !pixel_data.len().is_multiple_of(4) {
                anyhow::bail!("Invalid 32-bit pixel data length");
            }

            let values: Vec<u32> = pixel_data
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            let (min, max) = find_min_max(&values);
            let range = if max > min { max - min } else { 1.0_f32 };

            Ok(values
                .iter()
                .map(|&v| {
                    let v_f32 = v as f32;
                    let normalized = (v_f32 - min) / range;
                    (normalized * 65535.0_f32) as u16
                })
                .collect())
        }
        _ => anyhow::bail!(
            "Unsupported bits allocated for grayscale: {}",
            metadata.bits_allocated
        ),
    }
}
