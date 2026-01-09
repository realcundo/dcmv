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
        PhotometricInterpretation::YbrFull | PhotometricInterpretation::YbrFull422 => {
            convert_ycbcr(metadata)
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

/// Convert YCbCr DICOM data to RGB image
///
/// Uses ITU-R BT.601 color space conversion for full-range YCbCr (YBR_FULL).
/// YBR_FULL_422 is handled the same way since pixel data is already decoded.
fn convert_ycbcr(metadata: &DicomMetadata) -> Result<DynamicImage> {
    let pixel_data = extract_ycbcr_pixels(metadata)?;

    // YBR_FULL uses full range (0-255), not video range
    // Conversion formulas from ITU-R BT.601:
    // R = Y + 1.402 * (Cr - 128)
    // G = Y - 0.344136 * (Cb - 128) - 0.714136 * (Cr - 128)
    // B = Y + 1.772 * (Cb - 128)

    let rgb_pixels: Vec<u8> = pixel_data
        .chunks_exact(3)
        .flat_map(|ycbcr| {
            let y = f32::from(ycbcr[0]);
            let cb = f32::from(ycbcr[1]);
            let cr = f32::from(ycbcr[2]);

            // Convert to RGB using full-range coefficients
            let r = y.mul_add(1.0_f32, (cr - 128.0_f32).mul_add(1.402_f32, 0.0_f32));
            let g = y.mul_add(1.0_f32, (cb - 128.0_f32).mul_add(-0.344136_f32, (cr - 128.0_f32).mul_add(-0.714136_f32, 0.0_f32)));
            let b = y.mul_add(1.0_f32, (cb - 128.0_f32).mul_add(1.772_f32, 0.0_f32));

            // Clamp to valid range and convert to u8
            [
                r.clamp(0.0, 255.0) as u8,
                g.clamp(0.0, 255.0) as u8,
                b.clamp(0.0, 255.0) as u8,
            ]
        })
        .collect();

    let rgb_image: RgbImage = ImageBuffer::from_raw(
        u32::from(metadata.cols),
        u32::from(metadata.rows),
        rgb_pixels,
    )
    .context("Failed to create RGB image buffer from YCbCr")?;

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
        32 => {
            // 32-bit grayscale: normalize to 16-bit for processing
            // Use min/max normalization to preserve dynamic range
            if !metadata.pixel_data.len().is_multiple_of(4) {
                anyhow::bail!("Invalid 32-bit pixel data length");
            }

            // Extract 32-bit values
            let values: Vec<u32> = metadata
                .pixel_data
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

/// Extract RGB pixel data from raw bytes, handling bit depth and planar configuration
fn extract_rgb_pixels(metadata: &DicomMetadata) -> Result<Vec<u8>> {
    match metadata.bits_allocated {
        8 => extract_rgb_8bit(metadata),
        32 => extract_rgb_32bit(metadata),
        _ => anyhow::bail!(
            "Unsupported bits allocated for RGB: {} (expected 8 or 32)",
            metadata.bits_allocated
        ),
    }
}

/// Extract 8-bit RGB pixel data with planar configuration handling
fn extract_rgb_8bit(metadata: &DicomMetadata) -> Result<Vec<u8>> {
    let bytes_per_sample = (metadata.bits_allocated / 8) as usize;
    let pixels_per_frame = metadata.rows as usize * metadata.cols as usize;
    let expected_size = pixels_per_frame * 3 * bytes_per_sample;

    // For multi-frame images, only extract the first frame
    let pixel_data = if metadata.pixel_data.len() > expected_size {
        &metadata.pixel_data[..expected_size]
    } else {
        &metadata.pixel_data
    };

    if pixel_data.len() != expected_size {
        anyhow::bail!(
            "Invalid RGB pixel data size: expected {} bytes for first frame, got {}",
            expected_size,
            pixel_data.len()
        );
    }

    match metadata.planar_configuration {
        None | Some(0) => {
            // Planar Configuration 0: interleaved RGBRGB...
            Ok(pixel_data.to_vec())
        }
        Some(1) => {
            // Planar Configuration 1: planar RRR...GGG...BBB...
            let mut interleaved = vec![0u8; expected_size];

            for (i, pixel) in interleaved.chunks_exact_mut(3).enumerate() {
                pixel[0] = pixel_data[i];
                pixel[1] = pixel_data[pixels_per_frame + i];
                pixel[2] = pixel_data[2 * pixels_per_frame + i];
            }

            Ok(interleaved)
        }
        Some(other) => anyhow::bail!(
            "Unsupported planar configuration: {other}"
        ),
    }
}

/// Extract 32-bit RGB pixel data and normalize to 8-bit
fn extract_rgb_32bit(metadata: &DicomMetadata) -> Result<Vec<u8>> {
    let pixel_count = metadata.rows as usize * metadata.cols as usize;

    // For multi-frame images, only extract the first frame
    let bytes_per_sample = (metadata.bits_allocated / 8) as usize;
    let expected_size = pixel_count * 3 * bytes_per_sample;

    let pixel_data = if metadata.pixel_data.len() > expected_size {
        &metadata.pixel_data[..expected_size]
    } else {
        &metadata.pixel_data
    };

    if pixel_data.len() != expected_size {
        anyhow::bail!(
            "Invalid RGB pixel data size: expected {} bytes for first frame, got {}",
            expected_size,
            pixel_data.len()
        );
    }

    // Parse 32-bit RGB values
    let mut r_values = Vec::with_capacity(pixel_count);
    let mut g_values = Vec::with_capacity(pixel_count);
    let mut b_values = Vec::with_capacity(pixel_count);

    match metadata.planar_configuration {
        None | Some(0) => {
            // Interleaved: R0(4B) G0(4B) B0(4B) R1(4B) G1(4B) B1(4B)...
            for chunk in pixel_data.chunks_exact(12) {
                let r = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                let g = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
                let b = u32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]);
                r_values.push(r);
                g_values.push(g);
                b_values.push(b);
            }
        }
        Some(1) => {
            // Planar: RRRR... GGGG... BBBB... (each 4 bytes per sample)
            let bytes_per_channel = pixel_count * 4;

            let r_data = &pixel_data[..bytes_per_channel];
            let g_data = &pixel_data[bytes_per_channel..2 * bytes_per_channel];
            let b_data = &pixel_data[2 * bytes_per_channel..];

            for chunk in r_data.chunks_exact(4) {
                r_values.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
            for chunk in g_data.chunks_exact(4) {
                g_values.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
            for chunk in b_data.chunks_exact(4) {
                b_values.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        Some(other) => anyhow::bail!(
            "Unsupported planar configuration for 32-bit RGB: {other}"
        ),
    }

    // Find min/max for each channel for normalization
    let (r_min, r_max) = find_min_max(&r_values);
    let (g_min, g_max) = find_min_max(&g_values);
    let (b_min, b_max) = find_min_max(&b_values);

    // Calculate ranges (avoid division by zero)
    let r_range = if r_max > r_min { r_max - r_min } else { 1.0_f32 };
    let g_range = if g_max > g_min { g_max - g_min } else { 1.0_f32 };
    let b_range = if b_max > b_min { b_max - b_min } else { 1.0_f32 };

    // Normalize to 0-255 and interleave
    let mut result = Vec::with_capacity(pixel_count * 3);
    for i in 0..pixel_count {
        let r = normalize_u32_to_u8(r_values[i], r_min, r_range);
        let g = normalize_u32_to_u8(g_values[i], g_min, g_range);
        let b = normalize_u32_to_u8(b_values[i], b_min, b_range);
        result.push(r);
        result.push(g);
        result.push(b);
    }

    Ok(result)
}

/// Find min and max values in a slice of u32
fn find_min_max(values: &[u32]) -> (f32, f32) {
    values.iter().fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(min, max), &val| {
            let val_f32 = val as f32;
            (min.min(val_f32), max.max(val_f32))
        },
    )
}

/// Normalize a u32 value from [min, max] range to [0, 255] as u8
fn normalize_u32_to_u8(value: u32, min: f32, range: f32) -> u8 {
    let value_f32 = value as f32;
    let normalized = (value_f32 - min) / range;
    (normalized * 255.0_f32) as u8
}

/// Extract YCbCr pixel data from raw bytes
///
/// YCbCr data is stored as interleaved Y, Cb, Cr values (planar_configuration = 0)
/// or in separate planes (planar_configuration = 1).
/// For uncompressed data, we expect 8-bit YCbCr samples.
///
/// YBR_FULL_422 has 2:1 horizontal chroma subsampling, so we need to upsample Cb/Cr.
fn extract_ycbcr_pixels(metadata: &DicomMetadata) -> Result<Vec<u8>> {
    // YCbCr should be 8-bit
    if metadata.bits_allocated != 8 {
        anyhow::bail!(
            "Unsupported bits allocated for YCbCr: {} (expected 8)",
            metadata.bits_allocated
        );
    }

    let rows = metadata.rows as usize;
    let cols = metadata.cols as usize;
    let pixel_count = rows * cols;

    // For multi-frame images, only extract the first frame
    let pixel_data = if metadata.number_of_frames > 1 {
        // Calculate expected size for first frame
        // YBR_FULL_422 subsampled: pixel_count * 2
        // Full resolution: pixel_count * 3
        let expected_full_size = pixel_count * 3;
        let expected_422_size = pixel_count * 2;

        // Determine which subsampling we have based on total data size
        let total_frames = metadata.pixel_data.len() / expected_full_size;
        let is_422 = if metadata.pixel_data.len() % expected_full_size == 0 {
            // Check if data size matches 422 subsampling
            metadata.pixel_data.len() == expected_422_size * total_frames
        } else {
            metadata.pixel_data.len() == expected_422_size * total_frames
        };

        let single_frame_size = if is_422 { expected_422_size } else { expected_full_size };

        if metadata.pixel_data.len() > single_frame_size {
            &metadata.pixel_data[..single_frame_size]
        } else {
            &metadata.pixel_data
        }
    } else {
        &metadata.pixel_data
    };

    // Check if we have subsampled data (YBR_FULL_422)
    // Full size would be pixel_count * 3
    // With 422 subsampling: Y (pixel_count) + Cb (pixel_count / 2) + Cr (pixel_count / 2)
    let has_422_subsampling = pixel_data.len() == pixel_count * 2;

    match metadata.planar_configuration {
        None | Some(0) => {
            // Interleaved format - for subsampled data, we need to upsample
            if has_422_subsampling {
                // YBR_FULL_422: Data is arranged as Y0 Y1 Cb0 Cr0 Y2 Y3 Cb1 Cr1 ...
                // Each Cb/Cr pair covers 2 Y pixels horizontally
                let mut output = vec![0u8; pixel_count * 3];

                for y in 0..rows {
                    for x in 0..cols {
                        let out_idx = (y * cols + x) * 3;
                        let in_idx = y * (cols * 2) + x;

                        // Y is at even positions in the input stream
                        let y_idx = if x % 2 == 0 { in_idx } else { in_idx - 1 };
                        output[out_idx] = pixel_data[y_idx];

                        // Cb and Cr are shared between pairs of pixels
                        let chroma_idx = y * cols + (x / 2) * 2 + cols;
                        output[out_idx + 1] = pixel_data[chroma_idx];     // Cb
                        output[out_idx + 2] = pixel_data[chroma_idx + 1]; // Cr
                    }
                }

                Ok(output)
            } else {
                // Full resolution interleaved YCbCr: Y0 Cb0 Cr0 Y1 Cb1 Cr1...
                if pixel_data.len() != pixel_count * 3 {
                    anyhow::bail!(
                        "Invalid YCbCr pixel data size: expected {} bytes, got {}",
                        pixel_count * 3,
                        pixel_data.len()
                    );
                }
                Ok(pixel_data.to_vec())
            }
        }
        Some(1) => {
            // Planar format
            if has_422_subsampling {
                // Planar with 422: Y plane is full, Cb/Cr planes are half-width
                let y_plane = &pixel_data[..pixel_count];
                let chroma_size = pixel_count / 2;
                let cb_plane = &pixel_data[pixel_count..pixel_count + chroma_size];
                let cr_plane = &pixel_data[pixel_count + chroma_size..pixel_count + chroma_size * 2];

                let mut output = vec![0u8; pixel_count * 3];

                for y in 0..rows {
                    for x in 0..cols {
                        let out_idx = (y * cols + x) * 3;
                        output[out_idx] = y_plane[y * cols + x]; // Y

                        // Upsample chroma horizontally
                        let chroma_x = x / 2;
                        output[out_idx + 1] = cb_plane[y * (cols / 2) + chroma_x]; // Cb
                        output[out_idx + 2] = cr_plane[y * (cols / 2) + chroma_x]; // Cr
                    }
                }

                Ok(output)
            } else {
                // Full resolution planar: YYY... CbCbCb... CrCrCr...
                let expected_size = pixel_count * 3;
                if pixel_data.len() != expected_size {
                    anyhow::bail!(
                        "Invalid YCbCr pixel data size: expected {} bytes, got {}",
                        expected_size,
                        pixel_data.len()
                    );
                }

                let mut interleaved = vec![0u8; expected_size];

                for i in 0..pixel_count {
                    interleaved[i * 3] = pixel_data[i];              // Y
                    interleaved[i * 3 + 1] = pixel_data[pixel_count + i]; // Cb
                    interleaved[i * 3 + 2] = pixel_data[pixel_count * 2 + i]; // Cr
                }

                Ok(interleaved)
            }
        }
        Some(other) => anyhow::bail!(
            "Unsupported planar configuration for YCbCr: {other}"
        ),
    }
}
