use skrifa::outline::STYLE_CLASSES;

pub(crate) const STYLE_COUNT: usize = STYLE_CLASSES.len();
pub(crate) const STYLE_INDEX_UNASSIGNED: u16 = u16::MAX;

/// Style information for a glyph.
///
/// Replaces the bit-packed u16 representation with explicit fields for clarity and maintainability.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GlyphStyle {
    /// The style index (TA-style 0-83, will become Skrifa index 0-89).
    /// Special value: 0x3FFF = unassigned
    pub style_index: u16,
    /// True if this glyph is a digit
    pub is_digit: bool,
    /// True if this glyph is a non-base character
    pub is_non_base: bool,
}

impl GlyphStyle {
    /// Create an unassigned style (no specific style, flags cleared)
    pub const fn unassigned() -> Self {
        GlyphStyle {
            style_index: STYLE_INDEX_UNASSIGNED,
            is_digit: false,
            is_non_base: false,
        }
    }

    /// Check if this style is unassigned
    pub fn is_unassigned(&self) -> bool {
        self.style_index == STYLE_INDEX_UNASSIGNED
    }

    /// Create a style with the given index and flags
    pub const fn new(style_index: u16, is_digit: bool, is_non_base: bool) -> Self {
        GlyphStyle {
            style_index,
            is_digit,
            is_non_base,
        }
    }
}
