use crate::{
    bytecode::Bytecode,
    font::Font,
    opcodes::{CALL, PUSHB_1, PUSHB_2, PUSHW_1, PUSHW_2},
    AutohintError,
};
use skrifa::{
    raw::{
        tables::glyf::{CompositeGlyphFlags, Glyph},
        TableProvider,
    },
    GlyphId,
};

const MAX_COMPOSITE_VISITS: usize = 1 << 16;

fn glyph_totals(
    font: &skrifa::FontRef<'_>,
    glyph_id: GlyphId,
) -> Result<(u32, i32), AutohintError> {
    let glyf = font.glyf()?;
    let loca = font.loca(None)?;

    let mut stack = vec![glyph_id];
    let mut num_points: u32 = 0;
    let mut num_contours: i32 = 0;
    let mut visits = 0usize;

    while let Some(gid) = stack.pop() {
        visits += 1;
        if visits > MAX_COMPOSITE_VISITS {
            return Err(AutohintError::CompositeTooDeeplyNested);
        }

        let glyph = loca.get_glyf(gid, &glyf)?;

        match glyph {
            None => {}
            Some(Glyph::Simple(g)) => {
                num_points = num_points.saturating_add(g.num_points() as u32);
                num_contours = num_contours.saturating_add(g.number_of_contours() as i32);
            }
            Some(Glyph::Composite(g)) => {
                for component in g.components() {
                    stack.push(component.glyph.into());
                }
            }
        }
    }

    Ok((num_points, num_contours))
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum LoaderGlyphKind {
    #[default]
    Empty = 0,
    Simple = 1,
    Composite = 2,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct LoaderGlyphInfo {
    pub kind: LoaderGlyphKind,
    pub num_points: u16,
}

impl LoaderGlyphInfo {
    pub fn new(font: &Font, glyph_id: GlyphId) -> Result<Self, AutohintError> {
        let glyf = font.fontref.glyf()?;
        let loca = font.fontref.loca(None)?;
        let glyph = loca.get_glyf(glyph_id, &glyf)?;
        match glyph {
            None => Ok(Self {
                kind: LoaderGlyphKind::Empty,
                num_points: 0,
            }),
            Some(Glyph::Simple(g)) => Ok(Self {
                kind: LoaderGlyphKind::Simple,
                num_points: g.num_points() as u16,
            }),
            Some(Glyph::Composite(_)) => Ok(Self {
                kind: LoaderGlyphKind::Composite,
                num_points: glyph_totals(&font.fontref, glyph_id)
                    .ok()
                    .and_then(|(points, _)| u16::try_from(points).ok())
                    .unwrap_or(u16::MAX),
            }),
        }
    }
}

pub(crate) fn build_subglyph_shifter_bytecode(
    font: &Font,
    glyph_id: GlyphId,
) -> Result<Bytecode, AutohintError> {
    let font = &font.fontref;
    let glyf = font.glyf()?;
    let loca = font.loca(None)?;
    let glyph = loca.get_glyf(glyph_id, &glyf)?;

    let mut bytecode = Bytecode::new();
    let mut curr_contour: i32 = 0;

    let Some(Glyph::Composite(composite)) = glyph else {
        return Ok(bytecode);
    };

    for component in composite.components() {
        let flags = component.flags;
        let y_offset = match component.anchor {
            skrifa::raw::tables::glyf::Anchor::Offset { y, .. } => y as i32,
            skrifa::raw::tables::glyf::Anchor::Point { component, .. } => component as i16 as i32,
        };
        let num_contours = glyph_totals(font, component.glyph.into())?.1;

        if !flags.intersects(CompositeGlyphFlags::ARGS_ARE_XY_VALUES)
            || y_offset == 0
            || num_contours == 0
        {
            curr_contour = curr_contour.saturating_add(num_contours);
            continue;
        }

        if num_contours > 0xFF || curr_contour > 0xFF {
            bytecode.push_u8(PUSHW_2);
            bytecode.push_word_i32(curr_contour);
            bytecode.push_word_i32(num_contours);
        } else {
            bytecode.push_u8(PUSHB_2);
            bytecode.push_u8(curr_contour as u8);
            bytecode.push_u8(num_contours as u8);
        }

        if !(0..=0xFF).contains(&y_offset) {
            bytecode.push_u8(PUSHW_1);
            bytecode.push_word_i32(y_offset);
        } else {
            bytecode.push_u8(PUSHB_1);
            bytecode.push_u8(y_offset as u8);
        }

        bytecode.push_u8(PUSHB_1);
        bytecode.push_u8(crate::opcodes::FunctionNumbers::bci_shift_subglyph as u8);
        bytecode.push_u8(CALL);

        curr_contour = curr_contour.saturating_add(num_contours);
    }

    Ok(bytecode)
}
