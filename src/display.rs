use anyhow::{anyhow, Result};
use image::DynamicImage;
use viuer::{print, Config as ViuerConfig};
use crate::cli::Args;
use crate::dicom::DicomMetadata;
use std::io::{IsTerminal, Write};

pub fn print_image(image: &DynamicImage, metadata: &DicomMetadata, args: &Args) -> Result<()> {
    let is_tty = std::io::stdout().is_terminal();

    // PAR = (vertical, horizontal): (1,1)=square, (2,1)=2x tall pixels
    let par_ratio = metadata.pixel_aspect_ratio
        .map_or(1.0, |par| par.ratio());

    let (config_width, config_height) = match (args.width, args.height) {
        (Some(w), ..) => (Some(w), None),
        (None, Some(h)) => (None, Some((f64::from(h) * par_ratio).round() as u32)),
        (None, None) => (Some(24), None),
    };

    let config = ViuerConfig {
        width: config_width,
        height: config_height,
        absolute_offset: false,
        use_kitty: is_tty,
        use_iterm: is_tty,
        use_sixel: is_tty,
        ..Default::default()
    };

    std::io::stdout().flush()
        .map_err(|e| anyhow!("Failed to flush stdout: {e}"))?;

    print(image, &config)
        .map_err(|e| anyhow!("Failed to display image: {e}"))?;

    Ok(())
}
