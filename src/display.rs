use crate::cli::Args;
use crate::dicom::DicomMetadata;
use anyhow::{Result, anyhow};
use crossterm::{cursor::MoveToColumn, execute, terminal::Clear, terminal::ClearType};
use image::DynamicImage;
use std::io::{IsTerminal, Write};
use viuer::{Config as ViuerConfig, get_kitty_support, is_iterm_supported, print};

/// Initialize terminal graphics protocol detection at startup.
///
/// Forces viuer's terminal capability queries to happen once at startup
/// rather than during file processing, preventing escape sequences from
/// appearing randomly. Results are cached internally by viuer's `LazyLock`.
pub fn init_terminal_display() {
    // Only query protocols in TTY - skip if piped/redirected
    if std::io::stdout().is_terminal() {
        let _kitty = get_kitty_support();
        let _iterm = is_iterm_supported();

        // Clear line to hide escape sequences, then move cursor to start
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, Clear(ClearType::CurrentLine), MoveToColumn(0));
        let _ = stdout.flush();
    }
}

/// Print a DICOM image to the terminal using Sixel graphics
///
/// # Errors
///
/// Returns an error if terminal rendering fails
pub fn print_image(image: &DynamicImage, metadata: &DicomMetadata, args: &Args) -> Result<()> {
    let is_tty = std::io::stdout().is_terminal();

    // PAR = (vertical, horizontal): (1,1)=square, (2,1)=2x tall pixels
    let par_ratio = metadata.pixel_aspect_ratio.map_or(1.0, |par| par.ratio());

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

    std::io::stdout()
        .flush()
        .map_err(|e| anyhow!("Failed to flush stdout: {e}"))?;

    print(image, &config).map_err(|e| anyhow!("Failed to display image: {e}"))?;

    Ok(())
}
