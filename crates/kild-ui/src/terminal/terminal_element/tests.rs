use super::element::{detect_urls, scroll_delta_lines};
use super::types::LineText;
use gpui::{Bounds, point, px, size};

/// Build col_offsets for a line where each char is either wide (2 cols) or normal (1 col).
/// `wide_indices` lists the char indices that are wide (2 grid columns).
fn build_col_offsets(text: &str, wide_indices: &[usize]) -> Vec<(usize, usize)> {
    let mut offsets = Vec::new();
    let mut grid_col = 0usize;
    for (i, _ch) in text.chars().enumerate() {
        let width = if wide_indices.contains(&i) { 2 } else { 1 };
        offsets.push((grid_col, width));
        grid_col += width;
    }
    offsets
}

fn test_bounds() -> Bounds<gpui::Pixels> {
    Bounds::new(point(px(0.0), px(0.0)), size(px(800.0), px(600.0)))
}

fn cell_w() -> gpui::Pixels {
    px(10.0)
}

fn cell_h() -> gpui::Pixels {
    px(20.0)
}

#[test]
fn single_url_basic() {
    let text = "Check https://example.com here".to_string();
    let col_offsets = build_col_offsets(&text, &[]);
    let line_texts = vec![(0i32, text, col_offsets)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].url, "https://example.com");
    // "Check " = 6 chars, so URL starts at col 6
    assert_eq!(regions[0].bounds.origin.x, px(60.0));
    assert_eq!(regions[0].bounds.origin.y, px(0.0));
    // "https://example.com" = 19 chars = 19 cols wide
    assert_eq!(regions[0].bounds.size.width, px(190.0));
    assert_eq!(regions[0].bounds.size.height, cell_h());
}

#[test]
fn url_at_column_zero() {
    let text = "https://example.com rest".to_string();
    let col_offsets = build_col_offsets(&text, &[]);
    let line_texts = vec![(0i32, text, col_offsets)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].url, "https://example.com");
    assert_eq!(regions[0].bounds.origin.x, px(0.0));
}

#[test]
fn url_at_end_of_line() {
    let text = "prefix https://example.com".to_string();
    let col_offsets = build_col_offsets(&text, &[]);
    let line_texts = vec![(0i32, text, col_offsets)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].url, "https://example.com");
    // "prefix " = 7 chars
    assert_eq!(regions[0].bounds.origin.x, px(70.0));
    // 19 cols wide
    assert_eq!(regions[0].bounds.size.width, px(190.0));
}

#[test]
fn empty_line_no_panic() {
    let text = "".to_string();
    let col_offsets: Vec<(usize, usize)> = vec![];
    let line_texts = vec![(0i32, text, col_offsets)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert!(regions.is_empty());
}

#[test]
fn no_urls_in_plain_text() {
    let text = "just plain text with no links".to_string();
    let col_offsets = build_col_offsets(&text, &[]);
    let line_texts = vec![(0i32, text, col_offsets)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert!(regions.is_empty());
}

#[test]
fn empty_input_vec() {
    let line_texts: Vec<(i32, String, Vec<(usize, usize)>)> = vec![];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert!(regions.is_empty());
}

#[test]
fn url_with_query_string() {
    let text = "https://example.com/path?q=1&b=2".to_string();
    let col_offsets = build_col_offsets(&text, &[]);
    let line_texts = vec![(0i32, text.clone(), col_offsets)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].url, text);
    assert_eq!(regions[0].bounds.origin.x, px(0.0));
    // 32 chars = 32 cols
    assert_eq!(regions[0].bounds.size.width, px(320.0));
}

#[test]
fn multiple_urls_non_overlapping() {
    let text = "Visit https://a.com or https://b.com end".to_string();
    let col_offsets = build_col_offsets(&text, &[]);
    let line_texts = vec![(0i32, text, col_offsets)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert_eq!(regions.len(), 2);
    assert_eq!(regions[0].url, "https://a.com");
    assert_eq!(regions[1].url, "https://b.com");

    // Verify non-overlapping: first region ends before second starts
    let first_end_x = regions[0].bounds.origin.x + regions[0].bounds.size.width;
    let second_start_x = regions[1].bounds.origin.x;
    assert!(
        first_end_x <= second_start_x,
        "URL regions overlap: first ends at {first_end_x:?}, second starts at {second_start_x:?}"
    );

    // Same line, same y coordinate
    assert_eq!(regions[0].bounds.origin.y, regions[1].bounds.origin.y);
}

#[test]
fn emoji_before_url_wide_char_bounds() {
    // Rocket emoji is a wide char (2 grid cols), then space (1 col), then URL
    // Grid layout: cols 0-1 = rocket, col 2 = space, cols 3+ = URL
    let text = "\u{1F680} https://example.com".to_string();
    let col_offsets = build_col_offsets(&text, &[0]); // index 0 is the emoji (wide)
    let line_texts = vec![(0i32, text, col_offsets)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].url, "https://example.com");
    // Emoji=2 cols + space=1 col = URL starts at grid col 3
    assert_eq!(regions[0].bounds.origin.x, px(30.0));
    // 19 chars, all normal width = 19 cols
    assert_eq!(regions[0].bounds.size.width, px(190.0));
}

#[test]
fn cjk_before_url_wide_char_bounds() {
    // Two CJK chars (each 2 grid cols), then space, then URL
    // Grid layout: cols 0-1 = first CJK, cols 2-3 = second CJK, col 4 = space, cols 5+ = URL
    let text = "\u{4F60}\u{597D} https://example.com".to_string();
    let col_offsets = build_col_offsets(&text, &[0, 1]); // indices 0,1 are CJK (wide)
    let line_texts = vec![(0i32, text, col_offsets)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].url, "https://example.com");
    // 2 wide chars = 4 cols + space = 1 col → URL starts at grid col 5
    assert_eq!(regions[0].bounds.origin.x, px(50.0));
    assert_eq!(regions[0].bounds.size.width, px(190.0));
}

#[test]
fn url_on_different_line_index() {
    let text = "https://example.com".to_string();
    let col_offsets = build_col_offsets(&text, &[]);
    // Line index 3 means y offset = 3 * cell_height
    let line_texts = vec![(3i32, text, col_offsets)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].bounds.origin.y, px(60.0)); // 3 * 20.0
}

#[test]
fn bounds_origin_offset() {
    let text = "https://example.com".to_string();
    let col_offsets = build_col_offsets(&text, &[]);
    let line_texts = vec![(0i32, text, col_offsets)];
    let offset_bounds = Bounds::new(point(px(100.0), px(50.0)), size(px(800.0), px(600.0)));
    let regions = detect_urls(&line_texts, offset_bounds, cell_w(), cell_h());

    assert_eq!(regions.len(), 1);
    // x should be offset by bounds origin
    assert_eq!(regions[0].bounds.origin.x, px(100.0));
    assert_eq!(regions[0].bounds.origin.y, px(50.0));
}

#[test]
fn bare_domain_without_scheme_not_matched() {
    // linkify with LinkKind::Url should not match bare domains without a scheme
    let text = "not a url example.com something".to_string();
    let col_offsets = build_col_offsets(&text, &[]);
    let line_texts = vec![(0i32, text, col_offsets)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert!(regions.is_empty());
}

#[test]
fn multiple_lines_with_urls() {
    let text1 = "line1 https://a.com".to_string();
    let offsets1 = build_col_offsets(&text1, &[]);
    let text2 = "line2 https://b.com".to_string();
    let offsets2 = build_col_offsets(&text2, &[]);
    let line_texts = vec![(0i32, text1, offsets1), (2i32, text2, offsets2)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

    assert_eq!(regions.len(), 2);
    assert_eq!(regions[0].url, "https://a.com");
    assert_eq!(regions[0].bounds.origin.y, px(0.0)); // line 0
    assert_eq!(regions[1].url, "https://b.com");
    assert_eq!(regions[1].bounds.origin.y, px(40.0)); // line 2 * 20.0
}

// --- scroll direction tests ---

#[test]
fn scroll_up_produces_positive_delta() {
    // GPUI: positive pixel_delta.y = user scrolled up (toward history).
    // Alacritty: positive Scroll::Delta = scroll display up.
    // These must agree — no negation.
    let lines = scroll_delta_lines(px(60.0), px(20.0));
    assert_eq!(lines, 3);
}

#[test]
fn scroll_down_produces_negative_delta() {
    let lines = scroll_delta_lines(px(-60.0), px(20.0));
    assert_eq!(lines, -3);
}

#[test]
fn scroll_small_delta_rounds_to_zero() {
    // Sub-half-cell movement should round to 0 and be ignored.
    let lines = scroll_delta_lines(px(5.0), px(20.0));
    assert_eq!(lines, 0);
}

#[test]
fn scroll_half_cell_rounds_to_one() {
    let lines = scroll_delta_lines(px(10.0), px(20.0));
    assert_eq!(lines, 1);
}

// --- cmd_held URL gate tests ---

#[test]
fn url_detection_returns_empty_when_no_lines_accumulated() {
    // When cmd_held is false, no line text is accumulated → detect_urls
    // receives an empty slice and returns no regions. This test documents
    // that invariant to prevent performance regression if the gate is removed.
    let empty: Vec<LineText> = vec![];
    let regions = detect_urls(&empty, test_bounds(), cell_w(), cell_h());
    assert!(regions.is_empty());
}

#[test]
fn url_detection_finds_urls_when_lines_accumulated() {
    // When cmd_held is true, lines are accumulated and passed to detect_urls.
    let text = "See https://example.com for details".to_string();
    let col_offsets = build_col_offsets(&text, &[]);
    let line_texts = vec![(0i32, text, col_offsets)];
    let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].url, "https://example.com");
}
