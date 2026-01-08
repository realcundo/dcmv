use anyhow::Result;
use clap::{CommandFactory, Parser};
use dcmv::cli::Args;
use dcmv::dicom;
use dcmv::image;
use dcmv::display;

fn main() {
    let args = Args::parse();

    // Show help if no files provided
    if args.files.is_empty() {
        let _ = Args::command().print_help();
        println!();
        return;
    }

    let multiple_files = args.files.len() > 1;
    let mut any_failed = false;

    for (idx, file_path) in args.files.iter().enumerate() {
        // Print filename if multiple files
        if multiple_files {
            println!("{}", file_path.display());
        }

        // Process file and handle any errors
        if let Err(e) = process_file(file_path, &args) {
            eprintln!("Error: {e}");
            any_failed = true;
        }

        // Add blank line between files when processing multiple files
        if multiple_files && idx < args.files.len() - 1 {
            println!();
        }
    }

    // Exit with error code if any file failed
    if any_failed {
        std::process::exit(1);
    }
}

/// Process a single DICOM file
fn process_file(file_path: &std::path::Path, args: &Args) -> Result<()> {
    // Open and parse DICOM file
    let obj = dicom::open_dicom_file(file_path)?;

    // Extract DICOM data (parses everything once)
    let metadata = dicom::extract_dicom_data(&obj)?;

    // Display metadata if verbose (between filename and image)
    if args.verbose {
        dcmv::print_metadata(&metadata);
    }

    // Convert to image
    let image = image::convert_to_image(&metadata)?;

    // Display image in terminal
    display::print_image(&image, &metadata, args)?;

    Ok(())
}
