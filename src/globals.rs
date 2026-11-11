use crate::{
    font::Font,
    style::{StyleIndex, STYLE_INDEX_UNASSIGNED},
    AutohintError,
};
use indexmap::IndexMap;
use skrifa::{
    outline::autohint::{GlyphStyle, GlyphStyles, STYLE_CLASSES},
    raw::{
        tables::glyf::{Anchor, CompositeGlyphFlags, Glyph},
        TableProvider,
    },
    GlyphId, MetadataProvider,
};

/// Helper to convert a tag string to a 4-byte array, padding with spaces if needed.
#[cfg(feature = "control")]
fn string_to_tag_bytes(s: &str) -> Option<[u8; 4]> {
    let bytes = s.as_bytes();
    if bytes.len() > 4 {
        return None; // Tag too long
    }
    let mut tag = [b' '; 4];
    tag[..bytes.len()].copy_from_slice(bytes);
    Some(tag)
}

/// Resolve a script/feature pair to a Skrifa style index.
/// Returns the index of the first matching style in STYLE_CLASSES, or None if not found.
#[cfg(feature = "control")]
pub(crate) fn resolve_script_feature_to_style_index(script: &str, feature: &str) -> Option<usize> {
    use skrifa::Tag;

    // Convert strings to tags
    let script_bytes = string_to_tag_bytes(script)?;
    let script_tag = Tag::new(&script_bytes);

    let feature_tag = if feature.is_empty() || feature == "dflt" {
        None
    } else {
        string_to_tag_bytes(feature).map(|bytes| Tag::new(&bytes))
    };

    // Search STYLE_CLASSES for a match
    for (idx, style_class) in STYLE_CLASSES.iter().enumerate() {
        // Check if script matches
        if style_class.script.tag != script_tag {
            continue;
        }

        // Check if feature matches
        match (feature_tag, style_class.feature) {
            // No feature requested, and style has no feature (uses DFLT)
            (None, None) => return Some(idx),
            // Feature requested, and style has matching feature
            (Some(req_feat), Some(style_feat)) if req_feat == style_feat => return Some(idx),
            // Otherwise, continue searching
            _ => continue,
        }
    }

    None
}

pub(crate) fn compute_style_coverage(
    font: &Font,
    glyph_count: usize,
    fallback_style: u16,
    debug_dump: bool,
    face_index: i32,
    num_faces: i32,
) -> Result<(Vec<GlyphStyle>, IndexMap<StyleIndex, GlyphId>), AutohintError> {
    let mut glyph_styles_out = vec![GlyphStyle::default(); glyph_count];
    let mut sample_glyphs_map: IndexMap<StyleIndex, GlyphId> = IndexMap::new();

    fn propagate_style_to_composites(
        gindex: usize,
        gstyle: GlyphStyle,
        glyph_styles_out: &mut [GlyphStyle],
        composite_children: &[Vec<usize>],
        nesting_level: usize,
    ) -> Result<(), AutohintError> {
        // Match C behavior in ta_face_globals_scan_composite.
        if nesting_level > 100 {
            return Err(AutohintError::InvalidTable);
        }

        for &child in &composite_children[gindex] {
            if !glyph_styles_out[child].is_unassigned() {
                continue;
            }

            glyph_styles_out[child] = gstyle;
            propagate_style_to_composites(
                child,
                gstyle,
                glyph_styles_out,
                composite_children,
                nesting_level + 1,
            )?;
        }

        Ok(())
    }

    fn dump_style_coverage(
        glyph_styles: &[GlyphStyle],
        sample_glyphs: &IndexMap<StyleIndex, GlyphId>,
        face_index: i32,
        num_faces: i32,
    ) {
        if num_faces > 1 {
            eprintln!("\nstyle coverage (subfont {face_index})\n==========================\n");
        } else {
            eprintln!("\nstyle coverage\n==============\n");
        }

        for &style_idx in sample_glyphs.keys() {
            let style_name = STYLE_CLASSES
                .get(style_idx.as_usize())
                .map(|style| style.name)
                .unwrap_or("(unknown)");

            eprintln!("{style_name}:");

            let mut count = 0usize;
            for (idx, style) in glyph_styles.iter().enumerate() {
                if style.style_index().unwrap_or(0xFFFF) as usize == style_idx.as_usize() {
                    if count.is_multiple_of(10) {
                        eprint!(" ");
                    }
                    eprint!(" {idx}");
                    count += 1;
                    if count.is_multiple_of(10) {
                        eprintln!();
                    }
                }
            }

            if count == 0 {
                eprintln!("  (none)");
            } else if !count.is_multiple_of(10) {
                eprintln!();
            }
        }
    }

    let outlines = font.fontref.outline_glyphs();
    let styles = GlyphStyles::new(&outlines);
    let glyf = font.fontref.glyf()?;
    let loca = font.fontref.loca(None)?;

    let mut composite_children = vec![Vec::<usize>::new(); glyph_styles_out.len()];
    #[allow(clippy::needless_range_loop)]
    for gid in 0..glyph_styles_out.len() {
        let Some(glyph) = loca.get_glyf(GlyphId::new(gid as u32), &glyf)? else {
            continue;
        };

        let Glyph::Composite(composite) = glyph else {
            continue;
        };

        for component in composite.components() {
            let cidx = component.glyph.to_u16() as usize;
            if cidx >= glyph_styles_out.len() {
                continue;
            }

            let should_propagate = component
                .flags
                .contains(CompositeGlyphFlags::ARGS_ARE_XY_VALUES)
                && matches!(component.anchor, Anchor::Offset { y: 0, .. });

            if should_propagate {
                composite_children[gid].push(cidx);
            }
        }
    }

    for (gid, style_out) in glyph_styles_out.iter_mut().enumerate() {
        let mut style_index = STYLE_INDEX_UNASSIGNED;
        let style = styles.get((gid as u32).into()).unwrap_or_default();
        let is_non_base = style.is_non_base();
        let is_digit = style.is_digit();

        if let Some(skrifa_style) = style.style_index() {
            style_index = skrifa_style;
            sample_glyphs_map
                .entry(StyleIndex::new(skrifa_style as usize)?)
                .or_insert(GlyphId::new(gid as u32));
        }

        *style_out = GlyphStyle::from_raw_parts(style_index, is_non_base, is_digit);
    }

    for gid in 0..glyph_styles_out.len() {
        let gstyle = glyph_styles_out[gid];
        if gstyle.is_unassigned() {
            continue;
        }

        propagate_style_to_composites(gid, gstyle, &mut glyph_styles_out, &composite_children, 0)?;
    }

    if fallback_style != STYLE_INDEX_UNASSIGNED {
        for style in glyph_styles_out.iter_mut() {
            if style.is_unassigned() {
                *style = GlyphStyle::from_raw_parts(
                    fallback_style,
                    style.is_non_base(),
                    style.is_digit(),
                );
            }
        }
    }

    if debug_dump {
        dump_style_coverage(&glyph_styles_out, &sample_glyphs_map, face_index, num_faces);
    }

    Ok((glyph_styles_out, sample_glyphs_map))
}
