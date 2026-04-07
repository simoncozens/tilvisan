#![allow(non_upper_case_globals)]
use crate::{
    bytecode::{high, low, Bytecode, CONTROL_DELTA_PPEM_MIN},
    font::{Font, TA_PROP_INCREASE_X_HEIGHT_MIN},
    glyf::GlyfData,
    intset::IntSet,
    opcodes::*,
    AutohintError,
};
use skrifa::Tag;

fn build_number_set(number_set: &IntSet) -> (Bytecode, usize) {
    let mut bytecode = Bytecode::new();
    let mut num_singles2 = 0;
    let mut num_singles = 0;
    let mut num_ranges2 = 0;
    let mut num_ranges = 0;
    for range in number_set.ranges() {
        if range.start == range.end {
            if range.start < 256 {
                num_singles += 1;
            } else {
                num_singles2 += 1;
            }
        } else {
            if range.start < 256 && range.end < 256 {
                num_ranges += 1;
            } else {
                num_ranges2 += 1;
            }
        }
    }
    let mut single_args = vec![0u32; num_singles + 1];
    let mut single2_args = vec![0u32; num_singles2 + 1];
    let mut range_args = vec![0u32; num_ranges * 2 + 1];
    let mut range2_args = vec![0u32; num_ranges2 * 2 + 1];
    let have_single = num_singles > 0 || num_singles2 > 0;
    let have_range = num_ranges > 0 || num_ranges2 > 0;
    if have_single {
        single_args[num_singles] = FunctionNumbers::bci_number_set_is_element as u32;
    }
    if have_range {
        range_args[num_ranges * 2] = FunctionNumbers::bci_number_set_is_element2 as u32;
    }

    let mut single2_arg_ix = num_singles2 as isize - 1;
    let mut single_arg_ix = num_singles as isize - 1;
    let mut range2_arg_ix = (num_ranges2 * 2) as isize - 1;
    let mut range_arg_ix = (num_ranges * 2) as isize - 1;

    for range in number_set.ranges() {
        let start = range.start;
        let end = range.end;
        if start == end {
            if start < 256 {
                single_args[single_arg_ix as usize] = start as u32;
                single_arg_ix -= 1;
            } else {
                single2_args[single2_arg_ix as usize] = start as u32;
                single2_arg_ix -= 1;
            }
        } else {
            if start < 256 && end < 256 {
                range_args[range_arg_ix as usize] = start as u32;
                range_arg_ix -= 1;
                range_args[range_arg_ix as usize] = end as u32;
                range_arg_ix -= 1;
            } else {
                range2_args[range2_arg_ix as usize] = start as u32;
                range2_arg_ix -= 1;
                range2_args[range2_arg_ix as usize] = end as u32;
                range2_arg_ix -= 1;
            }
        }
    }
    bytecode.push_u8(PUSHB_2);
    bytecode.push_u8(CvtLocations::cvtl_is_element as u8);
    bytecode.push_u8(0);
    bytecode.push_u8(WCVTP);

    bytecode.push(&single2_args, true, true).unwrap();
    bytecode.push(&single_args, false, true).unwrap();
    if have_single {
        bytecode.push_u8(CALL);
    }
    bytecode.push(&range2_args, true, true).unwrap();
    bytecode.push(&range_args, false, true).unwrap();
    if have_range {
        bytecode.push_u8(CALL);
    }

    let mut num_stack_elements = num_singles + num_singles2;
    if num_stack_elements > num_ranges + num_ranges2 {
        num_stack_elements = num_ranges + num_ranges2;
    }
    num_stack_elements += 20; // ADDITIONAL_STACK_ELEMENTS
    (bytecode, num_stack_elements)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_prep_table(font: &mut Font, glyf_data: &GlyfData) -> Result<(), AutohintError> {
    if font.get_processed(Tag::new(b"glyf")) {
        return Err(AutohintError::TableAlreadyProcessed);
    }

    let num_used_styles = glyf_data.num_used_styles() as u8;
    let mut num_stack_elements = None;
    let windows_compatibility = font.args.windows_compatibility;
    let x_height_snapping_exceptions = if font.args.x_height_snapping_exceptions.is_empty() {
        None
    } else {
        crate::orchestrate::parse_number_set_to_intset(
            &font.args.x_height_snapping_exceptions,
            TA_PROP_INCREASE_X_HEIGHT_MIN,
            0x7FFF,
        )
    };
    let mut bytecode = Bytecode::new();
    if let Some(x_height_snapping_exceptions) = x_height_snapping_exceptions.as_ref() {
        let (bc, nse) = build_number_set(x_height_snapping_exceptions);
        bytecode.extend(bc);
        num_stack_elements = Some(nse);
    }
    if font.args.hinting_limit > 0 {
        bytecode.extend(PREP_hinting_limit_a);
        bytecode.push_word(font.args.hinting_limit);
        bytecode.extend(PREP_hinting_limit_b);
    }
    bytecode.extend(PREP_store_funits_to_pixels);
    if x_height_snapping_exceptions.is_some() {
        bytecode.extend(PREP_test_exception_a);
    }
    bytecode.extend(PREP_align_x_height_a);
    if num_used_styles > 6 {
        bytecode.push_u8(NPUSHB);
        bytecode.push_u8(num_used_styles + 2);
    } else {
        bytecode.push_u8(PUSHB_1 - 1 + num_used_styles + 2);
    }
    for (style_idx, _) in glyf_data.style_offsets.iter().rev() {
        let offset = glyf_data.cvt_x_height_blue_offset(*style_idx);
        bytecode.push_u8(if offset >= 0xFFFF { 0 } else { offset as u8 });
    }
    bytecode.push_u8(num_used_styles);
    bytecode.extend(PREP_align_x_height_b);

    bytecode.extend(PREP_loop_cvt_a);
    if num_used_styles > 3 {
        bytecode.push_u8(NPUSHB);
        bytecode.push_u8(2 * num_used_styles + 2);
    } else {
        bytecode.push_u8(PUSHB_1 - 1 + 2 * num_used_styles + 2);
    }
    for (style_idx, _) in glyf_data.style_offsets.iter().rev() {
        let style_idx = *style_idx;
        let vert_standard_width_offset = glyf_data.cvt_vert_standard_width_offset(style_idx);
        bytecode.push_u8(vert_standard_width_offset as u8);
        let vert_widths_size = glyf_data.cvt_vert_widths_size(style_idx);
        let blues_size = glyf_data.cvt_blues_size(style_idx);
        let num_blues = if blues_size > 1 {
            blues_size - if windows_compatibility { 2 } else { 0 }
        } else {
            0
        };
        bytecode.push_u8((1 + vert_widths_size + num_blues) as u8);
    }
    bytecode.push_u8(num_used_styles);
    bytecode.extend(PREP_loop_cvt_b);
    if num_used_styles > 3 {
        bytecode.push_u8(NPUSHB);
        bytecode.push_u8(2 * num_used_styles + 2);
    } else {
        bytecode.push_u8(PUSHB_1 - 1 + 2 * num_used_styles + 2);
    }
    for (style_idx, _) in glyf_data.style_offsets.iter().rev() {
        let style_idx = *style_idx;
        let blue_shoots_offset = glyf_data.cvt_blue_shoots_offset(style_idx);
        bytecode.push_u8(blue_shoots_offset as u8);
        let blues_size = glyf_data.cvt_blues_size(style_idx);
        let num_blues = if blues_size > 1 {
            blues_size
                - if font.args.windows_compatibility {
                    2
                } else {
                    0
                }
        } else {
            0
        };
        bytecode.push_u8(num_blues as u8);
    }
    bytecode.push_u8(num_used_styles);
    bytecode.extend(PREP_loop_cvt_c);

    if x_height_snapping_exceptions.is_some() {
        bytecode.extend(PREP_test_exception_b);
    }
    bytecode.extend(PREP_store_vwidth_data_a);
    bytecode.push_u8(glyf_data.cvt_vwidth_offset_data(0) as u8);
    bytecode.extend(PREP_store_vwidth_data_b);
    if num_used_styles > 6 {
        bytecode.push_u8(NPUSHW);
        bytecode.push_u8(num_used_styles + 2);
    } else {
        bytecode.push_u8(PUSHW_1 - 1 + num_used_styles + 2);
    }
    for (style_idx, _) in glyf_data.style_offsets.iter().rev() {
        let offset = glyf_data.cvt_vert_widths_offset(*style_idx) * 64;
        bytecode.push_u8(high(offset));
        bytecode.push_u8(low(offset));
    }
    bytecode.push_u8(high(num_used_styles.into()));
    bytecode.push_u8(low(num_used_styles.into()));

    bytecode.extend(PREP_store_vwidth_data_c);
    bytecode.push_u8(glyf_data.cvt_vwidth_size_data(0) as u8);
    bytecode.extend(PREP_store_vwidth_data_d);
    if num_used_styles > 6 {
        bytecode.push_u8(NPUSHW);
        bytecode.push_u8(num_used_styles + 2);
    } else {
        bytecode.push_u8(PUSHW_1 - 1 + num_used_styles + 2);
    }
    for (style_idx, _) in glyf_data.style_offsets.iter().rev() {
        let size = glyf_data.cvt_vert_widths_size(*style_idx) * 64;
        bytecode.push_u8(high(size));
        bytecode.push_u8(low(size));
    }
    bytecode.push_u8(high(num_used_styles.into()));
    bytecode.push_u8(low(num_used_styles.into()));
    bytecode.extend(PREP_store_vwidth_data_e);

    let gray_mode_word = font.args.stem_width_mode.gray.to_word();
    let gdi_mode_word = font.args.stem_width_mode.gdi_cleartype.to_word();
    let dw_mode_word = font.args.stem_width_mode.dw_cleartype.to_word();

    bytecode.extend(PREP_set_stem_width_mode_a);
    bytecode.push_word_i32(gray_mode_word);
    bytecode.extend(PREP_set_stem_width_mode_b);
    bytecode.push_word_i32(gdi_mode_word);
    bytecode.extend(PREP_set_stem_width_mode_c);
    bytecode.push_word_i32(dw_mode_word);
    bytecode.extend(PREP_set_stem_width_mode_d);
    bytecode.push_word_i32(dw_mode_word);
    bytecode.extend(PREP_set_stem_width_mode_e);

    if num_used_styles > 3 {
        bytecode.push_u8(NPUSHB);
        bytecode.push_u8(2 * num_used_styles + 2);
    } else {
        bytecode.push_u8(PUSHB_1 - 1 + 2 * num_used_styles + 2);
    }
    for (style_idx, _) in glyf_data.style_offsets.iter().rev() {
        let style_idx = *style_idx;
        let blue_refs_offset = glyf_data.cvt_blue_refs_offset(style_idx);
        bytecode.push_u8(blue_refs_offset as u8);
        bytecode.push_u8(glyf_data.cvt_blues_size(style_idx) as u8);
    }
    bytecode.push_u8(num_used_styles);
    bytecode.extend(PREP_round_blues);

    bytecode.extend(PREP_set_dropout_mode);
    bytecode.extend(PREP_reset_component_counter);
    if x_height_snapping_exceptions.is_some() {
        bytecode.extend(PREP_adjust_delta_exceptions);
    }
    bytecode.extend(PREP_set_default_cvs_values);

    font.prep = bytecode.as_slice().to_vec();
    font.final_maxp_data
        .update_max_stack_elements(num_stack_elements.unwrap_or(0) as u16);
    Ok(())
}

const PREP_hinting_limit_a: [u8; 3] = [
    /* all our measurements are taken along the y axis, */
    /* including the ppem and CVT values */
    SVTCA_y, /* first of all, check whether we do hinting at all */
    MPPEM, PUSHW_1,
];

/*  %d, hinting size limit */

const PREP_hinting_limit_b: [u8; 7] = [
    GT, IF, PUSHB_2, 1, /* switch off hinting */
    1, INSTCTRL, EIF,
];

/* we store 0x10000 in CVT index `cvtl_funits_to_pixels' as a scaled value */
/* to have a conversion factor from FUnits to pixels */

const PREP_store_funits_to_pixels: [u8; 9] = [
    PUSHB_1,
    CvtLocations::cvtl_funits_to_pixels as u8,
    PUSHW_2,
    0x08, /* 0x800 */
    0x00,
    0x08, /* 0x800 */
    0x00,
    MUL,   /* 0x10000 */
    WCVTF, /* store value 1 in 16.16 format, scaled */
];

/* if the current ppem value is an exception, don't apply scaling */

const PREP_test_exception_a: [u8; 5] =
    [PUSHB_1, CvtLocations::cvtl_is_element as u8, RCVT, NOT, IF];

/* provide scaling factors for all styles */

const PREP_align_x_height_a: [u8; 4] = [
    PUSHB_2,
    StorageAreaLocations::sal_i as u8,
    CVT_SCALING_VALUE_OFFSET(0),
    WS,
];

/*  PUSHB (num_used_styles + 2) */
/*    ... */
/*    %c, style 1's x height blue zone idx */
/*    %c, style 0's x height blue zone idx */
/*    %c, num_used_styles */

const PREP_align_x_height_b: [u8; 2] = [FunctionNumbers::bci_align_x_height as u8, LOOPCALL];
const PREP_loop_cvt_a: [u8; 4] = [
    /* loop over (almost all) vertical CVT entries of all styles, part 1 */
    PUSHB_2,
    StorageAreaLocations::sal_i as u8,
    CVT_SCALING_VALUE_OFFSET(0),
    WS,
];
/*  PUSHB (2*num_used_styles + 2) */
/*    ... */
/*    %c, style 1's first vertical index */
/*    %c, style 1's number of vertical indices */
/*        (std. width, widths, flat blues zones without artifical ones) */
/*    %c, style 0's first vertical index */
/*    %c, style 0's number of vertical indices */
/*        (std. width, widths, flat blues zones without artifical ones) */
/*    %c, num_used_styles */

const PREP_loop_cvt_b: [u8; 6] = [
    FunctionNumbers::bci_cvt_rescale_range as u8,
    LOOPCALL,
    /* loop over (almost all) vertical CVT entries of all styles, part 2 */
    PUSHB_2,
    StorageAreaLocations::sal_i as u8,
    CVT_SCALING_VALUE_OFFSET(0),
    WS,
];
/*  PUSHB (2*num_used_styles + 2) */
/*    ... */
/*    %c, style 1's first round blue zone index */
/*    %c, style 1's number of round blue zones (without artificial ones) */
/*    %c, style 0's first round blue zone index */
/*    %c, style 0's number of round blue zones (without artificial ones) */
/*    %c, num_used_styles */

const PREP_loop_cvt_c: [u8; 2] = [FunctionNumbers::bci_cvt_rescale_range as u8, LOOPCALL];
const PREP_test_exception_b: [u8; 1] = [EIF];
const PREP_store_vwidth_data_a: [u8; 2] = [PUSHB_2, StorageAreaLocations::sal_i as u8];
/*  %c, offset to vertical width offset data in CVT */

const PREP_store_vwidth_data_b: [u8; 1] = [WS];
/*PUSHW (num_used_styles + 2) */
/*  ... */
/*  %d, style 1's first vertical width index (in multiples of 64) */
/*  %d, style 0's first vertical width index (in multiples of 64) */
/*  %d, num_used_styles */

const PREP_store_vwidth_data_c: [u8; 5] = [
    0x00,                                         /* high byte */
    FunctionNumbers::bci_vwidth_data_store as u8, /* low byte */
    LOOPCALL,
    PUSHB_2,
    StorageAreaLocations::sal_i as u8,
];
/*  %c, offset to vertical width size data in CVT */

const PREP_store_vwidth_data_d: [u8; 1] = [WS];
/*PUSHW (num_used_styles + 2) */
/*  ... */
/*  %d, style 1's number of vertical widths (in multiples of 64) */
/*  %d, style 0's number of vertical widths (in multiples of 64) */
/*  %d, num_used_styles */

const PREP_store_vwidth_data_e: [u8; 3] = [
    0x00,                                         /* high byte */
    FunctionNumbers::bci_vwidth_data_store as u8, /* low byte */
    LOOPCALL,
];
const PREP_set_stem_width_mode_a: [u8; 3] = [
    /*
     * ttfautohint provides two different functions for stem width computation
     * and blue zone rounding: `smooth' and `strong'.  The former tries to
     * align stem widths and blue zones to some discrete, possibly non-integer
     * values.  The latter snaps everything to integer pixels as much as
     * possible.
     *
     * We test ClearType capabilities to find out which of the two functions
     * should be used.  Due to the various TrueType interpreter versions this
     * is quite convoluted.
     *
     *   interpreter version  action
     *   ---------------------------------------------------------------------
     *          <= 35         this version predates ClearType -> smooth
     *
     *          36-38         use bit 6 in the GETINFO instruction to check
     *                        whether ClearType is enabled; if set, we have
     *                        (old) GDI ClearType -> strong, otherwise
     *                        grayscale rendering -> smooth
     *
     *          39            if ClearType is enabled, use bit 10 in the
     *                        GETINFO instruction to check whether ClearType
     *                        sub-pixel positioning is available; if set, we
     *                        have DW ClearType -> smooth, else GDI ClearType
     *                        -> strong
     *
     *          >= 40         if ClearType is enabled, use bit 11 in the
     *                        GETINFO instruction to check whether ClearType
     *                        symmetric rendering is available; if not set,
     *                        the engine behaves like a B/W renderer along the
     *                        y axis -> strong, else it does vertical
     *                        smoothing -> smooth
     *
     * ClearType on Windows was introduced in 2000 for the GDI framework (no
     * symmetric rendering, no sub-pixel positioning).  In 2008, Windows got
     * the DirectWrite (DW) framework which uses symmetric rendering and
     * sub-pixel positioning.
     *
     * Note that in 2017 GDI on Windows 10 has changed rendering parameters:
     * it now uses symmetric rendering but no sub-pixel positioning.
     * Consequently, we treat this as `DW ClearType' also.
     */

    /* set default value */
    PUSHW_2,
    0,
    CvtLocations::cvtl_stem_width_mode as u8, /* target: grayscale rendering */
];
/*  %d, either -100, 0 or 100 */

const PREP_set_stem_width_mode_b: [u8; 14] = [
    WCVTP,
    /* get rasterizer version (bit 0) */
    PUSHB_2,
    36,
    0x01,
    GETINFO,
    /* `(old) GDI ClearType': version >= 36 || version <= 38 */
    LTEQ,
    IF,
    /* check whether ClearType is enabled (bit 6) */
    PUSHB_1,
    0x40,
    GETINFO,
    IF,
    PUSHW_2,
    0,
    CvtLocations::cvtl_stem_width_mode as u8, /* target: GDI ClearType */
];
/*      %d, either -100, 0 or 100 */

const PREP_set_stem_width_mode_c: [u8; 15] = [
    WCVTP,
    /* get rasterizer version (bit 0) */
    PUSHB_2,
    40,
    0x01,
    GETINFO,
    /* `DW ClearType': version >= 40 */
    LTEQ,
    IF,
    /* check whether symmetric rendering is enabled (bit 11) */
    PUSHW_1,
    0x08,
    0x00,
    GETINFO,
    IF,
    PUSHW_2,
    0,
    CvtLocations::cvtl_stem_width_mode as u8, /* target: DirectWrite ClearType */
];
/*          %d, either -100, 0 or 100 */

const PREP_set_stem_width_mode_d: [u8; 23] = [
    WCVTP,
    EIF,
    ELSE,
    /* get rasterizer version (bit 0) */
    PUSHB_2,
    39,
    0x01,
    GETINFO,
    /* `DW ClearType': version == 39 */
    LTEQ,
    IF,
    /* check whether sub-pixel positioning is enabled (bit 10) -- */
    /* due to a bug in FreeType 2.5.0 and earlier, */
    /* bit 6 must be set also to get the correct information, */
    /* so we test that both return values (in bits 13 and 17) are set */
    PUSHW_3,
    0x08, /* bits 13 and 17 right-shifted by 6 bits */
    0x80,
    0x00, /* we do `MUL' with value 1, */
    0x01, /* which is essentially a division by 64 */
    0x04, /* bits 6 and 10 */
    0x40,
    GETINFO,
    MUL,
    EQ,
    IF,
    PUSHW_2,
    0,
    CvtLocations::cvtl_stem_width_mode as u8, /* target: DirectWrite ClearType */
];
/*            %d, either -100, 0 or 100 */

const PREP_set_stem_width_mode_e: [u8; 6] = [WCVTP, EIF, EIF, EIF, EIF, EIF];
/*PUSHB (2*num_used_styles + 2) */
/*  ... */
/*  %c, style 1's first blue ref index */
/*  %c, style 1's number of blue ref indices */
/*  %c, style 0's first blue ref index */
/*  %c, style 0's number of blue ref indices */
/*  %c, num_used_styles */

const PREP_round_blues: [u8; 2] = [FunctionNumbers::bci_blue_round_range as u8, LOOPCALL];
const PREP_set_dropout_mode: [u8; 7] = [
    PUSHW_1, 0x01, /* 0x01FF, activate dropout handling unconditionally */
    0xFF, SCANCTRL, PUSHB_1, 4, /* smart dropout include stubs */
    SCANTYPE,
];
const PREP_reset_component_counter: [u8; 4] = [
    /* In case an application tries to render `.ttfautohint' */
    /* (which it should never do), */
    /* hinting of all glyphs rendered afterwards is disabled */
    /* because the `cvtl_is_subglyph' counter gets incremented, */
    /* but there is no counterpart to decrement it. */
    /* Font inspection tools like the FreeType demo programs */
    /* are an exception to that rule, however, */
    /* since they can directly access a font by glyph indices. */
    /* The following guard alleviates the problem a bit: */
    /* Any change of the graphics state */
    /* (for example, rendering at a different size or with a different mode) */
    /* resets the counter to zero. */
    PUSHB_2,
    CvtLocations::cvtl_is_subglyph as u8,
    0,
    WCVTP,
];
const PREP_adjust_delta_exceptions: [u8; 3] = [
    /* set delta base */
    PUSHB_1,
    CONTROL_DELTA_PPEM_MIN,
    SDB,
];

const PREP_set_default_cvs_values: [u8; 7] = [
    /* We set a default value for `cvtl_do_iup_y'. */
    /* If we have delta exceptions before IUP_y, */
    /* the glyph's bytecode sets this CVT value temporarily to zero */
    /* and manually inserts IUP_y afterwards. */

    /* We set a default value for `cvtl_ignore_std_width'. */
    /* As the name implies, the stem width computation routines */
    /* ignore the standard width(s) if this flag gets set. */

    /* It would be more elegant to use storage area locations instead, */
    /* however, it is not possible to have default values for them */
    /* since storage area locations might be reset on a per-glyph basis */
    /* (this is dependent on the bytecode interpreter implementation). */
    PUSHB_4,
    CvtLocations::cvtl_do_iup_y as u8,
    100,
    CvtLocations::cvtl_ignore_std_width as u8,
    0,
    WCVTP,
    WCVTP,
];
