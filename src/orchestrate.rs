use skrifa::{GlyphId, Tag};

use crate::{
    control::{
        ControlEntryAst, GlyphRef, GlyphSetElem, NumberSetAst, NumberSetElem, PointMode,
        SegmentDirection,
    },
    control_index::ResolvedControlEntry,
    font::Font as TaFont,
    info::InfoData,
    intset::{IntSet, RangeExpr},
    maxp::sfnt_has_ttfautohint_glyph,
    prep::build_prep_table,
    recorder::build_glyph_instructions,
    Args, AutohintError,
};
use std::{
    ffi::{c_int, c_long},
    io::{self, Read},
};

const HINTING_RANGE_MIN_MIN: u32 = 2;
const INCREASE_X_HEIGHT_MIN: u32 = 6;

fn ta_sfnt_build_glyph_instructions_cb(
    font: &mut TaFont,
    sfnt_idx: usize,
    idx: GlyphId,
) -> Result<c_int, AutohintError> {
    build_glyph_instructions(font, sfnt_idx, idx)?;
    Ok(0)
}

pub fn ttf_autohint_font(font: &mut TaFont) -> Result<Vec<u8>, AutohintError> {
    let dehint = font.args.dehint;
    let adjust_subglyphs = font.args.adjust_subglyphs || font.args.pre_hinting;
    let hint_composites = font.args.composites;
    let fallback_style = fallback_style_for_script(font.args.fallback_script);

    if sfnt_has_ttfautohint_glyph(font)? {
        return Err(AutohintError::FontAlreadyProcessed);
    }

    let glyph_count = crate::maxp::num_glyphs_in_font_binary(&font.in_buf)?;

    font.sfnt.glyph_count = glyph_count as c_long;
    font.sfnt.glyph_styles = vec![crate::style::GlyphStyle::unassigned(); glyph_count as usize];

    crate::control_index::control_build_tree(font)?;

    let has_legal_permission = crate::maxp::sfnt_has_legal_permission(font)?;
    if !has_legal_permission && !font.args.ignore_restrictions {
        return Err(AutohintError::MissingLegalPermission);
    }

    if dehint {
        crate::glyf::split_glyf_table(font)?;
    } else {
        if adjust_subglyphs {
            crate::glyf::create_glyf_data(font)?;
        } else {
            crate::glyf::split_glyf_table(font)?;
        }

        crate::glyf::handle_coverage(font)?;

        font.sfnt.increase_x_height = font.args.increase_x_height;
    }

    if !dehint {
        crate::glyf::adjust_coverage(font);
        crate::control_index::control_apply_coverage(font);
    }

    crate::gasp::update_gasp(font);

    if !dehint {
        crate::cvt::build_cvt_table_store(font)?;

        let glyf_data = font
            .glyf_ptr_owned
            .take()
            .ok_or(AutohintError::NullPointer)?;

        let fpgm_len = crate::fpgm::build_fpgm_table(
            font,
            &glyf_data,
            font.args.increase_x_height,
            font.control.has_index(),
            fallback_style as usize,
        )?;
        let sfnt_ref = &mut font.sfnt;
        if fpgm_len > sfnt_ref.max_instructions as usize {
            sfnt_ref.max_instructions = fpgm_len as u16;
        }

        let prep_stack = build_prep_table(font, &glyf_data)? as u16;
        let sfnt_ref = &mut font.sfnt;
        if prep_stack > sfnt_ref.max_stack_elements {
            sfnt_ref.max_stack_elements = prep_stack;
        }

        font.glyf_ptr_owned = Some(glyf_data);
    }

    crate::glyf::build_glyf_table(font, 0, Some(ta_sfnt_build_glyph_instructions_cb))?;

    let sfnt = &font.sfnt;
    let sfnt_max_components = sfnt.max_components;
    let sfnt_max_composite_points = sfnt.max_composite_points;
    let sfnt_max_composite_contours = sfnt.max_composite_contours;
    let sfnt_max_twilight_points = sfnt.max_twilight_points;
    let sfnt_max_storage = sfnt.max_storage;
    let sfnt_max_stack_elements = sfnt.max_stack_elements;
    let sfnt_max_instructions = sfnt.max_instructions;

    if dehint {
        crate::maxp::update_maxp_table_dehint(font)?
    } else {
        let data = font
            .glyf_ptr_owned
            .as_ref()
            .ok_or(AutohintError::NullPointer)?;
        let adjust_composites = sfnt_max_components != 0 && hint_composites;
        crate::maxp::update_maxp_table_hinted(
            font,
            adjust_composites,
            data.num_glyphs,
            sfnt_max_composite_points,
            sfnt_max_composite_contours,
            sfnt_max_twilight_points,
            sfnt_max_storage,
            sfnt_max_stack_elements,
            sfnt_max_instructions,
            sfnt_max_components,
        )?;
    }

    if !dehint && sfnt_max_components != 0 && !adjust_subglyphs && hint_composites {
        font.update_hmtx();
        font.update_post();

        crate::gpos::update_gpos(font)?;
    }

    if !font.has_table(Tag::new(b"TTFA")) {
        crate::name::update_name_table(font)?;
    }

    Ok(font.build_ttf_complete(font.have_dsig))
}

pub struct TtfautohintCall {
    pub in_buf: Vec<u8>,
    pub reference_buf: Option<Vec<u8>>,
    pub control_buf: Option<String>,
    pub ignore_restrictions: bool,
    pub debug: bool,
    pub epoch: u64,
}

impl TtfautohintCall {
    pub fn from_args(args: &Args) -> Result<Self, AutohintError> {
        let in_buf: Vec<u8> = if args.input == "-" {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf)?;
            buf
        } else {
            std::fs::read(&args.input)?
        };

        let control_buf: Option<String> = args
            .control_file
            .as_ref()
            .map(std::fs::read_to_string)
            .transpose()?;

        let reference_buf: Option<Vec<u8>> =
            args.reference.as_ref().map(std::fs::read).transpose()?;

        Ok(Self {
            in_buf,
            reference_buf,
            control_buf,
            ignore_restrictions: args.ignore_restrictions,
            debug: args.debug,
            epoch: args.epoch.unwrap_or(u64::MAX),
        })
    }
}

pub fn ttfautohint(
    call: &TtfautohintCall,
    args: &Args,
    idata: &mut InfoData,
) -> Result<Vec<u8>, AutohintError> {
    validate_options(args)?;

    let control_entries: Vec<ResolvedControlEntry> = if let Some(control_text) = &call.control_buf {
        match crate::control::SkrifaProvider::new(call.in_buf.to_vec()) {
            Ok(provider) => parse_control_entries(control_text, &provider)?,
            Err(_) => {
                let provider = crate::control::MinimalProvider::new(1);
                parse_control_entries(control_text, &provider)?
            }
        }
    } else {
        Vec::new()
    };

    let reference_slice = call.reference_buf.as_deref();

    if call.in_buf.len() < 100 {
        return Err(AutohintError::ValidationError("Font too small".to_string()));
    }

    if let Some(reference_buf) = reference_slice {
        if reference_buf.len() < 100 {
            return Err(AutohintError::ValidationError(
                "Reference font too small".to_string(),
            ));
        }
    }

    let mut font = TaFont::new(call.in_buf.clone())?;
    font.args = args.clone();

    font.progress = None;
    font.info_data = idata.clone();
    font.control.set_entries(control_entries);

    if let Some(reference_buf) = reference_slice {
        font.reference_buf = Some(reference_buf.to_vec());
    }

    ttf_autohint_font(&mut font)
}

fn validate_options(args: &Args) -> io::Result<()> {
    if args.hinting_range_min < HINTING_RANGE_MIN_MIN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "hinting-range-min must be at least 2",
        ));
    }

    if args.hinting_range_max < args.hinting_range_min {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "hinting-range-max must be >= hinting-range-min",
        ));
    }

    if args.hinting_limit > 0 && args.hinting_limit < args.hinting_range_max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "hinting-limit must be 0 or >= hinting-range-max",
        ));
    }

    if args.increase_x_height > 0 && args.increase_x_height < INCREASE_X_HEIGHT_MIN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "increase-x-height must be 0 or >= 6",
        ));
    }

    Ok(())
}

pub(crate) fn parse_number_set_to_intset(input: &str, min: i32, max: i32) -> Option<IntSet> {
    let ast = match NumberSetAst::parse(input) {
        Ok(ast) => ast,
        Err(_) => return None,
    };

    let exprs: Vec<RangeExpr> = ast
        .elems
        .iter()
        .map(|elem| match elem {
            NumberSetElem::Unlimited => RangeExpr::Unlimited,
            NumberSetElem::RightLimited(v) => RangeExpr::RightLimited(*v),
            NumberSetElem::LeftLimited(v) => RangeExpr::LeftLimited(*v),
            NumberSetElem::Single(v) => RangeExpr::Single(*v),
            NumberSetElem::Range(a, b) => RangeExpr::Range(*a, *b),
        })
        .collect();

    IntSet::from_exprs(&exprs, min, max).ok()
}

fn parse_control_entries<P: crate::control::ControlSemanticProvider>(
    input: &str,
    provider: &P,
) -> Result<Vec<ResolvedControlEntry>, AutohintError> {
    let entries = crate::control::parse_control(input)?;
    crate::control::validate_control_entries(&entries, provider)?;

    let mut out = Vec::new();

    for (idx, entry) in entries.iter().enumerate() {
        let line_number = (idx + 1) as i32;

        match entry {
            ControlEntryAst::Delta {
                font_idx,
                glyph,
                mode,
                points,
                x_shift,
                y_shift,
                ppems,
            } => {
                let glyph_idx = resolve_glyph_ref(*font_idx, glyph, provider, idx + 1)?;
                let point_count = provider
                    .glyph_point_count(*font_idx, glyph_idx)
                    .ok_or_else(|| AutohintError::ControlFileValidationError {
                        entry_index: idx + 1,
                        message: format!(
                            "unable to get point count for glyph index {} in font {}",
                            glyph_idx, font_idx
                        ),
                    })?;
                let points = number_set_to_intset(points, 0, point_count as i32 - 1)?;
                let ppems = number_set_to_intset(
                    ppems,
                    crate::control::CONTROL_DELTA_PPEM_MIN,
                    crate::control::CONTROL_DELTA_PPEM_MAX,
                )?;

                out.push(ResolvedControlEntry::Delta {
                    font_idx: *font_idx,
                    glyph_idx,
                    before_iup: matches!(mode, PointMode::Touch),
                    points,
                    ppems,
                    x_shift: (*x_shift * crate::control::CONTROL_DELTA_FACTOR as f64).round()
                        as i32,
                    y_shift: (*y_shift * crate::control::CONTROL_DELTA_FACTOR as f64).round()
                        as i32,
                    line_number,
                });
            }
            ControlEntryAst::SegmentDirection {
                font_idx,
                glyph,
                dir,
                points,
                offsets,
            } => {
                let glyph_idx = resolve_glyph_ref(*font_idx, glyph, provider, idx + 1)?;
                let point_count = provider
                    .glyph_point_count(*font_idx, glyph_idx)
                    .ok_or_else(|| AutohintError::ControlFileValidationError {
                        entry_index: idx + 1,
                        message: format!(
                            "unable to get point count for glyph index {} in font {}",
                            glyph_idx, font_idx
                        ),
                    })?;
                let points = number_set_to_intset(points, 0, point_count as i32 - 1)?;
                let (left_offset, right_offset) = offsets.unwrap_or((0, 0));

                out.push(ResolvedControlEntry::SegmentDirection {
                    font_idx: *font_idx,
                    glyph_idx,
                    points,
                    dir: match dir {
                        SegmentDirection::Left => -1,
                        SegmentDirection::Right => 1,
                        SegmentDirection::NoDir => 4,
                    },
                    left_offset,
                    right_offset,
                    line_number,
                });
            }
            ControlEntryAst::StyleAdjust {
                font_idx,
                script,
                feature,
                glyphs,
            } => {
                let mut glyph_indices = Vec::new();
                for glyph_elem in glyphs {
                    match glyph_elem {
                        GlyphSetElem::Single(g) => {
                            glyph_indices.push(resolve_glyph_ref(*font_idx, g, provider, idx + 1)?);
                        }
                        GlyphSetElem::Range(_, _) => {
                            return Err(AutohintError::ControlFileValidationError {
                                entry_index: idx + 1,
                                message: "glyph ranges in StyleAdjust not yet supported"
                                    .to_string(),
                            });
                        }
                    }
                }

                // Resolve script/feature directly to a Skrifa style index.
                let resolved_style =
                    crate::globals::resolve_script_feature_to_style_index(script, feature)
                        .ok_or_else(|| AutohintError::ControlFileValidationError {
                            entry_index: idx + 1,
                            message: format!(
                                "unknown or unsupported style: {}/{}",
                                script, feature
                            ),
                        })?;

                out.push(ResolvedControlEntry::StyleAdjust {
                    font_idx: *font_idx,
                    style: resolved_style as u16,
                    glyph_indices,
                });
            }
            ControlEntryAst::StemWidthAdjust { .. } => {
                out.push(ResolvedControlEntry::StemWidthAdjust);
            }
        }
    }

    Ok(out)
}

fn resolve_glyph_ref<P: crate::control::ControlSemanticProvider>(
    font_idx: i32,
    glyph: &GlyphRef,
    provider: &P,
    entry_index: usize,
) -> Result<GlyphId, AutohintError> {
    match glyph {
        GlyphRef::Index(idx) => Ok(GlyphId::new(*idx)),
        GlyphRef::Name(name) => provider.glyph_index_by_name(font_idx, name).ok_or_else(|| {
            AutohintError::ControlFileValidationError {
                entry_index,
                message: format!("invalid glyph name `{}`", name),
            }
        }),
    }
}

fn number_set_to_intset(set: &NumberSetAst, min: i32, max: i32) -> Result<IntSet, AutohintError> {
    let exprs: Vec<RangeExpr> = set
        .elems
        .iter()
        .map(|elem| match elem {
            NumberSetElem::Unlimited => RangeExpr::Unlimited,
            NumberSetElem::RightLimited(v) => RangeExpr::RightLimited(*v),
            NumberSetElem::LeftLimited(v) => RangeExpr::LeftLimited(*v),
            NumberSetElem::Single(v) => RangeExpr::Single(*v),
            NumberSetElem::Range(a, b) => RangeExpr::Range(*a, *b),
        })
        .collect();

    IntSet::from_exprs(&exprs, min, max)
        .map_err(|_| AutohintError::ValidationError("invalid number set".to_string()))
}

pub(crate) fn fallback_style_for_script(script_index: crate::scripts::ScriptClassIndex) -> i32 {
    crate::style_metadata::default_style_for_script(script_index.as_usize())
        .map(|style| style as i32)
        .or_else(|| crate::style_metadata::none_default_style().map(|style| style as i32))
        .unwrap_or(0)
}
