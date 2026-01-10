use crate::dicom::DicomMetadata;

pub fn print_metadata(metadata: &DicomMetadata) {
    print_field("Patient Name", metadata.patient_name.as_ref());
    print_field("Patient ID", metadata.patient_id.as_ref());
    print_field("Birth Date", metadata.patient_birth_date.as_ref());

    print_field("Accession Number", metadata.accession_number.as_ref());
    print_field("Study Date", metadata.study_date.as_ref());
    print_field("Study Description", metadata.study_description.as_ref());
    print_field("Modality", metadata.modality.as_ref());

    print_field("Series Description", metadata.series_description.as_ref());

    print_dimensions(metadata);

    print_pixel_aspect_ratio(metadata);
    print_sop_class_info(metadata);
    print_transfer_syntax_info(metadata);

    if let Some(thickness) = metadata.slice_thickness {
        println!("{:20}: {}", "Slice Thickness", thickness);
    }

    println!();
}

fn print_field(name: &str, value: Option<&String>) {
    if let Some(v) = value {
        println!("{name:20}: {v}");
    }
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
    }
}

fn print_sop_class_info(metadata: &DicomMetadata) {
    if let Some(sop_class) = &metadata.sop_class {
        println!("{:20}: {}", "SOP Class UID", sop_class);
    }
}

fn print_transfer_syntax_info(metadata: &DicomMetadata) {
    println!("{:20}: {}", "Transfer Syntax", metadata.transfer_syntax);
}
