use crate::{
    bytecode::Bytecode,
    font::{Font, Sfnt},
    style::STYLE_COUNT,
    AutohintError,
};
use skrifa::{
    outline::{compute_unscaled_style_metrics_exported, STYLE_CLASSES},
    FontRef, GlyphId, Tag,
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

pub(crate) fn build_cvt_table_store(font: &mut Font) -> Result<(), AutohintError> {
    let blob_data = build_cvt_table(font)?;

    if font.get_processed(Tag::new(b"glyf")) {
        return Ok(());
    }

    font.update_table(Tag::new(b"cvt "), blob_data.bytecode.as_slice());

    Ok(())
}

fn compute_style_metrics(
    font: &mut Font,
    style_index: usize,
    sample_glyph: GlyphId,
) -> Result<StyleMetrics, AutohintError> {
    if sample_glyph.to_u32() == 0 {
        return Err(AutohintError::MissingStyleSampleGlyph);
    }

    let ttf_bytes = font.build_ttf();
    let Ok(font) = FontRef::new(&ttf_bytes) else {
        return Err(AutohintError::InvalidTable);
    };

    let Some(style_class) = STYLE_CLASSES.get(style_index) else {
        return Err(AutohintError::InvalidTable);
    };

    let metrics = compute_unscaled_style_metrics_exported(&font, &[], style_class);

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
    pub num_used_styles: u32,
    pub style_ids: [u32; STYLE_SLOTS],
    pub cvt_offsets: [u32; STYLE_SLOTS],
    pub cvt_horz_width_sizes: [u32; STYLE_SLOTS],
    pub cvt_vert_width_sizes: [u32; STYLE_SLOTS],
    pub cvt_blue_zone_sizes: [u32; STYLE_SLOTS],
    pub cvt_blue_adjustment_offsets: [u32; STYLE_SLOTS],
}

fn checked_i32_to_u16(v: i32) -> Result<u16, AutohintError> {
    if v <= 0xFFFF {
        Ok((v as i64 & 0xFFFF) as u16)
    } else {
        Err(AutohintError::NumericOverflow)
    }
}

fn replace_style_with_fallback(sfnt: &mut Sfnt, style_idx: usize, fallback_style: u16) {
    if sfnt.glyph_styles.is_empty() || sfnt.glyph_count <= 0 {
        return;
    }

    for glyph_style in sfnt.glyph_styles.iter_mut() {
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

    let mut out = CvtBlobData {
        bytecode: Bytecode::new(),
        num_used_styles: 0,
        style_ids: [0xFFFFu32; STYLE_SLOTS],
        cvt_offsets: [0; STYLE_SLOTS],
        cvt_horz_width_sizes: [0; STYLE_SLOTS],
        cvt_vert_width_sizes: [0; STYLE_SLOTS],
        cvt_blue_zone_sizes: [0; STYLE_SLOTS],
        cvt_blue_adjustment_offsets: [0xFFFFu32; STYLE_SLOTS],
    };

    let mut hwidth_count = 0u32;
    let mut vwidth_count = 0u32;
    let mut blue_count = 0u32;
    let default_width = (50 * units_per_em as u32) / 2048;

    for (i, metrics) in metrics_arr.iter().enumerate() {
        if metrics.blue_refs.is_empty() {
            out.style_ids[i] = 0xFFFFu32;
            continue;
        }

        out.style_ids[i] = out.num_used_styles;
        out.num_used_styles += 1;

        hwidth_count += metrics.hwidths.len() as u32;
        vwidth_count += metrics.vwidths.len() as u32;
        blue_count += metrics.blue_refs.len() as u32;

        if windows_compatibility {
            blue_count += 2;
        }
    }

    let buf_len = CVTL_MAX_RUNTIME
        + out.num_used_styles
        + 2 * out.num_used_styles
        + 2 * out.num_used_styles
        + hwidth_count
        + vwidth_count
        + 2 * blue_count;
    let buf_len_bytes = buf_len * 2;
    let mut bytecode = Bytecode::new();

    let runtime_header_bytes =
        ((CVTL_MAX_RUNTIME + out.num_used_styles + 2 * out.num_used_styles) * 2) as usize;
    bytecode.extend(std::iter::repeat_n(0u8, runtime_header_bytes));

    let cvt_offset = bytecode.len() as u32;

    for (i, metrics) in metrics_arr.iter().enumerate() {
        out.cvt_offsets[i] = ((bytecode.len() as u32) - cvt_offset) >> 1;

        if out.style_ids[i] == 0xFFFFu32 {
            continue;
        }

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

        out.cvt_blue_adjustment_offsets[i] = 0xFFFFu32;

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
                out.cvt_blue_adjustment_offsets[i] = j as u32;
            }
        }

        if windows_compatibility {
            bytecode.push_word(0);
            bytecode.push_word(0);
        }

        out.cvt_horz_width_sizes[i] = metrics.hwidths.len() as u32;
        out.cvt_vert_width_sizes[i] = metrics.vwidths.len() as u32;
        out.cvt_blue_zone_sizes[i] = total_blue_count as u32;
    }

    if bytecode.len() as u32 != buf_len_bytes {
        return Err(AutohintError::InvalidTable);
    }

    out.bytecode = bytecode;
    Ok(out)
}

fn build_cvt_table(font: &mut Font) -> Result<CvtBlobData, AutohintError> {
    // Clone sample_glyphs to release the borrow before mutable access
    let sample_glyphs = font.sfnt.sample_glyphs.clone();
    let fallback_style = crate::orchestrate::fallback_style_for_script(
        crate::orchestrate::script_to_index(&font.args.fallback_script),
    );

    let mut style_metrics = vec![];

    for style_idx in 0..STYLE_SLOTS {
        let glyph_id = sample_glyphs
            .get(&style_idx)
            .copied()
            .unwrap_or_else(|| GlyphId::new(0));
        match compute_style_metrics(font, style_idx, glyph_id) {
            Ok(metrics) => {
                if metrics.blue_refs.is_empty() {
                    let sfnt_mut = &mut font.sfnt;
                    sfnt_mut.sample_glyphs.shift_remove(&style_idx);
                    replace_style_with_fallback(sfnt_mut, style_idx, fallback_style as u16);
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

    let units_per_em = crate::maxp::units_per_em_in_font_binary(&font.in_buf)?;

    let blob_data = build_cvt_blob(
        &style_metrics,
        font.args.windows_compatibility,
        units_per_em as u16,
    )?;

    if blob_data.num_used_styles == 0 && !font.args.symbol {
        return Err(AutohintError::NoUsableStyleMetrics);
    }
    let glyf_data = font
        .glyf_ptr_owned
        .as_mut()
        .ok_or(AutohintError::InvalidTable)?;

    glyf_data.num_used_styles = blob_data.num_used_styles;
    glyf_data.style_ids = blob_data.style_ids;
    glyf_data.cvt_offsets = blob_data.cvt_offsets;
    glyf_data.cvt_horz_width_sizes = blob_data.cvt_horz_width_sizes;
    glyf_data.cvt_vert_width_sizes = blob_data.cvt_vert_width_sizes;
    glyf_data.cvt_blue_zone_sizes = blob_data.cvt_blue_zone_sizes;
    glyf_data.cvt_blue_adjustment_offsets = blob_data.cvt_blue_adjustment_offsets;

    Ok(blob_data)
}
