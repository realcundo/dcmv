use anyhow::Result;
use clap::{CommandFactory, Parser};
use dcmv::cli::Args;
use dcmv::dicom;
use dcmv::image;
use dcmv::display;

fn main() -> Result<()> {
    let args = Args::parse();

    // Show help if no files provided
    if args.files.is_empty() {
        Args::command().print_help()?;
        println!();
        return Ok(());
    }

    let multiple_files = args.files.len() > 1;

    for (idx, file_path) in args.files.iter().enumerate() {
        // Print filename if multiple files
        if multiple_files {
            println!("{}", file_path.display());
        }

        // Open and parse DICOM file
        let obj = dicom::open_dicom_file(file_path)?;

        // Display metadata if verbose (between filename and image)
        if args.verbose {
            dicom::print_metadata(&obj);
        }

        // Extract DICOM data
        let metadata = dicom::extract_dicom_data(
            &obj,
            args.window_center,
            args.window_width,
        )?;

        // Convert to image
        let image = image::convert_to_image(&metadata)?;

        // Display image in terminal
        display::print_image(&image, &metadata, &args)?;

        // Add blank line between files when processing multiple files
        if multiple_files && idx < args.files.len() - 1 {
            println!();
        }
    }

    Ok(())
}
