use crate::{
    bytecode::Bytecode,
    opcodes::{EIF, ELSE, IF, LT, MPPEM, NPUSHB, NPUSHW, PUSHB_1, PUSHW_1},
};

#[derive(Clone)]
pub struct TaRsBytecodeHintsRecord {
    pub size: u32,
    pub buf: Bytecode,
}

fn emit_hints_record_into(out: &mut Bytecode, words_be: &[u8], optimize: bool) -> Result<u16, ()> {
    if !words_be.len().is_multiple_of(2) {
        return Err(());
    }

    let num_arguments = words_be.len() / 2;
    let mut need_words = false;
    for i in (0..words_be.len()).step_by(2) {
        if words_be[i] != 0 {
            need_words = true;
            break;
        }
    }

    let mut i = 0usize;
    while i < num_arguments {
        let num_args = (num_arguments - i).min(255);
        if need_words {
            if optimize && num_args <= 8 {
                out.push_u8(PUSHW_1 - 1 + num_args as u8);
            } else {
                out.push_u8(NPUSHW);
                out.push_u8(num_args as u8);
            }

            for j in 0..num_args {
                let src_word_idx = num_arguments - 1 - (i + j);
                let byte_ix = src_word_idx * 2;
                out.push_u8(words_be[byte_ix]);
                out.push_u8(words_be[byte_ix + 1]);
            }
        } else {
            if optimize && num_args <= 8 {
                out.push_u8(PUSHB_1 - 1 + num_args as u8);
            } else {
                out.push_u8(NPUSHB);
                out.push_u8(num_args as u8);
            }

            for j in 0..num_args {
                let src_word_idx = num_arguments - 1 - (i + j);
                let byte_ix = src_word_idx * 2;
                out.push_u8(words_be[byte_ix + 1]);
            }
        }

        i += 255;
    }

    Ok(u16::try_from(num_arguments).unwrap_or(u16::MAX))
}

pub(crate) fn emit_hints_records(
    records: &[TaRsBytecodeHintsRecord],
    optimize: bool,
) -> Result<(Bytecode, u16), ()> {
    let mut out = Bytecode::new();
    let mut max_stack_elements = 0u16;

    if records.is_empty() {
        return Ok((out, 0));
    }

    for i in 0..(records.len() - 1) {
        let curr = &records[i];
        let next = &records[i + 1];

        out.push_u8(MPPEM);
        if next.size > 0xFF {
            out.push_u8(PUSHW_1);

            out.push_word(next.size);
        } else {
            out.push_u8(PUSHB_1);
            out.push_u8(next.size as u8);
        }
        out.push_u8(LT);
        out.push_u8(IF);

        let n = emit_hints_record_into(&mut out, curr.buf.as_slice(), optimize)?;
        if n > max_stack_elements {
            max_stack_elements = n;
        }

        out.push_u8(ELSE);
    }

    let last = &records[records.len() - 1];
    let n = emit_hints_record_into(&mut out, last.buf.as_slice(), optimize)?;
    if n > max_stack_elements {
        max_stack_elements = n;
    }

    out.extend(std::iter::repeat_n(EIF, records.len() - 1));

    Ok((out, max_stack_elements))
}
