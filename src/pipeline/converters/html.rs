//! HTML output converter.
//!
//! Converts ordered text spans to HTML format with support for:
//! - **Layout Mode**: CSS absolute positioning to preserve spatial document layout
//! - **Semantic Mode**: HTML5 semantic elements (h1-h3, p, strong, em)
//! - **Style Preservation**: Font weight, italics, and color attributes
//! - **Proper Escaping**: XSS-safe HTML output

use crate::error::Result;
use crate::layout::FontWeight;
use crate::pipeline::{OrderedTextSpan, TextPipelineConfig};
use crate::structure::table_extractor::ExtractedTable;
use crate::text::HyphenationHandler;

use super::OutputConverter;

/// HTML output converter.
///
/// Converts ordered text spans to semantic HTML with proper structure and optional layout preservation.
pub struct HtmlOutputConverter {
    /// Line spacing threshold ratio for paragraph detection.
    paragraph_gap_ratio: f32,
}

impl HtmlOutputConverter {
    /// Create a new HTML converter with default settings.
    pub fn new() -> Self {
        Self {
            paragraph_gap_ratio: 1.5,
        }
    }

    /// Check if a span should be rendered as bold.
    fn is_bold(&self, span: &OrderedTextSpan) -> bool {
        matches!(
            span.span.font_weight,
            FontWeight::Bold | FontWeight::Black | FontWeight::ExtraBold | FontWeight::SemiBold
        )
    }

    /// Check if a span is italic.
    fn is_italic(&self, span: &OrderedTextSpan) -> bool {
        span.span.is_italic
    }

    /// Detect paragraph breaks between spans based on vertical spacing.
    fn is_paragraph_break(&self, current: &OrderedTextSpan, previous: &OrderedTextSpan) -> bool {
        let line_height = current.span.font_size.max(previous.span.font_size);
        let gap = (previous.span.bbox.y - current.span.bbox.y).abs();
        gap > line_height * self.paragraph_gap_ratio
    }

    /// Detect if span should be a heading based on font size.
    fn heading_level(&self, span: &OrderedTextSpan, base_font_size: f32) -> Option<u8> {
        let size_ratio = span.span.font_size / base_font_size;
        if size_ratio >= 2.0 {
            Some(1)
        } else if size_ratio >= 1.5 {
            Some(2)
        } else if size_ratio >= 1.25 {
            Some(3)
        } else {
            None
        }
    }

    /// Escape HTML special characters to prevent XSS.
    fn escape_html(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }

    /// Format a span as styled HTML.
    ///
    /// Applies bold (<strong>) and italic (<em>) tags as needed.
    fn format_span_with_styles(&self, span: &OrderedTextSpan, text: &str) -> String {
        let escaped = Self::escape_html(text);

        let mut result = escaped;

        // Apply italic tag if needed
        if self.is_italic(span) {
            result = format!("<em>{}</em>", result);
        }

        // Apply bold tag if needed
        if self.is_bold(span) {
            result = format!("<strong>{}</strong>", result);
        }

        result
    }

    /// Format a color as CSS hex notation.
    fn format_color(&self, span: &OrderedTextSpan) -> Option<String> {
        let color = &span.span.color;
        // Convert from 0.0-1.0 range to 0-255
        let r = (color.r * 255.0) as u8;
        let g = (color.g * 255.0) as u8;
        let b = (color.b * 255.0) as u8;

        // Only return color if not black (default)
        if r != 0 || g != 0 || b != 0 {
            Some(format!("#{:02x}{:02x}{:02x}", r, g, b))
        } else {
            None
        }
    }
}

impl Default for HtmlOutputConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputConverter for HtmlOutputConverter {
    fn convert(&self, spans: &[OrderedTextSpan], config: &TextPipelineConfig) -> Result<String> {
        if config.output.preserve_layout {
            self.convert_layout_mode(spans, config)
        } else {
            self.convert_semantic_mode(spans, &[], config)
        }
    }

    fn convert_with_tables(
        &self,
        spans: &[OrderedTextSpan],
        tables: &[ExtractedTable],
        config: &TextPipelineConfig,
    ) -> Result<String> {
        if config.output.preserve_layout {
            self.convert_layout_mode(spans, config)
        } else {
            self.convert_semantic_mode(spans, tables, config)
        }
    }

    fn name(&self) -> &'static str {
        "HtmlOutputConverter"
    }

    fn mime_type(&self) -> &'static str {
        "text/html"
    }
}

impl HtmlOutputConverter {
    /// Convert to HTML with layout preservation (CSS absolute positioning).
    ///
    /// Each span is placed in a div with inline CSS positioning to preserve
    /// the exact spatial layout from the PDF.
    fn convert_layout_mode(
        &self,
        spans: &[OrderedTextSpan],
        config: &TextPipelineConfig,
    ) -> Result<String> {
        if spans.is_empty() {
            return Ok(String::new());
        }

        // Sort by reading order
        let mut sorted: Vec<_> = spans.iter().collect();
        sorted.sort_by_key(|s| s.reading_order);

        let mut result = String::new();

        // Generate each span with absolute positioning
        for span in sorted {
            let text = self.format_span_with_styles(span, &span.span.text);
            let x = span.span.bbox.x;
            let y = span.span.bbox.y;
            let font_size = span.span.font_size;

            // Build style attribute
            let mut style =
                format!("position:absolute;left:{}pt;top:{}pt;font-size:{}pt;", x, y, font_size);

            // Add color if present
            if let Some(color) = self.format_color(span) {
                style.push_str(&format!("color:{};", color));
            }

            result.push_str(&format!("<div style=\"{}\">{}</div>\n", style, text));
        }

        // Apply hyphenation reconstruction if enabled
        if config.enable_hyphenation_reconstruction {
            let handler = HyphenationHandler::new();
            result = handler.process_text(&result);
        }

        Ok(result)
    }

    /// Convert to HTML with semantic markup (headings, paragraphs, etc.).
    ///
    /// Detects headings based on font size, creates paragraphs with proper
    /// markup, and applies style tags for bold and italic text.
    fn convert_semantic_mode(
        &self,
        spans: &[OrderedTextSpan],
        tables: &[ExtractedTable],
        config: &TextPipelineConfig,
    ) -> Result<String> {
        if spans.is_empty() && tables.is_empty() {
            return Ok(String::new());
        }

        // Sort by reading order
        let mut sorted: Vec<_> = spans.iter().collect();
        sorted.sort_by_key(|s| s.reading_order);

        // Calculate base font size for heading detection
        let base_font_size = if config.output.detect_headings {
            let sizes: Vec<f32> = sorted.iter().map(|s| s.span.font_size).collect();
            let mut sizes_sorted = sizes.clone();
            sizes_sorted.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
            sizes_sorted
                .get(sizes_sorted.len() / 2)
                .copied()
                .unwrap_or(12.0)
        } else {
            12.0
        };

        // Track which tables have been rendered
        let mut tables_rendered = vec![false; tables.len()];

        let mut result = String::new();
        let mut prev_span: Option<&OrderedTextSpan> = None;
        let mut in_paragraph = false;
        let mut current_content = String::new();

        for span in &sorted {
            // Check if span is in a table region
            if !tables.is_empty() {
                if let Some(table_idx) = self.span_in_table(span, tables) {
                    if !tables_rendered[table_idx] {
                        // Close any open paragraph
                        if in_paragraph && !current_content.is_empty() {
                            result.push_str(&format!("<p>{}</p>\n", current_content.trim()));
                            current_content.clear();
                            in_paragraph = false;
                        }

                        // Render the table
                        result.push_str(&Self::render_table_html(&tables[table_idx]));
                        tables_rendered[table_idx] = true;
                        prev_span = None;
                    }
                    continue;
                }
            }

            // Check for paragraph break
            if let Some(prev) = prev_span {
                if self.is_paragraph_break(span, prev) {
                    if in_paragraph && !current_content.is_empty() {
                        result.push_str(&format!("<p>{}</p>\n", current_content.trim()));
                        current_content.clear();
                        in_paragraph = false;
                    }
                }
            }

            // Check for heading
            if config.output.detect_headings {
                if let Some(level) = self.heading_level(span, base_font_size) {
                    if in_paragraph && !current_content.is_empty() {
                        result.push_str(&format!("<p>{}</p>\n", current_content.trim()));
                        current_content.clear();
                        in_paragraph = false;
                    }

                    let text = self.format_span_with_styles(span, span.span.text.trim());
                    result.push_str(&format!("<h{}>{}</h{}>\n", level, text, level));
                    prev_span = Some(span);
                    continue;
                }
            }

            if !in_paragraph {
                in_paragraph = true;
            }

            let formatted = self.format_span_with_styles(span, &span.span.text);
            current_content.push_str(&formatted);

            prev_span = Some(span);
        }

        // Render any tables that weren't matched to spans
        for (i, table) in tables.iter().enumerate() {
            if !tables_rendered[i] && !table.is_empty() {
                if in_paragraph && !current_content.is_empty() {
                    result.push_str(&format!("<p>{}</p>\n", current_content.trim()));
                    current_content.clear();
                    in_paragraph = false;
                }
                result.push_str(&Self::render_table_html(table));
            }
        }

        // Close any open paragraph
        if in_paragraph && !current_content.is_empty() {
            result.push_str(&format!("<p>{}</p>\n", current_content.trim()));
        }

        // Apply hyphenation reconstruction if enabled
        if config.enable_hyphenation_reconstruction {
            let handler = HyphenationHandler::new();
            result = handler.process_text(&result);
        }

        Ok(result)
    }

    /// Check if a span's bbox overlaps with any table region.
    fn span_in_table(&self, span: &OrderedTextSpan, tables: &[ExtractedTable]) -> Option<usize> {
        let sx = span.span.bbox.x;
        let sy = span.span.bbox.y;

        for (i, table) in tables.iter().enumerate() {
            if let Some(ref bbox) = table.bbox {
                let tolerance = 2.0;
                if sx >= bbox.x - tolerance
                    && sx <= bbox.x + bbox.width + tolerance
                    && sy >= bbox.y - tolerance
                    && sy <= bbox.y + bbox.height + tolerance
                {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Render an ExtractedTable as an HTML table string.
    fn render_table_html(table: &ExtractedTable) -> String {
        if table.rows.is_empty() {
            return String::new();
        }

        let mut html = String::from("<table>\n");

        // Determine header/body sections
        let has_header = table.has_header || table.rows.first().map_or(false, |r| r.is_header);
        let header_end = if has_header {
            table
                .rows
                .iter()
                .position(|r| !r.is_header)
                .unwrap_or(table.rows.len())
        } else {
            0
        };

        // Render header rows
        if header_end > 0 {
            html.push_str("<thead>\n");
            for row in &table.rows[..header_end] {
                html.push_str("<tr>");
                for cell in &row.cells {
                    let mut attrs = String::new();
                    if cell.colspan > 1 {
                        attrs.push_str(&format!(" colspan=\"{}\"", cell.colspan));
                    }
                    if cell.rowspan > 1 {
                        attrs.push_str(&format!(" rowspan=\"{}\"", cell.rowspan));
                    }
                    let text = Self::escape_html(cell.text.trim());
                    html.push_str(&format!("<th{}>{}</th>", attrs, text));
                }
                html.push_str("</tr>\n");
            }
            html.push_str("</thead>\n");
        }

        // Render body rows
        let body_rows = &table.rows[header_end..];
        if !body_rows.is_empty() {
            html.push_str("<tbody>\n");
            for row in body_rows {
                html.push_str("<tr>");
                for cell in &row.cells {
                    let mut attrs = String::new();
                    if cell.colspan > 1 {
                        attrs.push_str(&format!(" colspan=\"{}\"", cell.colspan));
                    }
                    if cell.rowspan > 1 {
                        attrs.push_str(&format!(" rowspan=\"{}\"", cell.rowspan));
                    }
                    let text = Self::escape_html(cell.text.trim());
                    html.push_str(&format!("<td{}>{}</td>", attrs, text));
                }
                html.push_str("</tr>\n");
            }
            html.push_str("</tbody>\n");
        }

        html.push_str("</table>\n");
        html
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;
    use crate::layout::{Color, TextSpan};

    fn make_span(
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        weight: FontWeight,
    ) -> OrderedTextSpan {
        OrderedTextSpan::new(
            TextSpan {
                text: text.to_string(),
                bbox: Rect::new(x, y, 50.0, font_size),
                font_name: "Test".to_string(),
                font_size,
                font_weight: weight,
                is_italic: false,
                color: Color::black(),
                mcid: None,
                sequence: 0,
                offset_semantic: false,
                split_boundary_before: false,
                char_spacing: 0.0,
                word_spacing: 0.0,
                horizontal_scaling: 100.0,
                primary_detected: false,
            },
            0,
        )
    }

    #[test]
    fn test_empty_spans() {
        let converter = HtmlOutputConverter::new();
        let config = TextPipelineConfig::default();
        let result = converter.convert(&[], &config).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_single_paragraph() {
        let converter = HtmlOutputConverter::new();
        let config = TextPipelineConfig::default();
        let spans = vec![make_span(
            "Hello world",
            0.0,
            100.0,
            12.0,
            FontWeight::Normal,
        )];
        let result = converter.convert(&spans, &config).unwrap();
        assert_eq!(result, "<p>Hello world</p>\n");
    }

    #[test]
    fn test_bold_text() {
        let converter = HtmlOutputConverter::new();
        let config = TextPipelineConfig::default();
        let spans = vec![make_span("Bold", 0.0, 100.0, 12.0, FontWeight::Bold)];
        let result = converter.convert(&spans, &config).unwrap();
        assert_eq!(result, "<p><strong>Bold</strong></p>\n");
    }

    #[test]
    fn test_html_escaping() {
        let converter = HtmlOutputConverter::new();
        let config = TextPipelineConfig::default();
        let spans = vec![make_span(
            "<script>alert('XSS')</script>",
            0.0,
            100.0,
            12.0,
            FontWeight::Normal,
        )];
        let result = converter.convert(&spans, &config).unwrap();
        assert!(result.contains("&lt;script&gt;"));
        assert!(!result.contains("<script>"));
    }

    // ============================================================================
    // render_table_html() tests
    // ============================================================================

    use crate::structure::table_extractor::{TableCell, TableRow};

    #[test]
    fn test_render_table_html_empty() {
        let table = ExtractedTable::new();
        let result = HtmlOutputConverter::render_table_html(&table);
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_table_html_basic() {
        let mut table = ExtractedTable::new();
        table.has_header = true;

        let mut header = TableRow::new(true);
        header.add_cell(TableCell::new("Name".to_string(), true));
        header.add_cell(TableCell::new("Age".to_string(), true));
        table.add_row(header);

        let mut data = TableRow::new(false);
        data.add_cell(TableCell::new("Alice".to_string(), false));
        data.add_cell(TableCell::new("30".to_string(), false));
        table.add_row(data);

        let result = HtmlOutputConverter::render_table_html(&table);
        assert!(result.contains("<table>"));
        assert!(result.contains("</table>"));
        assert!(result.contains("<thead>"));
        assert!(result.contains("</thead>"));
        assert!(result.contains("<tbody>"));
        assert!(result.contains("</tbody>"));
        assert!(result.contains("<th>Name</th>"));
        assert!(result.contains("<th>Age</th>"));
        assert!(result.contains("<td>Alice</td>"));
        assert!(result.contains("<td>30</td>"));
    }

    #[test]
    fn test_render_table_html_no_header() {
        let mut table = ExtractedTable::new();

        let mut row = TableRow::new(false);
        row.add_cell(TableCell::new("A".to_string(), false));
        table.add_row(row);

        let result = HtmlOutputConverter::render_table_html(&table);
        assert!(result.contains("<table>"));
        assert!(!result.contains("<thead>"), "Should not have thead when no header");
        assert!(result.contains("<tbody>"));
        assert!(result.contains("<td>A</td>"));
    }

    #[test]
    fn test_render_table_html_colspan() {
        let mut table = ExtractedTable::new();
        let mut row = TableRow::new(false);
        row.add_cell(TableCell::new("Wide".to_string(), false).with_colspan(3));
        table.add_row(row);

        let result = HtmlOutputConverter::render_table_html(&table);
        assert!(result.contains("colspan=\"3\""), "Should have colspan attribute: {}", result);
    }

    #[test]
    fn test_render_table_html_rowspan() {
        let mut table = ExtractedTable::new();
        let mut row = TableRow::new(false);
        row.add_cell(TableCell::new("Tall".to_string(), false).with_rowspan(2));
        table.add_row(row);

        let result = HtmlOutputConverter::render_table_html(&table);
        assert!(result.contains("rowspan=\"2\""), "Should have rowspan attribute: {}", result);
    }

    #[test]
    fn test_render_table_html_escapes_content() {
        let mut table = ExtractedTable::new();
        let mut row = TableRow::new(false);
        row.add_cell(TableCell::new("<b>bold</b>".to_string(), false));
        row.add_cell(TableCell::new("A & B".to_string(), false));
        table.add_row(row);

        let result = HtmlOutputConverter::render_table_html(&table);
        assert!(result.contains("&lt;b&gt;bold&lt;/b&gt;"), "HTML should be escaped: {}", result);
        assert!(result.contains("A &amp; B"), "Ampersand should be escaped: {}", result);
        assert!(!result.contains("<b>bold</b>"), "Raw HTML should not appear");
    }

    #[test]
    fn test_render_table_html_all_header_rows() {
        let mut table = ExtractedTable::new();
        table.has_header = true;

        let mut h1 = TableRow::new(true);
        h1.add_cell(TableCell::new("H1".to_string(), true));
        table.add_row(h1);

        let mut h2 = TableRow::new(true);
        h2.add_cell(TableCell::new("H2".to_string(), true));
        table.add_row(h2);

        let result = HtmlOutputConverter::render_table_html(&table);
        assert!(result.contains("<thead>"));
        assert!(result.contains("<th>H1</th>"));
        assert!(result.contains("<th>H2</th>"));
        // No tbody when all rows are headers
        assert!(!result.contains("<tbody>"));
    }

    // ============================================================================
    // convert_with_tables() tests
    // ============================================================================

    #[test]
    fn test_convert_with_tables_renders_html_table() {
        let converter = HtmlOutputConverter::new();
        let config = TextPipelineConfig::default();

        let mut table = ExtractedTable::new();
        table.bbox = Some(Rect::new(10.0, 50.0, 200.0, 100.0));
        table.has_header = true;

        let mut header = TableRow::new(true);
        header.add_cell(TableCell::new("X".to_string(), true));
        table.add_row(header);

        let mut data = TableRow::new(false);
        data.add_cell(TableCell::new("Y".to_string(), false));
        table.add_row(data);

        let result = converter
            .convert_with_tables(&[], &[table], &config)
            .unwrap();

        assert!(result.contains("<table>"), "Should contain HTML table: {}", result);
        assert!(result.contains("<th>X</th>"));
        assert!(result.contains("<td>Y</td>"));
    }

    #[test]
    fn test_convert_with_tables_mixed_content() {
        let converter = HtmlOutputConverter::new();
        let config = TextPipelineConfig::default();

        let mut span_before = make_span("Intro", 10.0, 200.0, 12.0, FontWeight::Normal);
        span_before.reading_order = 0;

        let mut span_in_table = make_span("Inside", 50.0, 70.0, 12.0, FontWeight::Normal);
        span_in_table.reading_order = 1;

        let mut table = ExtractedTable::new();
        table.bbox = Some(Rect::new(10.0, 50.0, 200.0, 100.0));
        let mut row = TableRow::new(false);
        row.add_cell(TableCell::new("Cell".to_string(), false));
        table.add_row(row);

        let result = converter
            .convert_with_tables(&[span_before, span_in_table], &[table], &config)
            .unwrap();

        assert!(result.contains("<p>Intro</p>"), "Should contain paragraph: {}", result);
        assert!(result.contains("<table>"), "Should contain table: {}", result);
        assert!(!result.contains("Inside"), "Should exclude span in table region");
    }

    #[test]
    fn test_convert_with_tables_no_tables_same_as_convert() {
        let converter = HtmlOutputConverter::new();
        let config = TextPipelineConfig::default();
        let spans = vec![make_span("Hello", 0.0, 100.0, 12.0, FontWeight::Normal)];

        let result_convert = converter.convert(&spans, &config).unwrap();
        let result_with_tables = converter
            .convert_with_tables(&spans, &[], &config)
            .unwrap();

        assert_eq!(result_convert, result_with_tables);
    }

    #[test]
    fn test_span_in_table_html() {
        let converter = HtmlOutputConverter::new();

        let mut table = ExtractedTable::new();
        table.bbox = Some(Rect::new(10.0, 50.0, 200.0, 100.0));

        let inside = make_span("inside", 50.0, 70.0, 12.0, FontWeight::Normal);
        let outside = make_span("outside", 500.0, 500.0, 12.0, FontWeight::Normal);

        assert_eq!(converter.span_in_table(&inside, &[table.clone()]), Some(0));
        assert_eq!(converter.span_in_table(&outside, &[table]), None);
    }
}
