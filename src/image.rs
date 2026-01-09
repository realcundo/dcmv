use anyhow::{Context, Result};
use image::{DynamicImage, ImageBuffer, RgbImage};
use crate::dicom::{DicomMetadata, PhotometricInterpretation};

/// Convert DICOM pixel data to a DynamicImage
pub fn convert_to_image(metadata: &DicomMetadata) -> Result<DynamicImage> {
    match metadata.photometric_interpretation {
        PhotometricInterpretation::Monochrome1 | PhotometricInterpretation::Monochrome2 => {
            convert_grayscale(metadata)
        }
        PhotometricInterpretation::Rgb => {
            convert_rgb(metadata)
        }
        _ => {
            anyhow::bail!(
                "Unsupported photometric interpretation: {:?}",
                metadata.photometric_interpretation
            )
        }
    }
}

/// Convert grayscale DICOM data to RGB image
///
/// Uses f32 for calculations which may be faster due to:
/// - Better SIMD utilization (8 floats per AVX2 register vs 4 for f64)
/// - Reduced memory bandwidth for intermediate values
fn convert_grayscale(metadata: &DicomMetadata) -> Result<DynamicImage> {
    let pixel_data = extract_grayscale_pixels(metadata)?;

    // Convert rescale parameters to f32
    let slope = metadata.rescale_slope as f32;
    let intercept = metadata.rescale_intercept as f32;

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
        u32::from(metadata.cols),
        u32::from(metadata.rows),
        rgb_pixels
    ).context("Failed to create RGB image buffer")?;

    Ok(DynamicImage::ImageRgb8(rgb_image))
}

/// Convert RGB DICOM data to RGB image
fn convert_rgb(metadata: &DicomMetadata) -> Result<DynamicImage> {
    let pixel_data = extract_rgb_pixels(metadata)?;

    // For RGB, we don't apply window/level or rescale
    // Just convert to proper format

    let rgb_image: RgbImage = ImageBuffer::from_raw(
        u32::from(metadata.cols),
        u32::from(metadata.rows),
        pixel_data
    ).context("Failed to create RGB image buffer")?;

    Ok(DynamicImage::ImageRgb8(rgb_image))
}

/// Extract grayscale pixel data from raw bytes based on bit depth
fn extract_grayscale_pixels(metadata: &DicomMetadata) -> Result<Vec<u16>> {
    match metadata.bits_allocated {
        8 => {
            // 8-bit grayscale: each byte is a pixel
            Ok(metadata.pixel_data.iter().map(|&b| u16::from(b)).collect())
        }
        16 => {
            // 16-bit grayscale: each pair of bytes is a pixel
            if !metadata.pixel_data.len().is_multiple_of(2) {
                anyhow::bail!("Invalid 16-bit pixel data length");
            }

            // Pixel data is normalized to little-endian in dicom.rs
            Ok(metadata
                .pixel_data
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect())
        }
        _ => anyhow::bail!(
            "Unsupported bits allocated for grayscale: {}",
            metadata.bits_allocated
        ),
    }
}

/// Extract RGB pixel data from raw bytes, handling planar configuration
fn extract_rgb_pixels(metadata: &DicomMetadata) -> Result<Vec<u8>> {
    let expected_size = metadata.rows as usize
        * metadata.cols as usize
        * 3; // 3 samples per pixel

    if metadata.pixel_data.len() != expected_size {
        anyhow::bail!(
            "Invalid RGB pixel data size: expected {} bytes, got {}",
            expected_size,
            metadata.pixel_data.len()
        );
    }

    match metadata.planar_configuration {
        None | Some(0) => {
            // Planar Configuration 0: interleaved RGBRGB...
            // Already in the format we need
            Ok(metadata.pixel_data.clone())
        }
        Some(1) => {
            // Planar Configuration 1: planar RRR...GGG...BBB...
            // Need to reorganize from planar to interleaved
            let pixel_count = metadata.rows as usize * metadata.cols as usize;
            let mut interleaved = vec![0u8; expected_size];

            for (i, pixel) in interleaved.chunks_exact_mut(3).enumerate() {
                pixel[0] = metadata.pixel_data[i];              // R
                pixel[1] = metadata.pixel_data[pixel_count + i]; // G
                pixel[2] = metadata.pixel_data[2 * pixel_count + i]; // B
            }

            Ok(interleaved)
        }
        Some(other) => anyhow::bail!(
            "Unsupported planar configuration: {other}"
        ),
    }
}
