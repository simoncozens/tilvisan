use crate::{
    bytecode::Bytecode,
    c_font::{Font, Sfnt},
    tablestore::TableStore,
    AutohintError,
};
use skrifa::Tag;
use skrifa::{
    outline::{compute_unscaled_style_metrics_exported, STYLE_CLASSES},
    FontRef,
};

const FT_ERR_INVALID_TABLE: u32 = 0x23;
const TA_ERR_HINTER_OVERFLOW: u32 = 0xF0;
const TA_ERR_MISSING_GLYPH: u32 = 0xF1;
const TA_STYLE_MASK: u16 = 0x3FFF;

// From tabytecode.h: CVT runtime section size
const CVTL_MAX_RUNTIME: u32 = 7;
const TA_STYLE_MAX: usize = 84;

#[derive(Default)]
pub struct TaRsStyleMetrics {
    pub hwidths: Vec<u16>,
    pub vwidths: Vec<u16>,
    pub blue_refs: Vec<u16>,
    pub blue_shoots: Vec<u16>,
    pub blue_adjustment: Vec<u8>,
}

pub(crate) fn build_cvt_table_store(
    font: &mut Font,
    sfnt_idx: usize,
) -> Result<(), AutohintError> {
    let blob_data = build_cvt_table_rs(font, sfnt_idx)?;

    // Bounds check and get sfnt table_store_sfnt_idx
    if sfnt_idx >= font.num_sfnts() || sfnt_idx >= font.sfnts_owned.len() {
        return Err(AutohintError::InvalidTable);
    }
    let sfnt_table_store_idx = font.sfnts_owned[sfnt_idx].table_store_sfnt_idx;

    let table_store = &mut font.table_store;

    if table_store.get_processed(sfnt_table_store_idx, Tag::new(b"glyf")) {
        return Ok(());
    }

    table_store.update_table(
        sfnt_table_store_idx,
        Tag::new(b"cvt "),
        blob_data.bytecode.as_slice(),
    );

    Ok(())
}

fn compute_style_metrics_rs(
    table_store: &TableStore,
    sfnt_idx: usize,
    ta_style: usize,
    sample_glyph: u32,
) -> Result<TaRsStyleMetrics, u32> {
    let Some(skrifa_style) = crate::globals::ta_style_to_skrifa_style(ta_style) else {
        return Err(TA_ERR_MISSING_GLYPH);
    };

    if sample_glyph == 0 {
        return Err(TA_ERR_MISSING_GLYPH);
    }

    let ttf_bytes = table_store.build_ttf(sfnt_idx as u32);
    let Ok(font) = FontRef::new(&ttf_bytes) else {
        return Err(FT_ERR_INVALID_TABLE);
    };

    let Some(style_class) = STYLE_CLASSES.get(skrifa_style) else {
        return Err(FT_ERR_INVALID_TABLE);
    };

    let metrics = compute_unscaled_style_metrics_exported(&font, &[], style_class);

    let mut hwidths = Vec::with_capacity(metrics.horizontal_widths.len());
    for &w in &metrics.horizontal_widths {
        let Ok(w) = checked_i32_to_u16(w) else {
            return Err(TA_ERR_HINTER_OVERFLOW);
        };
        hwidths.push(w);
    }

    let mut vwidths = Vec::with_capacity(metrics.vertical_widths.len());
    for &w in &metrics.vertical_widths {
        let Ok(w) = checked_i32_to_u16(w) else {
            return Err(TA_ERR_HINTER_OVERFLOW);
        };
        vwidths.push(w);
    }

    let mut blue_refs = Vec::with_capacity(metrics.blues.len());
    let mut blue_shoots = Vec::with_capacity(metrics.blues.len());
    let mut blue_adjustment = Vec::with_capacity(metrics.blues.len());

    for blue in &metrics.blues {
        let Ok(reference) = checked_i32_to_u16(blue.reference) else {
            return Err(TA_ERR_HINTER_OVERFLOW);
        };
        let Ok(shoot) = checked_i32_to_u16(blue.shoot) else {
            return Err(TA_ERR_HINTER_OVERFLOW);
        };
        blue_refs.push(reference);
        blue_shoots.push(shoot);
        blue_adjustment.push(if blue.is_adjustment { 1 } else { 0 });
    }

    Ok(TaRsStyleMetrics {
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
    pub style_ids: [u32; TA_STYLE_MAX],
    pub cvt_offsets: [u32; TA_STYLE_MAX],
    pub cvt_horz_width_sizes: [u32; TA_STYLE_MAX],
    pub cvt_vert_width_sizes: [u32; TA_STYLE_MAX],
    pub cvt_blue_zone_sizes: [u32; TA_STYLE_MAX],
    pub cvt_blue_adjustment_offsets: [u32; TA_STYLE_MAX],
}

fn checked_i32_to_u16(v: i32) -> Result<u16, u32> {
    if v <= 0xFFFF {
        Ok((v as i64 & 0xFFFF) as u16)
    } else {
        Err(TA_ERR_HINTER_OVERFLOW)
    }
}

fn replace_style_with_fallback(sfnt: &mut Sfnt, style_idx: usize, fallback_style: u16) {
    if sfnt.glyph_styles.is_empty() || sfnt.glyph_count <= 0 {
        return;
    }

    for glyph_style in sfnt.glyph_styles.iter_mut() {
        if (*glyph_style & TA_STYLE_MASK) == style_idx as u16 {
            *glyph_style &= !TA_STYLE_MASK;
            *glyph_style |= fallback_style;
        }
    }
}

fn build_cvt_blob_rs(
    metrics_arr: &[TaRsStyleMetrics],
    windows_compatibility: bool,
    units_per_em: u16,
) -> Result<CvtBlobData, u32> {
    if metrics_arr.len() != TA_STYLE_MAX {
        return Err(FT_ERR_INVALID_TABLE);
    }

    let mut out = CvtBlobData {
        bytecode: Bytecode::new(),
        num_used_styles: 0,
        style_ids: [0xFFFFu32; TA_STYLE_MAX],
        cvt_offsets: [0; TA_STYLE_MAX],
        cvt_horz_width_sizes: [0; TA_STYLE_MAX],
        cvt_vert_width_sizes: [0; TA_STYLE_MAX],
        cvt_blue_zone_sizes: [0; TA_STYLE_MAX],
        cvt_blue_adjustment_offsets: [0xFFFFu32; TA_STYLE_MAX],
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
        return Err(FT_ERR_INVALID_TABLE);
    }

    out.bytecode = bytecode;
    Ok(out)
}

fn build_cvt_table_rs(font: &mut Font, sfnt_idx: usize) -> Result<CvtBlobData, AutohintError> {
    // Bounds check and get sfnt/glyf_data pointers
    if sfnt_idx >= font.num_sfnts() {
        return Err(AutohintError::InvalidTable);
    }
    if sfnt_idx >= font.glyf_ptrs_owned.len() {
        return Err(AutohintError::InvalidTable);
    }

    let glyf_data = font
        .glyf_ptrs_owned
        .get_mut(sfnt_idx)
        .and_then(Option::as_mut)
        .ok_or(AutohintError::InvalidTable)?;

    let sfnt_table_store_idx = font.sfnts_owned[sfnt_idx].table_store_sfnt_idx;
    let sample_glyphs = font.sfnts_owned[sfnt_idx].sample_glyphs;
    let fallback_style = font.fallback_style;

    let mut style_metrics: [TaRsStyleMetrics; TA_STYLE_MAX] =
        core::array::from_fn(|_| TaRsStyleMetrics::default());

    for style_idx in 0..TA_STYLE_MAX {
        match compute_style_metrics_rs(
            &font.table_store,
            sfnt_table_store_idx,
            style_idx,
            sample_glyphs[style_idx],
        ) {
            Ok(metrics) => {
                style_metrics[style_idx] = metrics;
            }
            Err(TA_ERR_MISSING_GLYPH) => continue,
            Err(error) => {
                return Err(AutohintError::UnportedError(error as i32));
            }
        }

        if style_metrics[style_idx].blue_refs.is_empty() {
            let sfnt_mut = &mut font.sfnts_owned[sfnt_idx];
            sfnt_mut.sample_glyphs[style_idx] = 0;
            replace_style_with_fallback(sfnt_mut, style_idx, fallback_style as u16);
        }
    }

    let units_per_em =
        crate::maxp::units_per_em_in_font_binary_at_index(&font.in_buf, sfnt_idx as u32)?;

    let blob_data = match build_cvt_blob_rs(
        &style_metrics,
        font.windows_compatibility,
        units_per_em as u16,
    ) {
        Ok(blob) => blob,
        Err(error) => {
            return Err(AutohintError::UnportedError(error as i32));
        }
    };

    if blob_data.num_used_styles == 0 && !font.symbol {
        return Err(AutohintError::UnportedError(TA_ERR_MISSING_GLYPH as i32));
    }

    glyf_data.num_used_styles = blob_data.num_used_styles;
    glyf_data.style_ids = blob_data.style_ids;
    glyf_data.cvt_offsets = blob_data.cvt_offsets;
    glyf_data.cvt_horz_width_sizes = blob_data.cvt_horz_width_sizes;
    glyf_data.cvt_vert_width_sizes = blob_data.cvt_vert_width_sizes;
    glyf_data.cvt_blue_zone_sizes = blob_data.cvt_blue_zone_sizes;
    glyf_data.cvt_blue_adjustment_offsets = blob_data.cvt_blue_adjustment_offsets;

    Ok(blob_data)
}
