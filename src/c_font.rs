use skrifa::GlyphId;

use crate::{
    args::Args, control_index::ControlState, glyf::GlyfData, tablestore::TableStore, InfoData,
};
use core::ffi::{c_int, c_long, c_uint};

pub(crate) const TA_STYLE_MAX: usize = 84;
pub(crate) const TA_PROP_INCREASE_X_HEIGHT_MIN: c_int = 6;
pub(crate) const MISSING: usize = usize::MAX;

pub(crate) type TaProgressFunc = Option<fn(GlyphId, GlyphId, c_long, usize) -> c_int>;

pub(crate) struct Sfnt {
    pub(crate) glyph_count: c_long,
    pub(crate) glyph_styles: Vec<u16>,
    pub(crate) sample_glyphs: [c_uint; TA_STYLE_MAX],
    pub(crate) increase_x_height: c_uint,
    pub(crate) max_composite_points: u16,
    pub(crate) max_composite_contours: u16,
    pub(crate) max_storage: u16,
    pub(crate) max_stack_elements: u16,
    pub(crate) max_twilight_points: u16,
    pub(crate) max_instructions: u16,
    pub(crate) max_components: u16,
}

impl Default for Sfnt {
    fn default() -> Self {
        Self {
            glyph_count: 0,
            glyph_styles: Vec::new(),
            sample_glyphs: [0; TA_STYLE_MAX],
            increase_x_height: 0,
            max_composite_points: 0,
            max_composite_contours: 0,
            max_storage: 0,
            max_stack_elements: 0,
            max_twilight_points: 0,
            max_instructions: 0,
            max_components: 0,
        }
    }
}

#[derive(Default)]
pub(crate) struct Font {
    pub(crate) args: Args,
    pub(crate) in_buf: Vec<u8>,
    pub(crate) reference_buf: Option<Vec<u8>>,
    pub(crate) sfnt: Sfnt,
    pub(crate) glyf_ptr_owned: Option<GlyfData>,
    pub(crate) table_store: TableStore,
    pub(crate) have_dsig: bool,
    pub(crate) control: ControlState,
    pub(crate) progress: TaProgressFunc,
    pub(crate) info_data: InfoData,
}
