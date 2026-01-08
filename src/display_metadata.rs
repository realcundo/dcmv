use crate::dicom::DicomMetadata;

/// Print DICOM metadata to stdout
pub fn print_metadata(metadata: &DicomMetadata) {
    // Print fields directly from metadata struct
    print_field("Patient Name", &metadata.patient_name);
    print_field("Patient ID", &metadata.patient_id);
    print_field("Birth Date", &metadata.patient_birth_date);

    print_field("Accession Number", &metadata.accession_number);
    print_field("Study Date", &metadata.study_date);
    print_field("Study Description", &metadata.study_description);
    print_field("Modality", &metadata.modality);

    print_field("Series Description", &metadata.series_description);

    print_dimensions(metadata);

    print_pixel_aspect_ratio(metadata);
    print_sop_class_info(metadata);
    print_transfer_syntax_info(metadata);

    if let Some(thickness) = metadata.slice_thickness {
        println!("{:20}: {}", "Slice Thickness", thickness);
    }

    println!();
}

fn print_field(name: &str, value: &Option<String>) {
    if let Some(v) = value {
        println!("{name:20}: {v}");
    }
}

/// Print dimensions as "WIDTHxHEIGHTx(number_of_planes) [${photometric_interpretation}]"
fn print_dimensions(metadata: &DicomMetadata) {
    let dims = format!("{}x{}", metadata.cols, metadata.rows);
    println!(
        "{:20}: {}x{} [{}]",
        "Dimensions",
        dims,
        metadata.samples_per_pixel,
        metadata.photometric_interpretation
    );
}

fn print_pixel_aspect_ratio(metadata: &DicomMetadata) {
    if let Some((vertical, horizontal)) = metadata.pixel_aspect_ratio {
        println!("{:20}: {}:{}", "Pixel Aspect Ratio", vertical, horizontal);
    }
}

fn print_sop_class_info(metadata: &DicomMetadata) {
    if let Some((uid, name)) = &metadata.sop_class {
        println!("{:20}: {} ({})", "SOP Class UID", name, uid);
    }
}

fn print_transfer_syntax_info(metadata: &DicomMetadata) {
    let (uid, name) = &metadata.transfer_syntax;
    println!("{:20}: {} ({})", "Transfer Syntax", name, uid);
}
