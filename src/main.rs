use anyhow::Result;
use clap::Parser;
use dcmv::cli::Args;
use dcmv::dicom;
use dcmv::image;
use dcmv::display;

fn main() -> Result<()> {
    let args = Args::parse();

    // Open and parse DICOM file
    let obj = dicom::open_dicom_file(&args.file)?;

    // Display metadata if verbose
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

    Ok(())
}
