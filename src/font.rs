use indexmap::IndexMap;
use skrifa::{raw::TableProvider, FontRef, GlyphId, GlyphNames, Tag};
use write_fonts::{
    from_obj::ToOwnedTable as _,
    tables::{glyf::Glyf, gpos::Gpos, head::Head, loca::Loca, maxp::Maxp, name::Name, post::Post},
    types::Version16Dot16,
};

use crate::{
    args::Args, control_index::ControlState, glyf::GlyfData, style::StyleIndex, AutohintError,
    InfoData,
};
use core::ffi::{c_int, c_long, c_uint};
use std::collections::HashSet;

pub(crate) const TA_PROP_INCREASE_X_HEIGHT_MIN: c_int = 6;

pub(crate) type TaProgressFunc = Option<fn(GlyphId, GlyphId, c_long, usize) -> c_int>;

#[derive(Default)]
pub(crate) struct FinalMaxpData {
    pub max_composite_points: u16,
    pub max_composite_contours: u16,
    pub max_twilight_points: u16,
    pub max_storage: u16,
    pub max_stack_elements: u16,
    pub max_size_of_instructions: u16,
    pub max_component_elements: u16,
}

impl FinalMaxpData {
    fn from_maxp(maxp: &Maxp) -> Self {
        Self {
            max_composite_points: maxp.max_composite_points.unwrap_or(0),
            max_composite_contours: maxp.max_composite_contours.unwrap_or(0),
            max_twilight_points: maxp.max_twilight_points.unwrap_or(0),
            max_storage: maxp.max_storage.unwrap_or(0),
            max_stack_elements: maxp.max_stack_elements.unwrap_or(0),
            max_size_of_instructions: maxp.max_size_of_instructions.unwrap_or(0),
            max_component_elements: maxp.max_component_elements.unwrap_or(0),
        }
    }
    pub(crate) fn update_max_storage(&mut self, new_value: u16) {
        if new_value > self.max_storage {
            self.max_storage = new_value;
        }
    }
    pub(crate) fn update_max_size_of_instructions(&mut self, new_value: u16) {
        if new_value > self.max_size_of_instructions {
            self.max_size_of_instructions = new_value;
        }
    }
    pub(crate) fn update_max_stack_elements(&mut self, new_value: u16) {
        if new_value > self.max_stack_elements {
            self.max_stack_elements = new_value;
        }
    }
    pub(crate) fn update_max_twilight_points(&mut self, new_value: u16) {
        if new_value > self.max_twilight_points {
            self.max_twilight_points = new_value;
        }
    }
}

pub(crate) struct Font<'a> {
    pub(crate) args: Args,
    pub(crate) fontref: FontRef<'a>,
    pub(crate) reference_buf: Option<Vec<u8>>,

    pub(crate) glyph_count: c_long,
    pub(crate) glyph_styles: Vec<crate::style::GlyphStyle>,
    pub(crate) sample_glyphs: IndexMap<StyleIndex, GlyphId>,
    pub(crate) increase_x_height: c_uint,
    pub(crate) glyf_data: Option<GlyfData>,
    pub(crate) final_maxp_data: FinalMaxpData,

    pub(crate) control: ControlState,
    pub(crate) progress: TaProgressFunc,
    pub(crate) info_data: InfoData,

    pub(crate) gpos: Option<Gpos>,
    pub(crate) glyf_loca: Option<(Glyf, Loca)>,
    pub(crate) head: Head,
    pub(crate) post: Post,
    pub(crate) maxp: Maxp,
    pub(crate) cvt: Vec<u8>,
    pub(crate) hmtx: Vec<u8>,
    pub(crate) prep: Vec<u8>,
    pub(crate) fpgm: Vec<u8>,
    pub(crate) name: Option<Name>,
    pub(crate) processed: HashSet<Tag>,
}

const DUMMY_DSIG: &[u8] = &[0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00];
const GASP_BYTES: &[u8] = &[0x00, 0x01, 0x00, 0x01, 0xFF, 0xFF, 0x00, 0x0F];

impl<'a> Font<'a> {
    pub fn new(fontref: FontRef<'a>) -> Result<Self, AutohintError> {
        let glyphnames = GlyphNames::new(&fontref);
        let maxp = fontref.maxp()?;
        for needed in [b"glyf", b"loca", b"head", b"maxp"] {
            if fontref.table_data(Tag::new(needed)).is_none() {
                return Err(AutohintError::MissingTable(Tag::new(needed)));
            }
        }
        let gpos: Option<Gpos> = fontref.gpos().ok().map(|t| t.to_owned_table());
        let name: Option<Name> = fontref.name().ok().map(|t| t.to_owned_table());
        let post = fontref.post()?.to_owned_table();
        let head = fontref.head()?.to_owned_table();
        let hmtx = fontref
            .table_data(Tag::new(b"hmtx"))
            .map(|t| t.as_bytes().to_vec())
            .unwrap_or_default();

        let slf = Self {
            fontref,
            args: Args::default(),
            reference_buf: None,
            glyph_count: glyphnames.num_glyphs() as i64,
            glyph_styles: vec![],
            sample_glyphs: Default::default(),
            final_maxp_data: FinalMaxpData::from_maxp(&maxp.to_owned_table()),
            increase_x_height: 0,
            glyf_data: None,
            control: ControlState::default(),
            progress: None,
            info_data: InfoData::default(),
            gpos,
            post,
            head,
            glyf_loca: None,
            maxp: maxp.to_owned_table(),
            cvt: vec![],
            prep: vec![],
            fpgm: vec![],
            hmtx,
            name,
            processed: HashSet::new(),
        };
        Ok(slf)
    }

    pub fn set_processed(&mut self, tag: Tag) {
        self.processed.insert(tag);
    }

    pub(crate) fn has_table(&self, tag: Tag) -> bool {
        self.fontref.table_data(tag).is_some()
    }

    pub(crate) fn get_processed(&self, tag: Tag) -> bool {
        self.processed.contains(&tag)
    }

    pub(crate) fn build_ttf_complete(mut self) -> Result<Vec<u8>, AutohintError> {
        let _ = crate::head::update_head(&mut self);
        let mut new_font = write_fonts::FontBuilder::new();
        // If we had a DSIG originally, we need to add a dummy one.
        if self.has_table(Tag::new(b"DSIG")) {
            new_font.add_raw(Tag::new(b"DSIG"), DUMMY_DSIG);
        }
        // Update GASP
        new_font.add_raw(Tag::new(b"gasp"), GASP_BYTES);
        new_font.add_table(&self.head)?;
        new_font.add_table(&self.post)?;
        new_font.add_table(&self.maxp)?;
        if let Some((glyf, loca)) = self.glyf_loca.take() {
            new_font.add_table(&glyf)?;
            new_font.add_table(&loca)?;
        }
        new_font.add_raw(Tag::new(b"hmtx"), &self.hmtx);
        if !self.cvt.is_empty() {
            new_font.add_raw(Tag::new(b"cvt "), &self.cvt);
        }
        if !self.prep.is_empty() {
            new_font.add_raw(Tag::new(b"prep"), &self.prep);
        }
        if !self.fpgm.is_empty() {
            new_font.add_raw(Tag::new(b"fpgm"), &self.fpgm);
        }
        if let Some(gpos) = &self.gpos {
            new_font.add_table(gpos)?;
        }
        if let Some(name) = &self.name {
            new_font.add_table(name)?;
        }
        new_font.copy_missing_tables(self.fontref.clone());
        Ok(new_font.build())
    }

    pub(crate) fn update_hmtx(&mut self) {
        // Append two zero bytes to the end of the `hmtx` table
        self.hmtx.extend_from_slice(&[0x00, 0x00]);
    }

    pub(crate) fn update_post(&mut self) -> Result<(), AutohintError> {
        match self.post.version {
            Version16Dot16::VERSION_2_5 => {
                self.post.num_glyphs = self.post.num_glyphs.map(|x| x + 1);
            }
            Version16Dot16::VERSION_2_0 => {
                // Gather old string names
                let mut order = GlyphNames::new(&self.fontref)
                    .iter()
                    .map(|(_, name)| name.to_string())
                    .collect::<Vec<_>>();
                order.push(".ttfautohint".to_string());
                let mut new_table = Post::new_v2(order.iter().map(|x| x.as_str()));
                // Copy old fields
                new_table.is_fixed_pitch = self.post.is_fixed_pitch;
                new_table.italic_angle = self.post.italic_angle;
                new_table.underline_position = self.post.underline_position;
                new_table.underline_thickness = self.post.underline_thickness;
                new_table.max_mem_type1 = self.post.max_mem_type1;
                new_table.max_mem_type42 = self.post.max_mem_type42;
                new_table.max_mem_type1 = self.post.max_mem_type1;
                self.post = new_table;
            }
            _ => {}
        }

        Ok(())
    }

    pub(crate) fn has_ttfautohint_glyph(&self) -> bool {
        GlyphNames::new(&self.fontref)
            .iter()
            .any(|(_, name)| name.as_str() == ".ttfautohint")
    }

    pub(crate) fn has_legal_permission(&self) -> Result<bool, AutohintError> {
        Ok(self.fontref.os2()?.fs_type() & 0x02 == 0)
    }
}
