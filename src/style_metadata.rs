use crate::globals::ta_style_to_skrifa_style;
use skrifa::outline::{SCRIPT_CLASSES, STYLE_CLASSES};

/// Check if a script is oriented top-to-bottom (e.g., Mongolian, Gothic).
/// Returns false for out-of-bounds style indices.
pub(crate) fn script_hints_top_to_bottom(style_index: usize) -> bool {
    if let Some(style) =
        ta_style_to_skrifa_style(style_index).and_then(|idx| STYLE_CLASSES.get(idx))
    {
        style.script.hint_top_to_bottom
    } else {
        false
    }
}

/// Return the TA default style for a script index.
///
/// Returns `None` if no default style exists for the given script index.
pub(crate) fn default_style_for_script(script_index: usize) -> Option<usize> {
    let script = SCRIPT_CLASSES.get(script_index)?;

    for (style_index, style) in STYLE_CLASSES.iter().enumerate() {
        if style.feature.is_some() {
            continue;
        }

        if style.script.tag != script.tag {
            continue;
        }

        if let Some(ta_style) = crate::globals::skrifa_style_to_ta_style(style_index) {
            return Some(ta_style as usize);
        }
    }

    None
}
