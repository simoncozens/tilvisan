use skrifa::outline::{SCRIPT_CLASSES, STYLE_CLASSES};

/// Check if a script is oriented top-to-bottom (e.g., Mongolian, Gothic).
/// Returns false for out-of-bounds style indices.
pub(crate) fn script_hints_top_to_bottom(style_index: usize) -> bool {
    if let Some(style) = STYLE_CLASSES.get(style_index) {
        style.script.hint_top_to_bottom
    } else {
        false
    }
}

/// Return the default Skrifa style for a script index.
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

        return Some(style_index);
    }

    None
}

pub(crate) fn none_default_style() -> Option<usize> {
    STYLE_CLASSES
        .iter()
        .enumerate()
        .find(|(_, style)| style.name == "NONE_DFLT")
        .map(|(idx, _)| idx)
}
