use crate::dicom::DicomMetadata;

/// String displayed for missing DICOM tags in verbose mode
const UNKNOWN_TAG_VALUE: &str = "unknown";

pub fn print_metadata(metadata: &DicomMetadata) {
    print_field("Patient Name", metadata.patient_name());
    print_field("Patient ID", metadata.patient_id());
    print_field("Birth Date", metadata.patient_birth_date());

    print_field("Accession Number", metadata.accession_number());
    print_field("Study Date", metadata.study_date());
    print_field("Study Description", metadata.study_description());
    print_field("Modality", metadata.modality());

    print_field("Series Description", metadata.series_description());

    print_dimensions(metadata);

    print_pixel_aspect_ratio(metadata);
    print_sop_class_info(metadata);
    print_transfer_syntax_info(metadata);

    println!();
}

fn print_field(name: &str, value: Option<&str>) {
    let display_value = value.unwrap_or(UNKNOWN_TAG_VALUE);
    println!("{name:20}: {display_value}");
}

fn print_dimensions(metadata: &DicomMetadata) {
    let dims = format!("{}x{}", metadata.cols(), metadata.rows());
    println!(
        "{:20}: {}x{} [{}]",
        "Dimensions", dims, metadata.samples_per_pixel, metadata.photometric_interpretation
    );
}

fn print_pixel_aspect_ratio(metadata: &DicomMetadata) {
    if let Some(par) = &metadata.pixel_aspect_ratio {
        println!("{:20}: {}", "Pixel Aspect Ratio", par);
    } else {
        println!("{:20}: {}", "Pixel Aspect Ratio", UNKNOWN_TAG_VALUE);
    }
}

fn print_sop_class_info(metadata: &DicomMetadata) {
    let display_value = metadata
        .sop_class
        .as_ref()
        .map_or_else(|| UNKNOWN_TAG_VALUE.to_string(), ToString::to_string);
    println!("{:20}: {}", "SOP Class UID", display_value);
}

fn print_transfer_syntax_info(metadata: &DicomMetadata) {
    println!("{:20}: {}", "Transfer Syntax", metadata.transfer_syntax);
}
