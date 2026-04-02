use skrifa::{GlyphId, Tag};

use crate::c_font::{
    Font as TaFont, TA_HINTING_LIMIT, TA_HINTING_RANGE_MAX, TA_HINTING_RANGE_MIN,
    TA_INCREASE_X_HEIGHT, TA_PROP_INCREASE_X_HEIGHT_MIN,
};
use crate::control::{
    ControlEntryAst, GlyphRef, GlyphSetElem, NumberSetAst, NumberSetElem, PointMode,
    SegmentDirection,
};
use crate::control_index::ResolvedControlEntry;
use crate::intset::{IntSet, RangeExpr};
use crate::maxp::sfnt_has_ttfautohint_glyph;
use crate::prep::build_prep_table;
use crate::recorder::ta_rs_build_glyph_instructions;
use crate::tablestore::TableStore;
use crate::Args;
use crate::{info::InfoData, AutohintError};
use std::ffi::{c_int, c_long};
use std::io::{self, Read};

const STEM_MODE_MIN: i32 = -1;
const STEM_MODE_MAX: i32 = 1;
const HINTING_RANGE_MIN_MIN: i32 = 2;
const INCREASE_X_HEIGHT_MIN: i32 = 6;
const TA_STYLE_NONE_DFLT: i32 = 83;
const TA_ERR_ALREADY_PROCESSED: i32 = 0xF5;
const TA_ERR_MISSING_LEGAL_PERMISSION: i32 = 0x0F;

fn ta_sfnt_build_glyph_instructions_cb(
    font: &mut TaFont,
    sfnt_idx: usize,
    idx: GlyphId,
) -> Result<c_int, AutohintError> {
    ta_rs_build_glyph_instructions(font, sfnt_idx, idx)?;
    Ok(0)
}

pub fn ttf_autohint_font(font: &mut TaFont) -> Result<Vec<u8>, AutohintError> {
    let num_sfnts = crate::maxp::num_faces_in_font_binary(&font.in_buf)?;
    font.init_owned_sfnts(num_sfnts as usize);
    font.table_store = TableStore::default();
    font.init_owned_glyf_ptrs(num_sfnts as usize);

    for i in 0..num_sfnts {
        font.sfnts_owned[i as usize].table_store_sfnt_idx = font.table_store.add_sfnt() as usize;
    }
    for (i, sfnt_ref) in font.sfnts_owned.iter_mut().enumerate() {
        if sfnt_has_ttfautohint_glyph(&font.table_store, i)? {
            return Err(AutohintError::UnportedError(TA_ERR_ALREADY_PROCESSED));
        }

        let glyph_count = crate::maxp::num_glyphs_in_font_binary_at_index(&font.in_buf, i as u32)?;

        sfnt_ref.glyph_count = glyph_count as c_long;
        sfnt_ref.glyph_styles = vec![0; glyph_count as usize];
    }

    crate::control_index::ta_rs_control_build_tree_rs(font)?;

    for i in 0..font.num_sfnts() {
        let sfnt_table_store_idx = font.sfnts_owned[i].table_store_sfnt_idx;

        let (have_dsig, max_components) =
            crate::tablestore::c_api::ta_table_store_populate_sfnt_from_font(
                &mut font.table_store,
                sfnt_table_store_idx as u32,
                &font.in_buf,
            )?;

        font.have_dsig = have_dsig;
        font.sfnts_owned[i].max_components = max_components;

        let has_legal_permission =
            crate::maxp::sfnt_has_legal_permission(&font.table_store, sfnt_table_store_idx)?;
        if !has_legal_permission && !font.ignore_restrictions {
            return Err(AutohintError::UnportedError(
                TA_ERR_MISSING_LEGAL_PERMISSION,
            ));
        }

        if font.dehint {
            crate::glyf::ta_rs_split_glyf_table(font, i)?;
        } else {
            if font.adjust_subglyphs {
                crate::glyf::ta_rs_create_glyf_data(font, i)?;
            } else {
                crate::glyf::ta_rs_split_glyf_table(font, i)?;
            }

            crate::glyf::ta_rs_handle_coverage(font, i)?;

            font.sfnts_owned[i].increase_x_height = font.increase_x_height;
        }
    }

    if !font.dehint {
        for i in 0..font.num_sfnts() {
            crate::glyf::ta_rs_adjust_coverage(font, i);
        }
        for i in 0..font.num_sfnts() {
            crate::control_index::ta_rs_control_apply_coverage(font, i);
        }
    }

    for i in 0..font.num_sfnts() {
        let sfnt_table_store_idx = font.sfnts_owned[i].table_store_sfnt_idx;

        crate::gasp::update_gasp(&mut font.table_store, sfnt_table_store_idx);

        if !font.dehint {
            crate::cvt::ta_rs_build_cvt_table_store(font, i)?;

            let glyf_data = font
                .glyf_ptrs_owned
                .get_mut(i)
                .and_then(Option::take)
                .ok_or(AutohintError::NullPointer)?;

            let fpgm_len = crate::fpgm::build_fpgm_table(
                &mut font.table_store,
                sfnt_table_store_idx,
                &glyf_data,
                font.increase_x_height,
                font.control.has_index(),
                font.fallback_style as usize,
            )?;
            let sfnt_ref = &mut font.sfnts_owned[i];
            if fpgm_len > sfnt_ref.max_instructions as usize {
                sfnt_ref.max_instructions = fpgm_len as u16;
            }

            let prep_stack = build_prep_table(font, sfnt_table_store_idx, &glyf_data)? as u16;
            let sfnt_ref = &mut font.sfnts_owned[i];
            if prep_stack > sfnt_ref.max_stack_elements {
                sfnt_ref.max_stack_elements = prep_stack;
            }

            font.glyf_ptrs_owned[i] = Some(glyf_data);
        }

        crate::glyf::ta_rs_build_glyf_table(font, i, Some(ta_sfnt_build_glyph_instructions_cb))?;
    }

    for i in 0..font.num_sfnts() {
        let sfnt = &font.sfnts_owned[i];
        let sfnt_table_store_idx = sfnt.table_store_sfnt_idx;
        let sfnt_max_components = sfnt.max_components;
        let sfnt_max_composite_points = sfnt.max_composite_points;
        let sfnt_max_composite_contours = sfnt.max_composite_contours;
        let sfnt_max_twilight_points = sfnt.max_twilight_points;
        let sfnt_max_storage = sfnt.max_storage;
        let sfnt_max_stack_elements = sfnt.max_stack_elements;
        let sfnt_max_instructions = sfnt.max_instructions;

        if font.dehint {
            crate::maxp::update_maxp_table_dehint(&mut font.table_store, sfnt_table_store_idx)?
        } else {
            let data = font
                .glyf_ptrs_owned
                .get(i)
                .and_then(Option::as_ref)
                .ok_or(AutohintError::NullPointer)?;
            let adjust_composites = sfnt_max_components != 0 && font.hint_composites;
            crate::maxp::update_maxp_table_hinted(
                &mut font.table_store,
                sfnt_table_store_idx,
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

        if !font.dehint
            && sfnt_max_components != 0
            && !font.adjust_subglyphs
            && font.hint_composites
        {
            crate::hmtx::update_hmtx(&mut font.table_store, sfnt_table_store_idx);
            crate::post::update_post(&mut font.table_store, sfnt_table_store_idx);

            let data = font
                .glyf_ptrs_owned
                .get(i)
                .and_then(Option::as_ref)
                .ok_or(AutohintError::NullPointer)?;
            crate::gpos::update_gpos(&mut font.table_store, sfnt_table_store_idx, &data.glyphs)?;
        }

        if !font
            .table_store
            .has_table(sfnt_table_store_idx, Tag::new(b"TTFA"))
        {
            crate::name::update_name_table(
                &mut font.table_store,
                sfnt_table_store_idx,
                &mut font.info_data,
            )?;
        }
    }

    Ok(font.table_store.build_ttf_complete(0, font.have_dsig))
}

// Keep these tables in sync with C sources:
// - lib/ttfautohint-scripts.h for DEFAULT_SCRIPTS
// - lib/tastyles.h (TA_COVERAGE_DEFAULT styles) for FALLBACK_SCRIPTS
const DEFAULT_SCRIPTS: &[&str] = &[
    "adlm", "arab", "armn", "avst", "bamu", "beng", "buhd", "cakm", "cans", "cari", "cher", "copt",
    "cprt", "cyrl", "deva", "dsrt", "ethi", "geor", "geok", "glag", "goth", "grek", "gujr", "guru",
    "hebr", "hmnp", "kali", "khmr", "khms", "knda", "lao", "latn", "latb", "latp", "lisu", "mlym",
    "medf", "mong", "mymr", "nkoo", "olck", "orkh", "osge", "osma", "rohg", "saur", "shaw", "sinh",
    "sund", "taml", "tavt", "telu", "tfng", "thai", "vaii", "yezi", "none",
];

const FALLBACK_SCRIPTS: &[&str] = &[
    "adlm", "arab", "armn", "avst", "bamu", "beng", "buhd", "cakm", "cans", "cari", "cher", "copt",
    "cprt", "deva", "dsrt", "ethi", "geor", "geok", "glag", "goth", "gujr", "guru", "hebr", "hmnp",
    "kali", "khmr", "khms", "knda", "lao", "latb", "latp", "latn", "lisu", "mlym", "medf", "mong",
    "mymr", "nkoo", "olck", "orkh", "osge", "osma", "rohg", "saur", "shaw", "sinh", "sund", "taml",
    "tavt", "telu", "tfng", "thai", "vaii", "yezi", "none",
];

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

pub fn ttfautohint(call: &TtfautohintCall, idata: &mut InfoData) -> Result<Vec<u8>, AutohintError> {
    validate_options(idata)?;

    let default_script_idx = script_to_index(&idata.default_script);
    let fallback_script_idx = script_to_index(&idata.fallback_script);

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

    let mut font = TaFont::default();

    if !idata.dehint {
        let mut x_height_snapping_exceptions = None;

        if !idata.x_height_snapping_exceptions_string.is_empty() {
            x_height_snapping_exceptions = parse_number_set_to_intset(
                &idata.x_height_snapping_exceptions_string,
                TA_PROP_INCREASE_X_HEIGHT_MIN,
                0x7FFF,
            );
        }

        font.reference_index = idata.reference_index as c_long;
        font.reference_name = idata.reference_name.clone();
        font.hinting_range_min =
            normalized_or_default(idata.hinting_range_min, TA_HINTING_RANGE_MIN);
        font.hinting_range_max =
            normalized_or_default(idata.hinting_range_max, TA_HINTING_RANGE_MAX);
        font.hinting_limit = normalized_or_default(idata.hinting_limit, TA_HINTING_LIMIT);
        font.increase_x_height =
            normalized_or_default(idata.increase_x_height, TA_INCREASE_X_HEIGHT);
        font.x_height_snapping_exceptions = x_height_snapping_exceptions;
        font.fallback_stem_width = idata.fallback_stem_width as u32;
        font.gray_stem_width_mode = idata.gray_stem_width_mode;
        font.gdi_cleartype_stem_width_mode = idata.gdi_cleartype_stem_width_mode;
        font.dw_cleartype_stem_width_mode = idata.dw_cleartype_stem_width_mode;
        font.windows_compatibility = idata.windows_compatibility;
        font.ignore_restrictions = call.ignore_restrictions;
        font.adjust_subglyphs = idata.adjust_subglyphs;
        font.hint_composites = idata.hint_composites;
        font.fallback_style = fallback_style_for_script(fallback_script_idx);
        font.fallback_scaling = idata.fallback_scaling;
        font.default_script = default_script_idx;
        font.symbol = idata.symbol;
    }

    font.progress = None;
    font.info_data = idata.clone();
    font.debug = call.debug;
    font.dehint = idata.dehint;
    font.ttfa_info = idata.ttfa_info;
    font.epoch = call.epoch;
    font.gasp_idx = u64::MAX;
    font.in_buf = call.in_buf.clone();
    font.control.set_entries(control_entries);

    if let Some(reference_buf) = reference_slice {
        font.reference_buf = Some(reference_buf.to_vec());
    }

    ttf_autohint_font(&mut font)
}

fn validate_options(idata: &InfoData) -> io::Result<()> {
    if !(STEM_MODE_MIN..=STEM_MODE_MAX).contains(&idata.gray_stem_width_mode)
        || !(STEM_MODE_MIN..=STEM_MODE_MAX).contains(&idata.gdi_cleartype_stem_width_mode)
        || !(STEM_MODE_MIN..=STEM_MODE_MAX).contains(&idata.dw_cleartype_stem_width_mode)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "stem-width mode values must be in -1..=1",
        ));
    }

    if idata.hinting_range_min < HINTING_RANGE_MIN_MIN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "hinting-range-min must be at least 2",
        ));
    }

    if idata.hinting_range_max < idata.hinting_range_min {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "hinting-range-max must be >= hinting-range-min",
        ));
    }

    if idata.hinting_limit > 0 && idata.hinting_limit < idata.hinting_range_max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "hinting-limit must be 0 or >= hinting-range-max",
        ));
    }

    if idata.increase_x_height > 0 && idata.increase_x_height < INCREASE_X_HEIGHT_MIN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "increase-x-height must be 0 or >= 6",
        ));
    }

    if idata.fallback_stem_width < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "fallback-stem-width must be non-negative",
        ));
    }

    validate_default_script(&idata.default_script)?;
    validate_fallback_script(&idata.fallback_script)?;

    Ok(())
}

fn validate_default_script(value: &str) -> io::Result<()> {
    if DEFAULT_SCRIPTS.contains(&value) {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("default-script '{value}' is not supported"),
    ))
}

fn validate_fallback_script(value: &str) -> io::Result<()> {
    if FALLBACK_SCRIPTS.contains(&value) {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("fallback-script '{value}' is not supported"),
    ))
}

fn script_to_index(tag: &str) -> i32 {
    // Fine to unwrap: callers have already validated the tag is in DEFAULT_SCRIPTS.
    DEFAULT_SCRIPTS.iter().position(|&s| s == tag).unwrap() as i32
}

fn normalized_or_default(value: i32, default_value: u32) -> u32 {
    if value < 0 {
        default_value
    } else {
        value as u32
    }
}

fn parse_number_set_to_intset(input: &str, min: i32, max: i32) -> Option<IntSet> {
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

                let style_key = format!("{}/{}", script, feature);
                let style_hash = style_key
                    .as_bytes()
                    .iter()
                    .fold(0i64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as i64));

                out.push(ResolvedControlEntry::StyleAdjust {
                    font_idx: *font_idx,
                    style: style_hash as u16,
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

fn fallback_style_for_script(script_index: i32) -> i32 {
    crate::style_metadata::default_style_for_script(script_index as usize)
        .map(|style| style as i32)
        .unwrap_or(TA_STYLE_NONE_DFLT)
}
