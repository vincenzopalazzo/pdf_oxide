//! Structure tree traversal for extracting reading order.
//!
//! Implements pre-order traversal of structure trees to determine correct reading order.

use super::types::{StructChild, StructElem, StructTreeRoot, StructType};
use crate::error::Error;

/// Represents an ordered content item extracted from structure tree.
#[derive(Debug, Clone)]
pub struct OrderedContent {
    /// Page number
    pub page: u32,

    /// Marked Content ID (None for word break markers)
    pub mcid: Option<u32>,

    /// Structure type (for semantic information)
    pub struct_type: String,

    /// Pre-parsed structure type for efficient access
    pub parsed_type: StructType,

    /// Is this a heading?
    pub is_heading: bool,

    /// Is this a block-level element?
    pub is_block: bool,

    /// Is this a word break marker (WB element)?
    ///
    /// When true, a space should be inserted at this position during
    /// text assembly. This supports CJK text that uses WB elements
    /// to mark word boundaries.
    pub is_word_break: bool,

    /// Actual text replacement from /ActualText (optional)
    /// Per PDF spec Section 14.9.4, when present this replaces all
    /// descendant content with the specified text.
    pub actual_text: Option<String>,
}

/// Traverse the structure tree and extract ordered content for a specific page.
///
/// This performs a pre-order traversal of the structure tree, extracting
/// marked content references in document order.
///
/// # Arguments
/// * `struct_tree` - The structure tree root
/// * `page_num` - The page number to extract content for
///
/// # Returns
/// * Vector of ordered content items for the specified page
pub fn traverse_structure_tree(
    struct_tree: &StructTreeRoot,
    page_num: u32,
) -> Result<Vec<OrderedContent>, Error> {
    let mut result = Vec::new();

    // Traverse each root element
    for root_elem in &struct_tree.root_elements {
        traverse_element(root_elem, page_num, &mut result)?;
    }

    Ok(result)
}

/// Traverse the structure tree once and build content for ALL pages.
///
/// This is much more efficient than calling `traverse_structure_tree` once per page,
/// which would walk the entire tree N times. Instead, we walk the tree once and
/// collect content items into per-page buckets.
///
/// Returns a HashMap mapping page numbers to their ordered content items.
pub fn traverse_structure_tree_all_pages(
    struct_tree: &StructTreeRoot,
) -> std::collections::HashMap<u32, Vec<OrderedContent>> {
    let mut result: std::collections::HashMap<u32, Vec<OrderedContent>> =
        std::collections::HashMap::new();

    for root_elem in &struct_tree.root_elements {
        traverse_element_all_pages(root_elem, &mut result);
    }

    result
}

/// Recursively traverse a structure element, collecting content for all pages.
fn traverse_element_all_pages(
    elem: &StructElem,
    result: &mut std::collections::HashMap<u32, Vec<OrderedContent>>,
) {
    let struct_type_str = format!("{:?}", elem.struct_type);
    let parsed_type = elem.struct_type.clone();
    let is_heading = elem.struct_type.is_heading();
    let is_block = elem.struct_type.is_block();
    let is_word_break = elem.struct_type.is_word_break();

    // If /ActualText is present, it replaces all descendant content (PDF spec 14.9.4)
    if let Some(ref actual_text) = elem.actual_text {
        // Collect all pages this element has content on
        let pages = collect_pages(elem);
        for page in pages {
            result.entry(page).or_default().push(OrderedContent {
                page,
                mcid: None,
                struct_type: struct_type_str.clone(),
                parsed_type: parsed_type.clone(),
                is_heading,
                is_block,
                is_word_break: false,
                actual_text: Some(actual_text.clone()),
            });
        }
        return;
    }

    // If this is a WB (word break) element, emit a word break marker for all relevant pages
    if is_word_break {
        // WB elements don't have a specific page, emit for parent's page context
        // Since we don't know the page here, we handle it in the child loop
    }

    // Process children in order
    for child in &elem.children {
        match child {
            StructChild::MarkedContentRef { mcid, page } => {
                result.entry(*page).or_default().push(OrderedContent {
                    page: *page,
                    mcid: Some(*mcid),
                    struct_type: struct_type_str.clone(),
                    parsed_type: parsed_type.clone(),
                    is_heading,
                    is_block,
                    is_word_break: false,
                    actual_text: None,
                });
            },

            StructChild::StructElem(child_elem) => {
                // If parent is WB, emit word break markers before processing child
                if is_word_break {
                    let child_pages = collect_pages(child_elem);
                    for page in child_pages {
                        result.entry(page).or_default().push(OrderedContent {
                            page,
                            mcid: None,
                            struct_type: struct_type_str.clone(),
                            parsed_type: parsed_type.clone(),
                            is_heading: false,
                            is_block: false,
                            is_word_break: true,
                            actual_text: None,
                        });
                    }
                }
                traverse_element_all_pages(child_elem, result);
            },

            StructChild::ObjectRef(_obj_num, _gen) => {
                log::debug!("Skipping unresolved ObjectRef({}, {})", _obj_num, _gen);
            },
        }
    }
}

/// Collect all page numbers that a structure element has content on.
fn collect_pages(elem: &StructElem) -> Vec<u32> {
    let mut pages = Vec::new();
    collect_pages_recursive(elem, &mut pages);
    pages.sort_unstable();
    pages.dedup();
    pages
}

fn collect_pages_recursive(elem: &StructElem, pages: &mut Vec<u32>) {
    if let Some(page) = elem.page {
        pages.push(page);
    }
    for child in &elem.children {
        match child {
            StructChild::MarkedContentRef { page, .. } => {
                pages.push(*page);
            },
            StructChild::StructElem(child_elem) => {
                collect_pages_recursive(child_elem, pages);
            },
            _ => {},
        }
    }
}

/// Recursively traverse a structure element.
///
/// Performs pre-order traversal:
/// 1. Process current element's marked content (if on target page)
/// 2. Recursively process children in order
/// 3. Handle WB (word break) elements by emitting markers
fn traverse_element(
    elem: &StructElem,
    target_page: u32,
    result: &mut Vec<OrderedContent>,
) -> Result<(), Error> {
    let struct_type_str = format!("{:?}", elem.struct_type);
    let parsed_type = elem.struct_type.clone();
    let is_heading = elem.struct_type.is_heading();
    let is_block = elem.struct_type.is_block();
    let is_word_break = elem.struct_type.is_word_break();

    // If /ActualText is present, it replaces all descendant content (PDF spec 14.9.4)
    if let Some(ref actual_text) = elem.actual_text {
        if has_content_on_page(elem, target_page) {
            result.push(OrderedContent {
                page: target_page,
                mcid: None,
                struct_type: struct_type_str,
                parsed_type,
                is_heading,
                is_block,
                is_word_break: false,
                actual_text: Some(actual_text.clone()),
            });
            return Ok(());
        }
    }

    // If this is a WB (word break) element, emit a word break marker
    if is_word_break {
        result.push(OrderedContent {
            page: target_page,
            mcid: None,
            struct_type: struct_type_str.clone(),
            parsed_type: parsed_type.clone(),
            is_heading: false,
            is_block: false,
            is_word_break: true,
            actual_text: None,
        });
        // WB elements typically have no children, but process any just in case
    }

    // Process children in order
    for child in &elem.children {
        match child {
            StructChild::MarkedContentRef { mcid, page } => {
                // If this marked content is on the target page, add it
                if *page == target_page {
                    result.push(OrderedContent {
                        page: *page,
                        mcid: Some(*mcid),
                        struct_type: struct_type_str.clone(),
                        parsed_type: parsed_type.clone(),
                        is_heading,
                        is_block,
                        is_word_break: false,
                        actual_text: None,
                    });
                }
            },

            StructChild::StructElem(child_elem) => {
                // Recursively traverse child element
                traverse_element(child_elem, target_page, result)?;
            },

            StructChild::ObjectRef(_obj_num, _gen) => {
                // ObjectRef should be resolved at parse time (structure/parser.rs).
                // If we encounter one here, it means the reference couldn't be resolved.
                log::debug!("Skipping unresolved ObjectRef({}, {})", _obj_num, _gen);
            },
        }
    }

    Ok(())
}

/// Check if a structure element has any content on the target page.
fn has_content_on_page(elem: &StructElem, target_page: u32) -> bool {
    if elem.page == Some(target_page) {
        return true;
    }
    for child in &elem.children {
        match child {
            StructChild::MarkedContentRef { page, .. } => {
                if *page == target_page {
                    return true;
                }
            },
            StructChild::StructElem(child_elem) => {
                if has_content_on_page(child_elem, target_page) {
                    return true;
                }
            },
            _ => {},
        }
    }
    false
}

/// Extract all marked content IDs in reading order for a page.
///
/// This is a simpler interface that just returns the MCIDs in order,
/// which can be used to reorder extracted text blocks.
///
/// Note: Word break (WB) markers are filtered out since they don't have MCIDs.
/// Use `traverse_structure_tree` directly if you need word break information.
///
/// # Arguments
/// * `struct_tree` - The structure tree root
/// * `page_num` - The page number
///
/// # Returns
/// * Vector of MCIDs in reading order
pub fn extract_reading_order(
    struct_tree: &StructTreeRoot,
    page_num: u32,
) -> Result<Vec<u32>, Error> {
    let ordered_content = traverse_structure_tree(struct_tree, page_num)?;
    Ok(ordered_content
        .into_iter()
        .filter_map(|c| c.mcid) // Filter out word break markers (mcid=None)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::types::{StructChild, StructElem, StructType};

    #[test]
    fn test_simple_traversal() {
        // Create a simple structure tree:
        // Document
        //   ├─ P (MCID=0, page=0)
        //   └─ P (MCID=1, page=0)
        let mut root = StructElem::new(StructType::Document);

        let mut p1 = StructElem::new(StructType::P);
        p1.add_child(StructChild::MarkedContentRef { mcid: 0, page: 0 });

        let mut p2 = StructElem::new(StructType::P);
        p2.add_child(StructChild::MarkedContentRef { mcid: 1, page: 0 });

        root.add_child(StructChild::StructElem(Box::new(p1)));
        root.add_child(StructChild::StructElem(Box::new(p2)));

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        // Extract reading order
        let order = extract_reading_order(&struct_tree, 0).unwrap();
        assert_eq!(order, vec![0, 1]);
    }

    #[test]
    fn test_page_filtering() {
        // Create structure with content on different pages
        let mut root = StructElem::new(StructType::Document);

        let mut p1 = StructElem::new(StructType::P);
        p1.add_child(StructChild::MarkedContentRef { mcid: 0, page: 0 });

        let mut p2 = StructElem::new(StructType::P);
        p2.add_child(StructChild::MarkedContentRef { mcid: 1, page: 1 });

        root.add_child(StructChild::StructElem(Box::new(p1)));
        root.add_child(StructChild::StructElem(Box::new(p2)));

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        // Extract page 0 - should only get MCID 0
        let order_page_0 = extract_reading_order(&struct_tree, 0).unwrap();
        assert_eq!(order_page_0, vec![0]);

        // Extract page 1 - should only get MCID 1
        let order_page_1 = extract_reading_order(&struct_tree, 1).unwrap();
        assert_eq!(order_page_1, vec![1]);
    }

    #[test]
    fn test_nested_structure() {
        // Create nested structure:
        // Document
        //   └─ Sect
        //       ├─ H1 (MCID=0)
        //       └─ P (MCID=1)
        let mut root = StructElem::new(StructType::Document);

        let mut sect = StructElem::new(StructType::Sect);

        let mut h1 = StructElem::new(StructType::H1);
        h1.add_child(StructChild::MarkedContentRef { mcid: 0, page: 0 });

        let mut p = StructElem::new(StructType::P);
        p.add_child(StructChild::MarkedContentRef { mcid: 1, page: 0 });

        sect.add_child(StructChild::StructElem(Box::new(h1)));
        sect.add_child(StructChild::StructElem(Box::new(p)));

        root.add_child(StructChild::StructElem(Box::new(sect)));

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        // Should traverse in order: H1 (MCID 0), then P (MCID 1)
        let order = extract_reading_order(&struct_tree, 0).unwrap();
        assert_eq!(order, vec![0, 1]);
    }

    #[test]
    fn test_word_break_elements() {
        // Create structure with WB (word break) elements for CJK text:
        // P
        //   ├─ Span (MCID=0) - "你好"
        //   ├─ WB             - word boundary marker
        //   └─ Span (MCID=1) - "世界"
        let mut root = StructElem::new(StructType::P);

        let mut span1 = StructElem::new(StructType::Span);
        span1.add_child(StructChild::MarkedContentRef { mcid: 0, page: 0 });

        let wb = StructElem::new(StructType::WB);

        let mut span2 = StructElem::new(StructType::Span);
        span2.add_child(StructChild::MarkedContentRef { mcid: 1, page: 0 });

        root.add_child(StructChild::StructElem(Box::new(span1)));
        root.add_child(StructChild::StructElem(Box::new(wb)));
        root.add_child(StructChild::StructElem(Box::new(span2)));

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        // traverse_structure_tree should include the word break marker
        let ordered = traverse_structure_tree(&struct_tree, 0).unwrap();
        assert_eq!(ordered.len(), 3); // MCID 0, WB, MCID 1
        assert_eq!(ordered[0].mcid, Some(0));
        assert!(!ordered[0].is_word_break);
        assert_eq!(ordered[1].mcid, None); // WB has no MCID
        assert!(ordered[1].is_word_break);
        assert_eq!(ordered[2].mcid, Some(1));
        assert!(!ordered[2].is_word_break);

        // extract_reading_order should filter out WB markers
        let mcids = extract_reading_order(&struct_tree, 0).unwrap();
        assert_eq!(mcids, vec![0, 1]); // Only MCIDs, no WB
    }

    #[test]
    fn test_empty_tree() {
        let struct_tree = StructTreeRoot::new();
        let order = extract_reading_order(&struct_tree, 0).unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn test_empty_page() {
        let mut root = StructElem::new(StructType::Document);
        let mut p = StructElem::new(StructType::P);
        p.add_child(StructChild::MarkedContentRef { mcid: 0, page: 0 });
        root.add_child(StructChild::StructElem(Box::new(p)));

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        // Page 5 has no content
        let order = extract_reading_order(&struct_tree, 5).unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn test_object_ref_skipped() {
        let mut root = StructElem::new(StructType::Document);
        root.add_child(StructChild::ObjectRef(42, 0));
        root.add_child(StructChild::MarkedContentRef { mcid: 0, page: 0 });

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        let order = extract_reading_order(&struct_tree, 0).unwrap();
        assert_eq!(order, vec![0]);
    }

    #[test]
    fn test_traverse_all_pages() {
        let mut root = StructElem::new(StructType::Document);

        let mut p1 = StructElem::new(StructType::P);
        p1.add_child(StructChild::MarkedContentRef { mcid: 0, page: 0 });

        let mut p2 = StructElem::new(StructType::P);
        p2.add_child(StructChild::MarkedContentRef { mcid: 1, page: 1 });

        let mut p3 = StructElem::new(StructType::P);
        p3.add_child(StructChild::MarkedContentRef { mcid: 2, page: 0 });

        root.add_child(StructChild::StructElem(Box::new(p1)));
        root.add_child(StructChild::StructElem(Box::new(p2)));
        root.add_child(StructChild::StructElem(Box::new(p3)));

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        let all_pages = traverse_structure_tree_all_pages(&struct_tree);
        assert_eq!(all_pages.len(), 2); // pages 0 and 1
        assert_eq!(all_pages[&0].len(), 2); // MCIDs 0 and 2
        assert_eq!(all_pages[&1].len(), 1); // MCID 1
    }

    #[test]
    fn test_actual_text_replaces_descendants() {
        let mut root = StructElem::new(StructType::Document);

        let mut elem = StructElem::new(StructType::Span);
        elem.actual_text = Some("Replacement text".to_string());
        elem.add_child(StructChild::MarkedContentRef { mcid: 0, page: 0 });

        root.add_child(StructChild::StructElem(Box::new(elem)));

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        let ordered = traverse_structure_tree(&struct_tree, 0).unwrap();
        assert_eq!(ordered.len(), 1);
        assert_eq!(ordered[0].actual_text, Some("Replacement text".to_string()));
        assert_eq!(ordered[0].mcid, None); // No MCID when actual_text is used
    }

    #[test]
    fn test_actual_text_wrong_page() {
        let mut root = StructElem::new(StructType::Document);

        let mut elem = StructElem::new(StructType::Span);
        elem.actual_text = Some("Replacement".to_string());
        elem.add_child(StructChild::MarkedContentRef { mcid: 0, page: 1 });

        root.add_child(StructChild::StructElem(Box::new(elem)));

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        // Page 0 has no content (actual_text elem is on page 1)
        let ordered = traverse_structure_tree(&struct_tree, 0).unwrap();
        assert!(ordered.is_empty());
    }

    #[test]
    fn test_heading_and_block_flags() {
        let mut root = StructElem::new(StructType::Document);

        let mut h1 = StructElem::new(StructType::H1);
        h1.add_child(StructChild::MarkedContentRef { mcid: 0, page: 0 });

        let mut span = StructElem::new(StructType::Span);
        span.add_child(StructChild::MarkedContentRef { mcid: 1, page: 0 });

        root.add_child(StructChild::StructElem(Box::new(h1)));
        root.add_child(StructChild::StructElem(Box::new(span)));

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        let ordered = traverse_structure_tree(&struct_tree, 0).unwrap();
        assert_eq!(ordered.len(), 2);
        assert!(ordered[0].is_heading);
        assert!(ordered[0].is_block);
        assert!(!ordered[1].is_heading);
        assert!(!ordered[1].is_block);
    }

    #[test]
    fn test_collect_pages() {
        let mut elem = StructElem::new(StructType::Document);
        elem.page = Some(0);

        let mut child = StructElem::new(StructType::P);
        child.add_child(StructChild::MarkedContentRef { mcid: 0, page: 1 });
        child.add_child(StructChild::MarkedContentRef { mcid: 1, page: 2 });

        elem.add_child(StructChild::StructElem(Box::new(child)));

        let pages = collect_pages(&elem);
        assert_eq!(pages, vec![0, 1, 2]);
    }

    #[test]
    fn test_traverse_all_pages_with_actual_text() {
        let mut root = StructElem::new(StructType::Document);

        let mut elem = StructElem::new(StructType::Span);
        elem.actual_text = Some("Hello".to_string());
        elem.add_child(StructChild::MarkedContentRef { mcid: 0, page: 0 });
        elem.add_child(StructChild::MarkedContentRef { mcid: 1, page: 1 });

        root.add_child(StructChild::StructElem(Box::new(elem)));

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        let all_pages = traverse_structure_tree_all_pages(&struct_tree);
        // Actual text should appear on both pages
        assert!(all_pages.contains_key(&0));
        assert!(all_pages.contains_key(&1));
        assert_eq!(all_pages[&0][0].actual_text, Some("Hello".to_string()));
    }

    #[test]
    fn test_traverse_all_pages_word_break_with_children() {
        let mut root = StructElem::new(StructType::P);

        let mut wb = StructElem::new(StructType::WB);
        let mut child = StructElem::new(StructType::Span);
        child.add_child(StructChild::MarkedContentRef { mcid: 0, page: 0 });
        wb.add_child(StructChild::StructElem(Box::new(child)));

        root.add_child(StructChild::StructElem(Box::new(wb)));

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        let all_pages = traverse_structure_tree_all_pages(&struct_tree);
        let page0 = &all_pages[&0];
        // Should have word break marker and the child's MCID
        assert!(page0.iter().any(|c| c.is_word_break));
        assert!(page0.iter().any(|c| c.mcid == Some(0)));
    }

    #[test]
    fn test_traverse_all_pages_object_ref() {
        let mut root = StructElem::new(StructType::Document);
        root.add_child(StructChild::ObjectRef(99, 0));
        root.add_child(StructChild::MarkedContentRef { mcid: 0, page: 0 });

        let mut struct_tree = StructTreeRoot::new();
        struct_tree.add_root_element(root);

        let all_pages = traverse_structure_tree_all_pages(&struct_tree);
        assert_eq!(all_pages[&0].len(), 1);
        assert_eq!(all_pages[&0][0].mcid, Some(0));
    }

    #[test]
    fn test_has_content_on_page_deep() {
        let mut root = StructElem::new(StructType::Document);
        let mut sect = StructElem::new(StructType::Sect);
        let mut p = StructElem::new(StructType::P);
        p.add_child(StructChild::MarkedContentRef { mcid: 0, page: 3 });
        sect.add_child(StructChild::StructElem(Box::new(p)));
        root.add_child(StructChild::StructElem(Box::new(sect)));

        assert!(has_content_on_page(&root, 3));
        assert!(!has_content_on_page(&root, 0));
    }
}
