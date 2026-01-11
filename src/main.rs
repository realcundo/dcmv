use clap::{CommandFactory, Parser};
use dcmv::cli::Args;
use dcmv::dicom::{self, ProcessError};
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
            println!("Error: {e}");
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
fn process_file(file_path: &std::path::Path, args: &Args) -> Result<(), ProcessError> {
    // Stage 1: Open DICOM file
    let obj = dicom::open_dicom_file(file_path)?;

    // Stage 2: Try to extract metadata and decode pixel data
    let metadata = match dicom::extract_dicom_data(&obj) {
        Ok(m) => m,
        Err(e) => {
            // Extraction failed - try to get partial metadata for verbose display
            let partial_metadata = dicom::extract_metadata_tags(&obj);

            if args.verbose
                && let Ok(meta) = partial_metadata {
                    // We have partial metadata - print it, then return error
                    dcmv::print_metadata(&meta);
                }

            return Err(ProcessError::ExtractionFailed(e));
        }
    };

    // Stage 3: Verbose output (print if extraction succeeded)
    if args.verbose {
        dcmv::print_metadata(&metadata);
    }

    // Stage 4: Convert to image
    let image = image::convert_to_image(&metadata)
        .map_err(|e| ProcessError::ConversionFailed {
            metadata: Box::new(metadata.clone()),
            error: e,
        })?;

    // Stage 5: Display
    display::print_image(&image, &metadata, args)
        .map_err(|e| ProcessError::DisplayFailed {
            metadata: Box::new(metadata),
            error: e,
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_rtstruct_returns_notadicomfile_error() {
        let file_path = Path::new(".test-files/rtstruct.dcm");
        assert!(file_path.exists());

        let args = Args {
            files: vec![file_path.to_path_buf()],
            verbose: true,
            width: None,
            height: None,
        };

        let result = process_file(file_path, &args);
        assert_matches::assert_matches!(result, Err(ProcessError::NotADicomFile(_)));
    }

    #[test]
    fn test_jpeg2k_returns_extractionfailed_error() {
        let file_path = Path::new(".test-files/examples_jpeg2k.dcm");
        assert!(file_path.exists());

        let args = Args {
            files: vec![file_path.to_path_buf()],
            verbose: true,
            width: None,
            height: None,
        };

        let result = process_file(file_path, &args);
        assert_matches::assert_matches!(result, Err(ProcessError::ExtractionFailed(_)));
    }
}
