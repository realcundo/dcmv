use criterion::{criterion_group, criterion_main, Criterion, Throughput};
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
// TIER 3: MICRO-BENCHMARKS (Algorithm-level)
// ============================================================================

/// Benchmark grayscale min/max calculation with actual pixel data
/// This tests the EXACT code path used in convert_grayscale (image.rs:28-32)
fn bench_grayscale_minmax(c: &mut Criterion) {
    let mut group = c.benchmark_group("grayscale_minmax");

    // Setup: Load actual pixel data from file3.dcm
    let file_path = Path::new(".test-files/file3.dcm");
    let obj = dicom::open_dicom_file(file_path).unwrap();
    let metadata = dicom::extract_dicom_data(&obj).unwrap();

    // Extract 16-bit grayscale pixels from raw bytes (same as extract_grayscale_pixels)
    // file3.dcm has bits_allocated=16, so we convert bytes to u16
    let pixel_data: Vec<u16> = metadata
        .pixel_data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    // Get rescale parameters from metadata
    let rescale_slope = metadata.rescale_slope;
    let rescale_intercept = metadata.rescale_intercept;

    group.throughput(Throughput::Elements(pixel_data.len() as u64));

    group.bench_function("file3_actual", |b| {
        b.iter(|| {
            // This is the EXACT code from image.rs:28-32
            let (min_val, max_val) = pixel_data.iter()
                .map(|&pixel| (pixel as f64 * rescale_slope) + rescale_intercept)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), val| {
                    (min.min(val), max.max(val))
                });
            black_box((min_val, max_val));
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

    // Micro-benchmarks (validate low-level optimizations)
    bench_grayscale_minmax,
);

criterion_main!(benches);
