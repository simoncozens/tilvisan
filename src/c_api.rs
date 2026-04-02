use skrifa::GlyphId;

use crate::bytecode::{high, low, Bytecode};
use crate::control::CONTROL_DELTA_PPEM_MIN;
use crate::control_index::ControlIndex;
use crate::glyf::ScaledGlyph;
use crate::opcodes::{
    IUP_y, SVTCA_x, SVTCA_y, DELTAP1, DELTAP2, DELTAP3, LOOPCALL, PUSHB_1, PUSHW_1, WCVTP,
};

// ============================================================================
// Control file parsing and conversion
// ============================================================================

fn build_delta_exception_into(
    ppem: i32,
    point_idx: i32,
    x_shift_value: i32,
    y_shift_value: i32,
    delta_args: &mut [Vec<u32>; 6],
) {
    let mut ppem = ppem - CONTROL_DELTA_PPEM_MIN;

    let mut offset = if ppem < 16 {
        0
    } else if ppem < 32 {
        1
    } else {
        2
    };

    ppem -= offset << 4;

    let x_shift = if x_shift_value < 0 {
        x_shift_value + 8
    } else {
        x_shift_value + 7
    };

    let y_shift = if y_shift_value < 0 {
        y_shift_value + 8
    } else {
        y_shift_value + 7
    };

    if x_shift_value != 0 {
        delta_args[offset as usize].push(((ppem << 4) + x_shift) as u32);
        delta_args[offset as usize].push(point_idx as u32);
    }

    if y_shift_value != 0 {
        offset += 3;
        delta_args[offset as usize].push(((ppem << 4) + y_shift) as u32);
        delta_args[offset as usize].push(point_idx as u32);
    }
}

pub(crate) fn ta_rs_build_delta_exceptions(
    index: Option<&ControlIndex>,
    font_idx: usize,
    glyph_idx: GlyphId,
    glyph: &mut ScaledGlyph,
) -> (Bytecode, usize) {
    let rules = index
        .map(|idx| crate::control_index::delta_rules_for_glyph(idx, font_idx as i32, glyph_idx))
        .unwrap_or_default();
    if rules.is_empty() {
        return (Bytecode::new(), 0);
    }

    let mut before_args: [Vec<u32>; 6] = std::array::from_fn(|_| Vec::new());
    let mut after_args: [Vec<u32>; 6] = std::array::from_fn(|_| Vec::new());

    let mut need_before_words = false;
    let mut need_after_words = false;
    let mut need_before_word_counts = false;
    let mut need_after_word_counts = false;
    let mut saw_before = false;
    let mut saw_after = false;

    for rule in &rules {
        if rule.before_iup != 0 {
            saw_before = true;
            build_delta_exception_into(
                rule.ppem,
                rule.point_idx,
                rule.x_shift,
                rule.y_shift,
                &mut before_args,
            );
            if rule.point_idx > 255 {
                need_before_words = true;
            }
        } else {
            saw_after = true;
            build_delta_exception_into(
                rule.ppem,
                rule.point_idx,
                rule.x_shift,
                rule.y_shift,
                &mut after_args,
            );
            if rule.point_idx > 255 {
                need_after_words = true;
            }
        }
    }

    if !(saw_before || saw_after) {
        return (Bytecode::new(), 0);
    }

    for (i, args) in before_args.iter_mut().enumerate() {
        if !args.is_empty() {
            let n = (args.len() >> 1) as u32;
            if n > 255 {
                need_before_word_counts = true;
            }
            args.push(n);
            args.push((crate::opcodes::FunctionNumbers::bci_deltap1 as u8 + (i % 3) as u8) as u32);
        }
    }

    for args in &mut after_args {
        if !args.is_empty() {
            let n = (args.len() >> 1) as u32;
            if n > 255 {
                need_after_word_counts = true;
            }
            args.push(n);
        }
    }

    let mut bytecode = Bytecode::new();
    let mut before_stack_elements = 0usize;
    let mut after_stack_elements = 0usize;

    if need_before_words || !need_before_word_counts {
        let mut merged = Vec::new();
        for args in &before_args {
            merged.extend_from_slice(args);
        }
        before_stack_elements = merged.len();
        if bytecode.push(&merged, need_before_words, true).is_err() {
            return (bytecode, 2);
        }
    } else {
        for args in &before_args {
            if args.is_empty() {
                continue;
            }
            let num_delta_arg = args.len() - 2;
            if bytecode
                .push(&args[..num_delta_arg], need_before_words, true)
                .is_err()
            {
                return (bytecode, 2);
            }

            before_stack_elements = before_stack_elements.saturating_add(num_delta_arg + 2);

            let n = (num_delta_arg >> 1) as u32;
            bytecode.push_u8(PUSHW_1);
            bytecode.push_u8(high(n));
            bytecode.push_u8(low(n));

            bytecode.push_u8(PUSHB_1);
            bytecode.push_u8(args[args.len() - 1] as u8);
        }
    }

    if !before_args[5].is_empty() {
        bytecode.push_u8(LOOPCALL);
    }
    if !before_args[4].is_empty() {
        bytecode.push_u8(LOOPCALL);
    }
    if !before_args[3].is_empty() {
        bytecode.push_u8(LOOPCALL);
    }

    if !before_args[0].is_empty() || !before_args[1].is_empty() || !before_args[2].is_empty() {
        bytecode.push_u8(SVTCA_x);
    }

    if !before_args[2].is_empty() {
        bytecode.push_u8(LOOPCALL);
    }
    if !before_args[1].is_empty() {
        bytecode.push_u8(LOOPCALL);
    }
    if !before_args[0].is_empty() {
        bytecode.push_u8(LOOPCALL);
    }

    if !before_args[0].is_empty() || !before_args[1].is_empty() || !before_args[2].is_empty() {
        bytecode.push_u8(SVTCA_y);
    }

    let has_before_iup_y =
        !before_args[3].is_empty() || !before_args[4].is_empty() || !before_args[5].is_empty();
    if has_before_iup_y {
        bytecode.push_u8(IUP_y);
    }

    if need_after_words || !need_after_word_counts {
        let mut merged = Vec::new();
        for args in &after_args {
            merged.extend_from_slice(args);
        }
        after_stack_elements = merged.len();
        if bytecode.push(&merged, need_after_words, true).is_err() {
            return (bytecode, 2);
        }
    } else {
        for args in &after_args {
            if args.is_empty() {
                continue;
            }
            let num_delta_arg = args.len() - 1;
            if bytecode
                .push(&args[..num_delta_arg], need_after_words, true)
                .is_err()
            {
                return (bytecode, 2);
            }

            after_stack_elements = after_stack_elements.saturating_add(num_delta_arg + 1);

            let n = (num_delta_arg >> 1) as u32;
            bytecode.push_u8(PUSHW_1);
            bytecode.push_u8(high(n));
            bytecode.push_u8(low(n));
        }
    }

    if !after_args[5].is_empty() {
        bytecode.push_u8(DELTAP3);
    }
    if !after_args[4].is_empty() {
        bytecode.push_u8(DELTAP2);
    }
    if !after_args[3].is_empty() {
        bytecode.push_u8(DELTAP1);
    }

    if !after_args[0].is_empty() || !after_args[1].is_empty() || !after_args[2].is_empty() {
        bytecode.push_u8(SVTCA_x);
    }

    if !after_args[2].is_empty() {
        bytecode.push_u8(DELTAP3);
    }
    if !after_args[1].is_empty() {
        bytecode.push_u8(DELTAP2);
    }
    if !after_args[0].is_empty() {
        bytecode.push_u8(DELTAP1);
    }

    if has_before_iup_y {
        bytecode.push_u8(crate::opcodes::PUSHB_2);
        bytecode.push_u8(crate::opcodes::CvtLocations::cvtl_do_iup_y as u8);
        bytecode.push_u8(100);
        bytecode.push_u8(WCVTP);

        glyph.ins_extra.push_u8(crate::opcodes::PUSHB_2);
        glyph
            .ins_extra
            .push_u8(crate::opcodes::CvtLocations::cvtl_do_iup_y as u8);
        glyph.ins_extra.push_u8(0);
        glyph.ins_extra.push_u8(WCVTP);
    }

    (bytecode, before_stack_elements.max(after_stack_elements))
}
