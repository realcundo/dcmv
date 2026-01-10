use anyhow::{Context, Result};
use image::{DynamicImage, ImageBuffer, RgbImage};
use crate::dicom::DicomMetadata;

pub fn convert_ycbcr(metadata: &DicomMetadata) -> Result<DynamicImage> {
    let pixel_data = extract_ycbcr_pixels(metadata)?;

    let rgb_pixels: Vec<u8> = pixel_data
        .chunks_exact(3)
        .flat_map(|ycbcr| {
            let y = f32::from(ycbcr[0]);
            let cb = f32::from(ycbcr[1]);
            let cr = f32::from(ycbcr[2]);

            let r = y.mul_add(1.0_f32, (cr - 128.0_f32).mul_add(1.402_f32, 0.0_f32));
            let g = y.mul_add(1.0_f32, (cb - 128.0_f32).mul_add(-0.344_136_f32, (cr - 128.0_f32).mul_add(-0.714_136_f32, 0.0_f32)));
            let b = y.mul_add(1.0_f32, (cb - 128.0_f32).mul_add(1.772_f32, 0.0_f32));

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

fn extract_ycbcr_pixels(metadata: &DicomMetadata) -> Result<Vec<u8>> {
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

    let pixel_data = if metadata.number_of_frames > 1 {
        let expected_full_size = pixel_count * 3;
        let expected_422_size = pixel_count * 2;

        let total_frames = data.len() / expected_full_size;
        let is_422 = if data.len().is_multiple_of(expected_full_size) {
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

    let has_422_subsampling = pixel_data.len() == pixel_count * 2;

    match metadata.planar_configuration {
        None | Some(0) => {
            if has_422_subsampling {
                upsample_ycbcr_422_interleaved(pixel_data, rows, cols)
            } else {
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
            if has_422_subsampling {
                upsample_ycbcr_422_planar(pixel_data, rows, cols, pixel_count)
            } else {
                interleave_ycbcr_planar(pixel_data, pixel_count)
            }
        }
        Some(other) => anyhow::bail!(
            "Unsupported planar configuration for YCbCr: {other}"
        ),
    }
}

fn upsample_ycbcr_422_interleaved(pixel_data: &[u8], rows: usize, cols: usize) -> Result<Vec<u8>> {
    let pixel_count = rows * cols;
    let mut output = vec![0u8; pixel_count * 3];

    for y in 0..rows {
        let row_offset = y * (cols * 2);

        for x in 0..cols {
            let out_idx = (y * cols + x) * 3;

            let group_num = x / 2;
            let pos_in_group = x % 2;
            let group_offset = group_num * 4;

            output[out_idx] = pixel_data[row_offset + group_offset + pos_in_group];

            output[out_idx + 1] = pixel_data[row_offset + group_offset + 2];
            output[out_idx + 2] = pixel_data[row_offset + group_offset + 3];
        }
    }

    Ok(output)
}

fn upsample_ycbcr_422_planar(pixel_data: &[u8], rows: usize, cols: usize, pixel_count: usize) -> Result<Vec<u8>> {
    let y_plane = &pixel_data[..pixel_count];
    let chroma_size = pixel_count / 2;
    let cb_plane = &pixel_data[pixel_count..pixel_count + chroma_size];
    let cr_plane = &pixel_data[pixel_count + chroma_size..pixel_count + chroma_size * 2];

    let mut output = vec![0u8; pixel_count * 3];

    for y in 0..rows {
        for x in 0..cols {
            let out_idx = (y * cols + x) * 3;
            output[out_idx] = y_plane[y * cols + x];

            let chroma_x = x / 2;
            output[out_idx + 1] = cb_plane[y * (cols / 2) + chroma_x];
            output[out_idx + 2] = cr_plane[y * (cols / 2) + chroma_x];
        }
    }

    Ok(output)
}

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
        interleaved[i * 3] = pixel_data[i];
        interleaved[i * 3 + 1] = pixel_data[pixel_count + i];
        interleaved[i * 3 + 2] = pixel_data[pixel_count * 2 + i];
    }

    Ok(interleaved)
}
