use clap::{CommandFactory, Parser};
use dcmv::cli::Args;
use dcmv::dicom::{self, ProcessError, read_stdin, DicomObject};
use dcmv::display;
use dcmv::image;
use std::io::{self, IsTerminal};

fn main() {
    let args = Args::parse();

    // Show help if no files provided in TTY mode
    if args.files.is_empty() && io::stdin().is_terminal() {
        let _ = Args::command().print_help();
        println!();
        return;
    }

    // Initialize terminal display before processing to prevent escape sequence race conditions
    dcmv::init_terminal_display();

    let use_stdin = args.files.is_empty() && !io::stdin().is_terminal();

    if use_stdin {
        match read_stdin() {
            Ok(dcm) => {
                if let Err(e) = process_dicom(&dcm, &args) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    } else {
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
}

/// Process a parsed DICOM object (common logic for files and stdin)
fn process_dicom(obj: &DicomObject, args: &Args) -> Result<(), ProcessError> {
    let metadata = match dicom::extract_dicom_data(obj) {
        Ok(m) => m,
        Err(e) => {
            // Try to get partial metadata for verbose display before failing
            let partial_metadata = dicom::extract_metadata_tags(obj);

            if args.verbose
                && let Ok(meta) = partial_metadata {
                    dcmv::print_metadata(&meta);
                }

            return Err(ProcessError::ExtractionFailed(e));
        }
    };

    if args.verbose {
        dcmv::print_metadata(&metadata);
    }

    let image = image::convert_to_image(&metadata)
        .map_err(|e| ProcessError::ConversionFailed {
            metadata: Box::new(metadata.clone()),
            error: e,
        })?;

    display::print_image(&image, &metadata, args)
        .map_err(|e| ProcessError::DisplayFailed {
            metadata: Box::new(metadata),
            error: e,
        })?;

    Ok(())
}

/// Process a single DICOM file
fn process_file(file_path: &std::path::Path, args: &Args) -> Result<(), ProcessError> {
    let obj = dicom::open_dicom_file(file_path)?;
    process_dicom(&obj, args)
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
