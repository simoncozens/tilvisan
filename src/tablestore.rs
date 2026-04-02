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
    sfnts: Vec<SFNT>, // Per-SFNT table_infos
}
#[derive(Debug, Default)]
pub struct SFNT {
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

impl SFNT {
    pub fn new(font: &FontRef) -> Result<Self, AutohintError> {
        let mut sfnt = SFNT::default();
        for table in font.table_directory.table_records() {
            if TAGS_WE_CREATE.contains(&table.tag()) {
                continue;
            }
            if let Some(data) = font.table_data(table.tag()) {
                sfnt.tables
                    .insert(table.tag(), TableEntry::new(data.as_bytes().to_vec()));
            }
        }
        // Ensure we have glyf, loca, head and maxp
        for needed in [b"glyf", b"loca", b"head", b"maxp"] {
            if !sfnt.tables.contains_key(&Tag::new(needed)) {
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
        sfnt.max_composite_points = maxp;
        Ok(sfnt)
    }
}

impl TableStore {
    pub fn new_from_font(font: &FontRef) -> Result<Self, AutohintError> {
        let mut store = TableStore::default();
        store.sfnts.push(SFNT::new(font)?);
        Ok(store)
    }
    pub fn add_sfnt(&mut self) -> u32 {
        let sfnt_index = self.sfnts.len() as u32;
        self.sfnts.push(SFNT::default());
        sfnt_index
    }

    pub fn add_table(&mut self, sfnt_index: usize, tag: Tag, data: &[u8]) {
        while self.sfnts.len() <= sfnt_index {
            self.add_sfnt();
        }
        self.sfnts[sfnt_index]
            .tables
            .insert(tag, TableEntry::new(data.to_vec()));
    }

    pub fn update_table(&mut self, sfnt_index: usize, tag: Tag, data: &[u8]) {
        while self.sfnts.len() <= sfnt_index {
            self.add_sfnt();
        }
        self.sfnts[sfnt_index]
            .tables
            .insert(tag, TableEntry::new_processed(data.to_vec()));
    }

    pub fn set_processed(&mut self, sfnt_index: usize, tag: Tag, processed: bool) {
        if let Some(sfnt) = self.sfnts.get_mut(sfnt_index) {
            if let Some(entry) = sfnt.tables.get_mut(&tag) {
                entry.processed = processed;
            }
        }
    }

    pub(crate) fn has_table(&self, sfnt_index: usize, tag: Tag) -> bool {
        if let Some(sfnt) = self.sfnts.get(sfnt_index) {
            return sfnt.tables.contains_key(&tag);
        }
        false
    }

    pub(crate) fn get_table(&self, sfnt_index: usize, tag: Tag) -> Option<&[u8]> {
        if let Some(sfnt) = self.sfnts.get(sfnt_index) {
            if let Some(entry) = sfnt.tables.get(&tag) {
                return Some(&entry.data);
            }
        }
        None
    }

    pub(crate) fn get_processed(&self, sfnt_index: usize, tag: Tag) -> bool {
        if let Some(sfnt) = self.sfnts.get(sfnt_index) {
            if let Some(entry) = sfnt.tables.get(&tag) {
                return entry.processed;
            }
        }
        false
    }

    pub(crate) fn clone_table(&self, sfnt_index: usize, tag: Tag) -> Option<Vec<u8>> {
        if let Some(sfnt) = self.sfnts.get(sfnt_index) {
            if let Some(entry) = sfnt.tables.get(&tag) {
                return Some(entry.data.clone());
            }
        }
        None
    }

    fn add_dummy_dsig(&mut self, sfnt_index: usize) {
        let dummy_dsig = vec![0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00];
        self.add_table(sfnt_index, Tag::new(b"DSIG"), &dummy_dsig);
    }

    pub(crate) fn build_ttf_complete(&mut self, sfnt_index: usize, have_dsig: bool) -> Vec<u8> {
        let _ = crate::head::update_head(self, sfnt_index);
        if have_dsig {
            self.add_dummy_dsig(sfnt_index);
        }
        self.build_ttf(sfnt_index as u32)
    }

    pub(crate) fn build_ttf(&self, sfnt_index: u32) -> Vec<u8> {
        let mut builder = write_fonts::FontBuilder::new();
        if let Some(sfnt) = self.sfnts.get(sfnt_index as usize) {
            for (tag, entry) in &sfnt.tables {
                builder.add_raw(*tag, &entry.data);
            }
        }
        builder.build()
    }
}

#[allow(clippy::missing_safety_doc)] // it's C, all bets are off
pub mod c_api {
    use crate::AutohintError;

    use super::TableStore;
    use skrifa::raw::TableProvider as _;
    use skrifa::FontRef;

    use write_fonts::types::Tag;
    /// Populate one SFNT slot from a raw font buffer + face index.
    pub(crate) fn ta_table_store_populate_sfnt_from_font(
        tablestore: &mut TableStore,
        sfnt_index: u32,
        font_data: &[u8],
    ) -> Result<(bool, u16), AutohintError> {
        let font = FontRef::from_index(font_data, sfnt_index)?;
        let maxp = font.maxp()?;
        let max_components = maxp
            .max_component_elements()
            .ok_or(AutohintError::InvalidFont(
                "maxp missing max_component_elements",
            ))?;

        let mut one_sfnt_store = TableStore::new_from_font(&font)?;
        let Some(new_sfnt) = one_sfnt_store.sfnts.pop() else {
            return Err(AutohintError::NullPointer);
        };

        while tablestore.sfnts.len() <= sfnt_index as usize {
            tablestore.add_sfnt();
        }
        tablestore.sfnts[sfnt_index as usize] = new_sfnt;
        let have_dsig_out = tablestore.has_table(sfnt_index as usize, Tag::new(b"DSIG"));
        Ok((have_dsig_out, max_components))
    }
}
