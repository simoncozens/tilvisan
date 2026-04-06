use indexmap::IndexMap;
use skrifa::{
    raw::{FontData, FontRead as _, TableProvider},
    FontRef, GlyphId, GlyphId16, Tag,
};
use write_fonts::{
    dump_table, from_obj::ToOwnedTable as _, tables::post::Post, types::Version16Dot16,
};

use crate::{
    args::Args, control_index::ControlState, glyf::GlyfData, style::StyleIndex, AutohintError,
    InfoData,
};
use core::ffi::{c_int, c_long, c_uint};
use std::collections::BTreeMap;

pub(crate) const TA_PROP_INCREASE_X_HEIGHT_MIN: c_int = 6;

pub(crate) type TaProgressFunc = Option<fn(GlyphId, GlyphId, c_long, usize) -> c_int>;

/// Metadata wrapper for a raw font table.
#[derive(Debug)]
pub struct TableEntry {
    pub data: Vec<u8>,
    /// True once ttfautohint has processed (and possibly modified) this table.
    pub processed: bool,
}

impl TableEntry {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            processed: false,
        }
    }

    fn new_processed(data: Vec<u8>) -> Self {
        Self {
            data,
            processed: true,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Sfnt {
    pub(crate) glyph_count: c_long,
    pub(crate) glyph_styles: Vec<crate::style::GlyphStyle>,
    pub(crate) sample_glyphs: IndexMap<StyleIndex, GlyphId>,
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
            sample_glyphs: IndexMap::new(),
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
    pub(crate) glyf_data: Option<GlyfData>,
    pub(crate) tables: BTreeMap<Tag, TableEntry>, // tag → raw bytes + processing metadata

    // Computed per-SFNT fields (like current SFNT in ta.h):
    pub(crate) have_dsig: bool,
    pub(crate) control: ControlState,
    pub(crate) progress: TaProgressFunc,
    pub(crate) info_data: InfoData,
}
const TAGS_WE_CREATE: [Tag; 8] = [
    Tag::new(b"cvt "),
    Tag::new(b"fpgm"),
    Tag::new(b"prep"),
    Tag::new(b"gasp"),
    Tag::new(b"hmdx"),
    Tag::new(b"LTSH"),
    Tag::new(b"TTFA"),
    Tag::new(b"VDMX"),
];

impl Font {
    pub fn new(in_buf: Vec<u8>) -> Result<Self, AutohintError> {
        let mut slf = Self {
            in_buf,
            ..Default::default()
        };
        // Populate tables from the input buffer.
        let fontref = FontRef::new(&slf.in_buf)?;
        for table in fontref.table_directory.table_records() {
            if TAGS_WE_CREATE.contains(&table.tag()) {
                continue;
            }
            if let Some(data) = fontref.table_data(table.tag()) {
                slf.tables
                    .insert(table.tag(), TableEntry::new(data.as_bytes().to_vec()));
            }
        }
        // Ensure we have glyf, loca, head and maxp
        for needed in [b"glyf", b"loca", b"head", b"maxp"] {
            if !slf.tables.contains_key(&Tag::new(needed)) {
                return Err(AutohintError::MissingTable(Tag::new(needed)));
            }
        }
        // Read maxp and store max_components
        let maxp = fontref
            .maxp()?
            .max_component_elements()
            .ok_or(AutohintError::InvalidFont(
                "maxp missing max_component_elements",
            ))?;
        slf.sfnt.max_components = maxp;
        Ok(slf)
    }
    pub fn add_table(&mut self, tag: Tag, data: &[u8]) {
        self.tables.insert(tag, TableEntry::new(data.to_vec()));
    }

    pub fn update_table(&mut self, tag: Tag, data: &[u8]) {
        self.tables
            .insert(tag, TableEntry::new_processed(data.to_vec()));
    }

    pub fn set_processed(&mut self, tag: Tag, processed: bool) {
        if let Some(entry) = self.tables.get_mut(&tag) {
            entry.processed = processed;
        }
    }

    pub(crate) fn has_table(&self, tag: Tag) -> bool {
        self.tables.contains_key(&tag)
    }

    pub(crate) fn get_table(&self, tag: Tag) -> Option<&[u8]> {
        self.tables.get(&tag).map(|entry| entry.data.as_slice())
    }

    pub(crate) fn get_processed(&self, tag: Tag) -> bool {
        if let Some(entry) = self.tables.get(&tag) {
            return entry.processed;
        }
        false
    }

    pub(crate) fn clone_table(&self, tag: Tag) -> Option<Vec<u8>> {
        if let Some(entry) = self.tables.get(&tag) {
            return Some(entry.data.clone());
        }
        None
    }

    fn add_dummy_dsig(&mut self) {
        let dummy_dsig = vec![0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00];
        self.add_table(Tag::new(b"DSIG"), &dummy_dsig);
    }

    pub(crate) fn build_ttf_complete(&mut self, have_dsig: bool) -> Vec<u8> {
        let _ = crate::head::update_head(self);
        if have_dsig {
            self.add_dummy_dsig();
        }
        self.build_ttf()
    }

    pub(crate) fn build_ttf(&self) -> Vec<u8> {
        let mut builder = write_fonts::FontBuilder::new();
        for (tag, entry) in &self.tables {
            builder.add_raw(*tag, &entry.data);
        }
        builder.build()
    }

    pub(crate) fn update_hmtx(&mut self) {
        if !self.get_processed(Tag::new(b"hmtx")) {
            if let Some(table) = self.get_table(Tag::new(b"hmtx")) {
                let mut bytes = table.to_vec();
                // Append two zero bytes to the end of the `hmtx` table
                bytes.extend_from_slice(&[0x00, 0x00]);
                self.update_table(Tag::new(b"hmtx"), &bytes);
            }
        }
    }

    pub(crate) fn update_post(&mut self) {
        if self.get_processed(Tag::new(b"post")) {
            println!("`post` table alread processed, skipping update");
            return;
        }
        if let Some(table) = self.get_table(Tag::new(b"post")) {
            let bytes = FontData::new(table);
            let read_table = write_fonts::read::tables::post::Post::read(bytes).unwrap();
            let mut write_table: Post = read_table.to_owned_table();
            match write_table.version {
                Version16Dot16::VERSION_2_5 => {
                    write_table.num_glyphs = write_table.num_glyphs.map(|x| x + 1);
                    self.update_table(Tag::new(b"post"), &dump_table(&write_table).unwrap());
                }
                Version16Dot16::VERSION_2_0 => {
                    // Gather old string names
                    let mut order = (0..read_table.num_glyphs().unwrap_or_default())
                        .filter_map(|gid| read_table.glyph_name(GlyphId16::new(gid)))
                        .collect::<Vec<_>>();
                    order.push(".ttfautohint");
                    let mut new_table = Post::new_v2(order);
                    // Copy old fields
                    new_table.is_fixed_pitch = read_table.is_fixed_pitch();
                    new_table.italic_angle = read_table.italic_angle();
                    new_table.underline_position = read_table.underline_position();
                    new_table.underline_thickness = read_table.underline_thickness();
                    new_table.max_mem_type1 = read_table.max_mem_type1();
                    new_table.max_mem_type42 = read_table.max_mem_type42();
                    new_table.max_mem_type1 = read_table.max_mem_type1();
                    self.update_table(Tag::new(b"post"), &dump_table(&new_table).unwrap());
                }
                _ => {}
            }
        }
    }
}
