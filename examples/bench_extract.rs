//! Benchmark text extraction with phase breakdown

use pdf_oxide::document::PdfDocument;
use std::env;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <pdf_file> [page_number]", args[0]);
        std::process::exit(1);
    }

    let pdf_path = &args[1];
    let specific_page: Option<usize> = args.get(2).and_then(|s| s.parse().ok());

    let t0 = Instant::now();
    let mut doc = PdfDocument::open(pdf_path)?;
    let open_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let page_count = doc.page_count()?;
    eprintln!("{}: {} pages (open: {:.0}ms)", pdf_path, page_count, open_ms);

    if let Some(pg) = specific_page {
        // Phase 1: extract_text (includes structure tree parse + spans + assembly)
        let t1 = Instant::now();
        let text = doc.extract_text(pg)?;
        eprintln!(
            "  extract_text: {:.0}ms ({} chars)",
            t1.elapsed().as_secs_f64() * 1000.0,
            text.len()
        );

        // Phase 2: extract_text again (cached structure tree + cached fonts)
        let t2 = Instant::now();
        let text2 = doc.extract_text(pg)?;
        eprintln!(
            "  extract_text (2nd): {:.0}ms ({} chars)",
            t2.elapsed().as_secs_f64() * 1000.0,
            text2.len()
        );

        // Phase 3: extract_spans only
        let t3 = Instant::now();
        let spans = doc.extract_spans(pg)?;
        let span_chars: usize = spans.iter().map(|s| s.text.len()).sum();
        eprintln!(
            "  extract_spans: {:.0}ms ({} spans, {} chars)",
            t3.elapsed().as_secs_f64() * 1000.0,
            spans.len(),
            span_chars
        );

        // Phase 4: extract_chars (different code path)
        let t4 = Instant::now();
        let chars = doc.extract_chars(pg)?;
        eprintln!(
            "  extract_chars: {:.0}ms ({} chars)",
            t4.elapsed().as_secs_f64() * 1000.0,
            chars.len()
        );

        return Ok(());
    }

    // Full document: time extract_text (what users actually call)
    let mut total_chars = 0usize;
    let mut slow_pages = Vec::new();
    let t_all = Instant::now();
    for i in 0..page_count {
        let t1 = Instant::now();
        let text = doc.extract_text(i)?;
        let ms = t1.elapsed().as_secs_f64() * 1000.0;
        total_chars += text.len();
        if ms > 200.0 {
            slow_pages.push((i, ms));
        }
    }

    let total_ms = t_all.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "  Total: {:.0}ms | {:.1}ms/page | {} chars",
        total_ms,
        total_ms / page_count as f64,
        total_chars
    );

    if !slow_pages.is_empty() {
        eprintln!("  Slow pages (>200ms):");
        for (pg, ms) in &slow_pages {
            eprintln!("    page {}: {:.0}ms", pg, ms);
        }
    }

    Ok(())
}
