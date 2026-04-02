#![allow(non_upper_case_globals, dead_code, non_snake_case)]

#[macro_export]
macro_rules! bytes {
    ($($x:expr),* $(,)?) => {
        [$(($x) as u8),*]
    };
}

/// set freedom and projection vectors to y axis
pub(crate) const SVTCA_y: u8 = 0x00;
/// set freedom and projection vectors to x axis
pub(crate) const SVTCA_x: u8 = 0x01;
/// set projection vector to y axis
pub(crate) const SPVTCA_y: u8 = 0x02;
/// set projection vector to x axis
pub(crate) const SPVTCA_x: u8 = 0x03;
/// set freedom vector to y axis
pub(crate) const SFVTCA_y: u8 = 0x04;
/// set freedom vector to x axis
pub(crate) const SFVTCA_x: u8 = 0x05;
/// set projection vector parallel to line
pub(crate) const SPVTL_para: u8 = 0x06;
/// set projection vector perpendicular to line
pub(crate) const SPVTL_perp: u8 = 0x07;
/// set freedom vector parallel to line
pub(crate) const SFVTL_para: u8 = 0x08;
/// set freedom vector perpendicular to line
pub(crate) const SFVTL_perp: u8 = 0x09;
/// set projection vector from stack
pub(crate) const SPVFS: u8 = 0x0A;
/// set freedom vector from stack
pub(crate) const SFVFS: u8 = 0x0B;
/// get projection vector
pub(crate) const GPV: u8 = 0x0C;
/// get freedom vector
pub(crate) const GFV: u8 = 0x0D;
/// set freedom vector to projection vector
pub(crate) const SFVTPV: u8 = 0x0E;
/// move point to intersection of lines
pub(crate) const ISECT: u8 = 0x0F;

/// set reference point 0
pub(crate) const SRP0: u8 = 0x10;
/// set reference point 1
pub(crate) const SRP1: u8 = 0x11;
/// set reference point 2
pub(crate) const SRP2: u8 = 0x12;
/// set zone pointer 0
pub(crate) const SZP0: u8 = 0x13;
/// set zone pointer 1
pub(crate) const SZP1: u8 = 0x14;
/// set zone pointer 2
pub(crate) const SZP2: u8 = 0x15;
/// set zone pointers
pub(crate) const SZPS: u8 = 0x16;
/// set loop counter
pub(crate) const SLOOP: u8 = 0x17;
/// round to grid
pub(crate) const RTG: u8 = 0x18;
/// round to half grid
pub(crate) const RTHG: u8 = 0x19;
/// set `minimum_distance'
pub(crate) const SMD: u8 = 0x1A;
/// begin of `else' clause
pub(crate) const ELSE: u8 = 0x1B;
/// jump relative
pub(crate) const JMPR: u8 = 0x1C;
/// set `control_value_cut_in'
pub(crate) const SCVTCI: u8 = 0x1D;
/// set `single_width_cut_in'
pub(crate) const SSWCI: u8 = 0x1E;
/// set `single_width_value'
pub(crate) const SSW: u8 = 0x1F;

/// duplicate top stack element
pub(crate) const DUP: u8 = 0x20;
/// pop top stack element
pub(crate) const POP: u8 = 0x21;
/// clear entire stack
pub(crate) const CLEAR: u8 = 0x22;
/// swap top two elements of stack
pub(crate) const SWAP: u8 = 0x23;
/// get depth of stack
pub(crate) const DEPTH: u8 = 0x24;
/// copy indexed element to top of stack
pub(crate) const CINDEX: u8 = 0x25;
/// move indexed element to top of stack
pub(crate) const MINDEX: u8 = 0x26;
/// align points
pub(crate) const ALIGNPTS: u8 = 0x27;
/// undefined
pub(crate) const INS_28: u8 = 0x28;
/// untouch point
pub(crate) const UTP: u8 = 0x29;
/// loop and call function
pub(crate) const LOOPCALL: u8 = 0x2A;
/// call function
pub(crate) const CALL: u8 = 0x2B;
/// define function
pub(crate) const FDEF: u8 = 0x2C;
/// end of function
pub(crate) const ENDF: u8 = 0x2D;
/// move direct absolute point without rounding
pub(crate) const MDAP_noround: u8 = 0x2E;
/// move direct absolute point with rounding
pub(crate) const MDAP_round: u8 = 0x2F;

/// interpolate untouched points along y axis
pub(crate) const IUP_y: u8 = 0x30;
/// interpolate untouched points along x axis
pub(crate) const IUP_x: u8 = 0x31;
/// shift point using rp2
pub(crate) const SHP_rp2: u8 = 0x32;
/// shift point using rp1
pub(crate) const SHP_rp1: u8 = 0x33;
/// shift contour using rp2
pub(crate) const SHC_rp2: u8 = 0x34;
/// shift contour using rp1
pub(crate) const SHC_rp1: u8 = 0x35;
/// shift zone using rp2
pub(crate) const SHZ_rp2: u8 = 0x36;
/// shift zone using rp1
pub(crate) const SHZ_rp1: u8 = 0x37;
/// shift point by pixel amount
pub(crate) const SHPIX: u8 = 0x38;
/// interpolate point
pub(crate) const IP: u8 = 0x39;
/// move stack indirect relative point, don't set rp0
pub(crate) const MSIRP_norp0: u8 = 0x3A;
/// move stack indirect relative point, set rp0
pub(crate) const MSIRP_rp0: u8 = 0x3B;
/// align relative point
pub(crate) const ALIGNRP: u8 = 0x3C;
/// round to double grid
pub(crate) const RTDG: u8 = 0x3D;
/// move indirect absolute point without rounding
pub(crate) const MIAP_noround: u8 = 0x3E;
/// move indirect absolute point with rounding
pub(crate) const MIAP_round: u8 = 0x3F;

/// push `n' bytes
pub(crate) const NPUSHB: u8 = 0x40;
/// push `n' words
pub(crate) const NPUSHW: u8 = 0x41;
/// write to storage area
pub(crate) const WS: u8 = 0x42;
/// read from storage area
pub(crate) const RS: u8 = 0x43;
/// write to CVT in pixel units
pub(crate) const WCVTP: u8 = 0x44;
/// read from CVT
pub(crate) const RCVT: u8 = 0x45;
/// get (projected) coordinate, current position
pub(crate) const GC_cur: u8 = 0x46;
/// get (projected) coordinate, original position
pub(crate) const GC_orig: u8 = 0x47;
/// set coordinate from stack
pub(crate) const SCFS: u8 = 0x48;
/// measure distance, current positions
pub(crate) const MD_cur: u8 = 0x49;
/// measure distance, original positions
pub(crate) const MD_orig: u8 = 0x4A;
/// measure PPEM
pub(crate) const MPPEM: u8 = 0x4B;
/// measure point size
pub(crate) const MPS: u8 = 0x4C;
/// set `auto_flip' to TRUE
pub(crate) const FLIPON: u8 = 0x4D;
/// set `auto_flip' to FALSE
pub(crate) const FLIPOFF: u8 = 0x4E;
/// ignored
pub(crate) const DEBUG: u8 = 0x4F;

/// lower than
pub(crate) const LT: u8 = 0x50;
/// lower than or equal
pub(crate) const LTEQ: u8 = 0x51;
/// greater than
pub(crate) const GT: u8 = 0x52;
/// greater than or equal
pub(crate) const GTEQ: u8 = 0x53;
/// equal
pub(crate) const EQ: u8 = 0x54;
/// not equal
pub(crate) const NEQ: u8 = 0x55;
/// TRUE if odd
pub(crate) const ODD: u8 = 0x56;
/// TRUE if even
pub(crate) const EVEN: u8 = 0x57;
/// start of `if' clause
pub(crate) const IF: u8 = 0x58;
/// end of `if' or `else' clause
pub(crate) const EIF: u8 = 0x59;
/// logical AND
pub(crate) const AND: u8 = 0x5A;
/// logical OR
pub(crate) const OR: u8 = 0x5B;
/// logical NOT
pub(crate) const NOT: u8 = 0x5C;
/// delta point exception 1
pub(crate) const DELTAP1: u8 = 0x5D;
/// set `delta_base'
pub(crate) const SDB: u8 = 0x5E;
/// set `delta_shift'
pub(crate) const SDS: u8 = 0x5F;

/// addition
pub(crate) const ADD: u8 = 0x60;
/// subtraction
pub(crate) const SUB: u8 = 0x61;
/// division
pub(crate) const DIV: u8 = 0x62;
/// multiplication
pub(crate) const MUL: u8 = 0x63;
/// absolute value
pub(crate) const ABS: u8 = 0x64;
/// negation
pub(crate) const NEG: u8 = 0x65;
/// floor operation
pub(crate) const FLOOR: u8 = 0x66;
/// ceiling operation
pub(crate) const CEILING: u8 = 0x67;
/// round value with gray compensation
pub(crate) const ROUND_gray: u8 = 0x68;
/// round value with black compensation
pub(crate) const ROUND_black: u8 = 0x69;
/// round value with white compensation
pub(crate) const ROUND_white: u8 = 0x6A;
/// undefined
pub(crate) const ROUND_3: u8 = 0x6B;
/// apply gray compensation
pub(crate) const NROUND_gray: u8 = 0x6C;
/// apply black compensation
pub(crate) const NROUND_black: u8 = 0x6D;
/// apply white compensation
pub(crate) const NROUND_white: u8 = 0x6E;
/// undefined
pub(crate) const NROUND_3: u8 = 0x6F;

/// write to CVT in font units
pub(crate) const WCVTF: u8 = 0x70;
/// delta point exception 2
pub(crate) const DELTAP2: u8 = 0x71;
/// delta point exception 3
pub(crate) const DELTAP3: u8 = 0x72;
/// delta cvt exception 1
pub(crate) const DELTAC1: u8 = 0x73;
/// delta cvt exception 2
pub(crate) const DELTAC2: u8 = 0x74;
/// delta cvt exception 3
pub(crate) const DELTAC3: u8 = 0x75;
/// super round  
pub(crate) const SROUND: u8 = 0x76;
/// super round at 45 degrees
pub(crate) const S45Round: u8 = 0x77;
/// jump relative on TRUE
pub(crate) const JROT: u8 = 0x78;
/// jump relative on FALSE
pub(crate) const JROF: u8 = 0x79;
/// turn off rounding
pub(crate) const ROFF: u8 = 0x7A;
/// undefined
pub(crate) const INS_7B: u8 = 0x7B;
/// round up to grid
pub(crate) const RUTG: u8 = 0x7C;
/// round down to grid
pub(crate) const RDTG: u8 = 0x7D;
/// ignored, obsolete
pub(crate) const SANGW: u8 = 0x7E;
/// ignored, obsolete
pub(crate) const AA: u8 = 0x7F;

/// flip point on-curve to off-curve and vice versa
pub(crate) const FLIPPT: u8 = 0x80;
/// flip range of points to be on-curve
pub(crate) const FLIPRGON: u8 = 0x81;
/// flip range of points to be off-curve
pub(crate) const FlIPRGOFF: u8 = 0x82;
/// undefined
pub(crate) const INS_83: u8 = 0x83;
/// undefined
pub(crate) const INS_84: u8 = 0x84;
/// scan conversion control
pub(crate) const SCANCTRL: u8 = 0x85;
/// set dual projection vector parallel to line
pub(crate) const SDPVTL_para: u8 = 0x86;
/// set dual projection vector perpendicular to line
pub(crate) const SDPVTL_perp: u8 = 0x87;
/// get information about font scaler and current glyph
pub(crate) const GETINFO: u8 = 0x88;
/// define instruction
pub(crate) const IDEF: u8 = 0x89;
/// roll top three stack elements
pub(crate) const ROLL: u8 = 0x8A;
/// maximum
pub(crate) const MAX: u8 = 0x8B;
/// minimum
pub(crate) const MIN: u8 = 0x8C;
/// set scan conversion rules
pub(crate) const SCANTYPE: u8 = 0x8D;
/// set instruction control state
pub(crate) const INSTCTRL: u8 = 0x8E;
/// undefined
pub(crate) const INS_8F: u8 = 0x8F;

/// undefined
pub(crate) const INS_90: u8 = 0x90;
pub(crate) const INS_91: u8 = 0x91;
pub(crate) const INS_92: u8 = 0x92;
pub(crate) const INS_93: u8 = 0x93;
pub(crate) const INS_94: u8 = 0x94;
pub(crate) const INS_95: u8 = 0x95;
pub(crate) const INS_96: u8 = 0x96;
pub(crate) const INS_97: u8 = 0x97;
pub(crate) const INS_98: u8 = 0x98;
pub(crate) const INS_99: u8 = 0x99;
pub(crate) const INS_9A: u8 = 0x9A;
pub(crate) const INS_9B: u8 = 0x9B;
pub(crate) const INS_9C: u8 = 0x9C;
pub(crate) const INS_9D: u8 = 0x9D;
pub(crate) const INS_9E: u8 = 0x9E;
pub(crate) const INS_9F: u8 = 0x9F;

pub(crate) const INS_A0: u8 = 0xA0; /* undefined */
pub(crate) const INS_A1: u8 = 0xA1;
pub(crate) const INS_A2: u8 = 0xA2;
pub(crate) const INS_A3: u8 = 0xA3;
pub(crate) const INS_A4: u8 = 0xA4;
pub(crate) const INS_A5: u8 = 0xA5;
pub(crate) const INS_A6: u8 = 0xA6;
pub(crate) const INS_A7: u8 = 0xA7;
pub(crate) const INS_A8: u8 = 0xA8;
pub(crate) const INS_A9: u8 = 0xA9;
pub(crate) const INS_AA: u8 = 0xAA;
pub(crate) const INS_AB: u8 = 0xAB;
pub(crate) const INS_AC: u8 = 0xAC;
pub(crate) const INS_AD: u8 = 0xAD;
pub(crate) const INS_AE: u8 = 0xAE;
pub(crate) const INS_AF: u8 = 0xAF;

pub(crate) const PUSHB_1: u8 = 0xB0; /* push 1 byte */
pub(crate) const PUSHB_2: u8 = 0xB1; /* push 2 bytes */
pub(crate) const PUSHB_3: u8 = 0xB2; /* push 3 bytes */
pub(crate) const PUSHB_4: u8 = 0xB3; /* push 4 bytes */
pub(crate) const PUSHB_5: u8 = 0xB4; /* push 5 bytes */
pub(crate) const PUSHB_6: u8 = 0xB5; /* push 6 bytes */
pub(crate) const PUSHB_7: u8 = 0xB6; /* push 7 bytes */
pub(crate) const PUSHB_8: u8 = 0xB7; /* push 8 bytes */
pub(crate) const PUSHW_1: u8 = 0xB8; /* push 1 word */
pub(crate) const PUSHW_2: u8 = 0xB9; /* push 2 words */
pub(crate) const PUSHW_3: u8 = 0xBA; /* push 3 words */
pub(crate) const PUSHW_4: u8 = 0xBB; /* push 4 words */
pub(crate) const PUSHW_5: u8 = 0xBC; /* push 5 words */
pub(crate) const PUSHW_6: u8 = 0xBD; /* push 6 words */
pub(crate) const PUSHW_7: u8 = 0xBE; /* push 7 words */
pub(crate) const PUSHW_8: u8 = 0xBF; /* push 8 words */

pub(crate) const MDRP_norp0_nokeep_noround_gray: u8 = 0xC0; /* move direct relative point */
pub(crate) const MDRP_norp0_nokeep_noround_black: u8 = 0xC1;
pub(crate) const MDRP_norp0_nokeep_noround_white: u8 = 0xC2;
pub(crate) const MDRP_norp0_nokeep_noround_3: u8 = 0xC3; /* undefined */
pub(crate) const MDRP_norp0_nokeep_round_gray: u8 = 0xC4;
pub(crate) const MDRP_norp0_nokeep_round_black: u8 = 0xC5;
pub(crate) const MDRP_norp0_nokeep_round_white: u8 = 0xC6;
pub(crate) const MDRP_norp0_nokeep_round_3: u8 = 0xC7;
pub(crate) const MDRP_norp0_keep_noround_gray: u8 = 0xC8;
pub(crate) const MDRP_norp0_keep_noround_black: u8 = 0xC9;
pub(crate) const MDRP_norp0_keep_noround_white: u8 = 0xCA;
pub(crate) const MDRP_norp0_keep_noround_3: u8 = 0xCB;
pub(crate) const MDRP_norp0_keep_round_gray: u8 = 0xCC;
pub(crate) const MDRP_norp0_keep_round_black: u8 = 0xCD;
pub(crate) const MDRP_norp0_keep_round_white: u8 = 0xCE;
pub(crate) const MDRP_norp0_keep_round_3: u8 = 0xCF;

pub(crate) const MDRP_rp0_nokeep_noround_gray: u8 = 0xD0;
pub(crate) const MDRP_rp0_nokeep_noround_black: u8 = 0xD1;
pub(crate) const MDRP_rp0_nokeep_noround_white: u8 = 0xD2;
pub(crate) const MDRP_rp0_nokeep_noround_3: u8 = 0xD3;
pub(crate) const MDRP_rp0_nokeep_round_gray: u8 = 0xD4;
pub(crate) const MDRP_rp0_nokeep_round_black: u8 = 0xD5;
pub(crate) const MDRP_rp0_nokeep_round_white: u8 = 0xD6;
pub(crate) const MDRP_rp0_nokeep_round_3: u8 = 0xD7;
pub(crate) const MDRP_rp0_keep_noround_gray: u8 = 0xD8;
pub(crate) const MDRP_rp0_keep_noround_black: u8 = 0xD9;
pub(crate) const MDRP_rp0_keep_noround_white: u8 = 0xDA;
pub(crate) const MDRP_rp0_keep_noround_3: u8 = 0xDB;
pub(crate) const MDRP_rp0_keep_round_gray: u8 = 0xDC;
pub(crate) const MDRP_rp0_keep_round_black: u8 = 0xDD;
pub(crate) const MDRP_rp0_keep_round_white: u8 = 0xDE;
pub(crate) const MDRP_rp0_keep_round_3: u8 = 0xDF;

pub(crate) const MIRP_norp0_nokeep_noround_gray: u8 = 0xE0; /* move indirect relative point */
pub(crate) const MIRP_norp0_nokeep_noround_black: u8 = 0xE1;
pub(crate) const MIRP_norp0_nokeep_noround_white: u8 = 0xE2;
pub(crate) const MIRP_norp0_nokeep_noround_3: u8 = 0xE3; /* undefined */
pub(crate) const MIRP_norp0_nokeep_round_gray: u8 = 0xE4;
pub(crate) const MIRP_norp0_nokeep_round_black: u8 = 0xE5;
pub(crate) const MIRP_norp0_nokeep_round_white: u8 = 0xE6;
pub(crate) const MIRP_norp0_nokeep_round_3: u8 = 0xE7;
pub(crate) const MIRP_norp0_keep_noround_gray: u8 = 0xE8;
pub(crate) const MIRP_norp0_keep_noround_black: u8 = 0xE9;
pub(crate) const MIRP_norp0_keep_noround_white: u8 = 0xEA;
pub(crate) const MIRP_norp0_keep_noround_3: u8 = 0xEB;
pub(crate) const MIRP_norp0_keep_round_gray: u8 = 0xEC;
pub(crate) const MIRP_norp0_keep_round_black: u8 = 0xED;
pub(crate) const MIRP_norp0_keep_round_white: u8 = 0xEE;
pub(crate) const MIRP_norp0_keep_round_3: u8 = 0xEF;

pub(crate) const MIRP_rp0_nokeep_noround_gray: u8 = 0xF0;
pub(crate) const MIRP_rp0_nokeep_noround_black: u8 = 0xF1;
pub(crate) const MIRP_rp0_nokeep_noround_white: u8 = 0xF2;
pub(crate) const MIRP_rp0_nokeep_noround_3: u8 = 0xF3;
pub(crate) const MIRP_rp0_nokeep_round_gray: u8 = 0xF4;
pub(crate) const MIRP_rp0_nokeep_round_black: u8 = 0xF5;
pub(crate) const MIRP_rp0_nokeep_round_white: u8 = 0xF6;
pub(crate) const MIRP_rp0_nokeep_round_3: u8 = 0xF7;
pub(crate) const MIRP_rp0_keep_noround_gray: u8 = 0xF8;
pub(crate) const MIRP_rp0_keep_noround_black: u8 = 0xF9;
pub(crate) const MIRP_rp0_keep_noround_white: u8 = 0xFA;
pub(crate) const MIRP_rp0_keep_noround_3: u8 = 0xFB;
pub(crate) const MIRP_rp0_keep_round_gray: u8 = 0xFC;
pub(crate) const MIRP_rp0_keep_round_black: u8 = 0xFD;
pub(crate) const MIRP_rp0_keep_round_white: u8 = 0xFE;
pub(crate) const MIRP_rp0_keep_round_3: u8 = 0xFF;

#[allow(non_camel_case_types)]
#[repr(u8)]
pub(crate) enum FunctionNumbers {
    bci_align_x_height = 0,
    bci_round,
    bci_natural_stem_width,
    bci_quantize_stem_width,
    bci_smooth_stem_width,
    bci_get_best_width,
    bci_strong_stem_width,
    bci_loop_do,
    bci_loop,
    bci_cvt_rescale,
    bci_cvt_rescale_range,
    bci_vwidth_data_store,
    bci_smooth_blue_round,
    bci_strong_blue_round,
    bci_blue_round_range,
    bci_decrement_component_counter,
    bci_get_point_extrema,
    bci_nibbles,
    bci_number_set_is_element,
    bci_number_set_is_element2,

    /* 20 */
    bci_create_segment,
    bci_create_segments,

    /* 22 */
    /* the next ten entries must stay in this order */
    bci_create_segments_0,
    bci_create_segments_1,
    bci_create_segments_2,
    bci_create_segments_3,
    bci_create_segments_4,
    bci_create_segments_5,
    bci_create_segments_6,
    bci_create_segments_7,
    bci_create_segments_8,
    bci_create_segments_9,

    bci_create_segments_composite,

    /* 33 */
    /* the next ten entries must stay in this order */
    bci_create_segments_composite_0,
    bci_create_segments_composite_1,
    bci_create_segments_composite_2,
    bci_create_segments_composite_3,
    bci_create_segments_composite_4,
    bci_create_segments_composite_5,
    bci_create_segments_composite_6,
    bci_create_segments_composite_7,
    bci_create_segments_composite_8,
    bci_create_segments_composite_9,

    /* 43 */
    /* the next three entries must stay in this order */
    bci_deltap1,
    bci_deltap2,
    bci_deltap3,

    /* 46 */
    bci_align_point,
    bci_align_segment,
    bci_align_segments,

    /* 49 */
    bci_scale_contour,
    bci_scale_glyph,
    bci_scale_composite_glyph,
    bci_shift_contour,
    bci_shift_subglyph,

    /* 54 */
    bci_ip_outer_align_point,
    bci_ip_on_align_points,
    bci_ip_between_align_point,
    bci_ip_between_align_points,

    /* 58 */
    bci_adjust_common,
    bci_stem_common,
    bci_serif_common,
    bci_serif_anchor_common,
    bci_serif_link1_common,
    bci_serif_link2_common,

    /* 64 */
    bci_lower_bound,
    bci_upper_bound,
    bci_upper_lower_bound,

    /* 67 */
    bci_adjust_bound,
    bci_stem_bound,
    bci_link,
    bci_anchor,
    bci_adjust,
    bci_stem,

    /* the order of the `bci_action_*' entries must correspond */
    /* to the order of the TA_Action enumeration entries (in `tahints.h') */

    /* 73 */
    bci_action_ip_before,
    bci_action_ip_after,
    bci_action_ip_on,
    bci_action_ip_between,

    /* 77 */
    bci_action_blue,
    bci_action_blue_anchor,

    /* 79 */
    bci_action_anchor,
    bci_action_anchor_serif,
    bci_action_anchor_round,
    bci_action_anchor_round_serif,

    /* 83 */
    bci_action_adjust,
    bci_action_adjust_serif,
    bci_action_adjust_round,
    bci_action_adjust_round_serif,
    bci_action_adjust_bound,
    bci_action_adjust_bound_serif,
    bci_action_adjust_bound_round,
    bci_action_adjust_bound_round_serif,
    bci_action_adjust_down_bound,
    bci_action_adjust_down_bound_serif,
    bci_action_adjust_down_bound_round,
    bci_action_adjust_down_bound_round_serif,

    /* 95 */
    bci_action_link,
    bci_action_link_serif,
    bci_action_link_round,
    bci_action_link_round_serif,

    /* 99 */
    bci_action_stem,
    bci_action_stem_serif,
    bci_action_stem_round,
    bci_action_stem_round_serif,
    bci_action_stem_bound,
    bci_action_stem_bound_serif,
    bci_action_stem_bound_round,
    bci_action_stem_bound_round_serif,
    bci_action_stem_down_bound,
    bci_action_stem_down_bound_serif,
    bci_action_stem_down_bound_round,
    bci_action_stem_down_bound_round_serif,

    /* 111 */
    bci_action_serif,
    bci_action_serif_lower_bound,
    bci_action_serif_upper_bound,
    bci_action_serif_upper_lower_bound,
    bci_action_serif_down_lower_bound,
    bci_action_serif_down_upper_bound,
    bci_action_serif_down_upper_lower_bound,

    /* 118 */
    bci_action_serif_anchor,
    bci_action_serif_anchor_lower_bound,
    bci_action_serif_anchor_upper_bound,
    bci_action_serif_anchor_upper_lower_bound,
    bci_action_serif_anchor_down_lower_bound,
    bci_action_serif_anchor_down_upper_bound,
    bci_action_serif_anchor_down_upper_lower_bound,

    /* 125 */
    bci_action_serif_link1,
    bci_action_serif_link1_lower_bound,
    bci_action_serif_link1_upper_bound,
    bci_action_serif_link1_upper_lower_bound,
    bci_action_serif_link1_down_lower_bound,
    bci_action_serif_link1_down_upper_bound,
    bci_action_serif_link1_down_upper_lower_bound,

    /* 132 */
    bci_action_serif_link2,
    bci_action_serif_link2_lower_bound,
    bci_action_serif_link2_upper_bound,
    bci_action_serif_link2_upper_lower_bound,
    bci_action_serif_link2_down_lower_bound,
    bci_action_serif_link2_down_upper_bound,
    bci_action_serif_link2_down_upper_lower_bound,

    /* 139 */
    bci_hint_glyph,

    /* 140 */
    bci_freetype_enable_deltas,
    MAX_FUNCTIONS,
}

#[allow(non_camel_case_types)]
#[repr(u8)]
pub(crate) enum CvtLocations {
    cvtl_temp = 0, /* used for creating twilight points */
    cvtl_funits_to_pixels,
    cvtl_is_subglyph,
    cvtl_stem_width_mode,
    cvtl_is_element,
    cvtl_do_iup_y,
    cvtl_ignore_std_width,

    cvtl_max_runtime,
}

#[allow(non_camel_case_types)]
#[repr(u8)]
pub(crate) enum StorageAreaLocations {
    sal_i = 0,
    sal_j,
    sal_k,
    sal_limit,
    sal_temp1,
    sal_temp2,
    sal_temp3,
    sal_best,
    sal_ref,
    sal_func,

    /* 10 */
    sal_anchor,
    sal_stem_width_function,
    sal_base_delta,
    sal_vwidth_data_offset,
    sal_scale,
    sal_point_min,
    sal_point_max,
    sal_base,
    sal_num_packed_segments,
    sal_num_stem_widths,

    /* 20 */
    sal_stem_width_offset,
    sal_have_cached_width,
    sal_cached_width_offset,
    sal_top_to_bottom_hinting,
    sal_segment_offset, /* must be last */
}

/* scaling value index of style ID id */
pub(crate) const fn CVT_SCALING_VALUE_OFFSET(id: u8) -> u8 {
    CvtLocations::cvtl_max_runtime as u8 + (id)
}
