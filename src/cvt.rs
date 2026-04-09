use std::collections::{HashMap, HashSet};

use crate::{
    bytecode::Bytecode,
    font::Font,
    glyf::StyleCvtData,
    style::{StyleIndex, STYLE_COUNT},
    AutohintError,
};
use indexmap::IndexMap;
use skrifa::{
    outline::{compute_unscaled_style_metrics_exported, STYLE_CLASSES},
    raw::TableProvider,
    GlyphId,
};
use write_fonts::{
    tables::{
        cvar::{Cvar, CvtDeltas},
        gvar::Tent,
    },
    types::F2Dot14,
};

// From tabytecode.h: CVT runtime section size
const CVTL_MAX_RUNTIME: u32 = 7;
const STYLE_SLOTS: usize = STYLE_COUNT;

#[derive(Default)]
pub struct StyleMetrics {
    pub hwidths: Vec<u16>,
    pub vwidths: Vec<u16>,
    pub blue_refs: Vec<u16>,
    pub blue_shoots: Vec<u16>,
    pub blue_adjustment: Vec<u8>,
}

fn compute_style_metrics(
    font: &mut Font,
    style_index: usize,
    sample_glyph: GlyphId,
    coords: &[F2Dot14],
) -> Result<StyleMetrics, AutohintError> {
    if sample_glyph.to_u32() == 0 {
        return Err(AutohintError::MissingStyleSampleGlyph);
    }

    let Some(style_class) = STYLE_CLASSES.get(style_index) else {
        return Err(AutohintError::InvalidTable);
    };

    let metrics = compute_unscaled_style_metrics_exported(&font.fontref, coords, style_class);

    let mut hwidths = Vec::with_capacity(metrics.horizontal_widths.len());
    for &w in &metrics.horizontal_widths {
        let Ok(w) = checked_i32_to_u16(w) else {
            return Err(AutohintError::NumericOverflow);
        };
        hwidths.push(w);
    }

    let mut vwidths = Vec::with_capacity(metrics.vertical_widths.len());
    for &w in &metrics.vertical_widths {
        let Ok(w) = checked_i32_to_u16(w) else {
            return Err(AutohintError::NumericOverflow);
        };
        vwidths.push(w);
    }

    let mut blue_refs = Vec::with_capacity(metrics.blues.len());
    let mut blue_shoots = Vec::with_capacity(metrics.blues.len());
    let mut blue_adjustment = Vec::with_capacity(metrics.blues.len());

    for blue in &metrics.blues {
        let Ok(reference) = checked_i32_to_u16(blue.reference) else {
            return Err(AutohintError::NumericOverflow);
        };
        let Ok(shoot) = checked_i32_to_u16(blue.shoot) else {
            return Err(AutohintError::NumericOverflow);
        };
        blue_refs.push(reference);
        blue_shoots.push(shoot);
        blue_adjustment.push(if blue.is_adjustment { 1 } else { 0 });
    }

    Ok(StyleMetrics {
        hwidths,
        vwidths,
        blue_refs,
        blue_shoots,
        blue_adjustment,
    })
}

pub struct CvtBlobData {
    pub bytecode: Bytecode,
    /// Per-style CVT layout, keyed by Skrifa style index.
    pub style_offsets: IndexMap<StyleIndex, StyleCvtData>,
}

fn checked_i32_to_u16(v: i32) -> Result<u16, AutohintError> {
    if v <= 0xFFFF {
        Ok((v as i64 & 0xFFFF) as u16)
    } else {
        Err(AutohintError::NumericOverflow)
    }
}

fn replace_style_with_fallback(font: &mut Font, style_idx: usize, fallback_style: u16) {
    if font.glyph_styles.is_empty() || font.glyph_count <= 0 {
        return;
    }

    for glyph_style in font.glyph_styles.iter_mut() {
        if glyph_style.style_index as usize == style_idx {
            glyph_style.style_index = fallback_style;
        }
    }
}

fn build_cvt_blob(
    metrics_arr: &[StyleMetrics],
    windows_compatibility: bool,
    units_per_em: u16,
) -> Result<CvtBlobData, AutohintError> {
    if metrics_arr.len() != STYLE_SLOTS {
        return Err(AutohintError::InvalidTable);
    }

    let default_width = (50 * units_per_em as u32) / 2048;

    // First pass: assign compact slots and count totals.
    let mut next_slot = 0u32;
    let mut hwidth_count = 0u32;
    let mut vwidth_count = 0u32;
    let mut blue_count = 0u32;
    let mut slot_assignments: Vec<Option<u32>> = vec![None; STYLE_SLOTS];

    for (i, metrics) in metrics_arr.iter().enumerate() {
        if metrics.blue_refs.is_empty() {
            continue;
        }
        slot_assignments[i] = Some(next_slot);
        next_slot += 1;
        hwidth_count += metrics.hwidths.len() as u32;
        vwidth_count += metrics.vwidths.len() as u32;
        blue_count += metrics.blue_refs.len() as u32;
        if windows_compatibility {
            blue_count += 2;
        }
    }
    let num_used_styles = next_slot;

    let buf_len = CVTL_MAX_RUNTIME
        + num_used_styles
        + 2 * num_used_styles
        + 2 * num_used_styles
        + hwidth_count
        + vwidth_count
        + 2 * blue_count;
    let buf_len_bytes = buf_len * 2;
    let mut bytecode = Bytecode::new();

    let runtime_header_bytes =
        ((CVTL_MAX_RUNTIME + num_used_styles + 2 * num_used_styles) * 2) as usize;
    bytecode.extend(std::iter::repeat_n(0u8, runtime_header_bytes));

    let cvt_offset_base = bytecode.len() as u32;
    let mut style_offsets: IndexMap<StyleIndex, StyleCvtData> = IndexMap::new();

    for (i, metrics) in metrics_arr.iter().enumerate() {
        let Some(slot) = slot_assignments[i] else {
            continue;
        };

        let cvt_offset = ((bytecode.len() as u32) - cvt_offset_base) >> 1;

        let metric_blue_count = metrics.blue_refs.len();
        let total_blue_count = if windows_compatibility {
            metric_blue_count + 2
        } else {
            metric_blue_count
        };

        let hstd = metrics
            .hwidths
            .first()
            .copied()
            .unwrap_or(default_width as u16);
        bytecode.push_word(hstd as u32);
        for &w in &metrics.hwidths {
            bytecode.push_word(w as u32);
        }

        let vstd = metrics
            .vwidths
            .first()
            .copied()
            .unwrap_or(default_width as u16);
        bytecode.push_word(vstd as u32);
        for &w in &metrics.vwidths {
            bytecode.push_word(w as u32);
        }

        let mut blue_adjustment_offset = 0xFFFFu32;

        for &ref_val in &metrics.blue_refs {
            bytecode.push_word(ref_val as u32);
        }
        if windows_compatibility {
            bytecode.push_word(0);
            bytecode.push_word(0);
        }

        for (j, (&shoot_val, &adjustment)) in metrics
            .blue_shoots
            .iter()
            .zip(metrics.blue_adjustment.iter())
            .enumerate()
        {
            bytecode.push_word(shoot_val as u32);
            if adjustment != 0 {
                blue_adjustment_offset = j as u32;
            }
        }
        if windows_compatibility {
            bytecode.push_word(0);
            bytecode.push_word(0);
        }

        style_offsets.insert(
            StyleIndex::new(i)?,
            StyleCvtData {
                slot,
                cvt_offset,
                horz_width_size: metrics.hwidths.len() as u32,
                vert_width_size: metrics.vwidths.len() as u32,
                blue_zone_size: total_blue_count as u32,
                blue_adjustment_offset,
            },
        );
    }

    if bytecode.len() as u32 != buf_len_bytes {
        return Err(AutohintError::InvalidTable);
    }

    Ok(CvtBlobData {
        bytecode,
        style_offsets,
    })
}

pub(crate) fn build_cvt_table(font: &mut Font, coords: &[F2Dot14]) -> Result<(), AutohintError> {
    // Clone sample_glyphs to release the borrow before mutable access
    let sample_glyphs = font.sample_glyphs.clone();
    let fallback_style = crate::orchestrate::fallback_style_for_script(font.args.fallback_script);

    let mut style_metrics = vec![];

    for style_idx in 0..STYLE_SLOTS {
        let style_key = StyleIndex::new(style_idx)?;
        let glyph_id = sample_glyphs
            .get(&style_key)
            .copied()
            .unwrap_or_else(|| GlyphId::new(0));
        match compute_style_metrics(font, style_idx, glyph_id, coords) {
            Ok(metrics) => {
                if metrics.blue_refs.is_empty() {
                    font.sample_glyphs.shift_remove(&style_key);
                    replace_style_with_fallback(font, style_idx, fallback_style as u16);
                }
                style_metrics.push(metrics);
            }
            Err(AutohintError::MissingStyleSampleGlyph) => {
                style_metrics.push(StyleMetrics::default());
                continue;
            }
            Err(error) => return Err(error),
        }
    }

    let blob_data = build_cvt_blob(
        &style_metrics,
        font.args.windows_compatibility,
        font.head.units_per_em,
    )?;

    if blob_data.style_offsets.is_empty() && !font.args.symbol {
        return Err(AutohintError::NoUsableStyleMetrics);
    }
    let glyf_data = font.glyf_data.as_mut().ok_or(AutohintError::InvalidTable)?;

    glyf_data.style_offsets = blob_data.style_offsets.clone();

    font.cvt = blob_data.bytecode.as_slice().to_vec();
    Ok(())
}

pub(crate) fn build_cvar_table(font: &mut Font) -> Result<(), AutohintError> {
    // Find all interesting locations.
    let locations: HashSet<Vec<F2Dot14>> = font
        .sample_glyphs
        .values()
        .flat_map(|g| glyph_variations(font, *g))
        .flatten()
        .collect();
    // cvt values are 16 bit FWORD types, so we need to
    // a) decompose the bytecode into 16-bit words, and
    // b) convert them to i32 for easier delta application
    let default: Vec<i32> = font
        .cvt
        .chunks_exact(2)
        .map(|b| i16::from_be_bytes([b[0], b[1]]) as i32)
        .collect();
    // Build a blob for each location
    let mut deltas: HashMap<Vec<F2Dot14>, Vec<i32>> = HashMap::default();
    for location in locations {
        let mut style_metrics = vec![];

        for style_idx in 0..STYLE_SLOTS {
            let style_key = StyleIndex::new(style_idx)?;
            let glyph_id = font
                .sample_glyphs
                .get(&style_key)
                .copied()
                .unwrap_or_else(|| GlyphId::new(0));
            match compute_style_metrics(font, style_idx, glyph_id, &location) {
                Ok(metrics) => {
                    style_metrics.push(metrics);
                }
                Err(AutohintError::MissingStyleSampleGlyph) => {
                    style_metrics.push(StyleMetrics::default());
                    continue;
                }
                Err(error) => return Err(error),
            }
        }

        let blob_data = build_cvt_blob(
            &style_metrics,
            font.args.windows_compatibility,
            font.head.units_per_em,
        )?;
        let blob_words: Vec<i32> = blob_data
            .bytecode
            .as_slice()
            .chunks_exact(2)
            .map(|b| i16::from_be_bytes([b[0], b[1]]) as i32)
            .collect();
        let delta: Vec<i32> = blob_words
            .iter()
            .zip(default.iter())
            .map(|(b, d)| b.saturating_sub(*d))
            .collect();
        deltas.insert(location, delta);
    }
    // Build a tuple variations store for the blobs
    let cvar = Cvar::new(
        deltas
            .iter()
            .map(|(location, delta)| CvtDeltas::new(peaks(location.clone()), delta.clone()))
            .collect(),
        font.fontref.fvar()?.axis_count(),
    )
    .map_err(|e| AutohintError::InvalidArgument(e.to_string()))?;
    font.cvar = Some(cvar);
    Ok(())
}

pub(crate) fn glyph_variations(
    font: &Font,
    gid: GlyphId,
) -> Result<Vec<Vec<F2Dot14>>, AutohintError> {
    let Some(gvd) = font.fontref.gvar()?.glyph_variation_data(gid)? else {
        return Ok(vec![]);
    };
    Ok(gvd
        .tuples()
        .map(|t| {
            t.peak()
                .values
                .iter()
                .map(|x| x.get())
                .collect::<Vec<F2Dot14>>()
        })
        .collect())
}

fn peaks(peaks: Vec<F2Dot14>) -> Vec<Tent> {
    peaks
        .into_iter()
        .map(|peak| Tent::new(peak, None))
        .collect()
}
