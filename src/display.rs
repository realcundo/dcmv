use anyhow::{anyhow, Result};
use image::DynamicImage;
use viuer::{print, Config as ViuerConfig};
use crate::cli::Args;
use crate::dicom::DicomMetadata;

/// Display an image in the terminal using viuer
pub fn print_image(image: &DynamicImage, metadata: &DicomMetadata, args: &Args) -> Result<()> {
    // Calculate pixel aspect ratio adjustment factor (defaults to 1.0 = no adjustment)
    // DICOM PAR is (vertical, horizontal): e.g., (1, 1) = square pixels, (2, 1) = pixels 2x taller
    let par_ratio = metadata.pixel_aspect_ratio
        .map(|(vertical, horizontal)| vertical / horizontal)
        .unwrap_or(1.0);

    // Determine dimensions: prefer width for aspect ratio, adjust height if specified
    let (config_width, config_height) = match (args.width, args.height) {
        (Some(w), ..) => (Some(w), None),
        (None, Some(h)) => (None, Some((h as f64 * par_ratio).round() as u32)),
        (None, None) => (Some(24), None),
    };

    // Viuer samples from the full resolution image for optimal quality
    // Print at current cursor location (not absolute 0,0) like cat does
    let config = ViuerConfig {
        width: config_width,
        height: config_height,
        absolute_offset: false,  // Relative to cursor, not top-left corner
        ..Default::default()
    };

    // Print image
    print(image, &config)
        .map_err(|e| anyhow!("Failed to display image: {}", e))?;

    Ok(())
}
