use anyhow::{Context, Result};
use dicom::object::{
    FileDicomObject,
    InMemDicomObject,
    StandardDataDictionary
};
use dicom::pixeldata::PixelDecoder;

#[derive(Debug, Clone)]
pub enum DecodedPixelData {
    YcbCr(Vec<u8>),
    Rgb(Vec<u8>),
    Native(Vec<u8>),
}

pub fn extract_pixel_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
    bits_allocated: u16,
    photometric_interpretation: &str,
    transfer_syntax_uid: &str,
    planar_configuration: Option<u16>,
) -> Result<DecodedPixelData> {
    const EXPLICIT_VR_BIG_ENDIAN_UID: &str = "1.2.840.10008.1.2.2";

    let is_big_endian = transfer_syntax_uid == EXPLICIT_VR_BIG_ENDIAN_UID;
    let is_compressed = detect_compression(transfer_syntax_uid);
    let is_ycbcr = photometric_interpretation.contains("YBR");

    if photometric_interpretation == "YBR_FULL" && !is_compressed {
        return extract_via_dynamic_image(obj);
    }

    if photometric_interpretation == "RGB" && planar_configuration == Some(1) && bits_allocated == 8 && !is_compressed {
        return extract_via_dynamic_image(obj);
    }

    if bits_allocated == 16 && is_big_endian && !is_ycbcr && photometric_interpretation == "RGB" && !is_compressed {
        return extract_via_dynamic_image(obj);
    }

    let format = if is_compressed && is_ycbcr {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum DecodedPixelFormat {
    YcbCr,
    Rgb,
    Native,
}

fn extract_via_dynamic_image(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<DecodedPixelData> {
    use dicom::pixeldata::{ConvertOptions, PixelDecoder};
    use image::DynamicImage::*;

    let decoded_pixel_data = obj
        .decode_pixel_data()
        .context("Failed to decode pixel data")?;

    let options = ConvertOptions::new()
        .with_modality_lut(dicom::pixeldata::ModalityLutOption::None);

    let dynamic_image = decoded_pixel_data
        .to_dynamic_image_with_options(0, &options)
        .context("Failed to convert to DynamicImage via to_dynamic_image_with_options")?;

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

#[inline]
#[must_use]
fn detect_compression(uid: &str) -> bool {
    uid.contains("1.2.840.10008.1.2.4")   // JPEG family
        || uid.contains("1.2.840.10008.1.2.4.50")  // JPEG Baseline
        || uid.contains("1.2.840.10008.1.2.5")   // RLE lossless
        || uid.contains("JPEG2000")
}

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

fn extract_decoded_pixel_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
    bits_allocated: u16,
) -> Result<Vec<u8>> {
    let decoded_pixel_data = obj
        .decode_pixel_data()
        .context("Failed to decode pixel data")?;

    if bits_allocated == 32 {
        let data = decoded_pixel_data
            .to_vec::<u32>()
            .context("Failed to convert 32-bit pixel data")?
            .iter()
            .flat_map(|&v| v.to_le_bytes())
            .collect();
        Ok(data)
    } else if bits_allocated == 16 {
        Ok(decoded_pixel_data.data().to_vec())
    } else {
        Ok(decoded_pixel_data
            .to_vec::<u8>()
            .context("Failed to convert pixel data to bytes")?)
    }
}
