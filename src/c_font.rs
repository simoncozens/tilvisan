use skrifa::{raw::TableProvider, FontRef, GlyphId, Tag};

use crate::{args::Args, control_index::ControlState, glyf::GlyfData, AutohintError, InfoData};
use core::ffi::{c_int, c_long, c_uint};
use std::collections::BTreeMap;

pub(crate) const TA_STYLE_MAX: usize = 84;
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
    pub(crate) tables: BTreeMap<Tag, TableEntry>, // tag → raw bytes + processing metadata

    // Computed per-SFNT fields (like current SFNT in ta.h):
    pub(crate) max_composite_points: u16,
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
        return self.tables.contains_key(&tag);
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
}
