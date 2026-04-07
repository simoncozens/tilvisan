use crate::{
    bytecode::{high, low, Bytecode},
    font::Font,
    opcodes::{CALL, PUSHB_1, PUSHB_2, PUSHW_1, PUSHW_2},
};
use skrifa::{
    raw::{
        tables::glyf::{Component, Glyph},
        TableProvider,
    },
    GlyphId,
};

const MAX_COMPOSITE_VISITS: usize = 1 << 16;

fn f2dot14_to_f16dot16(v: skrifa::raw::types::F2Dot14) -> i32 {
    (v.to_bits() as i32) << 2
}

fn glyph_totals(font: &skrifa::FontRef<'_>, glyph_id: GlyphId) -> Result<(u32, i32), LoaderStatus> {
    let glyf = font.glyf().map_err(|_| LoaderStatus::InvalidArgument)?;
    let loca = font.loca(None).map_err(|_| LoaderStatus::InvalidArgument)?;

    let mut stack = vec![glyph_id];
    let mut num_points: u32 = 0;
    let mut num_contours: i32 = 0;
    let mut visits = 0usize;

    while let Some(gid) = stack.pop() {
        visits += 1;
        if visits > MAX_COMPOSITE_VISITS {
            return Err(LoaderStatus::InvalidArgument);
        }

        let glyph = loca
            .get_glyf(gid, &glyf)
            .map_err(|_| LoaderStatus::InvalidArgument)?;

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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LoaderStatus {
    InvalidArgument = 0x23,
}

const FT_SUBGLYPH_FLAG_ARGS_ARE_XY_VALUES: u16 = 0x0002;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LoaderGlyphKind {
    Empty = 0,
    Simple = 1,
    Composite = 2,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct LoaderGlyphInfo {
    pub kind: u32,
    pub num_points: u16,
    pub num_contours: i16,
    pub num_components: u16,
    pub _reserved: u16,
}

impl LoaderGlyphInfo {
    pub fn new(font: &Font, glyph_id: GlyphId) -> Result<Self, LoaderStatus> {
        let glyf = font
            .fontref
            .glyf()
            .map_err(|_| LoaderStatus::InvalidArgument)?;
        let loca = font
            .fontref
            .loca(None)
            .map_err(|_| LoaderStatus::InvalidArgument)?;
        let glyph = loca
            .get_glyf(glyph_id, &glyf)
            .map_err(|_| LoaderStatus::InvalidArgument)?;
        match glyph {
            None => Ok(Self {
                kind: LoaderGlyphKind::Empty as u32,
                num_points: 0,
                num_contours: 0,
                num_components: 0,
                _reserved: 0,
            }),
            Some(Glyph::Simple(g)) => Ok(Self {
                kind: LoaderGlyphKind::Simple as u32,
                num_points: g.num_points() as u16,
                num_contours: g.number_of_contours(),
                num_components: 0,
                _reserved: 0,
            }),
            Some(Glyph::Composite(g)) => Ok(Self {
                kind: LoaderGlyphKind::Composite as u32,
                num_points: glyph_totals(&font.fontref, glyph_id)
                    .ok()
                    .and_then(|(points, _)| u16::try_from(points).ok())
                    .unwrap_or(u16::MAX),
                num_contours: -1,
                num_components: g.components().count() as u16,
                _reserved: 0,
            }),
        }
    }
}

pub(crate) fn load_glyph_info(
    font: &Font,
    glyph_id: GlyphId,
) -> Result<LoaderGlyphInfo, LoaderStatus> {
    LoaderGlyphInfo::new(font, glyph_id)
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct LoaderComponent {
    pub(crate) glyph_id: GlyphId,
    pub(crate) flags: u16,
    pub(crate) arg1: i16,
    pub(crate) arg2: i16,
    pub(crate) xx: i32,
    pub(crate) xy: i32,
    pub(crate) yx: i32,
    pub(crate) yy: i32,
}
impl LoaderComponent {
    pub(crate) fn from_composite_glyph_component(component: &Component) -> Self {
        Self {
            glyph_id: component.glyph.into(),
            flags: component.flags.bits(),
            arg1: match component.anchor {
                write_fonts::tables::glyf::Anchor::Offset { x, y: _ } => x,
                write_fonts::tables::glyf::Anchor::Point { base, component: _ } => base as i16,
            },
            arg2: match component.anchor {
                write_fonts::tables::glyf::Anchor::Offset { x: _, y } => y,
                write_fonts::tables::glyf::Anchor::Point { base: _, component } => component as i16,
            },
            xx: f2dot14_to_f16dot16(component.transform.xx),
            xy: f2dot14_to_f16dot16(component.transform.xy),
            yx: f2dot14_to_f16dot16(component.transform.yx),
            yy: f2dot14_to_f16dot16(component.transform.yy),
        }
    }
}

fn load_components_vec(
    font: &skrifa::FontRef<'_>,
    glyph_id: GlyphId,
) -> Result<Vec<LoaderComponent>, LoaderStatus> {
    let glyf = font.glyf().map_err(|_| LoaderStatus::InvalidArgument)?;
    let loca = font.loca(None).map_err(|_| LoaderStatus::InvalidArgument)?;
    let glyph = loca
        .get_glyf(glyph_id, &glyf)
        .map_err(|_| LoaderStatus::InvalidArgument)?;

    match glyph {
        Some(Glyph::Composite(g)) => Ok(g
            .components()
            .map(|c| LoaderComponent::from_composite_glyph_component(&c))
            .collect()),
        _ => Ok(Vec::new()),
    }
}

pub(crate) fn build_subglyph_shifter_bytecode(
    font: &Font,
    glyph_id: GlyphId,
) -> Result<Bytecode, LoaderStatus> {
    let font = &font.fontref;

    let components = load_components_vec(font, glyph_id)?;
    let mut bytecode = Bytecode::new();
    let mut curr_contour: i32 = 0;

    for component in components {
        let flags = component.flags;
        let y_offset = component.arg2 as i32;
        let num_contours = glyph_totals(font, component.glyph_id)?.1;

        if (flags & FT_SUBGLYPH_FLAG_ARGS_ARE_XY_VALUES) == 0 || y_offset == 0 || num_contours == 0
        {
            curr_contour = curr_contour.saturating_add(num_contours);
            continue;
        }

        if num_contours > 0xFF || curr_contour > 0xFF {
            bytecode.push_u8(PUSHW_2);
            bytecode.push_u8(high(curr_contour as u16 as u32));
            bytecode.push_u8(low(curr_contour as u16 as u32));
            bytecode.push_u8(high(num_contours as u16 as u32));
            bytecode.push_u8(low(num_contours as u16 as u32));
        } else {
            bytecode.push_u8(PUSHB_2);
            bytecode.push_u8(curr_contour as u8);
            bytecode.push_u8(num_contours as u8);
        }

        if !(0..=0xFF).contains(&y_offset) {
            bytecode.push_u8(PUSHW_1);
            bytecode.push_u8(high(y_offset as i16 as u16 as u32));
            bytecode.push_u8(low(y_offset as i16 as u16 as u32));
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
