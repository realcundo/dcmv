use super::normalization::{find_min_max, normalize_u32_to_u8};
use crate::dicom::DicomMetadata;
use anyhow::{Context, Result};
use image::{DynamicImage, ImageBuffer, RgbImage};

/// Convert RGB DICOM pixel data to a `DynamicImage`
///
/// # Errors
///
/// Returns an error if pixel data extraction or image buffer creation fails
pub fn convert_rgb(metadata: &DicomMetadata) -> Result<DynamicImage> {
    let pixel_data = extract_rgb_pixels(metadata)?;

    let rgb_image: RgbImage = ImageBuffer::from_raw(
        u32::from(metadata.cols()),
        u32::from(metadata.rows()),
        pixel_data,
    )
    .context("Failed to create RGB image buffer")?;

    Ok(DynamicImage::ImageRgb8(rgb_image))
}

fn extract_rgb_pixels(metadata: &DicomMetadata) -> Result<Vec<u8>> {
    match metadata.bits_allocated() {
        8 => extract_rgb_8bit(metadata),
        32 => extract_rgb_32bit(metadata),
        // TODO: Add 16-bit RGB support
        // Failing files: SC_rgb_rle_16bit.dcm, SC_rgb_rle_16bit_2frame.dcm
        // Need to normalize 16-bit RGB values to 8-bit (similar to 32-bit implementation)
        _ => anyhow::bail!(
            "Unsupported bits allocated for RGB: {} (expected 8 or 32)",
            metadata.bits_allocated()
        ),
    }
}

fn extract_rgb_8bit(metadata: &DicomMetadata) -> Result<Vec<u8>> {
    let bytes_per_sample = (metadata.bits_allocated() / 8) as usize;
    let pixels_per_frame = metadata.rows() as usize * metadata.cols() as usize;
    let expected_size = pixels_per_frame * 3 * bytes_per_sample;

    let data = metadata.pixel_data();

    let pixel_data = if data.len() > expected_size {
        &data[..expected_size]
    } else {
        data
    };

    if pixel_data.len() != expected_size {
        anyhow::bail!(
            "Invalid RGB pixel data size: expected {} bytes for first frame, got {}",
            expected_size,
            pixel_data.len()
        );
    }

    Ok(pixel_data.to_vec())
}

fn extract_rgb_32bit(metadata: &DicomMetadata) -> Result<Vec<u8>> {
    let pixel_count = metadata.rows() as usize * metadata.cols() as usize;

    let bytes_per_sample = (metadata.bits_allocated() / 8) as usize;
    let expected_size = pixel_count * 3 * bytes_per_sample;

    let data = metadata.pixel_data();
    let pixel_data = if data.len() > expected_size {
        &data[..expected_size]
    } else {
        data
    };

    if pixel_data.len() != expected_size {
        anyhow::bail!(
            "Invalid RGB pixel data size: expected {} bytes for first frame, got {}",
            expected_size,
            pixel_data.len()
        );
    }

    let mut r_values = Vec::with_capacity(pixel_count);
    let mut g_values = Vec::with_capacity(pixel_count);
    let mut b_values = Vec::with_capacity(pixel_count);

    match metadata.planar_configuration {
        None | Some(0) => {
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
        Some(other) => anyhow::bail!("Unsupported planar configuration for 32-bit RGB: {other}"),
    }

    let (r_min, r_max) = find_min_max(&r_values);
    let (g_min, g_max) = find_min_max(&g_values);
    let (b_min, b_max) = find_min_max(&b_values);

    let r_range = if r_max > r_min {
        r_max - r_min
    } else {
        1.0_f32
    };
    let g_range = if g_max > g_min {
        g_max - g_min
    } else {
        1.0_f32
    };
    let b_range = if b_max > b_min {
        b_max - b_min
    } else {
        1.0_f32
    };

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
