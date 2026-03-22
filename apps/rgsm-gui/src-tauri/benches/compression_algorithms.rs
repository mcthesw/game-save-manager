use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::fs::{self, File};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

#[derive(Clone, Copy)]
struct AlgorithmCase {
    name: &'static str,
    method: CompressionMethod,
    level: Option<i64>,
}

const ALGORITHMS: [AlgorithmCase; 4] = [
    AlgorithmCase {
        name: "stored",
        method: CompressionMethod::Stored,
        level: None,
    },
    AlgorithmCase {
        name: "deflated-l6",
        method: CompressionMethod::Deflated,
        level: Some(6),
    },
    AlgorithmCase {
        name: "bzip2-l9",
        method: CompressionMethod::Bzip2,
        level: Some(9),
    },
    AlgorithmCase {
        name: "zstd-l9",
        method: CompressionMethod::Zstd,
        level: Some(9),
    },
];

#[derive(Clone)]
struct PreparedArchive {
    case: AlgorithmCase,
    path: PathBuf,
    compressed_size: u64,
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn create_dataset(root: &Path, file_count: usize, file_size: usize) -> Result<(), std::io::Error> {
    fs::create_dir_all(root)?;
    let mut buffer = vec![0_u8; file_size];

    for index in 0..file_count {
        let subdir = root.join(format!("slot_{:03}", index / 200));
        fs::create_dir_all(&subdir)?;

        for (offset, byte) in buffer.iter_mut().enumerate() {
            *byte = ((index + offset) % 251) as u8;
        }

        let file_path = subdir.join(format!("save_{index:05}.bin"));
        fs::write(file_path, &buffer)?;
    }

    Ok(())
}

fn sum_file_sizes_recursive(root: &Path) -> Result<u64, std::io::Error> {
    if !root.exists() {
        return Ok(0);
    }

    let mut total_size = 0_u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                stack.push(entry_path);
            } else if entry_path.is_file() {
                total_size += entry.metadata()?.len();
            }
        }
    }

    Ok(total_size)
}

fn count_files_recursive(root: &Path) -> Result<usize, std::io::Error> {
    if !root.exists() {
        return Ok(0);
    }

    let mut count = 0_usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                stack.push(entry_path);
            } else if entry_path.is_file() {
                count += 1;
            }
        }
    }

    Ok(count)
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                stack.push(entry_path);
            } else if entry_path.is_file() {
                files.push(entry_path);
            }
        }
    }

    files.sort();
    Ok(files)
}

fn zip_options(case: AlgorithmCase) -> SimpleFileOptions {
    let options = SimpleFileOptions::default().compression_method(case.method);
    if let Some(level) = case.level {
        options.compression_level(Some(level))
    } else {
        options
    }
}

fn compress_dataset(
    source_root: &Path,
    zip_path: &Path,
    case: AlgorithmCase,
) -> Result<u64, Box<dyn std::error::Error>> {
    if let Some(parent) = zip_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = File::create(zip_path)?;
    let mut writer = ZipWriter::new(file);
    let options = zip_options(case);

    for file_path in collect_files(source_root)? {
        let relative = file_path
            .strip_prefix(source_root)?
            .to_string_lossy()
            .replace('\\', "/");
        writer.start_file(relative, options)?;
        let mut source = File::open(&file_path)?;
        std::io::copy(&mut source, &mut writer)?;
    }

    writer.finish()?;
    Ok(fs::metadata(zip_path)?.len())
}

fn extract_archive(
    zip_path: &Path,
    output_root: &Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    let mut restored_files = 0_usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(enclosed) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        let output_path = output_root.join(enclosed);

        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut output = File::create(&output_path)?;
        std::io::copy(&mut entry, &mut output)?;
        restored_files += 1;
    }

    Ok(restored_files)
}

fn prepare_archives(
    source_root: &Path,
    archive_root: &Path,
    raw_size_bytes: u64,
) -> Result<Vec<PreparedArchive>, Box<dyn std::error::Error>> {
    let mut prepared = Vec::with_capacity(ALGORITHMS.len());
    for case in ALGORITHMS {
        let archive_path = archive_root.join(format!("{}.zip", case.name));
        let started = Instant::now();
        let compressed_size = compress_dataset(source_root, &archive_path, case)?;
        let elapsed = started.elapsed();
        let ratio = if raw_size_bytes == 0 {
            0.0
        } else {
            compressed_size as f64 / raw_size_bytes as f64
        };
        let space_saving_pct = (1.0_f64 - ratio).max(0.0) * 100.0;
        println!(
            "BENCH ratio algorithm={} raw_size_bytes={} zip_size_bytes={} compression_ratio={:.4} space_saving_pct={:.2} compress_once_ms={}",
            case.name,
            raw_size_bytes,
            compressed_size,
            ratio,
            space_saving_pct,
            elapsed.as_millis()
        );
        prepared.push(PreparedArchive {
            case,
            path: archive_path,
            compressed_size,
        });
    }
    Ok(prepared)
}

fn benchmark_compression_algorithms(c: &mut Criterion) {
    let file_count = env_usize("RGSM_BENCH_FILE_COUNT", 1200);
    let file_size_kb = env_usize("RGSM_BENCH_FILE_SIZE_KB", 8);
    let file_size = file_size_kb * 1024;

    let temp_root = temp_dir::TempDir::new().expect("create benchmark temp dir");
    let source_root = temp_root.path().join("dataset");
    create_dataset(&source_root, file_count, file_size).expect("prepare benchmark dataset");
    let raw_size_bytes = sum_file_sizes_recursive(&source_root).expect("sum dataset size");
    let file_total = count_files_recursive(&source_root).expect("count dataset files");

    let archive_root = temp_root.path().join("archives");
    fs::create_dir_all(&archive_root).expect("create archive output dir");
    let prepared =
        prepare_archives(&source_root, &archive_root, raw_size_bytes).expect("prepare archives");

    println!(
        "BENCH dataset files={} file_size_kb={} raw_size_bytes={}",
        file_total, file_size_kb, raw_size_bytes
    );

    let mut compress_group = c.benchmark_group("compression/zip_compress");
    compress_group.sample_size(10);
    compress_group.measurement_time(Duration::from_secs(20));
    compress_group.throughput(Throughput::Bytes(raw_size_bytes));
    for case in ALGORITHMS {
        let bench_zip_path = temp_root
            .path()
            .join("bench")
            .join(format!("compress_{}.zip", case.name));
        compress_group.bench_with_input(
            BenchmarkId::from_parameter(case.name),
            &case,
            |b, case| {
                b.iter(|| {
                    let size = compress_dataset(&source_root, &bench_zip_path, *case)
                        .expect("compress dataset in benchmark");
                    black_box(size);
                    let _ = fs::remove_file(&bench_zip_path);
                });
            },
        );
    }
    compress_group.finish();

    let mut decompress_group = c.benchmark_group("compression/zip_decompress");
    decompress_group.sample_size(10);
    decompress_group.measurement_time(Duration::from_secs(20));
    for archive in prepared {
        decompress_group.throughput(Throughput::Bytes(archive.compressed_size));
        let archive_path = archive.path.clone();
        let algorithm_name = archive.case.name;
        decompress_group.bench_function(BenchmarkId::from_parameter(algorithm_name), |b| {
            b.iter(|| {
                let run_dir = temp_dir::TempDir::new().expect("create extract temp dir");
                let restored = extract_archive(&archive_path, run_dir.path())
                    .expect("extract archive in benchmark");
                black_box(restored);
            });
        });
    }
    decompress_group.finish();
}

criterion_group!(benches, benchmark_compression_algorithms);
criterion_main!(benches);
