//! Trace slow PDF extraction to identify bottleneck phases

use pdf_oxide::document::PdfDocument;
use std::env;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <pdf_file>", args[0]);
        std::process::exit(1);
    }

    let pdf_path = &args[1];

    let t0 = Instant::now();
    let mut doc = PdfDocument::open(pdf_path)?;
    let open_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let page_count = doc.page_count()?;
    eprintln!("{}: {} pages (open: {:.0}ms)", pdf_path, page_count, open_ms);

    // Phase 1: extract_text for all pages with per-page timing
    eprintln!("\n--- extract_text (all pages) ---");
    let mut doc2 = PdfDocument::open(pdf_path)?;
    let checkpoints = [1, 10, 100, 500, 1000, 2000, 5000, 10000];
    let mut slow_pages: Vec<(usize, f64)> = Vec::new();
    let t_seq = Instant::now();
    for i in 0..page_count {
        let tp = Instant::now();
        let _ = doc2.extract_text(i);
        let pg_ms = tp.elapsed().as_secs_f64() * 1000.0;
        if pg_ms > 50.0 {
            slow_pages.push((i, pg_ms));
        }
        for &cp in &checkpoints {
            if i + 1 == cp && cp <= page_count {
                let elapsed = t_seq.elapsed().as_secs_f64() * 1000.0;
                eprintln!(
                    "  pages 0..{}: {:.0}ms ({:.2}ms/page)",
                    cp,
                    elapsed,
                    elapsed / cp as f64
                );
            }
        }
    }
    let total_ms = t_seq.elapsed().as_secs_f64() * 1000.0;
    eprintln!("  Total: {:.0}ms ({:.2}ms/page)", total_ms, total_ms / page_count as f64);

    if !slow_pages.is_empty() {
        eprintln!("\n  Slow pages (>50ms):");
        for (pg, ms) in &slow_pages {
            eprintln!("    page {}: {:.1}ms", pg, ms);
        }
    }

    // Phase 2: extract_spans for all pages (bypasses structure tree)
    eprintln!("\n--- extract_spans (all pages) ---");
    let mut doc3 = PdfDocument::open(pdf_path)?;
    let t3 = Instant::now();
    for i in 0..page_count {
        let _ = doc3.extract_spans(i);
    }
    let spans_ms = t3.elapsed().as_secs_f64() * 1000.0;
    eprintln!("  Total: {:.0}ms ({:.2}ms/page)", spans_ms, spans_ms / page_count as f64);

    // Phase 3: get_page_content_data for all pages
    eprintln!("\n--- get_page_content_data (all pages) ---");
    let mut doc4 = PdfDocument::open(pdf_path)?;
    let t4 = Instant::now();
    let mut total_bytes = 0usize;
    for i in 0..page_count {
        if let Ok(data) = doc4.get_page_content_data(i) {
            total_bytes += data.len();
        }
    }
    let gpc_ms = t4.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "  Total: {:.0}ms ({:.3}ms/page, {} bytes total)",
        gpc_ms,
        gpc_ms / page_count as f64,
        total_bytes
    );

    // Phase 4: structure tree
    eprintln!("\n--- structure_tree() ---");
    let mut doc5 = PdfDocument::open(pdf_path)?;
    let t5 = Instant::now();
    let tree = doc5.structure_tree();
    let tree_ms = t5.elapsed().as_secs_f64() * 1000.0;
    eprintln!("  {:.0}ms (has_tree={})", tree_ms, tree.ok().flatten().is_some());

    Ok(())
}
