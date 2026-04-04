#![allow(non_upper_case_globals)]
use crate::{
    bytecode::{Bytecode, CONTROL_DELTA_PPEM_MIN},
    bytes,
    c_font::Font,
    glyf::GlyfData,
    opcodes::*,
    AutohintError,
};
use skrifa::Tag;
/*
 * Due to a bug in FreeType up to and including version 2.4.7,
 * it is not possible to use MD_orig with twilight points.
 * We circumvent this by using GC_orig.
 */
const MD_orig_fixed: &[u8] = &bytes![GC_orig, SWAP, GC_orig, SWAP, SUB];

const MD_orig_ZP2_0: &[u8] = MD_orig_fixed;
/*
 * Older versions of Monotype's `iType' bytecode interpreter have a serious
 * bug: The DIV instruction rounds the result, while the correct operation
 * is truncation.  (Note, however, that MUL always rounds the result.)
 * Since many printers contain this rasterizer without any possibility to
 * update to a non-buggy version, we have to work around the problem.
 *
 * DIV and MUL work on 26.6 numbers which means that the numerator gets
 * multiplied by 64 before the division, and the product gets divided by 64
 * after the multiplication, respectively.  For example, to divide 2 by 3,
 * you have to write
 *
 *   PUSHB_1,
 *     2,
 *     3*64,
 *   DIV
 *
 * The correct formula to divide two values in 26.6 format with truncation
 * is
 *
 *   a*64 / b    ,
 *
 * while older `iType' versions incorrectly apply rounding by using
 *
 *  (a*64 + b/2) / b    .
 *
 * Doing our 2/3 division, we have a=2 and b=3*64, so we get
 *
 *   (2*64 + (3*64)/2) / (3*64) = 1
 *
 * instead of the correct result 0.
 *
 * The solution to the rounding issue is to use a 26.6 value as an
 * intermediate result so that we can truncate to the nearest integer (in
 * 26.6 format) with the FLOOR operator before converting back to a plain
 * integer (in 32.0 format).
 *
 * For the common divisions by 2 and 2^10 we define macros.
 */

const DIV_POS_BY_2: &[u8] = &bytes![
    PUSHB_1, 2, DIV, /* multiply by 64, then divide by 2 */
    FLOOR, PUSHB_1, 1, MUL /* multiply by 1, then divide by 64 */
];

const DIV_BY_2: &[u8] = &bytes![
    PUSHB_1, 2, DIV, DUP, PUSHB_1, 0, LT, IF, PUSHB_1, 64,
    ADD, /* add 1 if value is negative */
    EIF, FLOOR, PUSHB_1, 1, MUL
];

const DIV_BY_1024: &[u8] = &bytes![
    PUSHW_1, 0x04, /* 2^10 */
    0x00, DIV, DUP, PUSHB_1, 0, LT, IF, PUSHB_1, 64, ADD, EIF, FLOOR, PUSHB_1, 1, MUL
];

// /* a convenience shorthand for scaling; see `bci_cvt_rescale' for details */
const DO_SCALE: &[u8] = &bytes![
    DUP, /* s: a a */
    PUSHB_1,
    StorageAreaLocations::sal_scale,
    RS,
    MUL, /* delta * 2^10 */
    // This is DIV_BY_1024
    PUSHW_1,
    0x04, /* 2^10 */
    0x00,
    DIV,
    DUP,
    PUSHB_1,
    0,
    LT,
    IF,
    PUSHB_1,
    64,
    ADD,
    EIF,
    FLOOR,
    PUSHB_1,
    1,
    MUL,
    // End of DIV_BY_1024
    ADD /* a + delta */
];
/* in the comments below, the top of the stack (`s:') */
/* is the rightmost element; the stack is shown */
/* after the instruction on the same line has been executed */

/*
 * bci_align_x_height
 *
 *   Optimize the alignment of the top of small letters to the pixel grid.
 *
 *   This function gets used in the `prep' table.
 *
 * in: blue_idx (CVT index for the style's top of small letters blue zone)
 *
 * sal: sal_i (CVT index of the style's scaling value;
 *             gets incremented by 1 after execution)
 */

const FPGM_bci_align_x_height_a: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_align_x_height,
    FDEF,
    /* only get CVT value for non-zero index */
    DUP,
    PUSHB_1,
    0,
    NEQ,
    IF,
    RCVT,
    EIF,
    DUP,
    DUP, /* s: blue blue blue */
];

/* if (font->increase_x_height) */
/* { */

const FPGM_bci_align_x_height_b1a: &[u8] = &bytes![
    /* apply much `stronger' rounding up of x height for */
    /* 6 <= PPEM <= increase_x_height */
    MPPEM, PUSHW_1,
];

/*  %d, x height increase limit */

const FPGM_bci_align_x_height_b1b: &[u8] = &bytes![
    LTEQ, MPPEM, PUSHB_1, 6, GTEQ, AND, IF, PUSHB_1, 52, /* threshold = 52 */
    ELSE, PUSHB_1, 40, /* threshold = 40 */
    EIF, ADD, FLOOR, /* fitted = FLOOR(blue + threshold) */
];

/* } */

/* if (!font->increase_x_height) */
/* { */

const FPGM_bci_align_x_height_b2: &[u8] =
    &bytes![PUSHB_1, 40, ADD, FLOOR /* fitted = FLOOR(blue + 40) */];

/* } */

const FPGM_bci_align_x_height_c: &[u8] = &bytes![
    DUP, /* s: blue blue fitted fitted */
    ROLL,
    NEQ,
    IF, /* s: blue fitted */
    PUSHB_1,
    2,
    CINDEX,
    SUB, /* s: blue (fitted-blue) */
    PUSHW_2,
    0x08, /* 0x800 */
    0x00,
    0x08, /* 0x800 */
    0x00,
    MUL, /* 0x10000 */
    MUL, /* (fitted-blue) in 16.16 format */
    SWAP,
    DIV, /* factor = ((fitted-blue) / blue) in 16.16 format */
    ELSE,
    POP,
    POP,
    PUSHB_1,
    0, /* factor = 0 */
    EIF,
    PUSHB_1,
    StorageAreaLocations::sal_i as u8,
    RS, /* s: factor idx */
    SWAP,
    WCVTP,
    PUSHB_3,
    StorageAreaLocations::sal_i as u8,
    1,
    StorageAreaLocations::sal_i as u8,
    RS,
    ADD, /* sal_i = sal_i + 1 */
    WS,
    ENDF,
];

/*
 * bci_round
 *
 *   Round a 26.6 number.  Contrary to the ROUND bytecode instruction, no
 *   engine specific corrections are applied.
 *
 * in: val
 *
 * out: ROUND(val)
 */

const FPGM_bci_round: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_round as u8,
    FDEF,
    PUSHB_1,
    32,
    ADD,
    FLOOR,
    ENDF,
];

/*
 * bci_quantize_stem_width
 *
 *   Take a stem width and compare it against a set of already stored stem
 *   widths.  If the difference to one of the values is less than 4, replace
 *   the argument with the stored one.  Otherwise add the argument to the
 *   array of stem widths, without changing the argument.
 *
 *   We do this to catch rounding errors.
 *
 *   in: val
 *
 *   out: quantized(val)
 *
 *   sal: sal_num_stem_widths
 *        StorageAreaLocations::sal_stem_width_offset as u8
 *        StorageAreaLocations::sal_k as u8
 *        StorageAreaLocations::sal_limit as u8
 *        StorageAreaLocations::sal_have_cached_width as u8
 *        StorageAreaLocations::sal_cached_width_offset as u8
 */

const FPGM_bci_quantize_stem_width: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_quantize_stem_width as u8,
    FDEF,
    DUP,
    ABS, /* s: val |val| */
    PUSHB_4,
    StorageAreaLocations::sal_limit as u8,
    StorageAreaLocations::sal_stem_width_offset as u8,
    StorageAreaLocations::sal_have_cached_width as u8,
    0,
    WS, /* sal_have_cached_width = 0 */
    RS,
    PUSHB_1,
    StorageAreaLocations::sal_num_stem_widths as u8,
    RS,
    DUP,
    ADD,
    ADD, /* sal_limit = sal_stem_width_offset + 2 * sal_num_stem_widths */
    WS,
    PUSHB_2,
    StorageAreaLocations::sal_k as u8,
    StorageAreaLocations::sal_stem_width_offset as u8,
    RS,
    WS, /* sal_k = sal_stem_width_offset */
    /* start_loop: */
    PUSHB_2,
    37, /* not_in_array jump offset */
    StorageAreaLocations::sal_limit as u8,
    RS,
    PUSHB_1,
    StorageAreaLocations::sal_k as u8,
    RS,
    EQ,   /* sal_limit == sal_k ? */
    JROT, /* goto not_in_array */
    DUP,
    PUSHB_1,
    12, /* found_stem jump offset */
    SWAP,
    PUSHB_1,
    StorageAreaLocations::sal_k as u8,
    RS,
    RS, /* cur_stem_width = sal[sal_k] */
    SUB,
    ABS,
    PUSHB_1,
    4,
    LT,   /* |val - cur_stem_width| < 4 ? */
    JROT, /* goto found_stem */
    PUSHB_3,
    StorageAreaLocations::sal_k as u8,
    2,
    StorageAreaLocations::sal_k as u8,
    RS,
    ADD, /* sal_k = sal_k + 2, skipping associated cache value */
    WS,
    PUSHB_1,
    33, /* start_loop jump offset */
    NEG,
    JMPR, /* goto start_loop */
    /* found_stem: */
    POP, /* discard val */
    PUSHB_1,
    StorageAreaLocations::sal_k as u8,
    RS,
    RS, /* val = sal[sal_k] */
    PUSHB_3,
    14, /* exit jump offset */
    StorageAreaLocations::sal_have_cached_width as u8,
    1,
    WS,   /* sal_have_cached_width = 1 */
    JMPR, /* goto exit */
    /* not_in_array: */
    DUP,
    PUSHB_1,
    StorageAreaLocations::sal_k as u8,
    RS,
    SWAP,
    WS, /* sal[sal_k] = val */
    PUSHB_3,
    StorageAreaLocations::sal_num_stem_widths as u8,
    1,
    StorageAreaLocations::sal_num_stem_widths as u8,
    RS,
    ADD,
    WS, /* sal_num_stem_widths = sal_num_stem_widths + 1 */
    /* exit: */
    SWAP, /* s: |new_val| val */
    PUSHB_1,
    0,
    LT, /* val < 0 */
    IF,
    NEG, /* new_val = -new_val */
    EIF,
    /* for input width array index `n' we have (or soon will have) */
    /* a cached output width at array index `n + 1' */
    PUSHB_3,
    StorageAreaLocations::sal_cached_width_offset as u8,
    1,
    StorageAreaLocations::sal_k as u8,
    RS,
    ADD,
    WS, /* sal_cached_width_offset = sal_k + 1 */
    ENDF,
];

/*
 * bci_natural_stem_width
 *
 *   This dummy function only pops arguments as necessary.
 *
 * in: width
 *     stem_is_serif
 *     base_is_round
 *
 * out: width
 */

const FPGM_bci_natural_stem_width: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_natural_stem_width as u8,
    FDEF,
    SWAP,
    POP,
    SWAP,
    POP,
    ENDF,
];

/*
 * bci_smooth_stem_width
 *
 *   This is the equivalent to the following code from function
 *   `ta_latin_compute_stem_width':
 *
 *      dist = |width|
 *
 *      if (stem_is_serif
 *          && dist < 3*64)
 *         || std_width < 40:
 *        return width
 *      else if base_is_round:
 *        if dist < 80:
 *          dist = 64
 *      else if dist < 56:
 *        dist = 56
 *
 *      delta = |dist - std_width|
 *
 *      if delta < 40:
 *        dist = std_width
 *        if dist < 48:
 *          dist = 48
 *        goto End
 *
 *      if dist < 3*64:
 *        delta = dist
 *        dist = FLOOR(dist)
 *        delta = delta - dist
 *
 *        if delta < 10:
 *          dist = dist + delta
 *        else if delta < 32:
 *          dist = dist + 10
 *        else if delta < 54:
 *          dist = dist + 54
 *        else:
 *          dist = dist + delta
 *      else:
 *        bdelta = 0
 *
 *        if width * base_delta > 0:
 *          if ppem < 10:
 *            bdelta = base_delta
 *          else if ppem < 30:
 *            bdelta = (base_delta * (30 - ppem)) / 20
 *
 *          bdelta = |bdelta|
 *
 *        dist = ROUND(dist - bdelta)
 *
 *    End:
 *      if width < 0:
 *        dist = -dist
 *      return dist
 *
 *   If `cvtl_ignore_std_width' is set, we simply set `std_width'
 *   equal to `dist'.
 *
 *   If `sal_have_cached_width' is set (by `bci_quantize_stem_width'), the
 *   cached value given by `sal_cached_width_offset' is directly taken, not
 *   computing the width again.  Otherwise, the computed width gets stored
 *   at the given offset.
 *
 * in: width
 *     stem_is_serif
 *     base_is_round
 *
 * out: new_width
 *
 * sal: sal_vwidth_data_offset
 *      StorageAreaLocations::sal_base_delta as u8
 *      StorageAreaLocations::sal_have_cached_width as u8
 *      StorageAreaLocations::sal_cached_width_offset as u8
 *
 * CVT: std_width
 *      cvtl_ignore_std_width
 *
 * uses: bci_round
 *       FunctionNumbers::bci_quantize_stem_widt as u8h
 */

const FPGM_bci_smooth_stem_width: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_smooth_stem_width as u8,
    FDEF,
    PUSHB_1,
    FunctionNumbers::bci_quantize_stem_width as u8,
    CALL,
    PUSHB_1,
    StorageAreaLocations::sal_have_cached_width as u8,
    RS,
    IF,
    SWAP,
    POP,
    SWAP,
    POP,
    PUSHB_1,
    StorageAreaLocations::sal_cached_width_offset as u8,
    RS,
    RS, /* cached_width = sal[sal_cached_width_offset] */
    SWAP,
    PUSHB_1,
    0,
    LT, /* width < 0 */
    IF,
    NEG, /* cached_width = -cached_width */
    EIF,
    ELSE,
    DUP,
    ABS, /* s: base_is_round stem_is_serif width dist */
    DUP,
    PUSHB_1,
    3 * 64,
    LT, /* dist < 3*64 */
    PUSHB_1,
    4,
    MINDEX, /* s: base_is_round width dist (dist<3*64) stem_is_serif */
    AND,    /* stem_is_serif && dist < 3*64 */
    PUSHB_3,
    40,
    1,
    StorageAreaLocations::sal_vwidth_data_offset as u8,
    RS,
    RCVT, /* first indirection */
    MUL,  /* divide by 64 */
    RCVT, /* second indirection */
    PUSHB_1,
    CvtLocations::cvtl_ignore_std_width as u8,
    RCVT,
    IF,
    POP, /* s: ... dist (stem_is_serif && dist < 3*64) 40 */
    PUSHB_1,
    3,
    CINDEX, /* standard_width = dist */
    EIF,
    GT, /* standard_width < 40 */
    OR, /* (stem_is_serif && dist < 3*64) || standard_width < 40 */
    IF, /* s: base_is_round width dist */
    POP,
    SWAP,
    POP, /* s: width */
    ELSE,
    ROLL, /* s: width dist base_is_round */
    IF,   /* s: width dist */
    DUP,
    PUSHB_1,
    80,
    LT, /* dist < 80 */
    IF, /* s: width dist */
    POP,
    PUSHB_1,
    64, /* dist = 64 */
    EIF,
    ELSE,
    DUP,
    PUSHB_1,
    56,
    LT, /* dist < 56 */
    IF, /* s: width dist */
    POP,
    PUSHB_1,
    56, /* dist = 56 */
    EIF,
    EIF,
    DUP, /* s: width dist dist */
    PUSHB_2,
    1,
    StorageAreaLocations::sal_vwidth_data_offset as u8,
    RS,
    RCVT, /* first indirection */
    MUL,  /* divide by 64 */
    RCVT, /* second indirection */
    SUB,
    ABS, /* s: width dist delta */
    PUSHB_1,
    40,
    LT, /* delta < 40 */
    IF, /* s: width dist */
    POP,
    PUSHB_2,
    1,
    StorageAreaLocations::sal_vwidth_data_offset as u8,
    RS,
    RCVT, /* first indirection */
    MUL,  /* divide by 64 */
    RCVT, /* second indirection; dist = std_width */
    DUP,
    PUSHB_1,
    48,
    LT, /* dist < 48 */
    IF,
    POP,
    PUSHB_1,
    48, /* dist = 48 */
    EIF,
    ELSE,
    DUP, /* s: width dist dist */
    PUSHB_1,
    3 * 64,
    LT, /* dist < 3*64 */
    IF,
    DUP,   /* s: width delta dist */
    FLOOR, /* dist = FLOOR(dist) */
    DUP,   /* s: width delta dist dist */
    ROLL,
    ROLL, /* s: width dist delta dist */
    SUB,  /* delta = delta - dist */
    DUP,  /* s: width dist delta delta */
    PUSHB_1,
    10,
    LT,  /* delta < 10 */
    IF,  /* s: width dist delta */
    ADD, /* dist = dist + delta */
    ELSE,
    DUP,
    PUSHB_1,
    32,
    LT, /* delta < 32 */
    IF,
    POP,
    PUSHB_1,
    10,
    ADD, /* dist = dist + 10 */
    ELSE,
    DUP,
    PUSHB_1,
    54,
    LT, /* delta < 54 */
    IF,
    POP,
    PUSHB_1,
    54,
    ADD, /* dist = dist + 54 */
    ELSE,
    ADD, /* dist = dist + delta */
    EIF,
    EIF,
    EIF,
    ELSE,
    PUSHB_1,
    2,
    CINDEX, /* s: width dist width */
    PUSHB_1,
    StorageAreaLocations::sal_base_delta as u8,
    RS,
    MUL, /* s: width dist width*base_delta */
    PUSHB_1,
    0,
    GT, /* width * base_delta > 0 */
    IF,
    PUSHB_1,
    0, /* s: width dist bdelta */
    MPPEM,
    PUSHB_1,
    10,
    LT, /* ppem < 10 */
    IF,
    POP,
    PUSHB_1,
    StorageAreaLocations::sal_base_delta as u8,
    RS, /* bdelta = base_delta */
    ELSE,
    MPPEM,
    PUSHB_1,
    30,
    LT, /* ppem < 30 */
    IF,
    POP,
    PUSHB_1,
    30,
    MPPEM,
    SUB, /* 30 - ppem */
    PUSHW_1,
    0x10, /* 64 * 64 */
    0x00,
    MUL, /* (30 - ppem) in 26.6 format */
    PUSHB_1,
    StorageAreaLocations::sal_base_delta as u8,
    RS,
    MUL, /* base_delta * (30 - ppem) */
    PUSHW_1,
    0x05, /* 20 * 64 */
    0x00,
    DIV, /* bdelta = (base_delta * (30 - ppem)) / 20 */
    EIF,
    EIF,
    ABS, /* bdelta = |bdelta| */
    SUB, /* dist = dist - bdelta */
    EIF,
    PUSHB_1,
    FunctionNumbers::bci_round as u8,
    CALL, /* dist = round(dist) */
    EIF,
    EIF,
    SWAP, /* s: dist width */
    PUSHB_1,
    0,
    LT, /* width < 0 */
    IF,
    NEG, /* dist = -dist */
    EIF,
    EIF,
    DUP,
    ABS,
    PUSHB_1,
    StorageAreaLocations::sal_cached_width_offset as u8,
    RS,
    SWAP,
    WS, /* sal[sal_cached_width_offset] = |dist| */
    EIF,
    ENDF,
];

/*
 * bci_get_best_width
 *
 *   An auxiliary function for `bci_strong_stem_width'.
 *
 * in: n (initialized with CVT index for first vertical width)
 *     dist
 *
 * out: n+1
 *      dist
 *
 * sal: sal_best
 *      StorageAreaLocations::sal_ref as u8
 *
 * CVT: widths[]
 */

const FPGM_bci_get_best_width: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_get_best_width as u8,
    FDEF,
    DUP,
    RCVT, /* s: dist n w */
    DUP,
    PUSHB_1,
    4,
    CINDEX, /* s: dist n w w dist */
    SUB,
    ABS, /* s: dist n w d */
    DUP,
    PUSHB_1,
    StorageAreaLocations::sal_best as u8,
    RS, /* s: dist n w d d best */
    LT, /* d < best */
    IF,
    PUSHB_1,
    StorageAreaLocations::sal_best as u8,
    SWAP,
    WS, /* best = d */
    PUSHB_1,
    StorageAreaLocations::sal_ref as u8,
    SWAP,
    WS, /* reference = w */
    ELSE,
    POP,
    POP,
    EIF,
    PUSHB_1,
    1,
    ADD, /* n = n + 1 */
    ENDF,
];

/*
 * bci_strong_stem_width
 *
 *   This is the equivalent to the following code (function
 *   `ta_latin_snap_width' and some lines from
 *   `ta_latin_compute_stem_width'):
 *
 *     best = 64 + 32 + 2
 *     dist = |width|
 *     reference = dist
 *
 *     for n in 0 .. num_widths:
 *       w = widths[n]
 *       d = |dist - w|
 *
 *       if d < best:
 *         best = d
 *         reference = w
 *
 *     if dist >= reference:
 *       if dist < ROUND(reference) + 48:
 *         dist = reference
 *     else:
 *       if dist > ROUND(reference) - 48:
 *         dist = reference
 *
 *     if dist >= 64:
 *       dist = ROUND(dist)
 *     else:
 *       dist = 64
 *
 *     if width < 0:
 *       dist = -dist
 *     return dist
 *
 *   If `cvtl_ignore_std_width' is set, we leave `reference = width'.
 *
 * in: width
 *     stem_is_serif (unused)
 *     base_is_round (unused)
 *
 * out: new_width
 *
 * sal: sal_best
 *      StorageAreaLocations::sal_ref as u8
 *      StorageAreaLocations::sal_vwidth_data_offset as u8
 *
 * CVT: widths[]
 *      cvtl_ignore_std_width
 *
 * uses: bci_get_best_width
 *       FunctionNumbers::bci_roun as u8d
 *       FunctionNumbers::bci_quantize_stem_widt as u8h
 */

const FPGM_bci_strong_stem_width_a: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_strong_stem_width as u8,
    FDEF,
    SWAP,
    POP,
    SWAP,
    POP,
    PUSHB_1,
    FunctionNumbers::bci_quantize_stem_width as u8,
    CALL,
    DUP,
    ABS, /* s: width dist */
    PUSHB_2,
    StorageAreaLocations::sal_best as u8,
    64 + 32 + 2,
    WS, /* sal_best = 98 */
    DUP,
    PUSHB_1,
    StorageAreaLocations::sal_ref as u8,
    SWAP,
    WS, /* sal_ref = width */
    PUSHB_1,
    CvtLocations::cvtl_ignore_std_width as u8,
    RCVT,
    IF,
    ELSE,
    /* s: width dist */
    PUSHB_2,
    1,
    StorageAreaLocations::sal_vwidth_data_offset as u8,
    RS,
    RCVT,
    MUL, /* divide by 64; first index of vertical widths */
    /* s: width dist vw_idx */
    PUSHB_2,
    1,
    StorageAreaLocations::sal_vwidth_data_offset as u8,
    RS,
    PUSHB_1,
];

/*    %c, number of used styles */

const FPGM_bci_strong_stem_width_b: &[u8] = &bytes![
    ADD,
    RCVT, /* number of vertical widths */
    MUL,  /* divide by 64 */
    /* s: width dist vw_idx loop_count */
    PUSHB_1,
    FunctionNumbers::bci_get_best_width as u8,
    LOOPCALL,
    /* clean up stack */
    POP, /* s: width dist */
    DUP,
    PUSHB_1,
    StorageAreaLocations::sal_ref as u8,
    RS, /* s: width dist dist reference */
    DUP,
    ROLL,
    DUP,
    ROLL,
    PUSHB_1,
    FunctionNumbers::bci_round as u8,
    CALL, /* s: width dist reference dist dist ROUND(reference) */
    PUSHB_2,
    48,
    5,
    CINDEX, /* s: width dist reference dist dist ROUND(reference) 48 reference */
    PUSHB_1,
    4,
    MINDEX, /* s: width dist reference dist ROUND(reference) 48 reference dist */
    LTEQ,   /* reference <= dist */
    IF,     /* s: width dist reference dist ROUND(reference) 48 */
    ADD,
    LT, /* dist < ROUND(reference) + 48 */
    ELSE,
    SUB,
    GT, /* dist > ROUND(reference) - 48 */
    EIF,
    IF,
    SWAP, /* s: width reference=new_dist dist */
    EIF,
    POP,
    EIF, /* !cvtl_ignore_std_width */
    DUP, /* s: width dist dist */
    PUSHB_1,
    64,
    GTEQ, /* dist >= 64 */
    IF,
    PUSHB_1,
    FunctionNumbers::bci_round as u8,
    CALL, /* dist = ROUND(dist) */
    ELSE,
    POP,
    PUSHB_1,
    64, /* dist = 64 */
    EIF,
    SWAP, /* s: dist width */
    PUSHB_1,
    0,
    LT, /* width < 0 */
    IF,
    NEG, /* dist = -dist */
    EIF,
    ENDF,
];

/*
 * bci_do_loop
 *
 *   An auxiliary function for `bci_loop'.
 *
 * sal: sal_i (gets incremented by 2 after execution)
 *      StorageAreaLocations::sal_func as u8
 *
 * uses: func[sal_func]
 */

const FPGM_bci_loop_do: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_loop_do as u8,
    FDEF,
    PUSHB_1,
    StorageAreaLocations::sal_func as u8,
    RS,
    CALL,
    PUSHB_3,
    StorageAreaLocations::sal_i as u8,
    2,
    StorageAreaLocations::sal_i as u8,
    RS,
    ADD, /* sal_i = sal_i + 2 */
    WS,
    ENDF,
];

/*
 * bci_loop
 *
 *   Take a range `start'..`end' and a function number and apply the
 *   associated function to the range elements `start', `start+2',
 *   `start+4', ...
 *
 * in: func_num
 *     end
 *     start
 *
 * sal: sal_i (counter initialized with `start')
 *      StorageAreaLocations::sal_func as u8 (`func_num')
 *
 * uses: bci_loop_do
 */

fn emit_bci_loop(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_loop as u8,
        FDEF,
        PUSHB_1,
        StorageAreaLocations::sal_func as u8,
        SWAP,
        WS, /* sal_func = func_num */
        SWAP,
        DUP,
        PUSHB_1,
        StorageAreaLocations::sal_i as u8,
        SWAP,
        WS, /* sal_i = start */
        SUB,
    ]);

    bytecode.extend_bytes(DIV_POS_BY_2);
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        1,
        ADD, /* number of loops ((end - start) / 2 + 1) */
        PUSHB_1,
        FunctionNumbers::bci_loop_do as u8,
        LOOPCALL,
        ENDF,
    ]);
}

/*
 * bci_cvt_rescale
 *
 *   Rescale CVT value by `sal_scale' (in 16.16 format).
 *
 *   The scaling factor `sal_scale' isn't stored as `b/c' but as `(b-c)/c';
 *   consequently, the calculation `a * b/c' is done as `a + delta' with
 *   `delta = a * (b-c)/c'.  This avoids overflow.
 *
 * in: cvt_idx
 *
 * out: cvt_idx+1
 *
 * sal: sal_scale
 */

fn emit_bci_cvt_rescale(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_cvt_rescale,
        FDEF,
        DUP,
        DUP,
        RCVT,
    ]);
    bytecode.extend_bytes(DO_SCALE);
    bytecode.extend_bytes(&bytes![WCVTP, PUSHB_1, 1, ADD, ENDF,])
}

/*
 * bci_cvt_rescale_range
 *
 *   Rescale a range of CVT values with `bci_cvt_rescale', using a custom
 *   scaling value.
 *
 *   This function gets used in the `prep' table.
 *
 * in: num_cvt
 *     cvt_start_idx
 *
 * sal: sal_i (CVT index of the style's scaling value;
 *             gets incremented by 1 after execution)
 *      StorageAreaLocations::sal_scale as u8
 *
 * uses: bci_cvt_rescale
 */

const FPGM_bci_cvt_rescale_range: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_cvt_rescale_range as u8,
    FDEF,
    /* store scaling value in `sal_scale' */
    PUSHB_3,
    FunctionNumbers::bci_cvt_rescale as u8,
    StorageAreaLocations::sal_scale as u8,
    StorageAreaLocations::sal_i as u8,
    RS,
    RCVT,
    WS, /* s: cvt_start_idx num_cvt bci_cvt_rescale */
    LOOPCALL,
    /* clean up stack */
    POP,
    PUSHB_3,
    StorageAreaLocations::sal_i as u8,
    1,
    StorageAreaLocations::sal_i as u8,
    RS,
    ADD, /* sal_i = sal_i + 1 */
    WS,
    ENDF,
];

/*
 * bci_vwidth_data_store
 *
 *   Store a vertical width array value.
 *
 *   This function gets used in the `prep' table.
 *
 * in: value
 *
 * sal: sal_i (CVT index of the style's vwidth data;
 *             gets incremented by 1 after execution)
 */

const FPGM_bci_vwidth_data_store: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_vwidth_data_store as u8,
    FDEF,
    PUSHB_1,
    StorageAreaLocations::sal_i as u8,
    RS,
    SWAP,
    WCVTP,
    PUSHB_3,
    StorageAreaLocations::sal_i as u8,
    1,
    StorageAreaLocations::sal_i as u8,
    RS,
    ADD, /* sal_i = sal_i + 1 */
    WS,
    ENDF,
];

/*
 * bci_smooth_blue_round
 *
 *   Round a blue ref value and adjust its corresponding shoot value.
 *
 *   This is the equivalent to the following code (function
 *   `ta_latin_metrics_scale_dim'):
 *
 *     delta = dist
 *     if dist < 0:
 *       delta = -delta
 *
 *     if delta < 32:
 *       delta = 0
 *     else if delta < 48:
 *       delta = 32
 *     else:
 *       delta = 64
 *
 *     if dist < 0:
 *       delta = -delta
 *
 * in: ref_idx
 *
 * sal: sal_i (number of blue zones)
 *
 * out: ref_idx+1
 *
 * uses: bci_round
 */

const FPGM_bci_smooth_blue_round: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_smooth_blue_round as u8,
    FDEF,
    DUP,
    DUP,
    RCVT, /* s: ref_idx ref_idx ref */
    DUP,
    PUSHB_1,
    FunctionNumbers::bci_round as u8,
    CALL,
    SWAP, /* s: ref_idx ref_idx round(ref) ref */
    PUSHB_1,
    StorageAreaLocations::sal_i as u8,
    RS,
    PUSHB_1,
    4,
    CINDEX,
    ADD, /* s: ref_idx ref_idx round(ref) ref shoot_idx */
    DUP,
    RCVT, /* s: ref_idx ref_idx round(ref) ref shoot_idx shoot */
    ROLL, /* s: ref_idx ref_idx round(ref) shoot_idx shoot ref */
    SWAP,
    SUB, /* s: ref_idx ref_idx round(ref) shoot_idx dist */
    DUP,
    ABS, /* s: ref_idx ref_idx round(ref) shoot_idx dist delta */
    DUP,
    PUSHB_1,
    32,
    LT, /* delta < 32 */
    IF,
    POP,
    PUSHB_1,
    0, /* delta = 0 */
    ELSE,
    PUSHB_1,
    48,
    LT, /* delta < 48 */
    IF,
    PUSHB_1,
    32, /* delta = 32 */
    ELSE,
    PUSHB_1,
    64, /* delta = 64 */
    EIF,
    EIF,
    SWAP, /* s: ref_idx ref_idx round(ref) shoot_idx delta dist */
    PUSHB_1,
    0,
    LT, /* dist < 0 */
    IF,
    NEG, /* delta = -delta */
    EIF,
    PUSHB_1,
    3,
    CINDEX,
    SWAP,
    SUB, /* s: ref_idx ref_idx round(ref) shoot_idx (round(ref) - delta) */
    WCVTP,
    WCVTP,
    PUSHB_1,
    1,
    ADD, /* s: (ref_idx + 1) */
    ENDF,
];

/*
 * bci_strong_blue_round
 *
 *   Round a blue ref value and adjust its corresponding shoot value.
 *
 *   This is the equivalent to the following code:
 *
 *     delta = dist
 *     if dist < 0:
 *       delta = -delta
 *
 *     if delta < 36:
 *       delta = 0
 *     else:
 *       delta = 64
 *
 *     if dist < 0:
 *       delta = -delta
 *
 *   It doesn't have corresponding code in talatin.c; however, some tests
 *   have shown that the `smooth' code works just fine for this case also.
 *
 * in: ref_idx
 *
 * sal: sal_i (number of blue zones)
 *
 * out: ref_idx+1
 *
 * uses: bci_round
 */

const FPGM_bci_strong_blue_round: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_strong_blue_round as u8,
    FDEF,
    DUP,
    DUP,
    RCVT, /* s: ref_idx ref_idx ref */
    DUP,
    PUSHB_1,
    FunctionNumbers::bci_round as u8,
    CALL,
    SWAP, /* s: ref_idx ref_idx round(ref) ref */
    PUSHB_1,
    StorageAreaLocations::sal_i as u8,
    RS,
    PUSHB_1,
    4,
    CINDEX,
    ADD, /* s: ref_idx ref_idx round(ref) ref shoot_idx */
    DUP,
    RCVT, /* s: ref_idx ref_idx round(ref) ref shoot_idx shoot */
    ROLL, /* s: ref_idx ref_idx round(ref) shoot_idx shoot ref */
    SWAP,
    SUB, /* s: ref_idx ref_idx round(ref) shoot_idx dist */
    DUP,
    ABS, /* s: ref_idx ref_idx round(ref) shoot_idx dist delta */
    PUSHB_1,
    36,
    LT, /* delta < 36 */
    IF,
    PUSHB_1,
    0, /* delta = 0 (set overshoot to zero if < 0.56 pixel units) */
    ELSE,
    PUSHB_1,
    64, /* delta = 64 (one pixel unit) */
    EIF,
    SWAP, /* s: ref_idx ref_idx round(ref) shoot_idx delta dist */
    PUSHB_1,
    0,
    LT, /* dist < 0 */
    IF,
    NEG, /* delta = -delta */
    EIF,
    PUSHB_1,
    3,
    CINDEX,
    SWAP,
    SUB, /* s: ref_idx ref_idx round(ref) shoot_idx (round(ref) - delta) */
    WCVTP,
    WCVTP,
    PUSHB_1,
    1,
    ADD, /* s: (ref_idx + 1) */
    ENDF,
];

/*
 * bci_blue_round_range
 *
 *   Round a range of blue zones (both reference and shoot values).
 *
 *   This function gets used in the `prep' table.
 *
 * in: num_blue_zones
 *     blue_ref_idx
 *
 * sal: sal_i (holds a copy of `num_blue_zones' for blue rounding function)
 *
 * uses: bci_smooth_blue_round
 *       FunctionNumbers::bci_strong_blue_roun as u8d
 */

const FPGM_bci_blue_round_range: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_blue_round_range as u8,
    FDEF,
    DUP,
    PUSHB_1,
    StorageAreaLocations::sal_i as u8,
    SWAP,
    WS,
    /* select blue rounding function based on flag in CVT; */
    /* for value >0 we use strong mode, else smooth mode */
    PUSHB_4,
    FunctionNumbers::bci_strong_blue_round as u8,
    FunctionNumbers::bci_smooth_blue_round as u8,
    0,
    CvtLocations::cvtl_stem_width_mode as u8,
    RCVT,
    LT,
    IF,
    POP,
    ELSE,
    SWAP,
    POP,
    EIF,
    LOOPCALL,
    /* clean up stack */
    POP,
    ENDF,
];

/*
 * bci_decrement_component_counter
 *
 *   An auxiliary function for composite glyphs.
 *
 * CVT: cvtl_is_subglyph
 */

const FPGM_bci_decrement_component_counter: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_decrement_component_counter as u8,
    FDEF,
    /* decrement `cvtl_is_subglyph' counter */
    PUSHB_2,
    CvtLocations::cvtl_is_subglyph as u8,
    CvtLocations::cvtl_is_subglyph as u8,
    RCVT,
    PUSHB_1,
    100,
    SUB,
    WCVTP,
    ENDF,
];

/*
 * bci_get_point_extrema
 *
 *   An auxiliary function for `bci_create_segment'.
 *
 * in: point-1
 *
 * out: point
 *
 * sal: sal_point_min
 *      StorageAreaLocations::sal_point_max as u8
 */

const FPGM_bci_get_point_extrema: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_get_point_extrema as u8,
    FDEF,
    PUSHB_1,
    1,
    ADD, /* s: point */
    DUP,
    DUP,
    /* check whether `point' is a new minimum */
    PUSHB_1,
    StorageAreaLocations::sal_point_min as u8,
    RS, /* s: point point point point_min */
    MD_orig,
    /* if distance is negative, we have a new minimum */
    PUSHB_1,
    0,
    LT,
    IF, /* s: point point */
    DUP,
    PUSHB_1,
    StorageAreaLocations::sal_point_min as u8,
    SWAP,
    WS,
    EIF,
    /* check whether `point' is a new maximum */
    PUSHB_1,
    StorageAreaLocations::sal_point_max as u8,
    RS, /* s: point point point_max */
    MD_orig,
    /* if distance is positive, we have a new maximum */
    PUSHB_1,
    0,
    GT,
    IF, /* s: point */
    DUP,
    PUSHB_1,
    StorageAreaLocations::sal_point_max as u8,
    SWAP,
    WS,
    EIF, /* s: point */
    ENDF,
];

/*
 * bci_nibbles
 *
 *   Pop a byte with two delta arguments in its nibbles and push the
 *   expanded arguments separately as two bytes.
 *
 * in: 16 * (end - start) + (start - base)
 *
 * out: start
 *      end
 *
 * sal: sal_base (set to `end' at return)
 */

const FPGM_bci_nibbles: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_nibbles as u8,
    FDEF,
    DUP,
    PUSHB_1, /* cf. DIV_POS_BY_2 macro */
    16,
    DIV,
    FLOOR,
    PUSHB_1,
    1,
    MUL, /* s: in hnibble */
    DUP,
    PUSHW_1,
    0x04, /* 16*64 */
    0x00,
    MUL, /* s: in hnibble (hnibble * 16) */
    ROLL,
    SWAP,
    SUB, /* s: hnibble lnibble */
    PUSHB_1,
    StorageAreaLocations::sal_base as u8,
    RS,
    ADD, /* s: hnibble start */
    DUP,
    ROLL,
    ADD, /* s: start end */
    DUP,
    PUSHB_1,
    StorageAreaLocations::sal_base as u8,
    SWAP,
    WS, /* sal_base = end */
    SWAP,
    ENDF,
];

/*
 * bci_number_set_is_element
 *
 *   Pop values from stack until it is empty.  If one of them is equal to
 *   the current PPEM value, set `cvtl_is_element' to 100 (and to 0
 *   otherwise).
 *
 * in: ppem_value_1
 *     ppem_value_2
 *     ...
 *
 * CVT: cvtl_is_element
 */

const FPGM_bci_number_set_is_element: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_number_set_is_element as u8,
    FDEF,
    /* start_loop: */
    MPPEM,
    EQ,
    IF,
    PUSHB_2,
    CvtLocations::cvtl_is_element as u8,
    100,
    WCVTP,
    EIF,
    DEPTH,
    PUSHB_1,
    13,
    NEG,
    SWAP,
    JROT, /* goto start_loop if stack depth != 0 */
    ENDF,
];

/*
 * bci_number_set_is_element2
 *
 *   Pop value ranges from stack until it is empty.  If one of them contains
 *   the current PPEM value, set `cvtl_is_element' to 100 (and to 0
 *   otherwise).
 *
 * in: ppem_range_1_start
 *     ppem_range_1_end
 *     ppem_range_2_start
 *     ppem_range_2_end
 *     ...
 *
 * CVT: cvtl_is_element
 */

const FPGM_bci_number_set_is_element2: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_number_set_is_element2 as u8,
    FDEF,
    /* start_loop: */
    MPPEM,
    LTEQ,
    IF,
    MPPEM,
    GTEQ,
    IF,
    PUSHB_2,
    CvtLocations::cvtl_is_element as u8,
    100,
    WCVTP,
    EIF,
    ELSE,
    POP,
    EIF,
    DEPTH,
    PUSHB_1,
    19,
    NEG,
    SWAP,
    JROT, /* goto start_loop if stack depth != 0 */
    ENDF,
];

/*
 * bci_create_segment
 *
 *   Store start and end point of a segment in the storage area,
 *   then construct a point in the twilight zone to represent it.
 *
 *   This function is used by `bci_create_segments'.
 *
 * in: start
 *     end
 *       [last (if wrap-around segment)]
 *       [first (if wrap-around segment)]
 *
 * sal: sal_i (start of current segment)
 *      StorageAreaLocations::sal_j as u8 (current twilight point)
 *      StorageAreaLocations::sal_point_min as u8
 *      StorageAreaLocations::sal_point_max as u8
 *      StorageAreaLocations::sal_base as u8
 *      StorageAreaLocations::sal_num_packed_segments as u8
 *      StorageAreaLocations::sal_scale as u8
 *
 * CVT: cvtl_temp
 *
 * uses: bci_get_point_extrema
 *       FunctionNumbers::bci_nibble as u8s
 *
 * If `sal_num_packed_segments' is > 0, the start/end pair is stored as
 * delta values in nibbles (without a wrap-around segment).
 */

fn emit_bci_create_segment(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_create_segment as u8,
        FDEF,
        PUSHB_2,
        0,
        StorageAreaLocations::sal_num_packed_segments as u8,
        RS,
        NEQ,
        IF,
        PUSHB_2,
        StorageAreaLocations::sal_num_packed_segments as u8,
        StorageAreaLocations::sal_num_packed_segments as u8,
        RS,
        PUSHB_1,
        1,
        SUB,
        WS, /* sal_num_packed_segments = sal_num_packed_segments - 1 */
        PUSHB_1,
        FunctionNumbers::bci_nibbles as u8,
        CALL,
        EIF,
        PUSHB_1,
        StorageAreaLocations::sal_i as u8,
        RS,
        PUSHB_1,
        2,
        CINDEX,
        WS, /* sal[sal_i] = start */
        /* initialize inner loop(s) */
        PUSHB_2,
        StorageAreaLocations::sal_point_min as u8,
        2,
        CINDEX,
        WS, /* sal_point_min = start */
        PUSHB_2,
        StorageAreaLocations::sal_point_max as u8,
        2,
        CINDEX,
        WS, /* sal_point_max = start */
        PUSHB_1,
        1,
        SZPS, /* set zp0, zp1, and zp2 to normal zone 1 */
        SWAP,
        DUP,
        PUSHB_1,
        3,
        CINDEX, /* s: start end end start */
        LT,     /* start > end */
        IF,
        /* we have a wrap-around segment with two more arguments */
        /* to give the last and first point of the contour, respectively; */
        /* our job is to store a segment `start'-`last', */
        /* and to get extrema for the two segments */
        /* `start'-`last' and `first'-`end' */

        /* s: first last start end */
        PUSHB_2,
        1,
        StorageAreaLocations::sal_i as u8,
        RS,
        ADD,
        PUSHB_1,
        4,
        CINDEX,
        WS, /* sal[sal_i + 1] = last */
        ROLL,
        ROLL, /* s: first end last start */
        DUP,
        ROLL,
        SWAP, /* s: first end start last start */
        SUB,  /* s: first end start loop_count */
        PUSHB_1,
        FunctionNumbers::bci_get_point_extrema as u8,
        LOOPCALL,
        /* clean up stack */
        POP,
        SWAP, /* s: end first */
        PUSHB_1,
        1,
        SUB,
        DUP,
        ROLL, /* s: (first - 1) (first - 1) end */
        SWAP,
        SUB, /* s: (first - 1) loop_count */
        PUSHB_1,
        FunctionNumbers::bci_get_point_extrema as u8,
        LOOPCALL,
        /* clean up stack */
        POP,
        ELSE, /* s: start end */
        PUSHB_2,
        1,
        StorageAreaLocations::sal_i as u8,
        RS,
        ADD,
        PUSHB_1,
        2,
        CINDEX,
        WS, /* sal[sal_i + 1] = end */
        PUSHB_1,
        2,
        CINDEX,
        SUB, /* s: start loop_count */
        PUSHB_1,
        FunctionNumbers::bci_get_point_extrema as u8,
        LOOPCALL,
        /* clean up stack */
        POP,
        EIF,
        /* the twilight point representing a segment */
        /* is in the middle between the minimum and maximum */
        PUSHB_1,
        StorageAreaLocations::sal_point_min as u8,
        RS,
        GC_orig,
        PUSHB_1,
        StorageAreaLocations::sal_point_max as u8,
        RS,
        GC_orig,
        ADD
    ]);
    bytecode.extend_bytes(DIV_BY_2); /* s: middle_pos */
    bytecode.extend_bytes(DO_SCALE); /* middle_pos = middle_pos * scale */
    bytecode.extend_bytes(&bytes![
        /* write it to temporary CVT location */
        PUSHB_2,
        CvtLocations::cvtl_temp as u8,
        0,
        SZP0, /* set zp0 to twilight zone 0 */
        SWAP,
        WCVTP,
        /* create twilight point with index `sal_j' */
        PUSHB_1,
        StorageAreaLocations::sal_j as u8,
        RS,
        PUSHB_1,
        CvtLocations::cvtl_temp as u8,
        MIAP_noround,
        PUSHB_3,
        StorageAreaLocations::sal_j as u8,
        1,
        StorageAreaLocations::sal_j as u8,
        RS,
        ADD, /* twilight_point = twilight_point + 1 */
        WS,
        ENDF,
    ]);
}

/*
 * bci_create_segments
 *
 *   This is the top-level entry function.
 *
 *   It pops point ranges from the stack to define segments, computes
 *   twilight points to represent segments, and finally calls
 *   `bci_hint_glyph' to handle the rest.
 *
 *   The second argument (`data_offset') addresses three CVT arrays in
 *   parallel:
 *
 *     CVT(data_offset):
 *       the current style's scaling value (stored in `sal_scale')
 *
 *     data_offset + num_used_styles:
 *       offset to the current style's vwidth index array (this value gets
 *       stored in `sal_vwidth_data_offset')
 *
 *     data_offset + 2*num_used_styles:
 *       offset to the current style's vwidth size
 *
 *   This addressing scheme ensures that (a) we only need a single argument,
 *   and (b) this argument supports up to (256-cvtl_max_runtime) styles,
 *   which should be sufficient for a long time.
 *
 * in: num_packed_segments
 *     data_offset
 *     num_segments (N)
 *     segment_start_0
 *     segment_end_0
 *       [contour_last 0 (if wrap-around segment)]
 *       [contour_first 0 (if wrap-around segment)]
 *     segment_start_1
 *     segment_end_1
 *       [contour_last 0 (if wrap-around segment)]
 *       [contour_first 0 (if wrap-around segment)]
 *     ...
 *     segment_start_(N-1)
 *     segment_end_(N-1)
 *       [contour_last (N-1) (if wrap-around segment)]
 *       [contour_first (N-1) (if wrap-around segment)]
 *     ... stuff for bci_hint_glyph ...
 *
 * sal: sal_i (start of current segment)
 *      StorageAreaLocations::sal_j as u8 (current twilight point)
 *      StorageAreaLocations::sal_num_packed_segments as u8
 *      StorageAreaLocations::sal_base as u8 (the base for delta values in nibbles)
 *      StorageAreaLocations::sal_vwidth_data_offset as u8
 *      StorageAreaLocations::sal_stem_width_offset as u8
 *      StorageAreaLocations::sal_scale as u8
 *
 * CVT: cvtl_is_subglyph
 *
 * uses: bci_create_segment
 *       FunctionNumbers::bci_loo as u8p
 *       FunctionNumbers::bci_hint_glyp as u8h
 *
 * If `num_packed_segments' is set to p, the first p start/end pairs are
 * stored as delta values in nibbles, with the `start' delta in the lower
 * nibble (and there are no wrap-around segments).  For example, if the
 * first three pairs are 1/3, 5/8, and 12/13, the topmost three bytes on the
 * stack are 0x21, 0x32, and 0x14.
 *
 */

const FPGM_bci_create_segments_a: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments as u8,
    FDEF,
    /* all our measurements are taken along the y axis */
    SVTCA_y,
    /* only do something if we are not a subglyph */
    PUSHB_2,
    0,
    CvtLocations::cvtl_is_subglyph as u8,
    RCVT,
    EQ,
    IF,
    PUSHB_1,
    StorageAreaLocations::sal_num_packed_segments as u8,
    SWAP,
    WS,
    DUP,
    RCVT,
    PUSHB_1,
    StorageAreaLocations::sal_scale as u8, /* sal_scale = CVT(data_offset) */
    SWAP,
    WS,
    PUSHB_1,
    StorageAreaLocations::sal_vwidth_data_offset as u8,
    SWAP,
    PUSHB_1,
];

/*  %c, number of used styles */

const FPGM_bci_create_segments_b: &[u8] = &bytes![
    ADD,
    WS, /* sal_vwidth_data_offset = data_offset + num_used_styles */
    DUP,
    ADD, /* delta = 2*num_segments */
    PUSHB_8,
    StorageAreaLocations::sal_segment_offset as u8,
    StorageAreaLocations::sal_segment_offset as u8,
    StorageAreaLocations::sal_j as u8,
    0,
    StorageAreaLocations::sal_base as u8,
    0,
    StorageAreaLocations::sal_num_stem_widths as u8,
    0,
    WS, /* sal_num_stem_widths = 0 */
    WS, /* sal_base = 0 */
    WS, /* sal_j = 0 (point offset) */
    ROLL,
    ADD, /* s: ... sal_segment_offset (sal_segment_offset + delta) */
    DUP,
    PUSHB_1,
    StorageAreaLocations::sal_stem_width_offset as u8,
    SWAP,
    WS, /* sal_stem_width_offset = sal_segment_offset + delta */
    PUSHB_1,
    1,
    SUB, /* s: ... sal_segment_offset (sal_segment_offset + delta - 1) */
    PUSHB_2,
    FunctionNumbers::bci_create_segment as u8,
    FunctionNumbers::bci_loop as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_hint_glyph as u8,
    CALL,
];

/* used if we have delta exceptions */

const FPGM_bci_create_segments_c: &[u8] = &bytes![PUSHB_1, 1, SZPS];

const FPGM_bci_create_segments_d: &[u8] = &bytes![ELSE, CLEAR, EIF, ENDF];

/*
 * bci_create_segments_X
 *
 * Top-level routines for calling `bci_create_segments'.
 */

const FPGM_bci_create_segments_0: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_0 as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_create_segments as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_1: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_1 as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_create_segments as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_2: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_2 as u8,
    FDEF,
    PUSHB_2,
    2,
    FunctionNumbers::bci_create_segments as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_3: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_3 as u8,
    FDEF,
    PUSHB_2,
    3,
    FunctionNumbers::bci_create_segments as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_4: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_4 as u8,
    FDEF,
    PUSHB_2,
    4,
    FunctionNumbers::bci_create_segments as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_5: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_5 as u8,
    FDEF,
    PUSHB_2,
    5,
    FunctionNumbers::bci_create_segments as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_6: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_6 as u8,
    FDEF,
    PUSHB_2,
    6,
    FunctionNumbers::bci_create_segments as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_7: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_7 as u8,
    FDEF,
    PUSHB_2,
    7,
    FunctionNumbers::bci_create_segments as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_8: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_8 as u8,
    FDEF,
    PUSHB_2,
    8,
    FunctionNumbers::bci_create_segments as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_9: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_9 as u8,
    FDEF,
    PUSHB_2,
    9,
    FunctionNumbers::bci_create_segments as u8,
    CALL,
    ENDF,
];

/*
 * bci_deltapX
 *
 *   Wrapper functions around DELTAP[123] that touch the affected points
 *   before applying the delta.  This is necessary for ClearType.
 *
 *   While DELTAP[123] implicitly do a loop, we have to process the
 *   arguments sequentially by calling `bci_deltaX' with LOOPCALL.
 *
 * in: point
 *     arg
 */

const FPGM_bci_deltap1: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_deltap1 as u8,
    FDEF,
    SWAP,
    DUP,     /* s: point arg arg */
    PUSHB_1, /* cf. DIV_POS_BY_2 macro */
    16,
    DIV,
    FLOOR,
    PUSHB_1,
    1,
    MUL, /* s: point arg hnibble(arg) */
    PUSHB_1,
    CONTROL_DELTA_PPEM_MIN,
    ADD, /* s: point arg ppem(arg) */
    MPPEM,
    EQ,
    IF,
    SWAP,
    DUP,
    MDAP_noround, /* touch `point' */
    PUSHB_1,
    1,
    DELTAP1, /* process one `(point,arg)' pair */
    ELSE,
    POP,
    POP,
    EIF,
    ENDF,
];

const FPGM_bci_deltap2: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_deltap2 as u8,
    FDEF,
    SWAP,
    DUP,     /* s: point arg arg */
    PUSHB_1, /* cf. DIV_POS_BY_2 macro */
    16,
    DIV,
    FLOOR,
    PUSHB_1,
    1,
    MUL, /* s: point arg hnibble(arg) */
    PUSHB_1,
    CONTROL_DELTA_PPEM_MIN + 16,
    ADD, /* s: point arg ppem(arg) */
    MPPEM,
    EQ,
    IF,
    SWAP,
    DUP,
    MDAP_noround, /* touch `point' */
    PUSHB_1,
    1,
    DELTAP2, /* process one `(point,arg)' pair */
    ELSE,
    POP,
    POP,
    EIF,
    ENDF,
];

const FPGM_bci_deltap3: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_deltap3 as u8,
    FDEF,
    SWAP,
    DUP,     /* s: point arg arg */
    PUSHB_1, /* cf. DIV_POS_BY_2 macro */
    16,
    DIV,
    FLOOR,
    PUSHB_1,
    1,
    MUL, /* s: point arg hnibble(arg) */
    PUSHB_1,
    CONTROL_DELTA_PPEM_MIN + 32,
    ADD, /* s: point arg ppem(arg) */
    MPPEM,
    EQ,
    IF,
    SWAP,
    DUP,
    MDAP_noround, /* touch `point' */
    PUSHB_1,
    1,
    DELTAP3, /* process one `(point,arg)' pair */
    ELSE,
    POP,
    POP,
    EIF,
    ENDF,
];

/*
 * bci_create_segments_composite
 *
 *   The same as `bci_create_segments'.
 *   It also decrements the composite component counter.
 *
 * sal: sal_num_packed_segments
 *      StorageAreaLocations::sal_segment_offset as u8
 *      StorageAreaLocations::sal_vwidth_data_offset as u8
 *
 * CVT: cvtl_is_subglyph
 *
 * uses: bci_decrement_component_counter
 *       FunctionNumbers::bci_create_segmen as u8t
 *       FunctionNumbers::bci_loo as u8p
 *       FunctionNumbers::bci_hint_glyp as u8h
 */

const FPGM_bci_create_segments_composite_a: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_composite as u8,
    FDEF,
    /* all our measurements are taken along the y axis */
    SVTCA_y,
    PUSHB_1,
    FunctionNumbers::bci_decrement_component_counter as u8,
    CALL,
    /* only do something if we are not a subglyph */
    PUSHB_2,
    0,
    CvtLocations::cvtl_is_subglyph as u8,
    RCVT,
    EQ,
    IF,
    PUSHB_1,
    StorageAreaLocations::sal_num_packed_segments as u8,
    SWAP,
    WS,
    DUP,
    RCVT,
    PUSHB_1,
    StorageAreaLocations::sal_scale as u8, /* sal_scale = CVT(data_offset) */
    SWAP,
    WS,
    PUSHB_1,
    StorageAreaLocations::sal_vwidth_data_offset as u8,
    SWAP,
    PUSHB_1,
];

/*  %c, number of used styles */

const FPGM_bci_create_segments_composite_b: &[u8] = &bytes![
    ADD,
    WS, /* sal_vwidth_data_offset = data_offset + num_used_styles */
    DUP,
    ADD,
    PUSHB_1,
    1,
    SUB, /* delta = (2*num_segments - 1) */
    PUSHB_6,
    StorageAreaLocations::sal_segment_offset as u8,
    StorageAreaLocations::sal_segment_offset as u8,
    StorageAreaLocations::sal_j as u8,
    0,
    StorageAreaLocations::sal_base as u8,
    0,
    WS, /* sal_base = 0 */
    WS, /* sal_j = 0 (point offset) */
    ROLL,
    ADD, /* s: ... sal_segment_offset (sal_segment_offset + delta) */
    PUSHB_2,
    FunctionNumbers::bci_create_segment as u8,
    FunctionNumbers::bci_loop as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_hint_glyph as u8,
    CALL,
];

/* used if we have delta exceptions */

const FPGM_bci_create_segments_composite_c: &[u8] = &bytes![PUSHB_1, 1, SZPS,];

const FPGM_bci_create_segments_composite_d: &[u8] = &bytes![ELSE, CLEAR, EIF, ENDF,];

/*
 * bci_create_segments_composite_X
 *
 * Top-level routines for calling `bci_create_segments_composite'.
 */

const FPGM_bci_create_segments_composite_0: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_composite_0 as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_create_segments_composite as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_composite_1: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_composite_1 as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_create_segments_composite as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_composite_2: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_composite_2 as u8,
    FDEF,
    PUSHB_2,
    2,
    FunctionNumbers::bci_create_segments_composite as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_composite_3: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_composite_3 as u8,
    FDEF,
    PUSHB_2,
    3,
    FunctionNumbers::bci_create_segments_composite as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_composite_4: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_composite_4 as u8,
    FDEF,
    PUSHB_2,
    4,
    FunctionNumbers::bci_create_segments_composite as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_composite_5: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_composite_5 as u8,
    FDEF,
    PUSHB_2,
    5,
    FunctionNumbers::bci_create_segments_composite as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_composite_6: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_composite_6 as u8,
    FDEF,
    PUSHB_2,
    6,
    FunctionNumbers::bci_create_segments_composite as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_composite_7: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_composite_7 as u8,
    FDEF,
    PUSHB_2,
    7,
    FunctionNumbers::bci_create_segments_composite as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_composite_8: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_composite_8 as u8,
    FDEF,
    PUSHB_2,
    8,
    FunctionNumbers::bci_create_segments_composite as u8,
    CALL,
    ENDF,
];

const FPGM_bci_create_segments_composite_9: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_create_segments_composite_9 as u8,
    FDEF,
    PUSHB_2,
    9,
    FunctionNumbers::bci_create_segments_composite as u8,
    CALL,
    ENDF,
];

/*
 * bci_align_point
 *
 *   An auxiliary function for `bci_align_segment'.
 *
 * in: point
 *
 * out: point+1
 */

const FPGM_bci_align_point: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_align_point as u8,
    FDEF,
    DUP,
    ALIGNRP, /* align point with rp0 */
    PUSHB_1,
    1,
    ADD,
    ENDF,
];

/*
 * bci_align_segment
 *
 *   Align all points in a segment to the twilight point in rp0.
 *   zp0 and zp1 must be set to 0 (twilight) and 1 (normal), respectively.
 *
 * in: segment_index
 *
 * sal: sal_segment_offset
 *
 * uses: bci_align_point
 */

const FPGM_bci_align_segment: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_align_segment as u8,
    FDEF,
    /* we need the values of `sal_segment_offset + 2*segment_index' */
    /* and `sal_segment_offset + 2*segment_index + 1' */
    DUP,
    ADD,
    PUSHB_1,
    StorageAreaLocations::sal_segment_offset as u8,
    ADD,
    DUP,
    RS,
    SWAP,
    PUSHB_1,
    1,
    ADD,
    RS, /* s: first last */
    PUSHB_1,
    2,
    CINDEX, /* s: first last first */
    SUB,
    PUSHB_1,
    1,
    ADD, /* s: first loop_count */
    PUSHB_1,
    FunctionNumbers::bci_align_point as u8,
    LOOPCALL,
    /* clean up stack */
    POP,
    ENDF,
];

/*
 * bci_align_segments
 *
 *   Align segments to the twilight point in rp0.
 *   zp0 and zp1 must be set to 0 (twilight) and 1 (normal), respectively.
 *
 * in: first_segment
 *     loop_counter (N)
 *       segment_1
 *       segment_2
 *       ...
 *       segment_N
 *
 * uses: bci_align_segment
 */

const FPGM_bci_align_segments: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_align_segments as u8,
    FDEF,
    PUSHB_1,
    FunctionNumbers::bci_align_segment as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_align_segment as u8,
    LOOPCALL,
    ENDF,
];

/*
 * bci_scale_contour
 *
 *   Scale a contour using two points giving the maximum and minimum
 *   coordinates.
 *
 *   It expects that no point on the contour is touched.
 *
 * in: min_point
 *     max_point
 *
 * sal: sal_scale
 */

fn emit_bci_scale_contour(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_scale_contour as u8,
        FDEF,
        DUP,
        DUP,
        GC_orig,
        DUP,
    ]);
    bytecode.extend_bytes(DO_SCALE); /* min_pos_new = min_pos * scale */
    bytecode.extend_bytes(&bytes![
        SWAP, SUB, SHPIX, /* don't scale a single-point contour twice */
        SWAP, DUP, ROLL, NEQ, IF, DUP, GC_orig, DUP,
    ]);
    bytecode.extend_bytes(DO_SCALE); /* max_pos_new = max_pos * scale */
    bytecode.extend_bytes(&bytes![SWAP, SUB, SHPIX, ELSE, POP, EIF, ENDF,]);
}

/*
 * bci_scale_glyph
 *
 *   Scale a glyph using a list of points (two points per contour, giving
 *   the maximum and mininum coordinates).
 *
 *   It expects that no point in the glyph is touched.
 *
 *   Note that the point numbers are sorted in ascending order;
 *   `min_point_X' and `max_point_X' thus refer to the two extrema of a
 *   contour without specifying which one is the minimum and maximum.
 *
 * in: num_contours (N)
 *       min_point_1
 *       max_point_1
 *       min_point_2
 *       max_point_2
 *       ...
 *       min_point_N
 *       max_point_N
 *
 * CVT: cvtl_is_subglyph
 *      cvtl_do_iup_y
 *
 * sal: sal_scale
 *
 * uses: bci_scale_contour
 */

const FPGM_bci_scale_glyph_a: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_scale_glyph as u8,
    FDEF,
    /* all our measurements are taken along the y axis */
    SVTCA_y,
    /* only do something if we are not a subglyph */
    PUSHB_2,
    0,
    CvtLocations::cvtl_is_subglyph as u8,
    RCVT,
    EQ,
    IF,
    /* use fallback scaling value */
    PUSHB_2,
    StorageAreaLocations::sal_scale as u8,
];

/*    %c, fallback scaling index */

const FPGM_bci_scale_glyph_b: &[u8] = &bytes![
    RCVT,
    WS,
    PUSHB_1,
    1,
    SZPS, /* set zp0, zp1, and zp2 to normal zone 1 */
    PUSHB_1,
    FunctionNumbers::bci_scale_contour as u8,
    LOOPCALL,
    PUSHB_2,
    CvtLocations::cvtl_do_iup_y as u8,
    1,
    SZP2, /* set zp2 to normal zone 1 */
    RCVT,
    IF,
    IUP_y,
    EIF,
    ELSE,
    CLEAR,
    EIF,
    ENDF,
];

/*
 * bci_scale_composite_glyph
 *
 *   The same as `bci_scale_glyph'.
 *   It also decrements the composite component counter.
 *
 * CVT: cvtl_is_subglyph
 *      cvtl_do_iup_y
 *
 * sal: sal_scale
 *
 * uses: bci_decrement_component_counter
 *       FunctionNumbers::bci_scale_contou as u8r
 */

const FPGM_bci_scale_composite_glyph_a: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_scale_composite_glyph as u8,
    FDEF,
    /* all our measurements are taken along the y axis */
    SVTCA_y,
    PUSHB_1,
    FunctionNumbers::bci_decrement_component_counter as u8,
    CALL,
    /* only do something if we are not a subglyph */
    PUSHB_2,
    0,
    CvtLocations::cvtl_is_subglyph as u8,
    RCVT,
    EQ,
    IF,
    /* use fallback scaling value */
    PUSHB_2,
    StorageAreaLocations::sal_scale as u8,
];

/*    %c, fallback scaling index */

const FPGM_bci_scale_composite_glyph_b: &[u8] = &bytes![
    RCVT,
    WS,
    PUSHB_1,
    1,
    SZPS, /* set zp0, zp1, and zp2 to normal zone 1 */
    PUSHB_1,
    FunctionNumbers::bci_scale_contour as u8,
    LOOPCALL,
    PUSHB_2,
    CvtLocations::cvtl_do_iup_y as u8,
    1,
    SZP2, /* set zp2 to normal zone 1 */
    RCVT,
    IF,
    IUP_y,
    EIF,
    ELSE,
    CLEAR,
    EIF,
    ENDF,
];

/*
 * bci_shift_contour
 *
 *   Shift a contour by a given amount.
 *
 *   It expects that rp1 (pointed to by zp0) is set up properly; zp2 must
 *   point to the normal zone 1.
 *
 * in: contour
 *
 * out: contour+1
 */

const FPGM_bci_shift_contour: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_shift_contour as u8,
    FDEF,
    DUP,
    SHC_rp1, /* shift `contour' by (rp1_pos - rp1_orig_pos) */
    PUSHB_1,
    1,
    ADD,
    ENDF,
];

/*
 * bci_shift_subglyph
 *
 *   Shift a subglyph.  To be more specific, it corrects the already applied
 *   subglyph offset (if any) from the `glyf' table which needs to be scaled
 *   also.
 *
 *   If this function is called, a point `x' in the subglyph has been scaled
 *   already (during the hinting of the subglyph itself), and `offset' has
 *   been applied also:
 *
 *     x  ->  x * scale + offset         (1)
 *
 *   However, the offset should be applied first, then the scaling:
 *
 *     x  ->  (x + offset) * scale       (2)
 *
 *   Our job is now to transform (1) to (2); a simple calculation shows that
 *   we have to shift all points of the subglyph by
 *
 *     offset * scale - offset = offset * (scale - 1)
 *
 *   Note that `sal_scale' is equal to the above `scale - 1'.
 *
 * in: offset (in FUnits)
 *     num_contours
 *     first_contour
 *
 * CVT: cvtl_funits_to_pixels
 *
 * sal: sal_scale
 *
 * uses: bci_round
 *       FunctionNumbers::bci_shift_contou as u8r
 */

const FPGM_bci_shift_subglyph_a: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_shift_subglyph as u8,
    FDEF,
    /* all our measurements are taken along the y axis */
    SVTCA_y,
    /* use fallback scaling value */
    PUSHB_2,
    StorageAreaLocations::sal_scale as u8,
];

/*  %c, fallback scaling index */

fn emit_bci_shift_subglyph_b(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        RCVT,
        WS,
        PUSHB_1,
        CvtLocations::cvtl_funits_to_pixels as u8,
        RCVT, /* scaling factor FUnits -> pixels */
        MUL,
    ]);
    bytecode.extend_bytes(DIV_BY_1024);
    bytecode.extend_bytes(&bytes![
        /* the autohinter always rounds offsets */
        PUSHB_1,
        FunctionNumbers::bci_round as u8,
        CALL, /* offset = round(offset) */
        PUSHB_1,
        StorageAreaLocations::sal_scale as u8,
        RS,
        MUL,
    ]);
    bytecode.extend_bytes(DIV_BY_1024 /* delta = offset * (scale - 1) */);
    bytecode.extend_bytes(&bytes![
        /* and round again */
        PUSHB_1,
        FunctionNumbers::bci_round as u8,
        CALL, /* offset = round(offset) */
        PUSHB_1,
        0,
        SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
        /* we create twilight point 0 as a reference point, */
        /* setting the original position to zero (using `cvtl_temp') */
        PUSHB_5,
        0,
        0,
        CvtLocations::cvtl_temp as u8,
        CvtLocations::cvtl_temp as u8,
        0,
        WCVTP,
        MIAP_noround, /* rp0 and rp1 now point to twilight point 0 */
        SWAP,         /* s: first_contour num_contours 0 delta */
        SHPIX,        /* rp1_pos - rp1_orig_pos = delta */
        PUSHB_2,
        FunctionNumbers::bci_shift_contour as u8,
        1,
        SZP2, /* set zp2 to normal zone 1 */
        LOOPCALL,
        /* clean up stack */
        POP,
    ]);
}

/* used if we have delta exceptions */

const FPGM_bci_shift_subglyph_c: &[u8] = &bytes![PUSHB_1, 1, SZPS,];

const FPGM_bci_shift_subglyph_d: &[u8] = &bytes![ENDF];

/*
 * bci_ip_outer_align_point
 *
 *   Auxiliary function for `bci_action_ip_before' and
 *   `bci_action_ip_after'.
 *
 *   It expects rp0 to contain the edge for alignment, zp0 set to twilight
 *   zone, and both zp1 and zp2 set to normal zone.
 *
 * in: point
 *
 * sal: sal_i (edge_orig_pos)
 *      StorageAreaLocations::sal_scale as u8
 */

fn emit_bci_ip_outer_align_point(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_ip_outer_align_point as u8,
        FDEF,
        DUP,
        ALIGNRP, /* align `point' with `edge' */
        DUP,
        GC_orig,
    ]);

    bytecode.extend_bytes(DO_SCALE); /* point_orig_pos = point_orig_pos * scale */
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        StorageAreaLocations::sal_i as u8,
        RS,
        SUB, /* s: point (point_orig_pos - edge_orig_pos) */
        SHPIX,
        ENDF,
    ]);
}

/*
 * bci_ip_on_align_points
 *
 *   Auxiliary function for `bci_action_ip_on'.
 *
 * in: edge (in twilight zone)
 *     loop_counter (N)
 *       point_1
 *       point_2
 *       ...
 *       point_N
 */

const FPGM_bci_ip_on_align_points: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_ip_on_align_points as u8,
    FDEF,
    MDAP_noround, /* set rp0 and rp1 to `edge' */
    SLOOP,
    ALIGNRP,
    ENDF,
];

/*
 * bci_ip_between_align_point
 *
 *   Auxiliary function for `bci_ip_between_align_points'.
 *
 *   It expects rp0 to contain the edge for alignment, zp0 set to twilight
 *   zone, and both zp1 and zp2 set to normal zone.
 *
 * in: point
 *
 * sal: sal_i (edge_orig_pos)
 *      StorageAreaLocations::sal_j as u8 (stretch_factor)
 *      StorageAreaLocations::sal_scale as u8
 */

fn emit_bci_ip_between_align_point(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_ip_between_align_point as u8,
        FDEF,
        DUP,
        ALIGNRP, /* align `point' with `edge' */
        DUP,
        GC_orig,
    ]);

    bytecode.extend_bytes(DO_SCALE /* point_orig_pos = point_orig_pos * scale */);
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        StorageAreaLocations::sal_i as u8,
        RS,
        SUB, /* s: point (point_orig_pos - edge_orig_pos) */
        PUSHB_1,
        StorageAreaLocations::sal_j as u8,
        RS,
        MUL, /* s: point delta */
        SHPIX,
        ENDF,
    ]);
}

/*
 * bci_ip_between_align_points
 *
 *   Auxiliary function for `bci_action_ip_between'.
 *
 * in: after_edge (in twilight zone)
 *     before_edge (in twilight zone)
 *     loop_counter (N)
 *       point_1
 *       point_2
 *       ...
 *       point_N
 *
 * sal: sal_i (before_orig_pos)
 *      StorageAreaLocations::sal_j as u8 (stretch_factor)
 *
 * uses: bci_ip_between_align_point
 */

fn emit_bci_ip_between_align_points(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_ip_between_align_points as u8,
        FDEF,
        PUSHB_2,
        2,
        0,
        SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
        CINDEX,
        DUP,          /* s: ... before after before before */
        MDAP_noround, /* set rp0 and rp1 to `before' */
        DUP,
        GC_orig, /* s: ... before after before before_orig_pos */
        PUSHB_1,
        StorageAreaLocations::sal_i as u8,
        SWAP,
        WS, /* sal_i = before_orig_pos */
        PUSHB_1,
        2,
        CINDEX, /* s: ... before after before after */
        MD_cur, /* a = after_pos - before_pos */
        ROLL,
        ROLL,
    ]);
    bytecode.extend_bytes(
        MD_orig_ZP2_0, /* b = after_orig_pos - before_orig_pos */
    );
    bytecode.extend_bytes(&bytes![
        DUP,
        IF,  /* b != 0 ? */
        DIV, /* s: a/b */
        ELSE,
        POP, /* avoid division by zero */
        EIF,
        PUSHB_1,
        StorageAreaLocations::sal_j as u8,
        SWAP,
        WS, /* sal_j = stretch_factor */
        PUSHB_3,
        FunctionNumbers::bci_ip_between_align_point as u8,
        1,
        1,
        SZP2, /* set zp2 to normal zone 1 */
        SZP1, /* set zp1 to normal zone 1 */
        LOOPCALL,
        ENDF,
    ]);
}

/*
 * bci_action_ip_before
 *
 *   Handle `ip_before' data to align points located before the first edge.
 *
 * in: first_edge (in twilight zone)
 *     loop_counter (N)
 *       point_1
 *       point_2
 *       ...
 *       point_N
 *
 * sal: sal_i (first_edge_orig_pos)
 *
 * uses: bci_ip_outer_align_point
 */

const FPGM_bci_action_ip_before: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_ip_before as u8,
    FDEF,
    PUSHB_1,
    0,
    SZP2, /* set zp2 to twilight zone 0 */
    DUP,
    GC_orig,
    PUSHB_1,
    StorageAreaLocations::sal_i as u8,
    SWAP,
    WS, /* sal_i = first_edge_orig_pos */
    PUSHB_3,
    0,
    1,
    1,
    SZP2,         /* set zp2 to normal zone 1 */
    SZP1,         /* set zp1 to normal zone 1 */
    SZP0,         /* set zp0 to twilight zone 0 */
    MDAP_noround, /* set rp0 and rp1 to `first_edge' */
    PUSHB_1,
    FunctionNumbers::bci_ip_outer_align_point as u8,
    LOOPCALL,
    ENDF,
];

/*
 * bci_action_ip_after
 *
 *   Handle `ip_after' data to align points located after the last edge.
 *
 * in: last_edge (in twilight zone)
 *     loop_counter (N)
 *       point_1
 *       point_2
 *       ...
 *       point_N
 *
 * sal: sal_i (last_edge_orig_pos)
 *
 * uses: bci_ip_outer_align_point
 */

const FPGM_bci_action_ip_after: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_ip_after as u8,
    FDEF,
    PUSHB_1,
    0,
    SZP2, /* set zp2 to twilight zone 0 */
    DUP,
    GC_orig,
    PUSHB_1,
    StorageAreaLocations::sal_i as u8,
    SWAP,
    WS, /* sal_i = last_edge_orig_pos */
    PUSHB_3,
    0,
    1,
    1,
    SZP2,         /* set zp2 to normal zone 1 */
    SZP1,         /* set zp1 to normal zone 1 */
    SZP0,         /* set zp0 to twilight zone 0 */
    MDAP_noround, /* set rp0 and rp1 to `last_edge' */
    PUSHB_1,
    FunctionNumbers::bci_ip_outer_align_point as u8,
    LOOPCALL,
    ENDF,
];

/*
 * bci_action_ip_on
 *
 *   Handle `ip_on' data to align points located on an edge coordinate (but
 *   not part of an edge).
 *
 * in: loop_counter (M)
 *       edge_1 (in twilight zone)
 *       loop_counter (N_1)
 *         point_1
 *         point_2
 *         ...
 *         point_N_1
 *       edge_2 (in twilight zone)
 *       loop_counter (N_2)
 *         point_1
 *         point_2
 *         ...
 *         point_N_2
 *       ...
 *       edge_M (in twilight zone)
 *       loop_counter (N_M)
 *         point_1
 *         point_2
 *         ...
 *         point_N_M
 *
 * uses: bci_ip_on_align_points
 */

const FPGM_bci_action_ip_on: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_ip_on as u8,
    FDEF,
    PUSHB_2,
    0,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    SZP0, /* set zp0 to twilight zone 0 */
    PUSHB_1,
    FunctionNumbers::bci_ip_on_align_points as u8,
    LOOPCALL,
    ENDF,
];

/*
 * bci_action_ip_between
 *
 *   Handle `ip_between' data to align points located between two edges.
 *
 * in: loop_counter (M)
 *       before_edge_1 (in twilight zone)
 *       after_edge_1 (in twilight zone)
 *       loop_counter (N_1)
 *         point_1
 *         point_2
 *         ...
 *         point_N_1
 *       before_edge_2 (in twilight zone)
 *       after_edge_2 (in twilight zone)
 *       loop_counter (N_2)
 *         point_1
 *         point_2
 *         ...
 *         point_N_2
 *       ...
 *       before_edge_M (in twilight zone)
 *       after_edge_M (in twilight zone)
 *       loop_counter (N_M)
 *         point_1
 *         point_2
 *         ...
 *         point_N_M
 *
 * uses: bci_ip_between_align_points
 */

const FPGM_bci_action_ip_between: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_ip_between as u8,
    FDEF,
    PUSHB_1,
    FunctionNumbers::bci_ip_between_align_points as u8,
    LOOPCALL,
    ENDF,
];

/*
 * bci_adjust_common
 *
 *   Common code for bci_action_adjust routines.
 *
 * in: top_to_bottom_hinting
 *     edge2_is_serif
 *     edge_is_round
 *     edge
 *     edge2
 *
 * out: edge (adjusted)
 *
 * sal: sal_top_to_bottom_hinting
 *      StorageAreaLocations::sal_base_delta as u8
 *
 * uses: func[sal_stem_width_function]
 */

fn emit_bci_adjust_common(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_adjust_common as u8,
        FDEF,
        PUSHB_1,
        0,
        SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
        PUSHB_1,
        StorageAreaLocations::sal_top_to_bottom_hinting as u8,
        SWAP,
        WS,
        PUSHB_1,
        4,
        CINDEX, /* s: [...] edge2 edge is_round is_serif edge2 */
        PUSHB_1,
        4,
        CINDEX, /* s: [...] edge2 edge is_round is_serif edge2 edge */
    ]);
    bytecode.extend_bytes(MD_orig_ZP2_0); /* s: [...] edge2 edge is_round is_serif org_len */
    bytecode.extend_bytes(&bytes![
        PUSHB_2,
        StorageAreaLocations::sal_base_delta as u8, /* no base_delta needed here */
        0,
        WS,
        PUSHB_1,
        StorageAreaLocations::sal_stem_width_function as u8,
        RS,
        CALL,
        NEG,          /* s: [...] edge2 edge -cur_len */
        ROLL,         /* s: [...] edge -cur_len edge2 */
        MDAP_noround, /* set rp0 and rp1 to `edge2' */
        SWAP,
        DUP,
        DUP,     /* s: [...] -cur_len edge edge edge */
        ALIGNRP, /* align `edge' with `edge2' */
        ROLL,
        SHPIX, /* shift `edge' by -cur_len */
        ENDF,
    ]);
}

/*
 * bci_adjust_bound
 *
 *   Handle the ADJUST + BOUND actions to align an edge of a stem if the
 *   other edge of the stem has already been moved, then moving it again if
 *   necessary to stay bound.
 *
 * in: top_to_bottom_hinting
 *     edge2_is_serif
 *     edge_is_round
 *     edge_point (in twilight zone)
 *     edge2_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * sal: sal_top_to_bottom_hinting
 *
 * uses: bci_adjust_common
 *       FunctionNumbers::bci_align_segment as u8s
 */

const FPGM_bci_adjust_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_adjust_bound as u8,
    FDEF,
    PUSHB_1,
    FunctionNumbers::bci_adjust_common as u8,
    CALL,
    SWAP, /* s: edge edge[-1] */
    DUP,
    MDAP_noround, /* set rp0 and rp1 to `edge[-1]' */
    GC_cur,
    PUSHB_1,
    2,
    CINDEX,
    GC_cur, /* s: edge edge[-1]_pos edge_pos */
    PUSHB_1,
    StorageAreaLocations::sal_top_to_bottom_hinting as u8,
    RS,
    IF,
    LT, /* edge_pos > edge[-1]_pos */
    ELSE,
    GT, /* edge_pos < edge[-1]_pos */
    EIF,
    IF,
    DUP,
    ALIGNRP, /* align `edge' to `edge[-1]' */
    EIF,
    MDAP_noround, /* set rp0 and rp1 to `edge' */
    PUSHB_2,
    FunctionNumbers::bci_align_segments as u8,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    CALL,
    ENDF,
];

/*
 * bci_action_adjust_bound
 * bci_action_adjust_bound_serif
 * bci_action_adjust_bound_round
 * bci_action_adjust_bound_round_serif
 * bci_action_adjust_down_bound
 * bci_action_adjust_down_bound_serif
 * bci_action_adjust_down_bound_round
 * bci_action_adjust_down_bound_round_serif
 *
 * Higher-level routines for calling `bci_adjust_bound'.
 */

const FPGM_bci_action_adjust_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_adjust_bound as u8,
    FDEF,
    PUSHB_4,
    0,
    0,
    0,
    FunctionNumbers::bci_adjust_bound as u8,
    CALL,
    ENDF,
];

const FPGM_bci_action_adjust_bound_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_adjust_bound_serif as u8,
    FDEF,
    PUSHB_4,
    0,
    1,
    0,
    FunctionNumbers::bci_adjust_bound as u8,
    CALL,
    ENDF,
];

const FPGM_bci_action_adjust_bound_round: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_adjust_bound_round as u8,
    FDEF,
    PUSHB_4,
    1,
    0,
    0,
    FunctionNumbers::bci_adjust_bound as u8,
    CALL,
    ENDF,
];

const FPGM_bci_action_adjust_bound_round_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_adjust_bound_round_serif as u8,
    FDEF,
    PUSHB_4,
    1,
    1,
    0,
    FunctionNumbers::bci_adjust_bound as u8,
    CALL,
    ENDF,
];

const FPGM_bci_action_adjust_down_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_adjust_down_bound as u8,
    FDEF,
    PUSHB_4,
    0,
    0,
    1,
    FunctionNumbers::bci_adjust_bound as u8,
    CALL,
    ENDF,
];

const FPGM_bci_action_adjust_down_bound_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_adjust_down_bound_serif as u8,
    FDEF,
    PUSHB_4,
    0,
    1,
    1,
    FunctionNumbers::bci_adjust_bound as u8,
    CALL,
    ENDF,
];

const FPGM_bci_action_adjust_down_bound_round: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_adjust_down_bound_round as u8,
    FDEF,
    PUSHB_4,
    1,
    0,
    1,
    FunctionNumbers::bci_adjust_bound as u8,
    CALL,
    ENDF,
];

const FPGM_bci_action_adjust_down_bound_round_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_adjust_down_bound_round_serif as u8,
    FDEF,
    PUSHB_4,
    1,
    1,
    1,
    FunctionNumbers::bci_adjust_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_adjust
 *
 *   Handle the ADJUST action to align an edge of a stem if the other edge
 *   of the stem has already been moved.
 *
 * in: edge2_is_serif
 *     edge_is_round
 *     edge_point (in twilight zone)
 *     edge2_point (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_adjust_common
 *       FunctionNumbers::bci_align_segment as u8s
 */

const FPGM_bci_adjust: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_adjust as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_adjust_common as u8,
    CALL,
    MDAP_noround, /* set rp0 and rp1 to `edge' */
    PUSHB_2,
    FunctionNumbers::bci_align_segments as u8,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    CALL,
    ENDF,
];

/*
 * bci_action_adjust
 * bci_action_adjust_serif
 * bci_action_adjust_round
 * bci_action_adjust_round_serif
 *
 * Higher-level routines for calling `bci_adjust'.
 */

const FPGM_bci_action_adjust: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_adjust as u8,
    FDEF,
    PUSHB_3,
    0,
    0,
    FunctionNumbers::bci_adjust as u8,
    CALL,
    ENDF,
];

const FPGM_bci_action_adjust_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_adjust_serif as u8,
    FDEF,
    PUSHB_3,
    0,
    1,
    FunctionNumbers::bci_adjust as u8,
    CALL,
    ENDF,
];

const FPGM_bci_action_adjust_round: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_adjust_round as u8,
    FDEF,
    PUSHB_3,
    1,
    0,
    FunctionNumbers::bci_adjust as u8,
    CALL,
    ENDF,
];

const FPGM_bci_action_adjust_round_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_adjust_round_serif as u8,
    FDEF,
    PUSHB_3,
    1,
    1,
    FunctionNumbers::bci_adjust as u8,
    CALL,
    ENDF,
];

/*
 * bci_stem_common
 *
 *   Common code for bci_action_stem routines.
 *
 * in: top_to_bottom_hinting
 *     edge2_is_serif
 *     edge_is_round
 *     edge
 *     edge2
 *
 * out: edge
 *      cur_len
 *      edge2
 *
 * sal: sal_anchor
 *      StorageAreaLocations::sal_temp1 as u8
 *      StorageAreaLocations::sal_temp2 as u8
 *      StorageAreaLocations::sal_temp3 as u8
 *      StorageAreaLocations::sal_top_to_bottom_hinting as u8
 *      StorageAreaLocations::sal_base_delta as u8
 *
 * uses: func[sal_stem_width_function]
 *       FunctionNumbers::bci_roun as u8d
 */

const SAL_U_OFF: u8 = StorageAreaLocations::sal_temp1 as u8;
const SAL_D_OFF: u8 = StorageAreaLocations::sal_temp2 as u8;
const SAL_ORG_LEN: u8 = StorageAreaLocations::sal_temp3 as u8;
const SAL_EDGE2: u8 = StorageAreaLocations::sal_temp3 as u8;

fn emit_bci_stem_common(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_stem_common as u8,
        FDEF,
        PUSHB_1,
        0,
        SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
        PUSHB_1,
        StorageAreaLocations::sal_top_to_bottom_hinting as u8,
        SWAP,
        WS,
        PUSHB_1,
        4,
        CINDEX,
        PUSHB_1,
        4,
        CINDEX,
        DUP,          /* s: [...] edge2 edge is_round is_serif edge2 edge edge */
        MDAP_noround, /* set rp0 and rp1 to `edge_point' (for ALIGNRP below) */
    ]);
    bytecode.extend_bytes(MD_orig_ZP2_0); /* s: [...] edge2 edge is_round is_serif org_len */
    bytecode.extend_bytes(&bytes![
        DUP,
        PUSHB_1,
        SAL_ORG_LEN,
        SWAP,
        WS,
        PUSHB_2,
        StorageAreaLocations::sal_base_delta, /* no base_delta needed here */
        0,
        WS,
        PUSHB_1,
        StorageAreaLocations::sal_stem_width_function,
        RS,
        CALL, /* s: [...] edge2 edge cur_len */
        DUP,
        PUSHB_1,
        96,
        LT, /* cur_len < 96 */
        IF,
        DUP,
        PUSHB_1,
        64,
        LTEQ, /* cur_len <= 64 */
        IF,
        PUSHB_4,
        SAL_U_OFF,
        32,
        SAL_D_OFF,
        32,
        ELSE,
        PUSHB_4,
        SAL_U_OFF,
        38,
        SAL_D_OFF,
        26,
        EIF,
        WS,
        WS,
        SWAP, /* s: [...] edge2 cur_len edge */
        DUP,
        PUSHB_1,
        StorageAreaLocations::sal_anchor,
        RS,
        DUP, /* s: [...] edge2 cur_len edge edge anchor anchor */
        ROLL,
        SWAP,
    ]);
    bytecode.extend_bytes(MD_orig_ZP2_0);
    bytecode.extend_bytes(&bytes![
        SWAP,
        GC_cur,
        ADD, /* s: [...] edge2 cur_len edge org_pos */
        PUSHB_1,
        SAL_ORG_LEN,
        RS,
    ]);
    bytecode.extend_bytes(DIV_BY_2);
    bytecode.extend_bytes(&bytes![
        ADD, /* s: [...] edge2 cur_len edge org_center */
        DUP,
        PUSHB_1,
        FunctionNumbers::bci_round,
        CALL, /* s: [...] edge2 cur_len edge org_center cur_pos1 */
        DUP,
        ROLL,
        ROLL,
        SUB, /* s: ... cur_len edge cur_pos1 (org_center - cur_pos1) */
        DUP,
        PUSHB_1,
        SAL_U_OFF,
        RS,
        ADD,
        ABS, /* s: ... cur_len edge cur_pos1 (org_center - cur_pos1) delta1 */
        SWAP,
        PUSHB_1,
        SAL_D_OFF,
        RS,
        SUB,
        ABS, /* s: [...] edge2 cur_len edge cur_pos1 delta1 delta2 */
        LT,  /* delta1 < delta2 */
        IF,
        PUSHB_1,
        SAL_U_OFF,
        RS,
        SUB, /* cur_pos1 = cur_pos1 - u_off */
        ELSE,
        PUSHB_1,
        SAL_D_OFF,
        RS,
        ADD, /* cur_pos1 = cur_pos1 + d_off */
        EIF, /* s: [...] edge2 cur_len edge cur_pos1 */
        PUSHB_1,
        3,
        CINDEX
    ]);
    bytecode.extend_bytes(DIV_BY_2);
    bytecode.extend_bytes(&bytes![
        SUB,  /* arg = cur_pos1 - cur_len/2 */
        SWAP, /* s: [...] edge2 cur_len arg edge */
        DUP,
        DUP,
        PUSHB_1,
        4,
        MINDEX,
        SWAP, /* s: [...] edge2 cur_len edge edge arg edge */
        GC_cur,
        SUB,
        SHPIX, /* edge = cur_pos1 - cur_len/2 */
        ELSE,
        SWAP, /* s: [...] edge2 cur_len edge */
        PUSHB_1,
        StorageAreaLocations::sal_anchor,
        RS,
        GC_cur, /* s: [...] edge2 cur_len edge anchor_pos */
        PUSHB_1,
        2,
        CINDEX,
        PUSHB_1,
        StorageAreaLocations::sal_anchor,
        RS,
    ]);
    bytecode.extend_bytes(MD_orig_ZP2_0);
    bytecode.extend_bytes(&bytes![
        ADD, /* s: [...] edge2 cur_len edge org_pos */
        DUP,
        PUSHB_1,
        SAL_ORG_LEN,
        RS,
    ]);
    bytecode.extend_bytes(DIV_BY_2);
    bytecode.extend_bytes(&bytes![
        ADD, /* s: [...] edge2 cur_len edge org_pos org_center */
        SWAP,
        DUP,
        PUSHB_1,
        FunctionNumbers::bci_round,
        CALL, /* cur_pos1 = ROUND(org_pos) */
        SWAP,
        PUSHB_1,
        SAL_ORG_LEN,
        RS,
        ADD,
        PUSHB_1,
        FunctionNumbers::bci_round,
        CALL,
        PUSHB_1,
        5,
        CINDEX,
        SUB, /* s: [...] edge2 cur_len edge org_center cur_pos1 cur_pos2 */
        PUSHB_1,
        5,
        CINDEX,
    ]);
    bytecode.extend_bytes(DIV_BY_2);
    bytecode.extend_bytes(&bytes![
        PUSHB_1, 4, MINDEX,
        SUB, /* s: ... cur_len edge cur_pos1 cur_pos2 (cur_len/2 - org_center) */
        DUP, PUSHB_1, 4, CINDEX, ADD,
        ABS, /* delta1 = |cur_pos1 + cur_len / 2 - org_center| */
        SWAP, PUSHB_1, 3, CINDEX, ADD,
        ABS, /* s: ... edge2 cur_len edge cur_pos1 cur_pos2 delta1 delta2 */
        LT,  /* delta1 < delta2 */
        IF, POP, /* arg = cur_pos1 */
        ELSE, SWAP, POP, /* arg = cur_pos2 */
        EIF, /* s: [...] edge2 cur_len edge arg */
        SWAP, DUP, DUP, PUSHB_1, 4, MINDEX,
        SWAP, /* s: [...] edge2 cur_len edge edge arg edge */
        GC_cur, SUB, SHPIX, /* edge = arg */
        EIF,   /* s: [...] edge2 cur_len edge */
        ENDF,
    ]);
}

/*
 * bci_stem_bound
 *
 *   Handle the STEM action to align two edges of a stem, then moving one
 *   edge again if necessary to stay bound.
 *
 *   The code after computing `cur_len' to shift `edge' and `edge2'
 *   is equivalent to the snippet below (part of `ta_latin_hint_edges'):
 *
 *      if cur_len < 96:
 *        if cur_len < = 64:
 *          u_off = 32
 *          d_off = 32
 *        else:
 *          u_off = 38
 *          d_off = 26
 *
 *        org_pos = anchor + (edge_orig - anchor_orig)
 *        org_center = org_pos + org_len / 2
 *
 *        cur_pos1 = ROUND(org_center)
 *        delta1 = |org_center - (cur_pos1 - u_off)|
 *        delta2 = |org_center - (cur_pos1 + d_off)|
 *        if (delta1 < delta2):
 *          cur_pos1 = cur_pos1 - u_off
 *        else:
 *          cur_pos1 = cur_pos1 + d_off
 *
 *        edge = cur_pos1 - cur_len / 2
 *
 *      else:
 *        org_pos = anchor + (edge_orig - anchor_orig)
 *        org_center = org_pos + org_len / 2
 *
 *        cur_pos1 = ROUND(org_pos)
 *        delta1 = |cur_pos1 + cur_len / 2 - org_center|
 *        cur_pos2 = ROUND(org_pos + org_len) - cur_len
 *        delta2 = |cur_pos2 + cur_len / 2 - org_center|
 *
 *        if (delta1 < delta2):
 *          edge = cur_pos1
 *        else:
 *          edge = cur_pos2
 *
 *      edge2 = edge + cur_len
 *
 * in: top_to_bottom_hinting
 *     edge2_is_serif
 *     edge_is_round
 *     edge_point (in twilight zone)
 *     edge2_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *     ... stuff for bci_align_segments (edge2)...
 *
 * sal: sal_anchor
 *      StorageAreaLocations::sal_temp1 as u8
 *      StorageAreaLocations::sal_temp2 as u8
 *      StorageAreaLocations::sal_temp3 as u8
 *      StorageAreaLocations::sal_top_to_bottom_hinting as u8
 *
 * uses: bci_stem_common
 *       FunctionNumbers::bci_align_segment as u8s
 */

const FPGM_bci_stem_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_stem_bound,
    FDEF,
    PUSHB_1,
    FunctionNumbers::bci_stem_common,
    CALL,
    ROLL, /* s: edge[-1] cur_len edge edge2 */
    DUP,
    DUP,
    ALIGNRP, /* align `edge2' with rp0 (still `edge') */
    PUSHB_1,
    SAL_EDGE2,
    SWAP,
    WS, /* s: edge[-1] cur_len edge edge2 */
    ROLL,
    SHPIX, /* edge2 = edge + cur_len */
    SWAP,  /* s: edge edge[-1] */
    DUP,
    MDAP_noround, /* set rp0 and rp1 to `edge[-1]' */
    GC_cur,
    PUSHB_1,
    2,
    CINDEX,
    GC_cur, /* s: edge edge[-1]_pos edge_pos */
    PUSHB_1,
    StorageAreaLocations::sal_top_to_bottom_hinting,
    RS,
    IF,
    LT, /* edge_pos > edge[-1]_pos */
    ELSE,
    GT, /* edge_pos < edge[-1]_pos */
    EIF,
    IF,
    DUP,
    ALIGNRP, /* align `edge' to `edge[-1]' */
    EIF,
    MDAP_noround, /* set rp0 and rp1 to `edge' */
    PUSHB_2,
    FunctionNumbers::bci_align_segments,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    CALL,
    PUSHB_1,
    SAL_EDGE2,
    RS,
    MDAP_noround, /* set rp0 and rp1 to `edge2' */
    PUSHB_1,
    FunctionNumbers::bci_align_segments,
    CALL,
    ENDF,
];

/*
 * bci_action_stem_bound
 * bci_action_stem_bound_serif
 * bci_action_stem_bound_round
 * bci_action_stem_bound_round_serif
 * bci_action_stem_down_bound
 * bci_action_stem_down_bound_serif
 * bci_action_stem_down_bound_round
 * bci_action_stem_down_bound_round_serif
 *
 * Higher-level routines for calling `bci_stem_bound'.
 */

const FPGM_bci_action_stem_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_stem_bound,
    FDEF,
    PUSHB_4,
    0,
    0,
    0,
    FunctionNumbers::bci_stem_bound,
    CALL,
    ENDF,
];

const FPGM_bci_action_stem_bound_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_stem_bound_serif,
    FDEF,
    PUSHB_4,
    0,
    1,
    0,
    FunctionNumbers::bci_stem_bound,
    CALL,
    ENDF,
];

const FPGM_bci_action_stem_bound_round: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_stem_bound_round,
    FDEF,
    PUSHB_4,
    1,
    0,
    0,
    FunctionNumbers::bci_stem_bound,
    CALL,
    ENDF,
];

const FPGM_bci_action_stem_bound_round_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_stem_bound_round_serif,
    FDEF,
    PUSHB_4,
    1,
    1,
    0,
    FunctionNumbers::bci_stem_bound,
    CALL,
    ENDF,
];

const FPGM_bci_action_stem_down_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_stem_down_bound,
    FDEF,
    PUSHB_4,
    0,
    0,
    1,
    FunctionNumbers::bci_stem_bound,
    CALL,
    ENDF,
];

const FPGM_bci_action_stem_down_bound_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_stem_down_bound_serif,
    FDEF,
    PUSHB_4,
    0,
    1,
    1,
    FunctionNumbers::bci_stem_bound,
    CALL,
    ENDF,
];

const FPGM_bci_action_stem_down_bound_round: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_stem_down_bound_round,
    FDEF,
    PUSHB_4,
    1,
    0,
    1,
    FunctionNumbers::bci_stem_bound,
    CALL,
    ENDF,
];

const FPGM_bci_action_stem_down_bound_round_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_stem_down_bound_round_serif,
    FDEF,
    PUSHB_4,
    1,
    1,
    1,
    FunctionNumbers::bci_stem_bound,
    CALL,
    ENDF,
];

/*
 * bci_stem
 *
 *   Handle the STEM action to align two edges of a stem.
 *
 *   See `bci_stem_bound' for more details.
 *
 * in: edge2_is_serif
 *     edge_is_round
 *     edge_point (in twilight zone)
 *     edge2_point (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *     ... stuff for bci_align_segments (edge2)...
 *
 * sal: sal_anchor
 *      StorageAreaLocations::sal_temp1 as u8
 *      StorageAreaLocations::sal_temp2 as u8
 *      StorageAreaLocations::sal_temp3 as u8
 *
 * uses: bci_stem_common
 *       FunctionNumbers::bci_align_segment as u8s
 */

const FPGM_bci_stem: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_stem,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_stem_common,
    CALL,
    POP,
    SWAP, /* s: cur_len edge2 */
    DUP,
    DUP,
    ALIGNRP, /* align `edge2' with rp0 (still `edge') */
    PUSHB_1,
    SAL_EDGE2,
    SWAP,
    WS, /* s: cur_len edge2 */
    SWAP,
    SHPIX, /* edge2 = edge + cur_len */
    PUSHB_2,
    FunctionNumbers::bci_align_segments,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    CALL,
    PUSHB_1,
    SAL_EDGE2,
    RS,
    MDAP_noround, /* set rp0 and rp1 to `edge2' */
    PUSHB_1,
    FunctionNumbers::bci_align_segments,
    CALL,
    ENDF,
];

/*
 * bci_action_stem
 * bci_action_stem_serif
 * bci_action_stem_round
 * bci_action_stem_round_serif
 *
 * Higher-level routines for calling `bci_stem'.
 */

const FPGM_bci_action_stem: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_stem,
    FDEF,
    PUSHB_3,
    0,
    0,
    FunctionNumbers::bci_stem,
    CALL,
    ENDF,
];

const FPGM_bci_action_stem_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_stem_serif,
    FDEF,
    PUSHB_3,
    0,
    1,
    FunctionNumbers::bci_stem,
    CALL,
    ENDF,
];

const FPGM_bci_action_stem_round: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_stem_round,
    FDEF,
    PUSHB_3,
    1,
    0,
    FunctionNumbers::bci_stem,
    CALL,
    ENDF,
];

const FPGM_bci_action_stem_round_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_stem_round_serif,
    FDEF,
    PUSHB_3,
    1,
    1,
    FunctionNumbers::bci_stem,
    CALL,
    ENDF,
];

/*
 * bci_link
 *
 *   Handle the LINK action to link an edge to another one.
 *
 * in: stem_is_serif
 *     base_is_round
 *     base_point (in twilight zone)
 *     stem_point (in twilight zone)
 *     ... stuff for bci_align_segments (base) ...
 *
 * sal: sal_base_delta
 *
 * uses: func[sal_stem_width_function]
 *       FunctionNumbers::bci_align_segment as u8s
 */

fn emit_bci_link(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_link,
        FDEF,
        PUSHB_1,
        0,
        SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
        PUSHB_1,
        4,
        CINDEX,
        PUSHB_1,
        4,
        MINDEX,
        DUP, /* s: stem is_round is_serif stem base base */
        DUP,
        DUP,
        GC_cur,
        SWAP,
        GC_orig,
        SUB, /* base_delta = base_point_pos - base_point_orig_pos */
        PUSHB_1,
        StorageAreaLocations::sal_base_delta,
        SWAP,
        WS,           /* sal_base_delta = base_delta */
        MDAP_noround, /* set rp0 and rp1 to `base_point' (for ALIGNRP below) */
    ]);
    bytecode.extend_bytes(MD_orig_ZP2_0);
    /* s: stem is_round is_serif dist_orig */
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        StorageAreaLocations::sal_stem_width_function,
        RS,
        CALL, /* s: stem new_dist */
        SWAP,
        DUP,
        ALIGNRP, /* align `stem_point' with `base_point' */
        DUP,
        MDAP_noround, /* set rp0 and rp1 to `stem_point' */
        SWAP,
        SHPIX, /* stem_point = base_point + new_dist */
        PUSHB_2,
        FunctionNumbers::bci_align_segments,
        1,
        SZP1, /* set zp1 to normal zone 1 */
        CALL,
        ENDF,
    ]);
}

/*
 * bci_action_link
 * bci_action_link_serif
 * bci_action_link_round
 * bci_action_link_round_serif
 *
 * Higher-level routines for calling `bci_link'.
 */

const FPGM_bci_action_link: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_link,
    FDEF,
    PUSHB_3,
    0,
    0,
    FunctionNumbers::bci_link,
    CALL,
    ENDF,
];

const FPGM_bci_action_link_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_link_serif,
    FDEF,
    PUSHB_3,
    0,
    1,
    FunctionNumbers::bci_link,
    CALL,
    ENDF,
];

const FPGM_bci_action_link_round: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_link_round,
    FDEF,
    PUSHB_3,
    1,
    0,
    FunctionNumbers::bci_link,
    CALL,
    ENDF,
];

const FPGM_bci_action_link_round_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_link_round_serif,
    FDEF,
    PUSHB_3,
    1,
    1,
    FunctionNumbers::bci_link,
    CALL,
    ENDF,
];

/*
 * bci_anchor
 *
 *   Handle the ANCHOR action to align two edges
 *   and to set the edge anchor.
 *
 *   The code after computing `cur_len' to shift `edge' and `edge2'
 *   is equivalent to the snippet below (part of `ta_latin_hint_edges'):
 *
 *      if cur_len < 96:
 *        if cur_len < = 64:
 *          u_off = 32
 *          d_off = 32
 *        else:
 *          u_off = 38
 *          d_off = 26
 *
 *        org_center = edge_orig + org_len / 2
 *        cur_pos1 = ROUND(org_center)
 *
 *        error1 = |org_center - (cur_pos1 - u_off)|
 *        error2 = |org_center - (cur_pos1 + d_off)|
 *        if (error1 < error2):
 *          cur_pos1 = cur_pos1 - u_off
 *        else:
 *          cur_pos1 = cur_pos1 + d_off
 *
 *        edge = cur_pos1 - cur_len / 2
 *        edge2 = edge + cur_len
 *
 *      else:
 *        edge = ROUND(edge_orig)
 *
 * in: edge2_is_serif
 *     edge_is_round
 *     edge_point (in twilight zone)
 *     edge2_point (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * sal: sal_anchor
 *      StorageAreaLocations::sal_temp1 as u8
 *      StorageAreaLocations::sal_temp2 as u8
 *      StorageAreaLocations::sal_temp3 as u8
 *      StorageAreaLocations::sal_base_delta as u8
 *
 * uses: func[sal_stem_width_function]
 *       FunctionNumbers::bci_roun as u8d
 *       FunctionNumbers::bci_align_segment as u8s
 */

// These are undef'd and redefined in the C, but it's the same definition as before.
// const SAL_U_OFF: u8 = StorageAreaLocations::sal_temp1 as u8;
// const SAL_D_OFF: u8 = StorageAreaLocations::sal_temp2 as u8;
// const SAL_ORG_LEN: u8 = StorageAreaLocations::sal_temp3 as u8;
// const SAL_EDGE2: u8 = StorageAreaLocations::sal_temp3 as u8;

fn emit_bci_anchor(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_anchor,
        FDEF,
        /* store anchor point number in `sal_anchor' */
        PUSHB_2,
        StorageAreaLocations::sal_anchor,
        4,
        CINDEX,
        WS, /* sal_anchor = edge_point */
        PUSHB_1,
        0,
        SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
        PUSHB_1,
        4,
        CINDEX,
        PUSHB_1,
        4,
        CINDEX,
        DUP,          /* s: edge2 edge is_round is_serif edge2 edge edge */
        MDAP_noround, /* set rp0 and rp1 to `edge_point' (for ALIGNRP below) */
    ]);
    bytecode.extend_bytes(MD_orig_ZP2_0); /* s: edge2 edge is_round is_serif org_len */
    bytecode.extend_bytes(&bytes![
        DUP,
        PUSHB_1,
        SAL_ORG_LEN,
        SWAP,
        WS,
        PUSHB_2,
        StorageAreaLocations::sal_base_delta, /* no base_delta needed here */
        0,
        WS,
        PUSHB_1,
        StorageAreaLocations::sal_stem_width_function,
        RS,
        CALL, /* s: edge2 edge cur_len */
        DUP,
        PUSHB_1,
        96,
        LT, /* cur_len < 96 */
        IF,
        DUP,
        PUSHB_1,
        64,
        LTEQ, /* cur_len <= 64 */
        IF,
        PUSHB_4,
        SAL_U_OFF,
        32,
        SAL_D_OFF,
        32,
        ELSE,
        PUSHB_4,
        SAL_U_OFF,
        38,
        SAL_D_OFF,
        26,
        EIF,
        WS,
        WS,
        SWAP, /* s: edge2 cur_len edge */
        DUP,  /* s: edge2 cur_len edge edge */
        GC_orig,
        PUSHB_1,
        SAL_ORG_LEN,
        RS,
    ]);
    bytecode.extend_bytes(DIV_BY_2);
    bytecode.extend_bytes(&bytes![
        ADD, /* s: edge2 cur_len edge org_center */
        DUP,
        PUSHB_1,
        FunctionNumbers::bci_round,
        CALL, /* s: edge2 cur_len edge org_center cur_pos1 */
        DUP,
        ROLL,
        ROLL,
        SUB, /* s: edge2 cur_len edge cur_pos1 (org_center - cur_pos1) */
        DUP,
        PUSHB_1,
        SAL_U_OFF,
        RS,
        ADD,
        ABS, /* s: edge2 cur_len edge cur_pos1 (org_center - cur_pos1) error1 */
        SWAP,
        PUSHB_1,
        SAL_D_OFF,
        RS,
        SUB,
        ABS, /* s: edge2 cur_len edge cur_pos1 error1 error2 */
        LT,  /* error1 < error2 */
        IF,
        PUSHB_1,
        SAL_U_OFF,
        RS,
        SUB, /* cur_pos1 = cur_pos1 - u_off */
        ELSE,
        PUSHB_1,
        SAL_D_OFF,
        RS,
        ADD, /* cur_pos1 = cur_pos1 + d_off */
        EIF, /* s: edge2 cur_len edge cur_pos1 */
        PUSHB_1,
        3,
        CINDEX,
    ]);
    bytecode.extend_bytes(DIV_BY_2);
    bytecode.extend_bytes(&bytes![
        SUB, /* s: edge2 cur_len edge (cur_pos1 - cur_len/2) */
        PUSHB_1,
        2,
        CINDEX, /* s: edge2 cur_len edge (cur_pos1 - cur_len/2) edge */
        GC_cur,
        SUB,
        SHPIX, /* edge = cur_pos1 - cur_len/2 */
        SWAP,  /* s: cur_len edge2 */
        DUP,
        ALIGNRP, /* align `edge2' with rp0 (still `edge') */
        SWAP,
        SHPIX, /* edge2 = edge1 + cur_len */
        ELSE,
        POP, /* s: edge2 edge */
        DUP,
        DUP,
        GC_cur,
        SWAP,
        GC_orig,
        PUSHB_1,
        FunctionNumbers::bci_round,
        CALL, /* s: edge2 edge edge_pos round(edge_orig_pos) */
        SWAP,
        SUB,
        SHPIX, /* edge = round(edge_orig) */
        /* clean up stack */
        POP,
        EIF,
        PUSHB_2,
        FunctionNumbers::bci_align_segments,
        1,
        SZP1, /* set zp1 to normal zone 1 */
        CALL,
        ENDF,
    ]);
}

/*
 * bci_action_anchor
 * bci_action_anchor_serif
 * bci_action_anchor_round
 * bci_action_anchor_round_serif
 *
 * Higher-level routines for calling `bci_anchor'.
 */

const FPGM_bci_action_anchor: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_anchor,
    FDEF,
    PUSHB_3,
    0,
    0,
    FunctionNumbers::bci_anchor,
    CALL,
    ENDF,
];

const FPGM_bci_action_anchor_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_anchor_serif,
    FDEF,
    PUSHB_3,
    0,
    1,
    FunctionNumbers::bci_anchor,
    CALL,
    ENDF,
];

const FPGM_bci_action_anchor_round: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_anchor_round,
    FDEF,
    PUSHB_3,
    1,
    0,
    FunctionNumbers::bci_anchor,
    CALL,
    ENDF,
];

const FPGM_bci_action_anchor_round_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_anchor_round_serif,
    FDEF,
    PUSHB_3,
    1,
    1,
    FunctionNumbers::bci_anchor,
    CALL,
    ENDF,
];

/*
 * bci_action_blue_anchor
 *
 *   Handle the BLUE_ANCHOR action to align an edge with a blue zone
 *   and to set the edge anchor.
 *
 * in: anchor_point (in twilight zone)
 *     blue_cvt_idx
 *     edge_point (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * sal: sal_anchor
 *
 * uses: bci_action_blue
 */

const FPGM_bci_action_blue_anchor: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_blue_anchor,
    FDEF,
    /* store anchor point number in `sal_anchor' */
    PUSHB_1,
    StorageAreaLocations::sal_anchor,
    SWAP,
    WS,
    PUSHB_1,
    FunctionNumbers::bci_action_blue,
    CALL,
    ENDF,
];

/*
 * bci_action_blue
 *
 *   Handle the BLUE action to align an edge with a blue zone.
 *
 * in: blue_cvt_idx
 *     edge_point (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_align_segments
 */

const FPGM_bci_action_blue: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_blue,
    FDEF,
    PUSHB_1,
    0,
    SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
    /* move `edge_point' to `blue_cvt_idx' position; */
    /* note that we can't use MIAP since this would modify */
    /* the twilight point's original coordinates also */
    RCVT,
    SWAP,
    DUP,
    MDAP_noround, /* set rp0 and rp1 to `edge' */
    DUP,
    GC_cur, /* s: new_pos edge edge_pos */
    ROLL,
    SWAP,
    SUB, /* s: edge (new_pos - edge_pos) */
    SHPIX,
    PUSHB_2,
    FunctionNumbers::bci_align_segments,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    CALL,
    ENDF,
];

/*
 * bci_serif_common
 *
 *   Common code for bci_action_serif routines.
 *
 * in: top_to_bottom_hinting
 *     serif
 *     base
 *
 * sal: sal_top_to_bottom_hinting
 *
 */

fn emit_bci_serif_common(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_serif_common,
        FDEF,
        PUSHB_1,
        0,
        SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
        PUSHB_1,
        StorageAreaLocations::sal_top_to_bottom_hinting,
        SWAP,
        WS,
        DUP,
        DUP,
        DUP,
        PUSHB_1,
        5,
        MINDEX, /* s: [...] serif serif serif serif base */
        DUP,
        MDAP_noround, /* set rp0 and rp1 to `base_point' */
    ]);
    bytecode.extend_bytes(MD_orig_ZP2_0);
    bytecode.extend_bytes(&bytes![
        SWAP, ALIGNRP, /* align `serif_point' with `base_point' */
        SHPIX,   /* serif = base + (serif_orig_pos - base_orig_pos) */
        ENDF,
    ]);
}

/*
 * bci_lower_bound
 *
 *   Move an edge if necessary to stay within a lower bound.
 *
 * in: edge
 *     bound
 *
 * sal: sal_top_to_bottom_hinting
 *
 * uses: bci_align_segments
 */

const FPGM_bci_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_lower_bound,
    FDEF,
    SWAP, /* s: edge bound */
    DUP,
    MDAP_noround, /* set rp0 and rp1 to `bound' */
    GC_cur,
    PUSHB_1,
    2,
    CINDEX,
    GC_cur, /* s: edge bound_pos edge_pos */
    PUSHB_1,
    StorageAreaLocations::sal_top_to_bottom_hinting,
    RS,
    IF,
    LT, /* edge_pos > bound_pos */
    ELSE,
    GT, /* edge_pos < bound_pos */
    EIF,
    IF,
    DUP,
    ALIGNRP, /* align `edge' to `bound' */
    EIF,
    MDAP_noround, /* set rp0 and rp1 to `edge_point' */
    PUSHB_2,
    FunctionNumbers::bci_align_segments,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    CALL,
    ENDF,
];

/*
 * bci_upper_bound
 *
 *   Move an edge if necessary to stay within an upper bound.
 *
 * in: edge
 *     bound
 *
 * sal: sal_top_to_bottom_hinting
 *
 * uses: bci_align_segments
 */

const FPGM_bci_upper_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_upper_bound,
    FDEF,
    SWAP, /* s: edge bound */
    DUP,
    MDAP_noround, /* set rp0 and rp1 to `bound' */
    GC_cur,
    PUSHB_1,
    2,
    CINDEX,
    GC_cur, /* s: edge bound_pos edge_pos */
    PUSHB_1,
    StorageAreaLocations::sal_top_to_bottom_hinting,
    RS,
    IF,
    GT, /* edge_pos < bound_pos */
    ELSE,
    LT, /* edge_pos > bound_pos */
    EIF,
    IF,
    DUP,
    ALIGNRP, /* align `edge' to `bound' */
    EIF,
    MDAP_noround, /* set rp0 and rp1 to `edge_point' */
    PUSHB_2,
    FunctionNumbers::bci_align_segments,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    CALL,
    ENDF,
];

/*
 * bci_upper_lower_bound
 *
 *   Move an edge if necessary to stay within a lower and lower bound.
 *
 * in: edge
 *     lower
 *     upper
 *
 * sal: sal_top_to_bottom_hinting
 *
 * uses: bci_align_segments
 */

const FPGM_bci_upper_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_upper_lower_bound,
    FDEF,
    SWAP, /* s: upper serif lower */
    DUP,
    MDAP_noround, /* set rp0 and rp1 to `lower' */
    GC_cur,
    PUSHB_1,
    2,
    CINDEX,
    GC_cur, /* s: upper serif lower_pos serif_pos */
    PUSHB_1,
    StorageAreaLocations::sal_top_to_bottom_hinting as u8,
    RS,
    IF,
    LT, /* serif_pos > lower_pos */
    ELSE,
    GT, /* serif_pos < lower_pos */
    EIF,
    IF,
    DUP,
    ALIGNRP, /* align `serif' to `lower' */
    EIF,
    SWAP, /* s: serif upper */
    DUP,
    MDAP_noround, /* set rp0 and rp1 to `upper' */
    GC_cur,
    PUSHB_1,
    2,
    CINDEX,
    GC_cur, /* s: serif upper_pos serif_pos */
    PUSHB_1,
    StorageAreaLocations::sal_top_to_bottom_hinting as u8,
    RS,
    IF,
    GT, /* serif_pos < upper_pos */
    ELSE,
    LT, /* serif_pos > upper_pos */
    EIF,
    IF,
    DUP,
    ALIGNRP, /* align `serif' to `upper' */
    EIF,
    MDAP_noround, /* set rp0 and rp1 to `serif_point' */
    PUSHB_2,
    FunctionNumbers::bci_align_segments as u8,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    CALL,
    ENDF,
];

/*
 * bci_action_serif
 *
 *   Handle the SERIF action to align a serif with its base.
 *
 * in: serif_point (in twilight zone)
 *     base_point (in twilight zone)
 *     ... stuff for bci_align_segments (serif) ...
 *
 * uses: bci_serif_common
 *       FunctionNumbers::bci_align_segment as u8s
 */

const FPGM_bci_action_serif: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_common as u8,
    CALL,
    MDAP_noround, /* set rp0 and rp1 to `serif_point' */
    PUSHB_2,
    FunctionNumbers::bci_align_segments as u8,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    CALL,
    ENDF,
];

/*
 * bci_action_serif_lower_bound
 *
 *   Handle the SERIF action to align a serif with its base, then moving it
 *   again if necessary to stay within a lower bound.
 *
 * in: serif_point (in twilight zone)
 *     base_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     ... stuff for bci_align_segments (serif) ...
 *
 * uses: bci_serif_common
 *       FunctionNumbers::bci_lower_boun as u8d
 */

const FPGM_bci_action_serif_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_lower_bound as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_upper_bound
 *
 *   Handle the SERIF action to align a serif with its base, then moving it
 *   again if necessary to stay within an upper bound.
 *
 * in: serif_point (in twilight zone)
 *     base_point (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (serif) ...
 *
 * uses: bci_serif_common
 *       FunctionNumbers::bci_upper_boun as u8d
 */

const FPGM_bci_action_serif_upper_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_upper_bound as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_upper_lower_bound
 *
 *   Handle the SERIF action to align a serif with its base, then moving it
 *   again if necessary to stay within a lower and upper bound.
 *
 * in: serif_point (in twilight zone)
 *     base_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (serif) ...
 *
 * uses: bci_serif_common
 *       FunctionNumbers::bci_upper_lower_boun as u8d
 */

const FPGM_bci_action_serif_upper_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_upper_lower_bound as u8,
    FDEF,
    PUSHB_1,
    0,
    SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_down_lower_bound
 *
 *   Handle the SERIF action to align a serif with its base, then moving it
 *   again if necessary to stay within a lower bound.  We hint top to
 *   bottom.
 *
 * in: serif_point (in twilight zone)
 *     base_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     ... stuff for bci_align_segments (serif) ...
 *
 * uses: bci_serif_common
 *       FunctionNumbers::bci_lower_boun as u8d
 */

const FPGM_bci_action_serif_down_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_down_lower_bound as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_serif_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_down_upper_bound
 *
 *   Handle the SERIF action to align a serif with its base, then moving it
 *   again if necessary to stay within an upper bound.  We hint top to
 *   bottom.
 *
 * in: serif_point (in twilight zone)
 *     base_point (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (serif) ...
 *
 * uses: bci_serif_common
 *       FunctionNumbers::bci_upper_boun as u8d
 */

const FPGM_bci_action_serif_down_upper_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_down_upper_bound as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_serif_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_down_upper_lower_bound
 *
 *   Handle the SERIF action to align a serif with its base, then moving it
 *   again if necessary to stay within a lower and upper bound.  We hint top
 *   to bottom.
 *
 * in: serif_point (in twilight zone)
 *     base_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (serif) ...
 *
 * uses: bci_serif_common
 *       FunctionNumbers::bci_upper_lower_boun as u8d
 */

const FPGM_bci_action_serif_down_upper_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_down_upper_lower_bound as u8,
    FDEF,
    PUSHB_1,
    0,
    SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
    PUSHB_2,
    1,
    FunctionNumbers::bci_serif_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_serif_anchor_common
 *
 *   Common code for bci_action_serif_anchor routines.
 *
 * in: top_to_bottom_hinting
 *     edge
 *
 * out: edge (adjusted)
 *
 * sal: sal_anchor
 *      StorageAreaLocations::sal_top_to_bottom_hinting as u8
 *
 * uses: bci_round
 */

const FPGM_bci_serif_anchor_common: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_serif_anchor_common as u8,
    FDEF,
    PUSHB_1,
    0,
    SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
    PUSHB_1,
    StorageAreaLocations::sal_top_to_bottom_hinting as u8,
    SWAP,
    WS,
    DUP,
    PUSHB_1,
    StorageAreaLocations::sal_anchor as u8,
    SWAP,
    WS, /* sal_anchor = edge_point */
    DUP,
    DUP,
    DUP,
    GC_cur,
    SWAP,
    GC_orig,
    PUSHB_1,
    FunctionNumbers::bci_round as u8,
    CALL, /* s: [...] edge edge edge_pos round(edge_orig_pos) */
    SWAP,
    SUB,
    SHPIX, /* edge = round(edge_orig) */
    ENDF,
];

/*
 * bci_action_serif_anchor
 *
 *   Handle the SERIF_ANCHOR action to align a serif and to set the edge
 *   anchor.
 *
 * in: edge_point (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_anchor_common
 *       FunctionNumbers::bci_align_segment as u8s
 */

const FPGM_bci_action_serif_anchor: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_anchor as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_anchor_common as u8,
    CALL,
    MDAP_noround, /* set rp0 and rp1 to `edge' */
    PUSHB_2,
    FunctionNumbers::bci_align_segments as u8,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    CALL,
    ENDF,
];

/*
 * bci_action_serif_anchor_lower_bound
 *
 *   Handle the SERIF_ANCHOR action to align a serif and to set the edge
 *   anchor, then moving it again if necessary to stay within a lower
 *   bound.
 *
 * in: edge_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_anchor_common
 *       FunctionNumbers::bci_lower_boun as u8d
 */

const FPGM_bci_action_serif_anchor_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_anchor_lower_bound as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_anchor_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_anchor_upper_bound
 *
 *   Handle the SERIF_ANCHOR action to align a serif and to set the edge
 *   anchor, then moving it again if necessary to stay within an upper
 *   bound.
 *
 * in: edge_point (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_anchor_common
 *       FunctionNumbers::bci_upper_boun as u8d
 */

const FPGM_bci_action_serif_anchor_upper_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_anchor_upper_bound as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_anchor_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_anchor_upper_lower_bound
 *
 *   Handle the SERIF_ANCHOR action to align a serif and to set the edge
 *   anchor, then moving it again if necessary to stay within a lower and
 *   upper bound.
 *
 * in: edge_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_anchor_common
 *       FunctionNumbers::bci_upper_lower_boun as u8d
 */

const FPGM_bci_action_serif_anchor_upper_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_anchor_upper_lower_bound as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_anchor_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_anchor_down_lower_bound
 *
 *   Handle the SERIF_ANCHOR action to align a serif and to set the edge
 *   anchor, then moving it again if necessary to stay within a lower
 *   bound.  We hint top to bottom.
 *
 * in: edge_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_anchor_common
 *       FunctionNumbers::bci_lower_boun as u8d
 */

const FPGM_bci_action_serif_anchor_down_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_anchor_down_lower_bound as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_serif_anchor_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_anchor_down_upper_bound
 *
 *   Handle the SERIF_ANCHOR action to align a serif and to set the edge
 *   anchor, then moving it again if necessary to stay within an upper
 *   bound.  We hint top to bottom.
 *
 * in: edge_point (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_anchor_common
 *       FunctionNumbers::bci_upper_boun as u8d
 */

const FPGM_bci_action_serif_anchor_down_upper_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_anchor_down_upper_bound as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_serif_anchor_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_anchor_down_upper_lower_bound
 *
 *   Handle the SERIF_ANCHOR action to align a serif and to set the edge
 *   anchor, then moving it again if necessary to stay within a lower and
 *   upper bound.  We hint top to bottom.
 *
 * in: edge_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_anchor_common
 *       FunctionNumbers::bci_upper_lower_boun as u8d
 */

const FPGM_bci_action_serif_anchor_down_upper_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_anchor_down_upper_lower_bound as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_serif_anchor_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_serif_link1_common
 *
 *   Common code for bci_action_serif_link1 routines.
 *
 * in: top_to_bottom_hinting
 *     before
 *     edge
 *     after
 *
 * out: edge (adjusted)
 *
 * sal: sal_top_to_bottom_hinting
 *
 */

fn emit_bci_serif_link1_common(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_serif_link1_common as u8,
        FDEF,
        PUSHB_1,
        0,
        SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
        PUSHB_1,
        StorageAreaLocations::sal_top_to_bottom_hinting as u8,
        SWAP,
        WS,
        PUSHB_1,
        3,
        CINDEX, /* s: [...] after edge before after */
        PUSHB_1,
        2,
        CINDEX, /* s: [...] after edge before after before */
    ]);
    bytecode.extend_bytes(MD_orig_ZP2_0);
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        0,
        EQ,           /* after_orig_pos == before_orig_pos */
        IF,           /* s: [...] after edge before */
        MDAP_noround, /* set rp0 and rp1 to `before' */
        DUP,
        ALIGNRP, /* align `edge' with `before' */
        SWAP,
        POP,
        ELSE,
        /* we have to execute `a*b/c', with b/c very near to 1: */
        /* to avoid overflow while retaining precision, */
        /* we transform this to `a + a * (b-c)/c' */
        PUSHB_1,
        2,
        CINDEX, /* s: [...] after edge before edge */
        PUSHB_1,
        2,
        CINDEX, /* s: [...] after edge before edge before */
    ]);
    bytecode.extend_bytes(MD_orig_ZP2_0 /* a = edge_orig_pos - before_orig_pos */);
    bytecode.extend_bytes(&bytes![
        DUP, PUSHB_1, 5, CINDEX, /* s: [...] after edge before a a after */
        PUSHB_1, 4, CINDEX, /* s: [...] after edge before a a after before */
    ]);
    bytecode.extend_bytes(
        MD_orig_ZP2_0, /* c = after_orig_pos - before_orig_pos */
    );
    bytecode.extend_bytes(&bytes![
        PUSHB_1, 6, CINDEX, /* s: [...] after edge before a a c after */
        PUSHB_1, 5, CINDEX, /* s: [...] after edge before a a c after before */
        MD_cur, /* b = after_pos - before_pos */
        PUSHB_1, 2, CINDEX, /* s: [...] after edge before a a c b c */
        SUB,    /* b-c */
        PUSHW_2, 0x08, /* 0x800 */
        0x00, 0x08, /* 0x800 */
        0x00, MUL, /* 0x10000 */
        MUL, /* (b-c) in 16.16 format */
        SWAP, DUP, IF,  /* c != 0 ? */
        DIV, /* s: [...] after edge before a a (b-c)/c */
        ELSE, POP, /* avoid division by zero */
        EIF, MUL, /* a * (b-c)/c * 2^10 */
    ]);
    bytecode.extend_bytes(DIV_BY_1024); /* a * (b-c)/c */
    bytecode.extend_bytes(&bytes![
        ADD, /* a*b/c */
        SWAP,
        MDAP_noround, /* set rp0 and rp1 to `before' */
        SWAP,         /* s: [...] after a*b/c edge */
        DUP,
        DUP,
        ALIGNRP, /* align `edge' with `before' */
        ROLL,
        SHPIX, /* shift `edge' by `a*b/c' */
        SWAP,  /* s: [...] edge after */
        POP,
        EIF,
        ENDF,
    ]);
}

/*
 * bci_action_serif_link1
 *
 *   Handle the SERIF_LINK1 action to align a serif, depending on edges
 *   before and after.
 *
 * in: before_point (in twilight zone)
 *     edge_point (in twilight zone)
 *     after_point (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link1_common
 *       FunctionNumbers::bci_align_segment as u8s
 */

const FPGM_bci_action_serif_link1: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link1 as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_link1_common as u8,
    CALL,
    MDAP_noround, /* set rp0 and rp1 to `edge' */
    PUSHB_2,
    FunctionNumbers::bci_align_segments as u8,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    CALL,
    ENDF,
];

/*
 * bci_action_serif_link1_lower_bound
 *
 *   Handle the SERIF_LINK1 action to align a serif, depending on edges
 *   before and after.  Additionally, move the serif again if necessary to
 *   stay within a lower bound.
 *
 * in: before_point (in twilight zone)
 *     edge_point (in twilight zone)
 *     after_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link1_common
 *       FunctionNumbers::bci_lower_boun as u8d
 */

const FPGM_bci_action_serif_link1_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link1_lower_bound as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_link1_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_link1_upper_bound
 *
 *   Handle the SERIF_LINK1 action to align a serif, depending on edges
 *   before and after.  Additionally, move the serif again if necessary to
 *   stay within an upper bound.
 *
 * in: before_point (in twilight zone)
 *     edge_point (in twilight zone)
 *     after_point (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link1_common
 *       FunctionNumbers::bci_upper_boun as u8d
 */

const FPGM_bci_action_serif_link1_upper_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link1_upper_bound as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_link1_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_link1_upper_lower_bound
 *
 *   Handle the SERIF_LINK1 action to align a serif, depending on edges
 *   before and after.  Additionally, move the serif again if necessary to
 *   stay within a lower and upper bound.
 *
 * in: before_point (in twilight zone)
 *     edge_point (in twilight zone)
 *     after_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link1_common
 *       FunctionNumbers::bci_upper_lower_boun as u8d
 */

const FPGM_bci_action_serif_link1_upper_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link1_upper_lower_bound as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_link1_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_link1_down_lower_bound
 *
 *   Handle the SERIF_LINK1 action to align a serif, depending on edges
 *   before and after.  Additionally, move the serif again if necessary to
 *   stay within a lower bound.  We hint top to bottom.
 *
 * in: before_point (in twilight zone)
 *     edge_point (in twilight zone)
 *     after_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link1_common
 *       FunctionNumbers::bci_lower_boun as u8d
 */

const FPGM_bci_action_serif_link1_down_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link1_down_lower_bound as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_serif_link1_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_link1_down_upper_bound
 *
 *   Handle the SERIF_LINK1 action to align a serif, depending on edges
 *   before and after.  Additionally, move the serif again if necessary to
 *   stay within an upper bound.  We hint top to bottom.
 *
 * in: before_point (in twilight zone)
 *     edge_point (in twilight zone)
 *     after_point (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link1_common
 *       FunctionNumbers::bci_upper_boun as u8d
 */

const FPGM_bci_action_serif_link1_down_upper_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link1_down_upper_bound as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_serif_link1_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_link1_down_upper_lower_bound
 *
 *   Handle the SERIF_LINK1 action to align a serif, depending on edges
 *   before and after.  Additionally, move the serif again if necessary to
 *   stay within a lower and upper bound.  We hint top to bottom.
 *
 * in: before_point (in twilight zone)
 *     edge_point (in twilight zone)
 *     after_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link1_common
 *       FunctionNumbers::bci_upper_lower_boun as u8d
 */

const FPGM_bci_action_serif_link1_down_upper_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link1_down_upper_lower_bound as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_serif_link1_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_serif_link2_common
 *
 *   Common code for bci_action_serif_link2 routines.
 *
 * in: top_to_bottom_hinting
 *     edge
 *
 * out: edge (adjusted)
 *
 * sal: sal_anchor
 *      StorageAreaLocations::sal_top_to_bottom_hinting as u8
 *
 */

fn emit_bci_serif_link2_common(bytecode: &mut Bytecode) {
    bytecode.extend_bytes(&bytes![
        PUSHB_1,
        FunctionNumbers::bci_serif_link2_common as u8,
        FDEF,
        PUSHB_1,
        0,
        SZPS, /* set zp0, zp1, and zp2 to twilight zone 0 */
        PUSHB_1,
        StorageAreaLocations::sal_top_to_bottom_hinting as u8,
        SWAP,
        WS,
        DUP, /* s: [...] edge edge */
        PUSHB_1,
        StorageAreaLocations::sal_anchor as u8,
        RS,
        DUP,          /* s: [...] edge edge anchor anchor */
        MDAP_noround, /* set rp0 and rp1 to `sal_anchor' */
    ]);
    bytecode.extend_bytes(MD_orig_ZP2_0);
    bytecode.extend_bytes(&bytes![DUP, ADD, PUSHB_1, 32, ADD, FLOOR,]);
    bytecode.extend_bytes(
        DIV_BY_2, /* delta = (edge_orig_pos - anchor_orig_pos + 16) & ~31 */
    );
    bytecode.extend_bytes(&bytes![
        SWAP, DUP, DUP, ALIGNRP, /* align `edge' with `sal_anchor' */
        ROLL, SHPIX, /* shift `edge' by `delta' */
        ENDF,
    ]);
}

/*
 * bci_action_serif_link2
 *
 *   Handle the SERIF_LINK2 action to align a serif relative to the anchor.
 *
 * in: edge_point (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link2_common
 *       FunctionNumbers::bci_align_segment as u8s
 */

const FPGM_bci_action_serif_link2: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link2 as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_link2_common as u8,
    CALL,
    MDAP_noround, /* set rp0 and rp1 to `edge' */
    PUSHB_2,
    FunctionNumbers::bci_align_segments as u8,
    1,
    SZP1, /* set zp1 to normal zone 1 */
    CALL,
    ENDF,
];

/*
 * bci_action_serif_link2_lower_bound
 *
 *   Handle the SERIF_LINK2 action to align a serif relative to the anchor.
 *   Additionally, move the serif again if necessary to stay within a lower
 *   bound.
 *
 * in: edge_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link2_common
 *       FunctionNumbers::bci_lower_boun as u8d
 */

const FPGM_bci_action_serif_link2_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link2_lower_bound as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_link2_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_link2_upper_bound
 *
 *   Handle the SERIF_LINK2 action to align a serif relative to the anchor.
 *   Additionally, move the serif again if necessary to stay within an upper
 *   bound.
 *
 * in: edge_point (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link2_common
 *       FunctionNumbers::bci_upper_boun as u8d
 */

const FPGM_bci_action_serif_link2_upper_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link2_upper_bound as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_link2_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_link2_upper_lower_bound
 *
 *   Handle the SERIF_LINK2 action to align a serif relative to the anchor.
 *   Additionally, move the serif again if necessary to stay within a lower
 *   and upper bound.
 *
 * in: edge_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link2_common
 *       FunctionNumbers::bci_upper_lower_boun as u8d
 */

const FPGM_bci_action_serif_link2_upper_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link2_upper_lower_bound as u8,
    FDEF,
    PUSHB_2,
    0,
    FunctionNumbers::bci_serif_link2_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_link2_down_lower_bound
 *
 *   Handle the SERIF_LINK2 action to align a serif relative to the anchor.
 *   Additionally, move the serif again if necessary to stay within a lower
 *   bound.  We hint top to bottom.
 *
 * in: edge_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link2_common
 *       FunctionNumbers::bci_lower_boun as u8d
 */

const FPGM_bci_action_serif_link2_down_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link2_down_lower_bound as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_serif_link2_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_link2_down_upper_bound
 *
 *   Handle the SERIF_LINK2 action to align a serif relative to the anchor.
 *   Additionally, move the serif again if necessary to stay within an upper
 *   bound.  We hint top to bottom.
 *
 * in: edge_point (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link2_common
 *       FunctionNumbers::bci_upper_boun as u8d
 */

const FPGM_bci_action_serif_link2_down_upper_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link2_down_upper_bound as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_serif_link2_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_action_serif_link2_down_upper_lower_bound
 *
 *   Handle the SERIF_LINK2 action to align a serif relative to the anchor.
 *   Additionally, move the serif again if necessary to stay within a lower
 *   and upper bound.  We hint top to bottom.
 *
 * in: edge_point (in twilight zone)
 *     edge[-1] (in twilight zone)
 *     edge[1] (in twilight zone)
 *     ... stuff for bci_align_segments (edge) ...
 *
 * uses: bci_serif_link2_common
 *       FunctionNumbers::bci_upper_lower_boun as u8d
 */

const FPGM_bci_action_serif_link2_down_upper_lower_bound: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_action_serif_link2_down_upper_lower_bound as u8,
    FDEF,
    PUSHB_2,
    1,
    FunctionNumbers::bci_serif_link2_common as u8,
    CALL,
    PUSHB_1,
    FunctionNumbers::bci_upper_lower_bound as u8,
    CALL,
    ENDF,
];

/*
 * bci_hint_glyph
 *
 *   This is the top-level glyph hinting function which parses the arguments
 *   on the stack and calls subroutines.
 *
 * in: action_0_func_idx
 *       ... data ...
 *     action_1_func_idx
 *       ... data ...
 *     ...
 *
 * CVT: cvtl_is_subglyph
 *      cvtl_stem_width_mode
 *      cvtl_do_iup_y
 *
 * sal: sal_stem_width_function
 *
 * uses: bci_action_ip_before
 *       FunctionNumbers::bci_action_ip_afte as u8r
 *       FunctionNumbers::bci_action_ip_o as u8n
 *       FunctionNumbers::bci_action_ip_betwee as u8n
 *
 *       FunctionNumbers::bci_action_adjust_boun as u8d
 *       FunctionNumbers::bci_action_adjust_bound_seri as u8f
 *       FunctionNumbers::bci_action_adjust_bound_roun as u8d
 *       FunctionNumbers::bci_action_adjust_bound_round_seri as u8f
 *
 *       FunctionNumbers::bci_action_stem_boun as u8d
 *       FunctionNumbers::bci_action_stem_bound_seri as u8f
 *       FunctionNumbers::bci_action_stem_bound_roun as u8d
 *       FunctionNumbers::bci_action_stem_bound_round_seri as u8f
 *
 *       FunctionNumbers::bci_action_lin as u8k
 *       FunctionNumbers::bci_action_link_seri as u8f
 *       FunctionNumbers::bci_action_link_roun as u8d
 *       FunctionNumbers::bci_action_link_round_seri as u8f
 *
 *       FunctionNumbers::bci_action_ancho as u8r
 *       FunctionNumbers::bci_action_anchor_seri as u8f
 *       FunctionNumbers::bci_action_anchor_roun as u8d
 *       FunctionNumbers::bci_action_anchor_round_seri as u8f
 *
 *       FunctionNumbers::bci_action_blue_ancho as u8r
 *
 *       FunctionNumbers::bci_action_adjus as u8t
 *       FunctionNumbers::bci_action_adjust_seri as u8f
 *       FunctionNumbers::bci_action_adjust_roun as u8d
 *       FunctionNumbers::bci_action_adjust_round_seri as u8f
 *
 *       FunctionNumbers::bci_action_ste as u8m
 *       FunctionNumbers::bci_action_stem_seri as u8f
 *       FunctionNumbers::bci_action_stem_roun as u8d
 *       FunctionNumbers::bci_action_stem_round_seri as u8f
 *
 *       FunctionNumbers::bci_action_blu as u8e
 *
 *       FunctionNumbers::bci_action_seri as u8f
 *       FunctionNumbers::bci_action_serif_lower_boun as u8d
 *       FunctionNumbers::bci_action_serif_upper_boun as u8d
 *       FunctionNumbers::bci_action_serif_upper_lower_boun as u8d
 *
 *       FunctionNumbers::bci_action_serif_ancho as u8r
 *       FunctionNumbers::bci_action_serif_anchor_lower_boun as u8d
 *       FunctionNumbers::bci_action_serif_anchor_upper_boun as u8d
 *       FunctionNumbers::bci_action_serif_anchor_upper_lower_boun as u8d
 *
 *       FunctionNumbers::bci_action_serif_link as u81
 *       FunctionNumbers::bci_action_serif_link1_lower_boun as u8d
 *       FunctionNumbers::bci_action_serif_link1_upper_boun as u8d
 *       FunctionNumbers::bci_action_serif_link1_upper_lower_boun as u8d
 *
 *       FunctionNumbers::bci_action_serif_link as u82
 *       FunctionNumbers::bci_action_serif_link2_lower_boun as u8d
 *       FunctionNumbers::bci_action_serif_link2_upper_boun as u8d
 *       FunctionNumbers::bci_action_serif_link2_upper_lower_boun as u8d
 */

const FPGM_bci_hint_glyph: &[u8] = &bytes![
    PUSHB_1,
    FunctionNumbers::bci_hint_glyph as u8,
    FDEF,
    /*
     * set up stem width function based on flag in CVT:
     *
     *    < 0: bci_natural_stem_width
     *   == 0: bci_smooth_stem_width
     *    > 0: bci_strong_stem_width
     */
    PUSHB_3,
    StorageAreaLocations::sal_stem_width_function as u8,
    0,
    CvtLocations::cvtl_stem_width_mode as u8,
    RCVT,
    LT,
    IF,
    PUSHB_1,
    FunctionNumbers::bci_strong_stem_width as u8,
    ELSE,
    PUSHB_3,
    FunctionNumbers::bci_smooth_stem_width as u8,
    FunctionNumbers::bci_natural_stem_width as u8,
    CvtLocations::cvtl_stem_width_mode as u8,
    RCVT,
    IF,
    SWAP,
    POP,
    ELSE,
    POP,
    EIF,
    EIF,
    WS,
    /* start_loop: */
    /* loop until all data on stack is used */
    CALL,
    PUSHB_1,
    8,
    NEG,
    PUSHB_1,
    3,
    DEPTH,
    LT,
    JROT, /* goto start_loop */
    PUSHB_2,
    CvtLocations::cvtl_do_iup_y as u8,
    1,
    SZP2, /* set zp2 to normal zone 1 */
    RCVT,
    IF,
    IUP_y,
    EIF,
    ENDF,
];

fn build_fpgm(
    glyphdata: &GlyfData,
    increase_x_height: u32,
    has_control_data: bool,
    font_fallback_style: usize,
) -> Bytecode {
    let fallback_style: u32 =
        CVT_SCALING_VALUE_OFFSET(0) as u32 + glyphdata.style_ids[font_fallback_style];
    let num_used_styles = glyphdata.num_used_styles as u8;
    let mut bytecode = Bytecode::new();

    bytecode.extend_bytes(FPGM_bci_align_x_height_a);
    if increase_x_height > 0 {
        bytecode.extend_bytes(FPGM_bci_align_x_height_b1a);
        bytecode.push_word(increase_x_height);
        bytecode.extend_bytes(FPGM_bci_align_x_height_b1b);
    } else {
        bytecode.extend_bytes(FPGM_bci_align_x_height_b2);
    }
    bytecode.extend_bytes(FPGM_bci_align_x_height_c);

    bytecode.extend_bytes(FPGM_bci_round);
    bytecode.extend_bytes(FPGM_bci_natural_stem_width);
    bytecode.extend_bytes(FPGM_bci_quantize_stem_width);
    bytecode.extend_bytes(FPGM_bci_smooth_stem_width);
    bytecode.extend_bytes(FPGM_bci_get_best_width);
    bytecode.extend_bytes(FPGM_bci_strong_stem_width_a);
    bytecode.push_u8(num_used_styles);
    bytecode.extend_bytes(FPGM_bci_strong_stem_width_b);
    bytecode.extend_bytes(FPGM_bci_loop_do);
    emit_bci_loop(&mut bytecode);
    emit_bci_cvt_rescale(&mut bytecode);
    bytecode.extend_bytes(FPGM_bci_cvt_rescale_range);
    bytecode.extend_bytes(FPGM_bci_vwidth_data_store);
    bytecode.extend_bytes(FPGM_bci_smooth_blue_round);
    bytecode.extend_bytes(FPGM_bci_strong_blue_round);
    bytecode.extend_bytes(FPGM_bci_blue_round_range);
    bytecode.extend_bytes(FPGM_bci_decrement_component_counter);
    bytecode.extend_bytes(FPGM_bci_get_point_extrema);
    bytecode.extend_bytes(FPGM_bci_nibbles);
    bytecode.extend_bytes(FPGM_bci_number_set_is_element);
    bytecode.extend_bytes(FPGM_bci_number_set_is_element2);

    emit_bci_create_segment(&mut bytecode);
    bytecode.extend_bytes(FPGM_bci_create_segments_a);
    bytecode.push_u8(num_used_styles);
    bytecode.extend_bytes(FPGM_bci_create_segments_b);
    if has_control_data {
        bytecode.extend_bytes(FPGM_bci_create_segments_c);
    }
    bytecode.extend_bytes(FPGM_bci_create_segments_d);

    bytecode.extend_bytes(FPGM_bci_create_segments_0);
    bytecode.extend_bytes(FPGM_bci_create_segments_1);
    bytecode.extend_bytes(FPGM_bci_create_segments_2);
    bytecode.extend_bytes(FPGM_bci_create_segments_3);
    bytecode.extend_bytes(FPGM_bci_create_segments_4);
    bytecode.extend_bytes(FPGM_bci_create_segments_5);
    bytecode.extend_bytes(FPGM_bci_create_segments_6);
    bytecode.extend_bytes(FPGM_bci_create_segments_7);
    bytecode.extend_bytes(FPGM_bci_create_segments_8);
    bytecode.extend_bytes(FPGM_bci_create_segments_9);

    bytecode.extend_bytes(FPGM_bci_deltap1);
    bytecode.extend_bytes(FPGM_bci_deltap2);
    bytecode.extend_bytes(FPGM_bci_deltap3);

    bytecode.extend_bytes(FPGM_bci_create_segments_composite_a);
    bytecode.push_u8(num_used_styles);
    bytecode.extend_bytes(FPGM_bci_create_segments_composite_b);
    if has_control_data {
        bytecode.extend_bytes(FPGM_bci_create_segments_composite_c);
    }
    bytecode.extend_bytes(FPGM_bci_create_segments_composite_d);

    bytecode.extend_bytes(FPGM_bci_create_segments_composite_0);
    bytecode.extend_bytes(FPGM_bci_create_segments_composite_1);
    bytecode.extend_bytes(FPGM_bci_create_segments_composite_2);
    bytecode.extend_bytes(FPGM_bci_create_segments_composite_3);
    bytecode.extend_bytes(FPGM_bci_create_segments_composite_4);
    bytecode.extend_bytes(FPGM_bci_create_segments_composite_5);
    bytecode.extend_bytes(FPGM_bci_create_segments_composite_6);
    bytecode.extend_bytes(FPGM_bci_create_segments_composite_7);
    bytecode.extend_bytes(FPGM_bci_create_segments_composite_8);
    bytecode.extend_bytes(FPGM_bci_create_segments_composite_9);

    bytecode.extend_bytes(FPGM_bci_align_point);
    bytecode.extend_bytes(FPGM_bci_align_segment);
    bytecode.extend_bytes(FPGM_bci_align_segments);

    emit_bci_scale_contour(&mut bytecode);
    bytecode.extend_bytes(FPGM_bci_scale_glyph_a);
    bytecode.push_u8(fallback_style as u8);
    bytecode.extend_bytes(FPGM_bci_scale_glyph_b);
    bytecode.extend_bytes(FPGM_bci_scale_composite_glyph_a);
    bytecode.push_u8(fallback_style as u8);
    bytecode.extend_bytes(FPGM_bci_scale_composite_glyph_b);
    bytecode.extend_bytes(FPGM_bci_shift_contour);
    bytecode.extend_bytes(FPGM_bci_shift_subglyph_a);
    bytecode.push_u8(fallback_style as u8);
    emit_bci_shift_subglyph_b(&mut bytecode);
    if has_control_data {
        bytecode.extend_bytes(FPGM_bci_shift_subglyph_c);
    }
    bytecode.extend_bytes(FPGM_bci_shift_subglyph_d);

    emit_bci_ip_outer_align_point(&mut bytecode);
    bytecode.extend_bytes(FPGM_bci_ip_on_align_points);
    emit_bci_ip_between_align_point(&mut bytecode);
    emit_bci_ip_between_align_points(&mut bytecode);

    emit_bci_adjust_common(&mut bytecode);
    emit_bci_stem_common(&mut bytecode);
    emit_bci_serif_common(&mut bytecode);
    bytecode.extend_bytes(FPGM_bci_serif_anchor_common);
    emit_bci_serif_link1_common(&mut bytecode);
    emit_bci_serif_link2_common(&mut bytecode);

    bytecode.extend_bytes(FPGM_bci_lower_bound);
    bytecode.extend_bytes(FPGM_bci_upper_bound);
    bytecode.extend_bytes(FPGM_bci_upper_lower_bound);

    bytecode.extend_bytes(FPGM_bci_adjust_bound);
    bytecode.extend_bytes(FPGM_bci_stem_bound);
    emit_bci_link(&mut bytecode);
    emit_bci_anchor(&mut bytecode);
    bytecode.extend_bytes(FPGM_bci_adjust);
    bytecode.extend_bytes(FPGM_bci_stem);

    bytecode.extend_bytes(FPGM_bci_action_ip_before);
    bytecode.extend_bytes(FPGM_bci_action_ip_after);
    bytecode.extend_bytes(FPGM_bci_action_ip_on);
    bytecode.extend_bytes(FPGM_bci_action_ip_between);

    bytecode.extend_bytes(FPGM_bci_action_blue);
    bytecode.extend_bytes(FPGM_bci_action_blue_anchor);

    bytecode.extend_bytes(FPGM_bci_action_anchor);
    bytecode.extend_bytes(FPGM_bci_action_anchor_serif);
    bytecode.extend_bytes(FPGM_bci_action_anchor_round);
    bytecode.extend_bytes(FPGM_bci_action_anchor_round_serif);

    bytecode.extend_bytes(FPGM_bci_action_adjust);
    bytecode.extend_bytes(FPGM_bci_action_adjust_serif);
    bytecode.extend_bytes(FPGM_bci_action_adjust_round);
    bytecode.extend_bytes(FPGM_bci_action_adjust_round_serif);
    bytecode.extend_bytes(FPGM_bci_action_adjust_bound);
    bytecode.extend_bytes(FPGM_bci_action_adjust_bound_serif);
    bytecode.extend_bytes(FPGM_bci_action_adjust_bound_round);
    bytecode.extend_bytes(FPGM_bci_action_adjust_bound_round_serif);
    bytecode.extend_bytes(FPGM_bci_action_adjust_down_bound);
    bytecode.extend_bytes(FPGM_bci_action_adjust_down_bound_serif);
    bytecode.extend_bytes(FPGM_bci_action_adjust_down_bound_round);
    bytecode.extend_bytes(FPGM_bci_action_adjust_down_bound_round_serif);

    bytecode.extend_bytes(FPGM_bci_action_link);
    bytecode.extend_bytes(FPGM_bci_action_link_serif);
    bytecode.extend_bytes(FPGM_bci_action_link_round);
    bytecode.extend_bytes(FPGM_bci_action_link_round_serif);

    bytecode.extend_bytes(FPGM_bci_action_stem);
    bytecode.extend_bytes(FPGM_bci_action_stem_serif);
    bytecode.extend_bytes(FPGM_bci_action_stem_round);
    bytecode.extend_bytes(FPGM_bci_action_stem_round_serif);
    bytecode.extend_bytes(FPGM_bci_action_stem_bound);
    bytecode.extend_bytes(FPGM_bci_action_stem_bound_serif);
    bytecode.extend_bytes(FPGM_bci_action_stem_bound_round);
    bytecode.extend_bytes(FPGM_bci_action_stem_bound_round_serif);
    bytecode.extend_bytes(FPGM_bci_action_stem_down_bound);
    bytecode.extend_bytes(FPGM_bci_action_stem_down_bound_serif);
    bytecode.extend_bytes(FPGM_bci_action_stem_down_bound_round);
    bytecode.extend_bytes(FPGM_bci_action_stem_down_bound_round_serif);

    bytecode.extend_bytes(FPGM_bci_action_serif);
    bytecode.extend_bytes(FPGM_bci_action_serif_lower_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_upper_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_upper_lower_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_down_lower_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_down_upper_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_down_upper_lower_bound);

    bytecode.extend_bytes(FPGM_bci_action_serif_anchor);
    bytecode.extend_bytes(FPGM_bci_action_serif_anchor_lower_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_anchor_upper_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_anchor_upper_lower_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_anchor_down_lower_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_anchor_down_upper_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_anchor_down_upper_lower_bound);

    bytecode.extend_bytes(FPGM_bci_action_serif_link1);
    bytecode.extend_bytes(FPGM_bci_action_serif_link1_lower_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_link1_upper_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_link1_upper_lower_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_link1_down_lower_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_link1_down_upper_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_link1_down_upper_lower_bound);

    bytecode.extend_bytes(FPGM_bci_action_serif_link2);
    bytecode.extend_bytes(FPGM_bci_action_serif_link2_lower_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_link2_upper_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_link2_upper_lower_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_link2_down_lower_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_link2_down_upper_bound);
    bytecode.extend_bytes(FPGM_bci_action_serif_link2_down_upper_lower_bound);

    bytecode.extend_bytes(FPGM_bci_hint_glyph);
    bytecode
}

pub(crate) fn build_fpgm_table(
    font: &mut Font,
    glyf_data: &GlyfData,
    increase_x_height: u32,
    has_control_data: bool,
    font_fallback_style: usize,
) -> Result<usize, AutohintError> {
    if font.get_processed(Tag::new(b"glyf")) {
        return Ok(0);
    }

    let bytecode = build_fpgm(
        glyf_data,
        increase_x_height,
        has_control_data,
        font_fallback_style,
    );
    let out: Vec<u8> = bytecode.as_slice().to_vec();
    let fpgm_len = out.len();
    font.update_table(Tag::new(b"fpgm"), &out);

    Ok(fpgm_len)
}
