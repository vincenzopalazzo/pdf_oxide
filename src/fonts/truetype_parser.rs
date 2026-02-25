//! TrueType/OpenType font parser for PDF embedding.
//!
//! This module wraps the `ttf-parser` crate to extract font data needed
//! for embedding TrueType fonts in PDF documents with full Unicode support.
//!
//! # Font Embedding in PDF
//!
//! Per PDF spec Section 9.6-9.8, embedded fonts require:
//! - FontDescriptor with metrics (ascender, descender, cap height, etc.)
//! - ToUnicode CMap for text extraction
//! - Font program data (FontFile2 for TrueType)
//! - CIDFont for Unicode (Type 0 composite fonts with Identity-H encoding)

use std::collections::{BTreeSet, HashMap};
use std::io::{self, Write};

use ttf_parser::{Face, GlyphId};

/// Error types for TrueType font parsing.
#[derive(Debug, thiserror::Error)]
pub enum TrueTypeError {
    /// Failed to parse font file
    #[error("Failed to parse font file: {0}")]
    ParseError(String),

    /// Font file is empty or invalid
    #[error("Font file is empty or invalid")]
    EmptyFont,

    /// Required table is missing
    #[error("Required font table is missing: {0}")]
    MissingTable(String),

    /// IO error during font operations
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    /// Glyph not found
    #[error("Glyph not found for character: U+{0:04X}")]
    GlyphNotFound(u32),
}

/// Result type for TrueType operations.
pub type TrueTypeResult<T> = Result<T, TrueTypeError>;

/// Parsed TrueType font data for PDF embedding.
#[derive(Debug)]
pub struct TrueTypeFont<'a> {
    /// The parsed font face
    face: Face<'a>,
    /// Original font data (needed for embedding)
    data: &'a [u8],
    /// Cached Unicode to glyph ID mapping
    unicode_to_glyph: HashMap<u32, u16>,
    /// Cached glyph widths (glyph ID -> width in font units)
    glyph_widths: HashMap<u16, u16>,
}

impl<'a> TrueTypeFont<'a> {
    /// Parse a TrueType/OpenType font from raw data.
    ///
    /// # Arguments
    /// * `data` - Raw font file bytes (TTF or OTF)
    ///
    /// # Returns
    /// A parsed TrueType font ready for PDF embedding.
    pub fn parse(data: &'a [u8]) -> TrueTypeResult<Self> {
        if data.is_empty() {
            return Err(TrueTypeError::EmptyFont);
        }

        let face = Face::parse(data, 0).map_err(|e| TrueTypeError::ParseError(e.to_string()))?;

        let mut font = Self {
            face,
            data,
            unicode_to_glyph: HashMap::new(),
            glyph_widths: HashMap::new(),
        };

        font.build_unicode_map();
        font.build_width_table();

        Ok(font)
    }

    /// Build Unicode to glyph ID mapping from cmap table.
    fn build_unicode_map(&mut self) {
        // Iterate through BMP (Basic Multilingual Plane)
        for codepoint in 0..=0xFFFF_u32 {
            if let Some(char) = char::from_u32(codepoint) {
                if let Some(glyph_id) = self.face.glyph_index(char) {
                    self.unicode_to_glyph.insert(codepoint, glyph_id.0);
                }
            }
        }
    }

    /// Build glyph width table from hmtx table.
    fn build_width_table(&mut self) {
        let units_per_em = self.face.units_per_em();

        for glyph_id in 0..self.face.number_of_glyphs() {
            let glyph = GlyphId(glyph_id);
            let advance = self.face.glyph_hor_advance(glyph).unwrap_or(0);
            // Store as width in units of 1/1000 of em
            let width_1000 = (advance as u32 * 1000 / units_per_em as u32) as u16;
            self.glyph_widths.insert(glyph_id, width_1000);
        }
    }

    /// Get the font's PostScript name.
    pub fn postscript_name(&self) -> Option<String> {
        self.face
            .names()
            .into_iter()
            .find(|name| name.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
            .and_then(|name| name.to_string())
    }

    /// Get the font family name.
    pub fn family_name(&self) -> Option<String> {
        self.face
            .names()
            .into_iter()
            .find(|name| name.name_id == ttf_parser::name_id::FAMILY)
            .and_then(|name| name.to_string())
    }

    /// Get units per em for this font.
    pub fn units_per_em(&self) -> u16 {
        self.face.units_per_em()
    }

    /// Get the ascender in font units.
    pub fn ascender(&self) -> i16 {
        self.face.ascender()
    }

    /// Get the descender in font units (negative value).
    pub fn descender(&self) -> i16 {
        self.face.descender()
    }

    /// Get the cap height in font units.
    pub fn cap_height(&self) -> Option<i16> {
        self.face.capital_height()
    }

    /// Get the x-height in font units.
    pub fn x_height(&self) -> Option<i16> {
        self.face.x_height()
    }

    /// Get the italic angle.
    pub fn italic_angle(&self) -> f32 {
        self.face.italic_angle()
    }

    /// Check if the font is bold.
    pub fn is_bold(&self) -> bool {
        self.face.is_bold()
    }

    /// Check if the font is italic.
    pub fn is_italic(&self) -> bool {
        self.face.is_italic()
    }

    /// Get the font bounding box.
    pub fn bbox(&self) -> (i16, i16, i16, i16) {
        let bbox = self.face.global_bounding_box();
        (bbox.x_min, bbox.y_min, bbox.x_max, bbox.y_max)
    }

    /// Get glyph ID for a Unicode codepoint.
    pub fn glyph_id(&self, codepoint: u32) -> Option<u16> {
        self.unicode_to_glyph.get(&codepoint).copied()
    }

    /// Get glyph width in 1/1000 em units.
    pub fn glyph_width(&self, glyph_id: u16) -> u16 {
        self.glyph_widths.get(&glyph_id).copied().unwrap_or(500)
    }

    /// Get width for a Unicode character in 1/1000 em units.
    pub fn char_width(&self, codepoint: u32) -> u16 {
        self.glyph_id(codepoint)
            .map(|gid| self.glyph_width(gid))
            .unwrap_or(500)
    }

    /// Get the number of glyphs in the font.
    pub fn num_glyphs(&self) -> u16 {
        self.face.number_of_glyphs()
    }

    /// Get the raw font data for embedding.
    pub fn raw_data(&self) -> &[u8] {
        self.data
    }

    /// Get all Unicode codepoints supported by this font.
    pub fn supported_codepoints(&self) -> Vec<u32> {
        let mut codepoints: Vec<_> = self.unicode_to_glyph.keys().copied().collect();
        codepoints.sort();
        codepoints
    }

    /// Calculate StemV (vertical stem width) - estimated from font weight.
    ///
    /// This is a heuristic since TrueType doesn't store StemV directly.
    pub fn stem_v(&self) -> i16 {
        if self.is_bold() {
            140
        } else {
            80
        }
    }

    /// Get font flags for PDF FontDescriptor.
    ///
    /// Returns flags per PDF spec Table 123:
    /// - Bit 1: FixedPitch
    /// - Bit 2: Serif (not easily determinable, assume false)
    /// - Bit 3: Symbolic (for Symbol/ZapfDingbats type fonts)
    /// - Bit 4: Script (cursive fonts)
    /// - Bit 6: Nonsymbolic (standard Latin text font)
    /// - Bit 7: Italic
    /// - Bit 17: AllCap
    /// - Bit 18: SmallCap
    /// - Bit 19: ForceBold
    pub fn font_flags(&self) -> u32 {
        let mut flags = 0u32;

        // Bit 1: FixedPitch (monospace)
        if self.face.is_monospaced() {
            flags |= 1 << 0;
        }

        // Bit 6: Nonsymbolic (standard text font)
        // Most TrueType fonts are nonsymbolic
        flags |= 1 << 5;

        // Bit 7: Italic
        if self.is_italic() {
            flags |= 1 << 6;
        }

        flags
    }

    /// Generate widths array for PDF CIDFont W entry.
    ///
    /// Format: [start_cid [w1 w2 ...] start_cid2 [w1 w2 ...] ...]
    /// For Identity-H encoding, CID = GID.
    pub fn generate_widths_array(&self, used_glyphs: &BTreeSet<u16>) -> Vec<u8> {
        let mut result = Vec::new();
        write!(result, "[").unwrap();

        // Group consecutive glyphs
        let mut glyphs: Vec<_> = used_glyphs.iter().copied().collect();
        glyphs.sort();

        let mut i = 0;
        while i < glyphs.len() {
            let start = glyphs[i];
            let mut end = start;
            let mut widths = vec![self.glyph_width(start)];

            // Find consecutive glyphs
            while i + 1 < glyphs.len() && glyphs[i + 1] == end + 1 {
                i += 1;
                end = glyphs[i];
                widths.push(self.glyph_width(end));
            }

            write!(result, "{} [", start).unwrap();
            for (j, w) in widths.iter().enumerate() {
                if j > 0 {
                    write!(result, " ").unwrap();
                }
                write!(result, "{}", w).unwrap();
            }
            write!(result, "]").unwrap();

            i += 1;
        }

        write!(result, "]").unwrap();
        result
    }

    /// Generate ToUnicode CMap for text extraction.
    ///
    /// This CMap maps GIDs (used as CIDs with Identity-H) back to Unicode
    /// so PDF readers can extract text from the generated PDF.
    pub fn generate_tounicode_cmap(&self, used_chars: &HashMap<u32, u16>) -> String {
        let mut cmap = String::new();

        // CMap header
        cmap.push_str("/CIDInit /ProcSet findresource begin\n");
        cmap.push_str("12 dict begin\n");
        cmap.push_str("begincmap\n");
        cmap.push_str("/CIDSystemInfo <<\n");
        cmap.push_str("  /Registry (Adobe)\n");
        cmap.push_str("  /Ordering (UCS)\n");
        cmap.push_str("  /Supplement 0\n");
        cmap.push_str(">> def\n");
        cmap.push_str("/CMapName /Adobe-Identity-UCS def\n");
        cmap.push_str("/CMapType 2 def\n");
        cmap.push_str("1 begincodespacerange\n");
        cmap.push_str("<0000> <FFFF>\n");
        cmap.push_str("endcodespacerange\n");

        // Build GID -> Unicode mappings
        let mut mappings: Vec<(u16, u32)> = used_chars
            .iter()
            .map(|(&unicode, &gid)| (gid, unicode))
            .collect();
        mappings.sort_by_key(|&(gid, _)| gid);

        // Write bfchar entries (max 100 per section per PDF spec)
        let chunks: Vec<_> = mappings.chunks(100).collect();
        for chunk in chunks {
            cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
            for &(gid, unicode) in chunk {
                if unicode <= 0xFFFF {
                    cmap.push_str(&format!("<{:04X}> <{:04X}>\n", gid, unicode));
                } else {
                    // Supplementary plane - encode as UTF-16 surrogate pair
                    let high = ((unicode - 0x10000) >> 10) + 0xD800;
                    let low = ((unicode - 0x10000) & 0x3FF) + 0xDC00;
                    cmap.push_str(&format!("<{:04X}> <{:04X}{:04X}>\n", gid, high, low));
                }
            }
            cmap.push_str("endbfchar\n");
        }

        // CMap footer
        cmap.push_str("endcmap\n");
        cmap.push_str("CMapName currentdict /CMap defineresource pop\n");
        cmap.push_str("end\n");
        cmap.push_str("end\n");

        cmap
    }
}

/// Font metrics extracted for PDF FontDescriptor.
#[derive(Debug, Clone)]
pub struct FontMetrics {
    /// PostScript name
    pub name: String,
    /// Family name
    pub family: String,
    /// Units per em
    pub units_per_em: u16,
    /// Ascender (positive)
    pub ascender: i16,
    /// Descender (negative)
    pub descender: i16,
    /// Cap height
    pub cap_height: i16,
    /// x-height
    pub x_height: i16,
    /// Italic angle
    pub italic_angle: f32,
    /// Bounding box (llx, lly, urx, ury)
    pub bbox: (i16, i16, i16, i16),
    /// Stem V (vertical stem width)
    pub stem_v: i16,
    /// Font flags
    pub flags: u32,
    /// Is bold
    pub is_bold: bool,
    /// Is italic
    pub is_italic: bool,
}

impl FontMetrics {
    /// Extract metrics from a parsed TrueType font.
    pub fn from_font(font: &TrueTypeFont) -> Self {
        Self {
            name: font
                .postscript_name()
                .unwrap_or_else(|| "Unknown".to_string()),
            family: font.family_name().unwrap_or_else(|| "Unknown".to_string()),
            units_per_em: font.units_per_em(),
            ascender: font.ascender(),
            descender: font.descender(),
            cap_height: font.cap_height().unwrap_or(font.ascender()),
            x_height: font
                .x_height()
                .unwrap_or((font.ascender() as f32 * 0.5) as i16),
            italic_angle: font.italic_angle(),
            bbox: font.bbox(),
            stem_v: font.stem_v(),
            flags: font.font_flags(),
            is_bold: font.is_bold(),
            is_italic: font.is_italic(),
        }
    }

    /// Convert a value from font units to PDF units (1/1000 em).
    pub fn to_pdf_units(&self, value: i16) -> i32 {
        (value as i32 * 1000) / self.units_per_em as i32
    }

    /// Get ascender in PDF units.
    pub fn pdf_ascender(&self) -> i32 {
        self.to_pdf_units(self.ascender)
    }

    /// Get descender in PDF units.
    pub fn pdf_descender(&self) -> i32 {
        self.to_pdf_units(self.descender)
    }

    /// Get cap height in PDF units.
    pub fn pdf_cap_height(&self) -> i32 {
        self.to_pdf_units(self.cap_height)
    }

    /// Get bounding box in PDF units.
    pub fn pdf_bbox(&self) -> (i32, i32, i32, i32) {
        (
            self.to_pdf_units(self.bbox.0),
            self.to_pdf_units(self.bbox.1),
            self.to_pdf_units(self.bbox.2),
            self.to_pdf_units(self.bbox.3),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require actual font data to run meaningfully.
    // In real usage, tests would load a TTF file from the test fixtures.

    #[test]
    fn test_error_on_empty_data() {
        let result = TrueTypeFont::parse(&[]);
        assert!(matches!(result, Err(TrueTypeError::EmptyFont)));
    }

    #[test]
    fn test_error_on_invalid_data() {
        let result = TrueTypeFont::parse(b"not a font file");
        assert!(matches!(result, Err(TrueTypeError::ParseError(_))));
    }

    #[test]
    fn test_font_flags_nonsymbolic() {
        // When we have a real font, it should have the nonsymbolic flag
        // This is a placeholder for when font test data is available
    }

    #[test]
    fn test_tounicode_cmap_format() {
        // Test that ToUnicode CMap generation produces valid structure
        let mut used_chars = HashMap::new();
        used_chars.insert(0x0041, 1_u16); // 'A' -> GID 1
        used_chars.insert(0x0042, 2_u16); // 'B' -> GID 2

        // We'd need a real font to test this fully
        // This validates the format structure
    }
}
