//! DICOM pixel data extraction
//!
//! This module handles pixel data extraction from DICOM objects, including
//! compression detection, endianness conversion, and pixel data decoding.

use anyhow::{Context, Result};
use dicom::object::{
    FileDicomObject,
    InMemDicomObject,
    StandardDataDictionary
};
use dicom::pixeldata::PixelDecoder;

/// Format of extracted pixel data
///
/// This enum tracks whether pixel data needs further conversion or is already
/// in a displayable format. The `Vec<u8>` is moved (not copied) when the enum
/// is transferred, ensuring zero-copy of the actual pixel buffer.
#[derive(Debug, Clone)]
pub enum DecodedPixelData {
    /// YCbCr pixel data that needs conversion to RGB
    YcbCr(Vec<u8>),
    /// RGB pixel data (already converted or originally RGB)
    Rgb(Vec<u8>),
    /// Grayscale or other pixel data in native format
    Native(Vec<u8>),
}

/// Extract pixel data from DICOM object, handling compression and endianness
///
/// Returns a `DecodedPixelData` enum indicating the format of the pixel data.
/// Uses `to_dynamic_image()` for supported formats (YBR_FULL, RGB planar, big-endian).
/// For JPEG-compressed YCbCr images, the decoder already converts to RGB.
/// For uncompressed YCbCr, we return `YcbCr` for manual conversion via
/// the ITU-R BT.601 color space.
pub fn extract_pixel_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
    bits_allocated: u16,
    photometric_interpretation: &str,
    transfer_syntax_uid: &str,
    planar_configuration: Option<u16>,
) -> Result<DecodedPixelData> {
    // Explicit VR Big Endian UID (retired but still in use in legacy files)
    const EXPLICIT_VR_BIG_ENDIAN_UID: &str = "1.2.840.10008.1.2.2";

    let is_big_endian = transfer_syntax_uid == EXPLICIT_VR_BIG_ENDIAN_UID;
    let is_compressed = detect_compression(transfer_syntax_uid);
    let is_ycbcr = photometric_interpretation.contains("YBR");

    // Phase 1: Use to_dynamic_image() for YBR_FULL (not YBR_FULL_422, not compressed)
    if photometric_interpretation == "YBR_FULL" && !is_compressed {
        return extract_via_dynamic_image(obj);
    }

    // Phase 2: Use to_dynamic_image() for 8-bit RGB with planar configuration
    if photometric_interpretation == "RGB" && planar_configuration == Some(1) && bits_allocated == 8 && !is_compressed {
        return extract_via_dynamic_image(obj);
    }

    // Phase 3: Use to_dynamic_image() for big-endian 16-bit RGB (not grayscale, not YCbCr)
    if bits_allocated == 16 && is_big_endian && !is_ycbcr && photometric_interpretation == "RGB" && !is_compressed {
        return extract_via_dynamic_image(obj);
    }

    // Determine the data format based on compression and photometric interpretation
    let format = if is_compressed && is_ycbcr {
        // JPEG decoder converts YCbCr → RGB automatically
        DecodedPixelFormat::Rgb
    } else if is_ycbcr || photometric_interpretation == "PALETTE COLOR" || bits_allocated == 32 {
        DecodedPixelFormat::YcbCr
    } else {
        DecodedPixelFormat::Native
    };

    let data = if !is_compressed && matches!(format, DecodedPixelFormat::YcbCr) {
        extract_raw_pixel_data(obj)?
    } else {
        extract_decoded_pixel_data(obj, bits_allocated)?
    };

    Ok(match format {
        DecodedPixelFormat::YcbCr => DecodedPixelData::YcbCr(data),
        DecodedPixelFormat::Rgb => DecodedPixelData::Rgb(data),
        DecodedPixelFormat::Native => DecodedPixelData::Native(data),
    })
}

/// Internal format classification for pixel data
#[derive(Clone, Copy, PartialEq, Eq)]
enum DecodedPixelFormat {
    YcbCr,
    Rgb,
    Native,
}

/// Extract pixel data using to_dynamic_image() for supported formats
///
/// This function leverages dicom-pixeldata's native conversions for:
/// - YBR_FULL → RGB color space conversion
/// - RGB planar → RGB interleaved conversion
/// - Big-endian → little-endian byte order conversion
fn extract_via_dynamic_image(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<DecodedPixelData> {
    use dicom::pixeldata::{ConvertOptions, PixelDecoder};
    use image::DynamicImage::*;

    let decoded_pixel_data = obj
        .decode_pixel_data()
        .context("Failed to decode pixel data")?;

    // Use minimal conversion options (no modality LUT)
    let options = ConvertOptions::new()
        .with_modality_lut(dicom::pixeldata::ModalityLutOption::None);

    let dynamic_image = decoded_pixel_data
        .to_dynamic_image_with_options(0, &options)
        .context("Failed to convert to DynamicImage via to_dynamic_image_with_options")?;

    // Extract RGB bytes from DynamicImage
    let rgb_bytes = match dynamic_image {
        ImageRgb8(buffer) => buffer.into_raw(),
        _ => {
            anyhow::bail!(
                "Expected RGB8 image from to_dynamic_image conversion, got {:?}",
                dynamic_image.color()
            );
        }
    };

    Ok(DecodedPixelData::Rgb(rgb_bytes))
}

/// Detect if transfer syntax uses compression
#[inline]
#[must_use]
fn detect_compression(uid: &str) -> bool {
    uid.contains("1.2.840.10008.1.2.4")   // JPEG family
        || uid.contains("1.2.840.10008.1.2.4.50")  // JPEG Baseline
        || uid.contains("1.2.840.10008.1.2.5")   // RLE lossless
        || uid.contains("JPEG2000")
}

/// Extract raw pixel data (for YCbCr, Palette, 32-bit)
fn extract_raw_pixel_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<Vec<u8>> {
    use dicom::dictionary_std::tags;

    let pixel_data_obj = obj
        .get(tags::PIXEL_DATA)
        .context("Missing pixel data")?;

    Ok(pixel_data_obj
        .to_bytes()
        .context("Failed to get raw pixel data bytes")?
        .to_vec())
}

/// Extract decoded pixel data (handles compression)
fn extract_decoded_pixel_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
    bits_allocated: u16,
) -> Result<Vec<u8>> {
    let decoded_pixel_data = obj
        .decode_pixel_data()
        .context("Failed to decode pixel data")?;

    if bits_allocated == 32 {
        // 32-bit pixel data
        let data = decoded_pixel_data
            .to_vec::<u32>()
            .context("Failed to convert 32-bit pixel data")?
            .iter()
            .flat_map(|&v| v.to_le_bytes())
            .collect();
        Ok(data)
    } else if bits_allocated == 16 {
        // 16-bit pixel data - use raw data to avoid LUT issues
        Ok(decoded_pixel_data.data().to_vec())
    } else {
        // 8-bit
        Ok(decoded_pixel_data
            .to_vec::<u8>()
            .context("Failed to convert pixel data to bytes")?)
    }
}
