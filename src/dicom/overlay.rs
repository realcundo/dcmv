//! Rendering of DICOM overlay planes on top of converted images
//!
//! Overlay planes (repeating groups 6000-601E) carry burned-in graphics
//! such as annotations, localization marks, and regions of interest.

use crate::dicom::DicomObject;
use anyhow::{Context, Result};
use dicom::pixeldata::{OverlayPlane, PixelDecoder};
use image::{DynamicImage, Rgb, RgbImage};

/// Color used to paint overlay pixels.
///
/// Yellow stays visible on both dark and bright image regions
/// in terminal rendering.
const OVERLAY_COLOR: Rgb<u8> = Rgb([255, 255, 0]);

/// The frame displayed by the viewer (0-based)
const DISPLAYED_FRAME: u32 = 0;

/// Decode all overlay planes of the object and paint them onto the image.
///
/// Returns the number of overlay planes that changed image pixels.
///
/// # Errors
///
/// Returns an error if the overlay planes cannot be decoded
/// or the image is not in RGB8 form.
pub fn apply_overlays(obj: &DicomObject, image: &mut DynamicImage) -> Result<usize> {
    let planes = obj
        .decode_overlays()
        .context("Failed to decode overlay planes")?;

    let rgb = image
        .as_mut_rgb8()
        .context("Expected RGB8 image for overlay rendering")?;

    Ok(planes
        .iter()
        .filter(|plane| paint_plane(rgb, plane))
        .count())
}

/// Paint a single overlay plane onto the image,
/// returning whether any pixel was changed.
///
/// Pixels outside the image canvas are clipped.
fn paint_plane(image: &mut RgbImage, plane: &OverlayPlane) -> bool {
    let Some(overlay_frame) = plane.frame_for_image_frame(DISPLAYED_FRAME) else {
        return false;
    };

    let (width, height) = (i64::from(image.width()), i64::from(image.height()));
    // origins are 1-based: an origin of [1, 1] aligns the overlay with the image
    let [origin_row, origin_col] = plane.origin();

    let mut painted = false;
    for row in 0..u32::from(plane.rows()) {
        for col in 0..u32::from(plane.columns()) {
            if !plane.is_set(overlay_frame, row, col) {
                continue;
            }

            let y = i64::from(origin_row) - 1 + i64::from(row);
            let x = i64::from(origin_col) - 1 + i64::from(col);
            if (0..width).contains(&x) && (0..height).contains(&y) {
                image.put_pixel(x as u32, y as u32, OVERLAY_COLOR);
                painted = true;
            }
        }
    }
    painted
}

#[cfg(test)]
mod tests {
    use super::*;
    use dicom::core::value::PrimitiveValue;
    use dicom::core::{DataElement, Tag, VR, dicom_value};
    use dicom::object::{FileDicomObject, FileMetaTableBuilder};
    use image::GenericImageView;
    use image::{Rgb, Rgba};

    /// `DynamicImage::get_pixel` always yields RGBA pixels
    const BLACK: Rgba<u8> = Rgba([0, 0, 0, 255]);
    const BLACK_RGB: Rgb<u8> = Rgb([0, 0, 0]);
    const YELLOW: Rgba<u8> = Rgba([255, 255, 0, 255]);

    fn empty_file_object() -> DicomObject {
        FileDicomObject::new_empty_with_meta(
            FileMetaTableBuilder::new()
                .transfer_syntax("1.2.840.10008.1.2.1") // Explicit VR Little Endian
                .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.7") // Secondary Capture
                .media_storage_sop_instance_uid("2.25.221545913343461877066277885826230616209")
                .build()
                .unwrap(),
        )
    }

    /// Build a DICOM object with a single 4x4 graphics overlay plane
    /// whose set bits form a 2x2 block at rows 1-2, columns 1-2.
    fn object_with_overlay(origin: [i16; 2]) -> DicomObject {
        // 1-bit packed bitmap, row-major, LSB first:
        // row 1 sets bits 5,6 (byte 0 = 96), row 2 sets bits 9,10 (byte 1 = 6)
        let packed = vec![96u8, 6];

        let mut obj = empty_file_object();
        let put = |obj: &mut DicomObject, element: u16, vr: VR, value: PrimitiveValue| {
            obj.put_element(DataElement::new(Tag(0x6000, element), vr, value));
        };

        put(&mut obj, 0x0010, VR::US, PrimitiveValue::from(4u16)); // Overlay Rows
        put(&mut obj, 0x0011, VR::US, PrimitiveValue::from(4u16)); // Overlay Columns
        put(&mut obj, 0x0015, VR::US, PrimitiveValue::from(1u16)); // Frames in Overlay
        put(&mut obj, 0x0040, VR::CS, PrimitiveValue::from("G")); // Type: Graphics
        put(
            &mut obj,
            0x0050,
            VR::SS,
            dicom_value!(I16, [origin[0], origin[1]]),
        ); // Origin
        put(&mut obj, 0x0051, VR::US, PrimitiveValue::from(1u16)); // Image Frame Origin
        put(&mut obj, 0x0100, VR::US, PrimitiveValue::from(1u16)); // Bits Allocated
        put(&mut obj, 0x0102, VR::US, PrimitiveValue::from(0u16)); // Bit Position
        put(&mut obj, 0x3000, VR::OW, PrimitiveValue::from(packed)); // Overlay Data
        obj
    }

    fn black_image(size: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(size, size, BLACK_RGB))
    }

    #[test]
    fn test_paint_overlay_bits_at_origin() {
        let obj = object_with_overlay([1, 1]);
        let mut image = black_image(4);

        let painted = apply_overlays(&obj, &mut image).unwrap();
        assert_eq!(painted, 1);

        // the 2x2 block of set bits is painted yellow
        for (row, col) in [(1, 1), (1, 2), (2, 1), (2, 2)] {
            assert_eq!(
                image.get_pixel(col, row),
                YELLOW,
                "pixel ({row},{col}) should be painted"
            );
        }

        // corners stay untouched
        for (row, col) in [(0, 0), (0, 3), (3, 0), (3, 3)] {
            assert_eq!(image.get_pixel(col, row), BLACK);
        }
    }

    #[test]
    fn test_overlay_shifted_by_origin() {
        // origin [2, 3] shifts the bitmap right by 2 and down by 1
        let obj = object_with_overlay([2, 3]);
        let mut image = black_image(4);

        let painted = apply_overlays(&obj, &mut image).unwrap();
        assert_eq!(painted, 1);
        assert_eq!(image.get_pixel(3, 2), YELLOW);
        assert_eq!(image.get_pixel(0, 1), BLACK);
    }

    #[test]
    fn test_overlay_clipped_to_image_bounds() {
        // origin beyond the canvas: nothing can be painted
        let obj = object_with_overlay([5, 5]);
        let mut image = black_image(4);

        let painted = apply_overlays(&obj, &mut image).unwrap();
        assert_eq!(painted, 0);

        for row in 0..4 {
            for col in 0..4 {
                assert_eq!(image.get_pixel(col, row), BLACK);
            }
        }
    }

    #[test]
    fn test_no_overlay_planes_leaves_image_unchanged() {
        let obj = empty_file_object();
        let mut image = black_image(4);

        let painted = apply_overlays(&obj, &mut image).unwrap();
        assert_eq!(painted, 0);
        assert_eq!(image.get_pixel(0, 0), BLACK);
    }
}
