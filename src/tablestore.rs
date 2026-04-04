use std::collections::BTreeMap;

use skrifa::{raw::TableProvider as _, FontRef};
use write_fonts::types::Tag;

use crate::AutohintError;

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

#[derive(Debug, Default)]
pub struct TableStore {
    tables: BTreeMap<Tag, TableEntry>, // tag → raw bytes + processing metadata

    // Computed per-SFNT fields (like current SFNT in ta.h):
    max_composite_points: u16,
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

impl TableStore {
    pub fn new_from_font(font: &FontRef) -> Result<Self, AutohintError> {
        let mut store = TableStore::default();
        for table in font.table_directory.table_records() {
            if TAGS_WE_CREATE.contains(&table.tag()) {
                continue;
            }
            if let Some(data) = font.table_data(table.tag()) {
                store
                    .tables
                    .insert(table.tag(), TableEntry::new(data.as_bytes().to_vec()));
            }
        }
        // Ensure we have glyf, loca, head and maxp
        for needed in [b"glyf", b"loca", b"head", b"maxp"] {
            if !store.tables.contains_key(&Tag::new(needed)) {
                return Err(AutohintError::MissingTable(Tag::new(needed)));
            }
        }
        // Read maxp and store max_components
        let maxp = font
            .maxp()?
            .max_component_elements()
            .ok_or(AutohintError::InvalidFont(
                "maxp missing max_component_elements",
            ))?;
        store.max_composite_points = maxp;
        Ok(store)
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

/// Populate one SFNT slot from a raw font buffer + face index.
pub(crate) fn ta_table_store_populate_sfnt_from_font(
    tablestore: &mut TableStore,
    font_data: &[u8],
) -> Result<(bool, u16), AutohintError> {
    let font = FontRef::new(font_data)?;
    let maxp = font.maxp()?;
    let max_components = maxp
        .max_component_elements()
        .ok_or(AutohintError::InvalidFont(
            "maxp missing max_component_elements",
        ))?;

    let one_sfnt_store = TableStore::new_from_font(&font)?;
    tablestore.tables.extend(one_sfnt_store.tables);
    let have_dsig_out = tablestore.has_table(Tag::new(b"DSIG"));
    Ok((have_dsig_out, max_components))
}
