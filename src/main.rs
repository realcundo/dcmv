use anyhow::Result;
use clap::{CommandFactory, Parser};
use dcmv::cli::Args;
use dcmv::dicom;
use dcmv::display;
use dcmv::image;

fn main() {
    let args = Args::parse();

    if args.files.is_empty() {
        let _ = Args::command().print_help();
        println!();
        return;
    }

    let multiple_files = args.files.len() > 1;
    let mut any_failed = false;

    for (idx, file_path) in args.files.iter().enumerate() {
        if multiple_files {
            println!("{}", file_path.display());
        }

        if let Err(e) = process_file(file_path, &args) {
            eprintln!("Error: {e}");
            any_failed = true;
        }

        if multiple_files && idx < args.files.len() - 1 {
            println!();
        }
    }

    if any_failed {
        std::process::exit(1);
    }
}

/// Process a single DICOM file
fn process_file(file_path: &std::path::Path, args: &Args) -> Result<()> {
    let obj = dicom::open_dicom_file(file_path)?;

    let metadata = dicom::extract_dicom_data(&obj)?;

    if args.verbose {
        dcmv::print_metadata(&metadata);
    }

    let image = image::convert_to_image(&metadata)?;

    display::print_image(&image, &metadata, args)?;

    Ok(())
}
