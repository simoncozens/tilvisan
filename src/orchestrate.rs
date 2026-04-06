use skrifa::Tag;

use crate::{
    control::{parse_control_entries, NumberSetAst, NumberSetElem},
    control_index::ResolvedControlEntry,
    font::Font,
    info::InfoData,
    intset::{IntSet, RangeExpr},
    prep::build_prep_table,
    Args, AutohintError,
};
use std::io::{self, Read};

pub fn autohint(args: &Args) -> Result<Vec<u8>, AutohintError> {
    let input_file = &args.input;

    // Read input file
    let in_buf: Vec<u8> = if args.input == "-" {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf)?;
        buf
    } else {
        std::fs::read(&args.input)?
    };

    // Read control file if provided
    let control_buf: Option<String> = args
        .control_file
        .as_ref()
        .map(std::fs::read_to_string)
        .transpose()?;

    // Read reference file if provided
    let reference_buf: Option<Vec<u8>> = args.reference.as_ref().map(std::fs::read).transpose()?;

    // Validate input sizes
    if in_buf.len() < 100 {
        return Err(AutohintError::ValidationError("Font too small".to_string()));
    }

    if let Some(reference_buf) = &reference_buf {
        if reference_buf.len() < 100 {
            return Err(AutohintError::ValidationError(
                "Reference font too small".to_string(),
            ));
        }
    }

    // Validate cross-field constraints
    args.validate_cross_field_constraints()?;

    // Create InfoData
    let idata = InfoData::from_args(args)
        .unwrap_or_else(|e| panic!("Failed to construct info data for {input_file:?}: {e}"));

    // Parse control entries
    let control_entries: Vec<ResolvedControlEntry> = if let Some(control_text) = &control_buf {
        match crate::control::SkrifaProvider::new(in_buf.to_vec()) {
            Ok(provider) => parse_control_entries(control_text, &provider)?,
            Err(_) => {
                let provider = crate::control::MinimalProvider::new(1);
                parse_control_entries(control_text, &provider)?
            }
        }
    } else {
        Vec::new()
    };

    // Create and initialize Font
    let mut font = Font::new(in_buf)?;
    font.args = args.clone();
    font.progress = None;
    font.info_data = idata;
    font.control.set_entries(control_entries);
    if let Some(ref_buf) = reference_buf {
        font.reference_buf = Some(ref_buf);
    }

    // Main hinting logic
    let dehint = font.args.dehint;
    let adjust_subglyphs = font.args.adjust_subglyphs || font.args.pre_hinting;
    let hint_composites = font.args.composites;
    let fallback_style = fallback_style_for_script(font.args.fallback_script);

    if font.has_ttfautohint_glyph()? {
        return Err(AutohintError::FontAlreadyProcessed);
    }

    let glyph_count = crate::maxp::num_glyphs_in_font_binary(&font.in_buf)?;

    font.glyph_count = glyph_count as i64;
    font.glyph_styles = vec![crate::style::GlyphStyle::unassigned(); glyph_count as usize];

    crate::control_index::control_build_tree(&mut font)?;

    let has_legal_permission = crate::maxp::sfnt_has_legal_permission(&font)?;
    if !has_legal_permission && !font.args.ignore_restrictions {
        return Err(AutohintError::MissingLegalPermission);
    }

    if dehint {
        crate::glyf::split_glyf_table(&mut font)?;
    } else {
        if adjust_subglyphs {
            crate::glyf::create_glyf_data(&mut font)?;
        } else {
            crate::glyf::split_glyf_table(&mut font)?;
        }

        crate::glyf::handle_coverage(&mut font)?;

        font.increase_x_height = font.args.increase_x_height;
    }

    if !dehint {
        crate::glyf::adjust_coverage(&mut font);
        crate::control_index::control_apply_coverage(&mut font);
    }

    crate::gasp::update_gasp(&mut font);

    if !dehint {
        crate::cvt::build_cvt_table_store(&mut font)?;

        let glyf_data = font.glyf_data.take().ok_or(AutohintError::NullPointer)?;

        // Extract values before mutable borrow
        let increase_x_height = font.args.increase_x_height;
        let has_index = font.control.has_index();

        let fpgm_len = crate::fpgm::build_fpgm_table(
            &mut font,
            &glyf_data,
            increase_x_height,
            has_index,
            fallback_style as usize,
        )?;
        let sfnt_ref = &mut font;
        if fpgm_len > sfnt_ref.max_instructions as usize {
            sfnt_ref.max_instructions = fpgm_len as u16;
        }

        let prep_stack = build_prep_table(&mut font, &glyf_data)? as u16;
        let sfnt_ref = &mut font;
        if prep_stack > sfnt_ref.max_stack_elements {
            sfnt_ref.max_stack_elements = prep_stack;
        }

        font.glyf_data = Some(glyf_data);
    }

    crate::glyf::build_glyf_table(&mut font)?;

    if dehint {
        crate::maxp::update_maxp_table_dehint(&mut font)?
    } else {
        let num_glyphs = font
            .glyf_data
            .as_ref()
            .ok_or(AutohintError::NullPointer)?
            .num_glyphs;
        let adjust_composites = font.max_components != 0 && hint_composites;
        crate::maxp::update_maxp_table_hinted(&mut font, adjust_composites, num_glyphs)?;
    }

    if !dehint && font.max_components != 0 && !adjust_subglyphs && hint_composites {
        font.update_hmtx();
        font.update_post();

        crate::gpos::update_gpos(&mut font)?;
    }

    if !font.has_table(Tag::new(b"TTFA")) {
        crate::name::update_name_table(&mut font)?;
    }

    Ok(font.build_ttf_complete(font.have_dsig))
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

pub(crate) fn fallback_style_for_script(script_index: crate::scripts::ScriptClassIndex) -> i32 {
    crate::style_metadata::default_style_for_script(script_index.as_usize())
        .map(|style| style as i32)
        .or_else(|| crate::style_metadata::none_default_style().map(|style| style as i32))
        .unwrap_or(0)
}
