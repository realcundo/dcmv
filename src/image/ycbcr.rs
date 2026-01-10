//! YCbCr image conversion
//!
//! This module handles conversion of DICOM YCbCr pixel data to RGB images,
//! supporting full-resolution (YBR_FULL) and 4:2:2 subsampled (YBR_FULL_422)
//! formats with both interleaved and planar configurations.

use anyhow::{Context, Result};
use image::{DynamicImage, ImageBuffer, RgbImage};
use crate::dicom::DicomMetadata;

/// Convert YCbCr DICOM data to RGB image
///
/// Uses ITU-R BT.601 color space conversion for full-range YCbCr (YBR_FULL).
/// YBR_FULL_422 is handled the same way since pixel data is already decoded.
pub fn convert_ycbcr(metadata: &DicomMetadata) -> Result<DynamicImage> {
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
            let g = y.mul_add(1.0_f32, (cb - 128.0_f32).mul_add(-0.344_136_f32, (cr - 128.0_f32).mul_add(-0.714_136_f32, 0.0_f32)));
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
        u32::from(metadata.cols()),
        u32::from(metadata.rows()),
        rgb_pixels,
    )
    .context("Failed to create RGB image buffer from YCbCr")?;

    Ok(DynamicImage::ImageRgb8(rgb_image))
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

    let rows = metadata.rows() as usize;
    let cols = metadata.cols() as usize;
    let pixel_count = rows * cols;

    let data = metadata.pixel_data();

    // For multi-frame images, only extract the first frame
    let pixel_data = if metadata.number_of_frames > 1 {
        // Calculate expected size for first frame
        // YBR_FULL_422 subsampled: pixel_count * 2
        // Full resolution: pixel_count * 3
        let expected_full_size = pixel_count * 3;
        let expected_422_size = pixel_count * 2;

        // Determine which subsampling we have based on total data size
        let total_frames = data.len() / expected_full_size;
        let is_422 = if data.len().is_multiple_of(expected_full_size) {
            // Check if data size matches 422 subsampling
            data.len() == expected_422_size * total_frames
        } else {
            data.len() == expected_422_size * total_frames
        };

        let single_frame_size = if is_422 { expected_422_size } else { expected_full_size };

        if data.len() > single_frame_size {
            &data[..single_frame_size]
        } else {
            data
        }
    } else {
        data
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
                upsample_ycbcr_422_interleaved(pixel_data, rows, cols)
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
                upsample_ycbcr_422_planar(pixel_data, rows, cols, pixel_count)
            } else {
                // Full resolution planar: YYY... CbCbCb... CrCrCr...
                interleave_ycbcr_planar(pixel_data, pixel_count)
            }
        }
        Some(other) => anyhow::bail!(
            "Unsupported planar configuration for YCbCr: {other}"
        ),
    }
}

/// Upsample YBR_FULL_422 interleaved data to full resolution
///
/// Input format: Y0 Y1 Cb0 Cr0 Y2 Y3 Cb1 Cr1 ...
/// Each Cb/Cr pair covers 2 Y pixels horizontally
fn upsample_ycbcr_422_interleaved(pixel_data: &[u8], rows: usize, cols: usize) -> Result<Vec<u8>> {
    let pixel_count = rows * cols;
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
}

/// Upsample YBR_FULL_422 planar data to full resolution
///
/// Input format: Y plane (full), Cb plane (half-width), Cr plane (half-width)
fn upsample_ycbcr_422_planar(pixel_data: &[u8], rows: usize, cols: usize, pixel_count: usize) -> Result<Vec<u8>> {
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
}

/// Interleave full-resolution planar YCbCr data
///
/// Input format: YYY... CbCbCb... CrCrCr...
/// Output format: Y0 Cb0 Cr0 Y1 Cb1 Cr1 ...
fn interleave_ycbcr_planar(pixel_data: &[u8], pixel_count: usize) -> Result<Vec<u8>> {
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
        interleaved[i * 3] = pixel_data[i];                    // Y
        interleaved[i * 3 + 1] = pixel_data[pixel_count + i]; // Cb
        interleaved[i * 3 + 2] = pixel_data[pixel_count * 2 + i]; // Cr
    }

    Ok(interleaved)
}
