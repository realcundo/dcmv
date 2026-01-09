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

/// Extract pixel data from DICOM object, handling compression and endianness
pub fn extract_pixel_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
    bits_allocated: u16,
    photometric_interpretation: &str,
    transfer_syntax_uid: &str,
) -> Result<Vec<u8>> {
    #[allow(deprecated)]
    use dicom::dictionary_std::uids::EXPLICIT_VR_BIG_ENDIAN;

    let is_big_endian = transfer_syntax_uid == EXPLICIT_VR_BIG_ENDIAN;
    let is_compressed = detect_compression(transfer_syntax_uid);
    let needs_raw_fallback = !is_compressed && (
        photometric_interpretation.contains("YBR")
        || photometric_interpretation == "PALETTE COLOR"
        || bits_allocated == 32
    );

    if bits_allocated == 16 && is_big_endian {
        extract_big_endian_16bit(obj)
    } else if needs_raw_fallback {
        extract_raw_pixel_data(obj)
    } else {
        extract_decoded_pixel_data(obj, bits_allocated)
    }
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

/// Extract big-endian 16-bit pixel data and convert to little-endian
fn extract_big_endian_16bit(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<Vec<u8>> {
    use dicom::dictionary_std::tags;

    let pixel_data_obj = obj
        .get(tags::PIXEL_DATA)
        .context("Missing pixel data")?;

    let raw_bytes = pixel_data_obj
        .to_bytes()
        .context("Failed to get raw pixel data bytes")?;

    if !raw_bytes.len().is_multiple_of(2) {
        anyhow::bail!("Invalid 16-bit pixel data length");
    }

    // Convert big-endian bytes to u16 values, store as little-endian
    Ok(raw_bytes
        .chunks_exact(2)
        .flat_map(|chunk| {
            let value = u16::from_be_bytes([chunk[0], chunk[1]]);
            value.to_le_bytes()
        })
        .collect())
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
