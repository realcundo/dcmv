use criterion::{Criterion, criterion_group, criterion_main};
use dcmv::dicom;
use dcmv::image;
use std::hint::black_box;
use std::path::Path;

// ============================================================================
// TIER 1: FULL PIPELINE BENCHMARKS (Primary Baseline)
// ============================================================================

/// Full pipeline with file I/O (cold start)
/// Measures real-world CLI performance
fn bench_full_pipeline_cold(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline_cold");

    group.bench_function("file3_27mb", |b| {
        b.iter(|| {
            let file_path = Path::new(".test-files/file3.dcm");
            let obj = dicom::open_dicom_file(black_box(file_path)).unwrap();
            let metadata = dicom::extract_dicom_data(black_box(&obj)).unwrap();
            image::convert_to_image(black_box(&metadata)).unwrap()
        });
    });

    group.finish();
}

/// Full pipeline with cached data (warm start)
/// Measures processing performance isolated from I/O
fn bench_full_pipeline_warm(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline_warm");

    // Setup: Load file once
    let file_path = Path::new(".test-files/file3.dcm");
    let obj = dicom::open_dicom_file(file_path).unwrap();
    let metadata = dicom::extract_dicom_data(&obj).unwrap();

    group.bench_function("file3_cached", |b| {
        b.iter(|| {
            let result = image::convert_to_image(black_box(&metadata)).unwrap();
            black_box(result);
        });
    });

    group.finish();
}

// ============================================================================
// TIER 2: COMPONENT-LEVEL BENCHMARKS (Diagnostic)
// ============================================================================

/// Benchmark DICOM file parsing and metadata extraction
fn bench_dicom_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("dicom_parsing");

    group.bench_function("parse_file3", |b| {
        b.iter(|| {
            let file_path = Path::new(".test-files/file3.dcm");
            let obj = dicom::open_dicom_file(black_box(file_path)).unwrap();
            dicom::extract_dicom_data(black_box(&obj)).unwrap()
        });
    });

    group.finish();
}

/// Benchmark image conversion (pixel normalization)
fn bench_image_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("image_conversion");

    let file_path = Path::new(".test-files/file3.dcm");
    let obj = dicom::open_dicom_file(file_path).unwrap();
    let metadata = dicom::extract_dicom_data(&obj).unwrap();

    group.bench_function("convert_file3", |b| {
        b.iter(|| {
            let result = image::convert_to_image(black_box(&metadata)).unwrap();
            black_box(result);
        });
    });

    group.finish();
}

// ============================================================================
// BENCHMARK REGISTRATION
// ============================================================================

criterion_group!(
    benches,
    // Primary baseline (these run by default with `cargo bench`)
    bench_full_pipeline_cold,
    bench_full_pipeline_warm,
    // Diagnostic benchmarks (help identify bottlenecks)
    bench_dicom_parsing,
    bench_image_conversion,
);

criterion_main!(benches);
