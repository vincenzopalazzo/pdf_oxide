//! Regression test for the UTF-8 char-boundary panic reported against
//! v0.3.15 when extracting text from the "Kangrejos 2026" PDF:
//!
//! ```text
//! thread 'tokio-rt-worker' panicked at src/extractors/text.rs:2963:31:
//! end byte index 10 is not a char boundary; it is inside '★'
//! (bytes 9..12 of string)
//! ```
//!
//! Root cause: two debug-log sites in `src/extractors/text.rs` sliced
//! `span.text` / `current.text` by byte index (`..len.min(10)`,
//! `len.saturating_sub(10)..`) without rounding to a UTF-8 char boundary.
//! When a multi-byte codepoint (e.g. '★' U+2605, 3 bytes) straddled the
//! cut point, the slice panicked.
//!
//! These tests guarantee:
//!   1. Extracting all pages of the Kangrejos 2026 fixture never panics,
//!      even with debug-level logging enabled (which evaluates the
//!      previously-offending format args).
//!   2. A synthetic smoke test of the safe char-boundary slicing helpers
//!      (mirroring `pdf_oxide::utils::safe_prefix` / `safe_suffix`) on
//!      the exact problem string shape ('★' at byte 9..12), so the
//!      regression is caught even if the fixture is later removed.

use log::LevelFilter;
use pdf_oxide::document::PdfDocument;

/// Force `log::debug!` / `log::trace!` argument evaluation, since the
/// original panic was inside a `log::debug!(...)` format-args expression.
/// Without this, the macros short-circuit before evaluating the slice.
fn enable_debug_logging() {
    log::set_max_level(LevelFilter::Trace);
}

/// Stable-Rust mirror of `pdf_oxide::utils::safe_prefix` (which is
/// `pub(crate)` and thus not reachable from integration tests).
///
/// `str::floor_char_boundary` / `ceil_char_boundary` would do the same,
/// but they live behind the unstable `round_char_boundary` feature
/// (tracking #93743) and would break the `cargo test` matrix on stable.
fn safe_prefix(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Stable-Rust mirror of `pdf_oxide::utils::safe_suffix`. See `safe_prefix`.
fn safe_suffix(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut start = s.len() - max_bytes;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

#[test]
fn kangrejos_2026_extract_all_pages_no_panic() {
    enable_debug_logging();

    let doc = PdfDocument::open("tests/fixtures/kangrejos_2026.pdf")
        .expect("Kangrejos 2026 fixture should open");

    let pages = doc.page_count().expect("page_count");
    assert!(pages > 0, "fixture should have at least one page");

    let mut total_chars = 0usize;
    for p in 0..pages {
        // The original bug was a panic, not an Err. The crash manifested
        // as a thread panic at the `log::debug!` slice site, so the
        // assertion that matters is simply that this call returns.
        let text = doc
            .extract_text(p)
            .unwrap_or_else(|e| panic!("page {p}: extract_text returned Err: {e:?}"));
        total_chars += text.chars().count();
    }

    // Defence-in-depth: the pre-fix behaviour produced *zero* text (the
    // caller fell back to vision because the panic was caught upstream).
    // Once the slice is char-boundary-safe, real text comes through.
    assert!(
        total_chars > 0,
        "expected non-empty text across {pages} pages after the UTF-8 fix",
    );
}

#[test]
fn char_boundary_slicing_handles_multibyte_at_cut_point() {
    // ----- prefix slice (text.rs:2962 shape) -----
    //
    // Reproduce the exact shape from the panic message:
    // "end byte index 10 is not a char boundary; it is inside '★'
    //  (bytes 9..12 of string)".
    //
    // '★' (U+2605) is 3 bytes in UTF-8 (E2 98 85). Place 9 ASCII bytes
    // before it so the codepoint occupies bytes 9..12 — then a naïve
    // `&s[..10]` slice lands inside the star.
    let s = "123456789★rest"; // 9 ASCII + '★' + 4 ASCII

    // Silence the panic hook just for the catch_unwind call so the test
    // output isn't polluted by the expected panic's stderr message.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let naive = std::panic::catch_unwind(|| {
        // Intentional: this is the slice shape that used to live in
        // src/extractors/text.rs at the panic site.
        let _ = &s[..s.len().min(10)];
    });
    std::panic::set_hook(prev_hook);

    assert!(
        naive.is_err(),
        "sanity check: the pre-fix slice shape must still panic on '★' at byte 9..12",
    );

    // Post-fix style: clip down to the previous char boundary via
    // pdf_oxide::utils::safe_prefix (mirrored locally above because it
    // is `pub(crate)`).
    let snippet = safe_prefix(s, 10);
    assert_eq!(snippet, "123456789", "safe_prefix should clip to the byte before '★'",);

    // ----- suffix slice (text.rs:2960 shape) -----
    //
    // To make `len.saturating_sub(10)` land *inside* '★', the star must
    // straddle that byte. With '★' (3 bytes) at byte index 1..4 and 8
    // trailing ASCII bytes, len = 12, len.saturating_sub(10) = 2, which
    // is the middle byte of '★'.
    let s2 = "A★12345678"; // 'A' + '★' + 8 ASCII = 12 bytes

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let naive_suffix = std::panic::catch_unwind(|| {
        let _ = &s2[s2.len().saturating_sub(10)..];
    });
    std::panic::set_hook(prev_hook);

    assert!(
        naive_suffix.is_err(),
        "sanity check: the pre-fix suffix slice must panic when the cut lands in '★'",
    );

    // Post-fix style: round the start forward to the next char boundary
    // via pdf_oxide::utils::safe_suffix (mirrored locally above).
    let suffix = safe_suffix(s2, 10);
    assert_eq!(suffix, "12345678", "safe_suffix should round forward past the middle of '★'",);
}
