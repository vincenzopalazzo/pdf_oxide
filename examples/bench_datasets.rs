//! Benchmark all slow PDF datasets with timing and error reporting
//! Usage: bench_datasets [pdf_dir1] [pdf_dir2] ...
//! If no args, benchmarks pdfs_slow through pdfs_slow5

use pdf_oxide::PdfDocument;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let dirs: Vec<PathBuf> = if args.is_empty() {
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let base = PathBuf::from(home).join("projects/pdf_oxide_tests");
        vec![
            base.join("pdfs_slow/slow_pdfs"),
            base.join("pdfs_slow2"),
            base.join("pdfs_slow3"),
            base.join("pdfs_slow4"),
            base.join("pdfs_slow5"),
            base.join("pdfs_slow6"),
        ]
    } else {
        args.iter().map(PathBuf::from).collect()
    };

    let timeout = Duration::from_secs(30);
    let mut total = 0u32;
    let mut pass = 0u32;
    let mut fail_count = 0u32;
    let mut slow_count = 0u32;
    let mut failures: Vec<(String, String, u128)> = Vec::new();
    let mut slow_pdfs: Vec<(String, u128)> = Vec::new();

    for dir in &dirs {
        if !dir.exists() {
            eprintln!("SKIP: {} (not found)", dir.display());
            continue;
        }

        let dir_name = dir.file_name().unwrap_or_default().to_string_lossy();
        eprintln!("\n=== {} ===", dir_name);

        let mut entries: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map_or(false, |ext| ext == "pdf" || ext == "PDF")
            })
            .collect();
        entries.sort_by_key(|e| e.file_name());

        let mut dir_pass = 0u32;
        let mut dir_fail = 0u32;

        for entry in &entries {
            let path = entry.path();
            let fname = entry.file_name().to_string_lossy().to_string();
            total += 1;

            let start = Instant::now();
            let result = std::panic::catch_unwind(|| extract_all_pages(&path, timeout));
            let elapsed = start.elapsed().as_millis();

            match result {
                Ok(Ok(char_count)) => {
                    if elapsed > 2_000 {
                        eprintln!("  SLOW   {:>6}ms  {:>8}ch  {}", elapsed, char_count, fname);
                        slow_pdfs.push((fname.clone(), elapsed));
                        slow_count += 1;
                    }
                    pass += 1;
                    dir_pass += 1;
                }
                Ok(Err(e)) => {
                    let err_short = format!("{}", e).chars().take(80).collect::<String>();
                    eprintln!("  FAIL   {:>6}ms  {}: {}", elapsed, fname, err_short);
                    failures.push((fname.clone(), err_short, elapsed));
                    fail_count += 1;
                    dir_fail += 1;
                }
                Err(panic_info) => {
                    let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_info.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };
                    let err_short: String = msg.chars().take(80).collect();
                    eprintln!("  PANIC  {:>6}ms  {}: {}", elapsed, fname, err_short);
                    failures.push((fname.clone(), format!("PANIC: {}", err_short), elapsed));
                    fail_count += 1;
                    dir_fail += 1;
                }
            }
        }

        eprintln!(
            "  --- {}: {} pass, {} fail (of {}) ---",
            dir_name,
            dir_pass,
            dir_fail,
            entries.len()
        );
    }

    eprintln!("\n============================================");
    eprintln!(
        "TOTAL: {} pass, {} fail, {} slow (of {})",
        pass, fail_count, slow_count, total
    );
    if !failures.is_empty() {
        eprintln!("\nFAILURES:");
        for (name, err, ms) in &failures {
            eprintln!("  {}ms  {}  ({})", ms, name, err);
        }
    }
    if !slow_pdfs.is_empty() {
        eprintln!("\nSLOW (>10s):");
        for (name, ms) in &slow_pdfs {
            eprintln!("  {}ms  {}", ms, name);
        }
    }
    eprintln!("============================================");
}

fn extract_all_pages(
    path: &std::path::Path,
    _timeout: Duration,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut doc = PdfDocument::open(path)?;
    let page_count = doc.page_count()?;
    let mut total_chars = 0;

    for page_idx in 0..page_count {
        let text = doc.extract_text(page_idx)?;
        total_chars += text.len();
    }

    Ok(total_chars)
}
