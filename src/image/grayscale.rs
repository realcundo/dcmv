//! Grayscale image conversion
//!
//! This module handles conversion of DICOM grayscale pixel data to RGB images,
//! supporting various bit depths (8, 16, and 32 bits) and MONOCHROME1/MONOCHROME2
//! photometric interpretations.

use anyhow::{Context, Result};
use image::{DynamicImage, ImageBuffer, RgbImage};
use crate::dicom::DicomMetadata;
use super::normalization::find_min_max;

/// Convert grayscale DICOM data to RGB image
///
/// Uses f32 for calculations which may be faster due to:
/// - Better SIMD utilization (8 floats per AVX2 register vs 4 for f64)
/// - Reduced memory bandwidth for intermediate values
pub fn convert_grayscale(metadata: &DicomMetadata) -> Result<DynamicImage> {
    let pixel_data = extract_grayscale_pixels(metadata)?;

    // Convert rescale parameters to f32
    let slope = metadata.rescale_slope() as f32;
    let intercept = metadata.rescale_intercept() as f32;

    // First pass: calculate min and max from rescaled pixel values
    let (min_val, max_val) = pixel_data.iter()
        .map(|&pixel| f32::from(pixel).mul_add(slope, intercept))
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), val| {
            (min.min(val), max.max(val))
        });

    // Handle edge case: all pixels have the same value
    let range = if max_val > min_val {
        max_val - min_val
    } else {
        1.0_f32 // Prevent division by zero, all pixels will map to middle gray
    };

    // Pre-calculate inversion flag (avoid calling method for every pixel)
    let should_invert = metadata.photometric_interpretation.should_invert();

    // Second pass: normalize pixels to 0-255 range
    // Note: Clamping is theoretically unnecessary since we normalize to [min_val, max_val]
    // However, we keep it as safety against floating-point rounding errors
    let rgb_pixels: Vec<u8> = pixel_data.iter().flat_map(|&pixel| {
        let rescaled = f32::from(pixel).mul_add(slope, intercept);

        // Map [min_val, max_val] to [0, 255]
        let normalized = (rescaled - min_val) / range;
        // Saturating cast: values < 0 become 0, values > 255 become 255
        // This guards against floating-point rounding errors (e.g., -0.0, 255.0001)
        let gray = (normalized * 255.0_f32) as u8;

        // Invert for MONOCHROME1 (min=white, max=black)
        let gray = if should_invert {
            255u8.saturating_sub(gray)
        } else {
            gray
        };

        [gray, gray, gray]
    }).collect();

    let rgb_image: RgbImage = ImageBuffer::from_raw(
        u32::from(metadata.cols()),
        u32::from(metadata.rows()),
        rgb_pixels
    ).context("Failed to create RGB image buffer")?;

    Ok(DynamicImage::ImageRgb8(rgb_image))
}

/// Extract grayscale pixel data from raw bytes based on bit depth
fn extract_grayscale_pixels(metadata: &DicomMetadata) -> Result<Vec<u16>> {
    let pixel_data = metadata.pixel_data();

    match metadata.bits_allocated {
        8 => {
            // 8-bit grayscale: each byte is a pixel
            Ok(pixel_data.iter().map(|&b| u16::from(b)).collect())
        }
        16 => {
            // 16-bit grayscale: each pair of bytes is a pixel
            if !pixel_data.len().is_multiple_of(2) {
                anyhow::bail!("Invalid 16-bit pixel data length");
            }

            // Pixel data is normalized to little-endian in dicom.rs
            Ok(pixel_data
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect())
        }
        32 => {
            // 32-bit grayscale: normalize to 16-bit for processing
            // Use min/max normalization to preserve dynamic range
            if !pixel_data.len().is_multiple_of(4) {
                anyhow::bail!("Invalid 32-bit pixel data length");
            }

            // Extract 32-bit values
            let values: Vec<u32> = pixel_data
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            // Find min/max for normalization
            let (min, max) = find_min_max(&values);
            let range = if max > min { max - min } else { 1.0_f32 };

            // Normalize to 16-bit range
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
