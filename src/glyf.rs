use core::ffi::c_long;
use std::fmt::Write;

extern crate libc;

use crate::{
    bytecode::Bytecode,
    error::AutohintError,
    font::Font,
    opcodes::{CvtLocations, ADD, PUSHB_2, PUSHB_3, RCVT, WCVTP},
};
use skrifa::{
    outline::{compute_hint_plan_exported, ExportedHintPlan, STYLE_CLASSES},
    prelude::*,
    raw::{tables::glyf::CurvePoint, FontData, FontRead, ReadError, TableProvider},
    GlyphId, MetadataProvider, Tag,
};
use write_fonts::{
    dump_table,
    from_obj::ToOwnedTable,
    tables::{
        glyf::{Bbox, Contour, GlyfLocaBuilder, Glyph as WriteGlyph, SimpleGlyph},
        head::Head,
    },
};
pub const TA_STYLE_MAX: usize = 84;
const TA_STYLE_MASK: u16 = 0x3FFF;
const TA_STYLE_UNASSIGNED: u16 = TA_STYLE_MASK;

type TaRsBuildGlyphInstructions =
    Option<fn(&mut Font, usize, GlyphId) -> Result<i32, AutohintError>>;

fn fallback_style(font: &Font) -> u16 {
    crate::orchestrate::fallback_style_for_script(crate::orchestrate::script_to_index(
        &font.args.fallback_script,
    )) as u16
}

#[derive(Copy, Clone, Default)]
pub struct TaRsOutlinePoint {
    pub x: i32,
    pub y: i32,
}

struct TaRsBuiltGlyphs {
    glyphs: Vec<ScaledGlyph>,
    num_glyphs: u16,
    max_composite_points: u16,
    max_composite_contours: u16,
}

pub(crate) struct GlyfData {
    pub num_glyphs: u16,
    pub glyphs: Vec<ScaledGlyph>,

    // Merged style coverage snapshot used by TA_sfnt_handle_coverage/adjust_coverage.
    pub master_glyph_styles: Vec<u16>,
    /* for coverage bookkeeping */
    pub adjusted: u8,

    /* styles present in a font get a running number; */
    /* unavailable styles get value 0xFFFF */
    pub style_ids: [u32; TA_STYLE_MAX],
    pub num_used_styles: u32,

    /* we have separate CVT data for each style */
    pub cvt_offsets: [u32; TA_STYLE_MAX],
    pub cvt_horz_width_sizes: [u32; TA_STYLE_MAX],
    pub cvt_vert_width_sizes: [u32; TA_STYLE_MAX],
    pub cvt_blue_zone_sizes: [u32; TA_STYLE_MAX],
    pub cvt_blue_adjustment_offsets: [u32; TA_STYLE_MAX],
}

fn merge_style_coverage(master: &mut [u16], current: &[u16]) {
    for (master_bits, current_bits) in master.iter_mut().zip(current.iter()) {
        if (*current_bits & TA_STYLE_MASK) != TA_STYLE_UNASSIGNED {
            *master_bits = *current_bits;
        }
    }
}

fn fallback_style_name(style_index: usize) -> &'static str {
    crate::globals::ta_style_to_skrifa_style(style_index)
        .and_then(|idx| STYLE_CLASSES.get(idx))
        .map(|style| style.name)
        .unwrap_or("(unknown)")
}

fn log_unassigned_glyphs(
    glyph_styles: &[u16],
    fallback_style: usize,
    sfnt_idx: usize,
    num_sfnts: usize,
) {
    let mut message = String::new();

    if num_sfnts > 1 {
        let _ = writeln!(
            message,
            "\nusing fallback style `{}` for unassigned glyphs (sfnt index {}):",
            fallback_style_name(fallback_style),
            sfnt_idx,
        );
    } else {
        let _ = writeln!(
            message,
            "\nusing fallback style `{}` for unassigned glyphs:",
            fallback_style_name(fallback_style),
        );
    }

    let mut count = 0usize;
    for (idx, style_bits) in glyph_styles.iter().enumerate() {
        if (*style_bits & TA_STYLE_MASK) == TA_STYLE_UNASSIGNED {
            if count.is_multiple_of(10) {
                message.push(' ');
            }

            let _ = write!(message, " {}", idx);
            count += 1;

            if count.is_multiple_of(10) {
                message.push('\n');
            }
        }
    }

    if count == 0 {
        message.push_str("  (none)\n");
    } else if !count.is_multiple_of(10) {
        message.push('\n');
    }

    log::debug!("{message}");
}

fn build_glyf_data_common(font: &mut Font, use_scaler: u8) -> Result<(), AutohintError> {
    let mut data = GlyfData {
        num_glyphs: 0,
        glyphs: Vec::new(),
        master_glyph_styles: Vec::new(),
        adjusted: 0,
        style_ids: [0; TA_STYLE_MAX],
        num_used_styles: 0,
        cvt_offsets: [0; TA_STYLE_MAX],
        cvt_horz_width_sizes: [0; TA_STYLE_MAX],
        cvt_vert_width_sizes: [0; TA_STYLE_MAX],
        cvt_blue_zone_sizes: [0; TA_STYLE_MAX],
        cvt_blue_adjustment_offsets: [0; TA_STYLE_MAX],
    };

    let sfnt_max_components = font.sfnt.max_components;

    let build_result =
        match build_glyphs_rs(font, use_scaler, font.args.composites, sfnt_max_components) {
            Ok(result) => result,
            Err(error) => return Err(AutohintError::UnportedError(error as i32)),
        };

    data.glyphs = build_result.glyphs;
    data.num_glyphs = build_result.num_glyphs;

    if font.args.composites && sfnt_max_components != 0 {
        let sfnt_ref = &mut font.sfnt;
        sfnt_ref.max_components += 1;
        sfnt_ref.max_composite_points = build_result.max_composite_points;
        sfnt_ref.max_composite_contours = build_result.max_composite_contours;
    }

    if font.glyf_ptr_owned.is_none() {
        font.glyf_ptr_owned = Some(data);
    }

    Ok(())
}

impl GlyfData {
    /// scaling value index of style ID id
    pub fn cvt_scaling_value_offset(&self, id: usize) -> u32 {
        CvtLocations::cvtl_max_runtime as u32 + (id as u32)
    }

    /// vwidth offset data of style ID id
    pub fn cvt_vwidth_offset_data(&self, id: usize) -> u32 {
        self.cvt_scaling_value_offset(id) + self.num_used_styles
    }

    /// vwidth size data of style ID id
    pub fn cvt_vwidth_size_data(&self, id: usize) -> u32 {
        self.cvt_vwidth_offset_data(id) + self.num_used_styles
    }
    /// horizontal standard width indices of style i
    pub fn cvt_horz_standard_width_offset(&self, i: usize) -> u32 {
        CvtLocations::cvtl_max_runtime as u32 + 3 * self.num_used_styles + self.cvt_offsets[i]
    }
    /// start of horizontal stem widths array of style i
    pub fn cvt_horz_widths_offset(&self, i: usize) -> u32 {
        self.cvt_horz_standard_width_offset(i) + 1
    }
    /// size of horizontal stem widths array of style i
    pub fn cvt_horz_widths_size(&self, i: usize) -> u32 {
        self.cvt_horz_width_sizes[i]
    }
    /// vertical standard width indices of style i
    pub fn cvt_vert_standard_width_offset(&self, i: usize) -> u32 {
        self.cvt_horz_widths_offset(i) + self.cvt_horz_widths_size(i)
    }
    /// start of vertical stem widths array of style i
    pub fn cvt_vert_widths_offset(&self, i: usize) -> u32 {
        self.cvt_vert_standard_width_offset(i) + 1
    }
    /// size of vertical stem widths array of style i
    pub fn cvt_vert_widths_size(&self, i: usize) -> u32 {
        self.cvt_vert_width_sizes[i]
    }
    /// number of blue zones (including artificial ones) of style i
    pub fn cvt_blues_size(&self, i: usize) -> u32 {
        self.cvt_blue_zone_sizes[i]
    }
    /// start of blue zone arrays for flat edges of style i
    pub fn cvt_blue_refs_offset(&self, i: usize) -> u32 {
        self.cvt_vert_widths_offset(i) + self.cvt_vert_widths_size(i)
    }
    /// start of blue zone arrays for round edges of style i
    pub fn cvt_blue_shoots_offset(&self, i: usize) -> u32 {
        self.cvt_blue_refs_offset(i) + self.cvt_blues_size(i)
    }
    /// x height blue zone (shoot) index of style i (valid if < 0xFFFF)
    pub fn cvt_x_height_blue_offset(&self, i: usize) -> u32 {
        self.cvt_blue_shoots_offset(i) + self.cvt_blue_adjustment_offsets[i]
    }
}

fn f26dot6_to_i16(v: skrifa::raw::types::F26Dot6) -> i16 {
    // Match C's `(x + 32) >> 6` behavior used in TA_create_glyph_data.
    ((v.to_bits() + 32) >> 6) as i16
}

fn f26dot6_to_i32(v: skrifa::raw::types::F26Dot6) -> i32 {
    (v.to_bits() + 32) >> 6
}

type OutlinePayload = (Vec<TaRsOutlinePoint>, Vec<u8>, Vec<u16>);

pub(crate) fn extract_unscaled_outline(
    font: &Font,
    glyph_id: GlyphId,
) -> Result<Option<OutlinePayload>, ReadError> {
    let ttf_bytes = font.build_ttf();
    let font = skrifa::FontRef::new(&ttf_bytes).map_err(|_| ReadError::ValidationError)?;
    let outlines = font.outline_glyphs();
    let upem = font.head()?.units_per_em() as f32;

    let Some(outline) = outlines.get(glyph_id) else {
        return Ok(None);
    };

    let mut extracted = None;
    outline
        .with_scaled_glyf_outline(Size::new(upem), LocationRef::default(), None, |scaled| {
            let mut points = Vec::with_capacity(scaled.points.len());
            let mut tags = Vec::with_capacity(scaled.flags.len());
            let mut contours = Vec::with_capacity(scaled.contours.len());

            for p in scaled.points.iter().copied() {
                points.push(TaRsOutlinePoint {
                    x: f26dot6_to_i32(p.x),
                    y: f26dot6_to_i32(p.y),
                });
            }

            for flag in scaled.flags.iter().copied() {
                tags.push(if flag.is_on_curve() { 1 } else { 0 });
            }

            for contour_end in scaled.contours.iter().copied() {
                contours.push(contour_end);
            }

            extracted = Some((points, tags, contours));
            Ok(())
        })
        .map_err(|_| ReadError::ValidationError)?;

    Ok(extracted)
}

#[derive(Debug, Clone)]
pub(crate) struct ScaledGlyph {
    // Primary Rust-owned data
    pub glyf: write_fonts::tables::glyf::Glyph,
    pub pointsums: Vec<u16>,
    /// Total expanded point count for composite glyphs (set during pointsum computation).
    pub composite_num_points: u16,
    /// Total expanded contour count for composite glyphs.
    pub num_composite_contours: u16,
    // ── Transition fields: C-malloc'd buffers freed by Drop ──────────────────
    /// New TT instructions generated by tabytecode.c (freed by Drop).
    pub ins: Bytecode,
    /// Extra (fpgm-preamble) instructions, appended incrementally (freed by Drop).
    pub ins_extra: Bytecode,
}

impl Default for ScaledGlyph {
    fn default() -> Self {
        Self {
            glyf: WriteGlyph::Empty,
            pointsums: Vec::new(),
            composite_num_points: 0,
            num_composite_contours: 0,
            ins: Bytecode::default(),
            ins_extra: Bytecode::default(),
        }
    }
}

impl ScaledGlyph {
    /// Construct a ScaledGlyph with the given glyf data, all other fields zeroed.
    pub(crate) fn with_glyf(glyf: WriteGlyph) -> Self {
        Self {
            glyf,
            pointsums: Vec::new(),
            composite_num_points: 0,
            num_composite_contours: 0,
            ins: Bytecode::default(),
            ins_extra: Bytecode::default(),
        }
    }

    pub fn set_instructions(&mut self, bytes: &[u8]) -> Result<(), i32> {
        self.ins = Bytecode::new();
        self.ins.extend_bytes(bytes);
        Ok(())
    }

    pub fn append_ignore_std_width(&mut self) {
        const EXTRA: [u8; 4] = [
            PUSHB_2,
            CvtLocations::cvtl_ignore_std_width as u8,
            100,
            WCVTP,
        ];

        self.ins_extra.extend_bytes(&EXTRA);
    }

    pub(crate) fn num_contours(&self) -> i16 {
        match &self.glyf {
            WriteGlyph::Simple(sg) => sg.contours.len() as i16,
            WriteGlyph::Composite(_) => -1_i16,
            WriteGlyph::Empty => 0_i16,
        }
    }

    pub(crate) fn num_points(&self) -> u16 {
        match &self.glyf {
            WriteGlyph::Simple(sg) => sg.contours.iter().map(|c| c.len() as u16).sum(),
            WriteGlyph::Composite(_) => self.composite_num_points,
            WriteGlyph::Empty => 0,
        }
    }

    pub(crate) fn num_components(&self) -> u16 {
        match &self.glyf {
            WriteGlyph::Composite(cg) => cg.components().len() as u16,
            _ => 0,
        }
    }

    pub(crate) fn pointsums_len(&self) -> u16 {
        self.pointsums.len().try_into().unwrap_or(u16::MAX)
    }

    pub(crate) fn pointsum(&self, idx: u16) -> u16 {
        self.pointsums.get(idx as usize).copied().unwrap_or(0)
    }
}

// ── Composite pointsum computation ──────────────────────────────────────────

/// Recursive inner loop mirroring C's `TA_iterate_composite_glyph`.
fn iterate_composite(
    glyphs: &[ScaledGlyph],
    components: &[u16],
    pointsums: &mut Vec<u16>,
    num_composite_contours: &mut u16,
    num_composite_points: &mut u16,
) -> Result<(), AutohintError> {
    if pointsums.len() == 0xFFFF {
        return Err(AutohintError::ValidationError(
            "too many composite nesting levels".into(),
        ));
    }
    pointsums.push(*num_composite_points);

    for &cid in components {
        let glyph = glyphs.get(cid as usize).ok_or_else(|| {
            AutohintError::ValidationError("component glyph index out of range".into())
        })?;

        match &glyph.glyf {
            WriteGlyph::Composite(cg) => {
                let sub: Vec<u16> = cg.components().iter().map(|c| c.glyph.to_u16()).collect();
                iterate_composite(
                    glyphs,
                    &sub,
                    pointsums,
                    num_composite_contours,
                    num_composite_points,
                )?;
            }
            WriteGlyph::Simple(sg) => {
                let n_pts: u16 = sg.contours.iter().map(|c| c.len() as u16).sum();
                let n_ctr: u16 = sg.contours.len() as u16;
                *num_composite_points =
                    num_composite_points.checked_add(n_pts).ok_or_else(|| {
                        AutohintError::ValidationError("composite point count overflow".into())
                    })?;
                *num_composite_contours = num_composite_contours.wrapping_add(n_ctr);
            }
            WriteGlyph::Empty => {}
        }
    }
    Ok(())
}

/// Populate `pointsums`, `composite_num_points`, `num_composite_contours` for
/// every composite glyph.  Mirrors `TA_sfnt_compute_composite_pointsums`.
/// Returns `(max_composite_points, max_composite_contours)` for `hint_composites`.
fn compute_composite_pointsums(
    glyphs: &mut Vec<ScaledGlyph>,
    hint_composites: bool,
) -> Result<(u16, u16), AutohintError> {
    let mut max_cp: u16 = 0;
    let mut max_cc: u16 = 0;

    for i in 0..glyphs.len() {
        let is_composite = matches!(&glyphs[i].glyf, WriteGlyph::Composite(_));
        if !is_composite {
            continue;
        }

        let components: Vec<u16> = if let WriteGlyph::Composite(cg) = &glyphs[i].glyf {
            cg.components().iter().map(|c| c.glyph.to_u16()).collect()
        } else {
            vec![]
        };

        let mut pointsums = Vec::new();
        let mut num_cc: u16 = 0;
        let mut num_cp: u16 = 0;
        iterate_composite(
            glyphs.as_slice(),
            &components,
            &mut pointsums,
            &mut num_cc,
            &mut num_cp,
        )?;

        glyphs[i].pointsums = pointsums;
        glyphs[i].num_composite_contours = num_cc;
        glyphs[i].composite_num_points = num_cp;

        if hint_composites {
            let n_ps = glyphs[i].pointsums.len() as u16;
            max_cp = max_cp.max(num_cp.saturating_add(n_ps));
            max_cc = max_cc.max(num_cc.saturating_add(n_ps));
        }
    }
    Ok((max_cp, max_cc))
}

// ── split_glyphs (TA_sfnt_split_glyf_table Rust half) ───────────────────────

fn split_glyphs(font: &mut Font) -> Result<Vec<ScaledGlyph>, AutohintError> {
    let ttf_bytes = font.build_ttf();
    let font = skrifa::FontRef::new(&ttf_bytes)?;
    let glyf = font.glyf()?;
    let maxp_table = font.maxp()?;
    let mut glyphs = vec![];
    for gid in 0..(maxp_table.num_glyphs() as u32) {
        let Some(raw_glyph) = font.loca(None)?.get_glyf(GlyphId::new(gid), &glyf)? else {
            glyphs.push(ScaledGlyph::default());
            continue;
        };
        let write_glyph: write_fonts::tables::glyf::Glyph = raw_glyph.to_owned_table();
        glyphs.push(ScaledGlyph::with_glyf(write_glyph));
    }
    Ok(glyphs)
}

// ── run_font_through_scaler (TA_sfnt_create_glyf_data Rust half) ─────────────

fn run_font_through_scaler(font: &mut Font) -> Result<Vec<ScaledGlyph>, AutohintError> {
    let ttf_bytes = font.build_ttf();
    let font = skrifa::FontRef::new(&ttf_bytes)?;
    let head_table = font.head()?;
    let maxp_table = font.maxp()?;
    let outlines = font.outline_glyphs();
    let upem = head_table.units_per_em() as f32;

    let mut scaled_glyphs = Vec::new();
    for gid in 0..(maxp_table.num_glyphs() as u32) {
        let Some(outline) = outlines.get(GlyphId::new(gid)) else {
            scaled_glyphs.push(ScaledGlyph::default());
            continue;
        };
        outline
            .with_scaled_glyf_outline(Size::new(upem), LocationRef::default(), None, |scaled| {
                let mut contours = Vec::new();
                let mut start = 0usize;

                for end in scaled.contours.iter().copied() {
                    let end = end as usize;
                    if end >= start && end < scaled.points.len() {
                        let mut contour = Vec::with_capacity(end - start + 1);
                        for i in start..=end {
                            let p = scaled.points[i];
                            let flags = scaled.flags[i];
                            contour.push(CurvePoint::new(
                                f26dot6_to_i16(p.x),
                                f26dot6_to_i16(p.y),
                                flags.is_on_curve(),
                            ));
                        }
                        contours.push(Contour::from(contour));
                    }
                    start = end + 1;
                }
                let glyph = if contours.is_empty() {
                    WriteGlyph::Empty
                } else {
                    let mut sg = SimpleGlyph {
                        contours,
                        bbox: Bbox::default(),
                        instructions: vec![],
                    };
                    sg.recompute_bounding_box();
                    WriteGlyph::Simple(sg)
                };
                scaled_glyphs.push(ScaledGlyph::with_glyf(glyph));
                Ok(())
            })
            .map_err(|_| {
                AutohintError::ValidationError("skrifa scaled-outline extraction failed".into())
            })?;
    }
    Ok(scaled_glyphs)
}

fn update_glyf_loca_tables(font: &mut Font) -> Result<(), AutohintError> {
    let glyphs = &font
        .glyf_ptr_owned
        .as_ref()
        .ok_or(AutohintError::InvalidTable)?
        .glyphs;
    fn staged_instruction_bytes(glyph: &ScaledGlyph) -> Result<Option<Vec<u8>>, ReadError> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(glyph.ins_extra.as_slice());
        bytes.extend_from_slice(glyph.ins.as_slice());
        if bytes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(bytes))
        }
    }

    fn patch_composite_instructions(raw: &[u8], instructions: &[u8]) -> Result<Vec<u8>, ReadError> {
        const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
        const WE_HAVE_A_SCALE: u16 = 0x0008;
        const MORE_COMPONENTS: u16 = 0x0020;
        const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
        const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;
        const WE_HAVE_INSTRUCTIONS: u16 = 0x0100;

        if raw.len() < 10 {
            return Err(ReadError::ValidationError);
        }

        let mut p = 10usize;
        let mut last_flags_pos;
        let mut last_flags;

        loop {
            if p + 4 > raw.len() {
                return Err(ReadError::ValidationError);
            }

            last_flags_pos = Some(p);
            last_flags = u16::from_be_bytes([raw[p], raw[p + 1]]);
            p += 4; // flags + component glyph id

            if (last_flags & ARG_1_AND_2_ARE_WORDS) != 0 {
                p += 4;
            } else {
                p += 2;
            }

            if (last_flags & WE_HAVE_A_SCALE) != 0 {
                p += 2;
            } else if (last_flags & WE_HAVE_AN_X_AND_Y_SCALE) != 0 {
                p += 4;
            } else if (last_flags & WE_HAVE_A_TWO_BY_TWO) != 0 {
                p += 8;
            }

            if p > raw.len() {
                return Err(ReadError::ValidationError);
            }

            if (last_flags & MORE_COMPONENTS) == 0 {
                break;
            }
        }

        let comp_end = p;
        let mut old_instr_end = comp_end;
        if (last_flags & WE_HAVE_INSTRUCTIONS) != 0 {
            if p + 2 > raw.len() {
                return Err(ReadError::ValidationError);
            }
            let old_len = u16::from_be_bytes([raw[p], raw[p + 1]]) as usize;
            let old_start = p + 2;
            old_instr_end = old_start
                .checked_add(old_len)
                .ok_or(ReadError::ValidationError)?;
            if old_instr_end > raw.len() {
                return Err(ReadError::ValidationError);
            }
        }

        let mut out = raw[..comp_end].to_vec();
        let flags_pos = last_flags_pos.ok_or(ReadError::ValidationError)?;
        let mut updated_last_flags = last_flags & !WE_HAVE_INSTRUCTIONS;
        if !instructions.is_empty() {
            updated_last_flags |= WE_HAVE_INSTRUCTIONS;
        }
        let flags_bytes = updated_last_flags.to_be_bytes();
        out[flags_pos] = flags_bytes[0];
        out[flags_pos + 1] = flags_bytes[1];

        if !instructions.is_empty() {
            let ins_len =
                u16::try_from(instructions.len()).map_err(|_| ReadError::ValidationError)?;
            out.extend_from_slice(&ins_len.to_be_bytes());
            out.extend_from_slice(instructions);
        }

        // Composite glyph data is 2-byte aligned.
        if (out.len() & 1) != 0 {
            out.push(0);
        }

        if old_instr_end < raw.len() {
            let trailing = &raw[old_instr_end..];
            if trailing.iter().any(|b| *b != 0) {
                return Err(ReadError::ValidationError);
            }
        }

        Ok(out)
    }

    fn glyph_with_staged_instructions(glyph: &ScaledGlyph) -> Result<WriteGlyph, ReadError> {
        let Some(instructions) = staged_instruction_bytes(glyph)? else {
            return Ok(glyph.glyf.clone());
        };

        match &glyph.glyf {
            WriteGlyph::Simple(simple) => {
                let mut simple = simple.clone();
                simple.instructions = instructions;
                Ok(WriteGlyph::Simple(simple))
            }
            WriteGlyph::Composite(composite) => {
                let raw = dump_table(composite).map_err(|_| ReadError::ValidationError)?;
                let patched = patch_composite_instructions(&raw, &instructions)?;
                let parsed = skrifa::raw::tables::glyf::Glyph::read(FontData::new(&patched))?;
                Ok(parsed.to_owned_table())
            }
            WriteGlyph::Empty => {
                if instructions.is_empty() {
                    Ok(WriteGlyph::Empty)
                } else {
                    Err(ReadError::ValidationError)
                }
            }
        }
    }

    if font.get_processed(Tag::new(b"glyf")) {
        return Ok(());
    }
    let mut builder = GlyfLocaBuilder::new();
    for new_glyph in glyphs.iter() {
        let glyph_for_write = glyph_with_staged_instructions(new_glyph)?;
        builder
            .add_glyph(&glyph_for_write)
            .map_err(|_| ReadError::ValidationError)?;
    }
    let (glyf_data, loca_data, loca_format) = builder.build();
    font.update_table(
        Tag::new(b"glyf"),
        &dump_table(&glyf_data).map_err(|_| ReadError::ValidationError)?,
    );
    font.update_table(
        Tag::new(b"loca"),
        &dump_table(&loca_data).map_err(|_| ReadError::ValidationError)?,
    );
    font.set_processed(Tag::new(b"glyf"), true);
    font.set_processed(Tag::new(b"loca"), true);
    if let Some(head) = font.clone_table(Tag::new(b"head")) {
        let read_head = write_fonts::read::tables::head::Head::read(FontData::new(&head))?;
        let mut write_head: Head = read_head.to_owned_table();
        write_head.index_to_loc_format = loca_format as i16;
        font.update_table(
            Tag::new(b"head"),
            &dump_table(&write_head).map_err(|_| ReadError::ValidationError)?,
        );
    }
    Ok(())
}

const TTFAUTOHINT_GLYPH_BYTECODE: &[u8] = &[
    /* increment `cvtl_is_subglyph' counter */
    PUSHB_3,
    CvtLocations::cvtl_is_subglyph as u8,
    100,
    CvtLocations::cvtl_is_subglyph as u8,
    RCVT,
    ADD,
    WCVTP,
];

fn add_ttfautohint_glyph(glyphs: &mut Vec<ScaledGlyph>) {
    let contour = Contour::from(vec![CurvePoint::new(0, 0, true)]);
    let simple_glyph = SimpleGlyph {
        contours: vec![contour],
        bbox: Bbox::default(),
        instructions: TTFAUTOHINT_GLYPH_BYTECODE.to_vec(),
    };
    let marker_glyph = ScaledGlyph::with_glyf(WriteGlyph::Simple(simple_glyph));
    glyphs.push(marker_glyph);
}

// ── Batch constructor ────────────────────────────────────────────────────────

fn build_glyphs_rs(
    font: &mut Font,
    use_scaler: u8,
    hint_composites: bool,
    max_components: u16,
) -> Result<TaRsBuiltGlyphs, u32> {
    let result = if use_scaler != 0 {
        run_font_through_scaler(font)
    } else {
        split_glyphs(font)
    };

    let mut glyphs = match result {
        Ok(glyphs) => glyphs,
        Err(_) => return Err(0x50),
    };

    let (max_composite_points, max_composite_contours) =
        match compute_composite_pointsums(&mut glyphs, hint_composites) {
            Ok(pair) => pair,
            Err(_) => return Err(0x23),
        };

    if max_components > 0 && hint_composites {
        add_ttfautohint_glyph(&mut glyphs);
    }

    let num_glyphs = glyphs.len();

    Ok(TaRsBuiltGlyphs {
        glyphs,
        num_glyphs: num_glyphs as u16,
        max_composite_points,
        max_composite_contours,
    })
}

pub(crate) fn compute_hint_plan_rs(
    font: &Font,
    glyph_id: GlyphId,
    ta_style: usize,
    is_non_base: u8,
    is_digit: u8,
    ppem: u16,
) -> Result<ExportedHintPlan, AutohintError> {
    let Some(skrifa_style) = crate::globals::ta_style_to_skrifa_style(ta_style) else {
        return Err(AutohintError::UnportedError(0x23));
    };

    let ttf_bytes = font.build_ttf();
    let font = skrifa::FontRef::new(&ttf_bytes)?;

    let Some(plan) = compute_hint_plan_exported(
        &font,
        &[],
        glyph_id.to_u32(),
        skrifa_style,
        is_non_base != 0,
        is_digit != 0,
        ppem as f32,
    ) else {
        return Err(AutohintError::UnportedError(0x23));
    };

    Ok(plan)
}

// ── Serialization: glyf/loca table building ──────────────────────────────────

pub(crate) fn split_glyf_table(font: &mut Font) -> Result<(), AutohintError> {
    build_glyf_data_common(font, 0)
}

pub(crate) fn create_glyf_data(font: &mut Font) -> Result<(), AutohintError> {
    build_glyf_data_common(font, 1)
}

pub(crate) fn handle_coverage(font: &mut Font) -> Result<(), AutohintError> {
    let glyph_count = font.sfnt.glyph_count;

    let (glyph_styles, sample_glyphs_local) = crate::globals::compute_style_coverage(
        font,
        glyph_count as usize,
        TA_STYLE_UNASSIGNED,
        font.args.debug,
        0,
        1,
    )?;
    let sample_glyphs = sample_glyphs_local;

    {
        let sfnt = &mut font.sfnt;
        sfnt.glyph_styles = glyph_styles;
        sfnt.sample_glyphs = *sample_glyphs.as_array().unwrap();
    }

    let data = font
        .glyf_ptr_owned
        .as_mut()
        .ok_or(AutohintError::InvalidTable)?;

    let current_styles = &font.sfnt.glyph_styles;

    if data.master_glyph_styles.is_empty() {
        data.master_glyph_styles = current_styles.clone();
        return Ok(());
    }

    if current_styles.is_empty() {
        return Ok(());
    }

    merge_style_coverage(&mut data.master_glyph_styles, current_styles);

    Ok(())
}

pub(crate) fn build_glyf_table(
    font: &mut Font,
    sfnt_idx: usize,
    build_glyph_instructions: TaRsBuildGlyphInstructions,
) -> Result<(), AutohintError> {
    if font.glyf_ptr_owned.is_none() {
        return Err(AutohintError::InvalidTable);
    }

    let data_num_glyphs = font
        .glyf_ptr_owned
        .as_ref()
        .map(|d| d.num_glyphs)
        .ok_or(AutohintError::InvalidTable)?;
    let sfnt_max_components = font.sfnt.max_components;

    if font.get_processed(Tag::new(b"glyf")) {
        return Ok(());
    }

    if !font.args.dehint {
        let Some(build_glyph_instructions) = build_glyph_instructions else {
            return Err(AutohintError::InvalidTable);
        };

        let mut loop_count = data_num_glyphs as u32;
        if sfnt_max_components != 0 && font.args.composites {
            loop_count = loop_count.saturating_sub(1);
        }

        for idx in 0..loop_count {
            build_glyph_instructions(font, sfnt_idx, GlyphId::new(idx))?;

            if let Some(progress) = font.progress {
                let ret = progress(
                    GlyphId::new(idx),
                    GlyphId::new(loop_count),
                    sfnt_idx as c_long,
                    1,
                );
                if ret != 0 {
                    return Err(AutohintError::ProgressCancelled);
                }
            }
        }
    }

    update_glyf_loca_tables(font)?;
    Ok(())
}

pub(crate) fn adjust_coverage(font: &mut Font) {
    if font.glyf_ptr_owned.is_none() {
        return;
    }

    let fallback_style = fallback_style(font);

    let Some(data_ref) = font.glyf_ptr_owned.as_mut() else {
        return;
    };
    if data_ref.adjusted != 0 || data_ref.master_glyph_styles.is_empty() {
        return;
    }

    let glyph_styles = &data_ref.master_glyph_styles;

    if font.args.debug {
        log_unassigned_glyphs(glyph_styles, fallback_style as usize, 0, 1);
    }

    for style_bits in data_ref.master_glyph_styles.iter_mut() {
        if (*style_bits & TA_STYLE_MASK) == TA_STYLE_UNASSIGNED {
            *style_bits &= !TA_STYLE_MASK;
            *style_bits |= fallback_style;
        }
    }

    let sfnt_ref = &mut font.sfnt;
    sfnt_ref
        .glyph_styles
        .clone_from(&data_ref.master_glyph_styles);

    data_ref.adjusted = 1;
}
