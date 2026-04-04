use crate::{
    font::{Font, TA_STYLE_MAX},
    AutohintError,
};
use skrifa::{
    outline::{GlyphStyles, STYLE_CLASSES},
    raw::{
        tables::glyf::{Anchor, CompositeGlyphFlags, Glyph},
        TableProvider,
    },
    FontRef, GlyphId, MetadataProvider,
};

const TA_STYLE_MASK: u16 = 0x3FFF;
const TA_STYLE_UNASSIGNED: u16 = TA_STYLE_MASK;
const TA_DIGIT: u16 = 0x8000;
const TA_NONBASE: u16 = 0x4000;

// Maps skrifa's STYLE_CLASSES index to ttfautohint's TA_STYLE_* index.
// `None` means there is no equivalent C style and the glyph remains unassigned.
pub(crate) fn skrifa_style_to_ta_style(style: usize) -> Option<u16> {
    match style {
        0 => Some(0),   // ADLM_DFLT
        1 => Some(1),   // ARAB_DFLT
        2 => Some(2),   // ARMN_DFLT
        3 => Some(3),   // AVST_DFLT
        4 => Some(4),   // BAMU_DFLT
        5 => Some(5),   // BENG_DFLT
        6 => Some(6),   // BUHD_DFLT
        7 => Some(7),   // CAKM_DFLT
        8 => Some(8),   // CANS_DFLT
        9 => Some(9),   // CARI_DFLT
        10 => Some(10), // CHER_DFLT
        11 => Some(11), // COPT_DFLT
        12 => Some(12), // CPRT_DFLT
        13 => Some(13), // CYRL_C2CP
        14 => Some(14), // CYRL_C2SC
        15 => Some(15), // CYRL_ORDN
        16 => Some(16), // CYRL_PCAP
        17 => None,     // CYRL_RUBY (not present in C style list)
        18 => Some(17), // CYRL_SINF
        19 => Some(18), // CYRL_SMCP
        20 => Some(19), // CYRL_SUBS
        21 => Some(20), // CYRL_SUPS
        22 => Some(21), // CYRL_TITL
        23 => Some(22), // CYRL_DFLT
        24 => Some(23), // DEVA_DFLT
        25 => Some(24), // DSRT_DFLT
        26 => Some(25), // ETHI_DFLT
        27 => Some(26), // GEOR_DFLT
        28 => Some(27), // GEOK_DFLT
        29 => Some(28), // GLAG_DFLT
        30 => Some(29), // GOTH_DFLT
        31 => Some(30), // GREK_C2CP
        32 => Some(31), // GREK_C2SC
        33 => Some(32), // GREK_ORDN
        34 => Some(33), // GREK_PCAP
        35 => None,     // GREK_RUBY (not present in C style list)
        36 => Some(34), // GREK_SINF
        37 => Some(35), // GREK_SMCP
        38 => Some(36), // GREK_SUBS
        39 => Some(37), // GREK_SUPS
        40 => Some(38), // GREK_TITL
        41 => Some(39), // GREK_DFLT
        42 => Some(40), // GUJR_DFLT
        43 => Some(41), // GURU_DFLT
        44 => Some(42), // HEBR_DFLT
        45 => Some(43), // HMNP_DFLT
        46 => Some(44), // KALI_DFLT
        47 => Some(45), // KHMR_DFLT
        48 => Some(46), // KHMS_DFLT
        49 => Some(47), // KNDA_DFLT
        50 => Some(48), // LAO_DFLT
        51 => Some(49), // LATN_C2CP
        52 => Some(50), // LATN_C2SC
        53 => Some(51), // LATN_ORDN
        54 => Some(52), // LATN_PCAP
        55 => None,     // LATN_RUBY (not present in C style list)
        56 => Some(53), // LATN_SINF
        57 => Some(54), // LATN_SMCP
        58 => Some(55), // LATN_SUBS
        59 => Some(56), // LATN_SUPS
        60 => Some(57), // LATN_TITL
        61 => Some(58), // LATN_DFLT
        62 => Some(59), // LATB_DFLT
        63 => Some(60), // LATP_DFLT
        64 => Some(61), // LISU_DFLT
        65 => Some(62), // MLYM_DFLT
        66 => Some(63), // MEDF_DFLT
        67 => Some(64), // MONG_DFLT
        68 => Some(65), // MYMR_DFLT
        69 => Some(83), // NONE_DFLT
        70 => Some(67), // OLCK_DFLT
        71 => Some(68), // ORKH_DFLT
        72 => Some(69), // OSGE_DFLT
        73 => Some(70), // OSMA_DFLT
        74 => Some(71), // ROHG_DFLT
        75 => Some(72), // SAUR_DFLT
        76 => Some(73), // SHAW_DFLT
        77 => Some(74), // SINH_DFLT
        78 => Some(75), // SUND_DFLT
        79 => Some(76), // TAML_DFLT
        80 => Some(77), // TAVT_DFLT
        81 => Some(78), // TELU_DFLT
        82 => Some(79), // TFNG_DFLT
        83 => Some(80), // THAI_DFLT
        84 => Some(81), // VAII_DFLT
        85 => None,     // LIMB (not present in C style list)
        86 => None,     // ORYA (not present in C style list)
        87 => None,     // SYLO (not present in C style list)
        88 => None,     // TIBT (not present in C style list)
        89 => None,     // HANI (not present in C style list)
        _ => None,
    }
}

pub(crate) fn ta_style_to_skrifa_style(ta_style: usize) -> Option<usize> {
    (0..90).find(|&skrifa_style| skrifa_style_to_ta_style(skrifa_style) == Some(ta_style as u16))
}

pub(crate) fn compute_style_coverage(
    font: &Font,
    glyph_count: usize,
    fallback_style: u16,
    debug_dump: bool,
    face_index: i32,
    num_faces: i32,
) -> Result<(Vec<u16>, Vec<u32>), AutohintError> {
    let mut glyph_styles_out = vec![TA_STYLE_UNASSIGNED; glyph_count];
    let mut sample_glyphs_out = vec![0u32; TA_STYLE_MAX];
    fn propagate_style_to_composites(
        gindex: usize,
        gstyle: u16,
        glyph_styles_out: &mut [u16],
        composite_children: &[Vec<usize>],
        nesting_level: usize,
    ) -> Result<(), AutohintError> {
        // Match C behavior in ta_face_globals_scan_composite.
        if nesting_level > 100 {
            return Err(AutohintError::InvalidTable);
        }

        for &child in &composite_children[gindex] {
            if (glyph_styles_out[child] & TA_STYLE_MASK) != TA_STYLE_UNASSIGNED {
                continue;
            }

            glyph_styles_out[child] = gstyle;
            propagate_style_to_composites(
                child,
                gstyle,
                glyph_styles_out,
                composite_children,
                nesting_level + 1,
            )?;
        }

        Ok(())
    }

    fn dump_style_coverage(
        glyph_styles: &[u16],
        sample_count: usize,
        face_index: i32,
        num_faces: i32,
    ) {
        if num_faces > 1 {
            eprintln!("\nstyle coverage (subfont {face_index})\n==========================\n");
        } else {
            eprintln!("\nstyle coverage\n==============\n");
        }

        for style_idx in 0..sample_count {
            let style_name = crate::globals::ta_style_to_skrifa_style(style_idx)
                .and_then(|idx| STYLE_CLASSES.get(idx))
                .map(|style| style.name)
                .unwrap_or("(unknown)");

            eprintln!("{style_name}:");

            let mut count = 0usize;
            for (idx, style_bits) in glyph_styles.iter().enumerate() {
                if (style_bits & TA_STYLE_MASK) as usize == style_idx {
                    if count.is_multiple_of(10) {
                        eprint!(" ");
                    }
                    eprint!(" {idx}");
                    count += 1;
                    if count.is_multiple_of(10) {
                        eprintln!();
                    }
                }
            }

            if count == 0 {
                eprintln!("  (none)");
            } else if !count.is_multiple_of(10) {
                eprintln!();
            }
        }
    }

    let ttf_bytes = font.build_ttf();
    let font = FontRef::new(&ttf_bytes)?;
    let outlines = font.outline_glyphs();
    let styles = GlyphStyles::new(&outlines);
    let glyf = font.glyf()?;
    let loca = font.loca(None)?;

    let mut composite_children = vec![Vec::<usize>::new(); glyph_styles_out.len()];
    for gid in 0..glyph_styles_out.len() {
        let Some(glyph) = loca.get_glyf(GlyphId::new(gid as u32), &glyf)? else {
            continue;
        };

        let Glyph::Composite(composite) = glyph else {
            continue;
        };

        for component in composite.components() {
            let cidx = component.glyph.to_u16() as usize;
            if cidx >= glyph_styles_out.len() {
                continue;
            }

            let should_propagate = component
                .flags
                .contains(CompositeGlyphFlags::ARGS_ARE_XY_VALUES)
                && matches!(component.anchor, Anchor::Offset { y: 0, .. });

            if should_propagate {
                composite_children[gid].push(cidx);
            }
        }
    }

    for sample in sample_glyphs_out.iter_mut() {
        *sample = 0;
    }

    for (gid, style_out) in glyph_styles_out.iter_mut().enumerate() {
        let mut style_bits = TA_STYLE_UNASSIGNED;

        if let Some(ta_style) = styles
            .style_index(gid as u32)
            .and_then(skrifa_style_to_ta_style)
        {
            style_bits = ta_style & TA_STYLE_MASK;

            let ta_style_usize = ta_style as usize;
            if ta_style_usize < sample_glyphs_out.len() && sample_glyphs_out[ta_style_usize] == 0 {
                sample_glyphs_out[ta_style_usize] = gid as u32;
            }
        }

        if styles.is_non_base(gid as u32) {
            style_bits |= TA_NONBASE;
        }
        if styles.is_digit(gid as u32) {
            style_bits |= TA_DIGIT;
        }

        *style_out = style_bits;
    }

    for gid in 0..glyph_styles_out.len() {
        let gstyle = glyph_styles_out[gid];
        if (gstyle & TA_STYLE_MASK) == TA_STYLE_UNASSIGNED {
            continue;
        }

        propagate_style_to_composites(gid, gstyle, &mut glyph_styles_out, &composite_children, 0)?;
    }

    if fallback_style != TA_STYLE_UNASSIGNED {
        for style_bits in glyph_styles_out.iter_mut() {
            if (*style_bits & TA_STYLE_MASK) == TA_STYLE_UNASSIGNED {
                *style_bits &= !TA_STYLE_MASK;
                *style_bits |= fallback_style & TA_STYLE_MASK;
            }
        }
    }

    if debug_dump {
        dump_style_coverage(
            &glyph_styles_out,
            sample_glyphs_out.len(),
            face_index,
            num_faces,
        );
    }

    Ok((glyph_styles_out, sample_glyphs_out))
}
