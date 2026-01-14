use anyhow::{Context, Result};
use dicom::core::header::HasLength;
use dicom::dictionary_std::tags;
use dicom::object::{FileDicomObject, InMemDicomObject, StandardDataDictionary};
use dicom::pixeldata::{ConvertOptions, PixelDecoder};
use dicom::transfer_syntax::entries;
use image::DynamicImage::ImageRgb8;

#[derive(Debug, Clone)]
pub enum DecodedPixelData {
    YcbCr(Box<[u8]>),
    Rgb(Box<[u8]>),
    Native(Box<[u8]>),
}

pub fn extract_pixel_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
    bits_allocated: u16,
    photometric_interpretation: &str,
    transfer_syntax_uid: &str,
    planar_configuration: Option<u16>,
) -> Result<DecodedPixelData> {
    // Check for pixel data presence early (without reading data into memory)
    match obj.element(tags::PIXEL_DATA) {
        Ok(element) => {
            // Check if pixel data is empty (zero length)
            if element.is_empty() {
                anyhow::bail!("Pixel data is empty (zero length)");
            }
        }
        Err(_) => {
            anyhow::bail!("This DICOM file does not contain pixel data");
        }
    }

    let is_big_endian = transfer_syntax_uid == entries::EXPLICIT_VR_BIG_ENDIAN.uid();
    let compressed = is_compressed(transfer_syntax_uid);
    let is_ycbcr = photometric_interpretation.contains("YBR");

    if photometric_interpretation == "YBR_FULL" && !compressed {
        return extract_via_dynamic_image(obj);
    }

    if photometric_interpretation == "RGB"
        && planar_configuration == Some(1)
        && bits_allocated == 8
        && !compressed
    {
        return extract_via_dynamic_image(obj);
    }

    if bits_allocated == 16
        && is_big_endian
        && !is_ycbcr
        && photometric_interpretation == "RGB"
        && !compressed
    {
        return extract_via_dynamic_image(obj);
    }

    let format = if is_ycbcr || photometric_interpretation == "PALETTE COLOR" || bits_allocated == 32 {
        DecodedPixelFormat::YcbCr
    } else {
        DecodedPixelFormat::Native
    };

    let data = if !compressed && matches!(format, DecodedPixelFormat::YcbCr) {
        extract_raw_pixel_data(obj)?
    } else {
        extract_decoded_pixel_data(obj, bits_allocated)?
    };

    Ok(match format {
        DecodedPixelFormat::YcbCr => DecodedPixelData::YcbCr(data),
        DecodedPixelFormat::Native => DecodedPixelData::Native(data),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DecodedPixelFormat {
    YcbCr,
    Native,
}

fn extract_via_dynamic_image(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<DecodedPixelData> {
    let decoded_pixel_data = obj
        .decode_pixel_data()
        .context("Failed to decode pixel data")?;

    let options =
        ConvertOptions::new().with_modality_lut(dicom::pixeldata::ModalityLutOption::None);

    let dynamic_image = decoded_pixel_data
        .to_dynamic_image_with_options(0, &options)
        .context("Failed to convert to DynamicImage via to_dynamic_image_with_options")?;

    let rgb_bytes = match dynamic_image {
        ImageRgb8(buffer) => buffer.into_raw().into_boxed_slice(),
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
fn is_compressed(uid: &str) -> bool {
    uid == entries::JPEG_BASELINE.uid()
        || uid == entries::JPEG_EXTENDED.uid()
        || uid == entries::JPEG_LOSSLESS_NON_HIERARCHICAL.uid()
        || uid == entries::JPEG_LOSSLESS_NON_HIERARCHICAL_FIRST_ORDER_PREDICTION.uid()
        || uid == entries::JPEG_2000_IMAGE_COMPRESSION.uid()
        || uid == entries::JPEG_2000_IMAGE_COMPRESSION_LOSSLESS_ONLY.uid()
        || uid == entries::RLE_LOSSLESS.uid()
        || uid == "1.2.840.10008.1.2.4.80" // JPEG-LS Lossless
        || uid == "1.2.840.10008.1.2.4.81" // JPEG-LS Near Lossless
}

fn extract_raw_pixel_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
) -> Result<Box<[u8]>> {
    use dicom::dictionary_std::tags;

    let pixel_data_obj = obj.get(tags::PIXEL_DATA).context("Missing pixel data")?;

    Ok(pixel_data_obj
        .to_bytes()
        .context("Failed to get raw pixel data bytes")?
        .into_owned()
        .into_boxed_slice())
}

fn extract_decoded_pixel_data(
    obj: &FileDicomObject<InMemDicomObject<StandardDataDictionary>>,
    bits_allocated: u16,
) -> Result<Box<[u8]>> {
    let decoded_pixel_data = obj
        .decode_pixel_data()
        .context("Failed to decode pixel data")?;

    if bits_allocated == 32 {
        let data = decoded_pixel_data
            .to_vec::<u32>()
            .context("Failed to convert 32-bit pixel data")?
            .iter()
            .flat_map(|&v| v.to_le_bytes())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(data)
    } else if bits_allocated == 16 {
        Ok(decoded_pixel_data.data().to_vec().into_boxed_slice())
    } else {
        Ok(decoded_pixel_data
            .to_vec::<u8>()
            .context("Failed to convert pixel data to bytes")?
            .into_boxed_slice())
    }
}
