use skrifa::{
    raw::{FontData, FontRead, TableProvider},
    FontRef, Tag,
};
use write_fonts::{dump_table, from_obj::ToOwnedTable, read::FileRef, tables::maxp::Maxp};

use crate::{error::AutohintError, opcodes::FunctionNumbers, tablestore::TableStore};

pub(crate) fn update_maxp_table_dehint(
    tablestore: &mut TableStore,
    sfnt_index: usize,
) -> Result<(), AutohintError> {
    if tablestore.get_processed(sfnt_index, Tag::new(b"maxp")) {
        return Ok(());
    }

    let Some(table) = tablestore.get_table(sfnt_index, Tag::new(b"maxp")) else {
        return Ok(());
    };

    let bytes = FontData::new(table);
    let read_table = write_fonts::read::tables::maxp::Maxp::read(bytes)?;
    let mut write_table: Maxp = read_table.to_owned_table();
    write_table.max_zones = Some(0);
    write_table.max_twilight_points = Some(0);
    write_table.max_storage = Some(0);
    write_table.max_function_defs = Some(0);
    write_table.max_instruction_defs = Some(0);
    write_table.max_stack_elements = Some(0);
    write_table.max_size_of_instructions = Some(0);

    let out = dump_table(&write_table)?;

    tablestore.update_table(sfnt_index, Tag::new(b"maxp"), &out);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_maxp_table_hinted(
    tablestore: &mut TableStore,
    sfnt_index: usize,
    adjust_composites: bool,
    num_glyphs: u16,
    max_composite_points: u16,
    max_composite_contours: u16,
    max_twilight_points: u16,
    max_storage: u16,
    max_stack_elements: u16,
    max_instructions: u16,
    max_components: u16,
) -> Result<(), AutohintError> {
    if tablestore.get_processed(sfnt_index, Tag::new(b"maxp")) {
        return Ok(());
    }

    let Some(table) = tablestore.get_table(sfnt_index, Tag::new(b"maxp")) else {
        return Ok(());
    };

    let bytes = FontData::new(table);
    let read_table = write_fonts::read::tables::maxp::Maxp::read(bytes)?;
    let mut write_table: Maxp = read_table.to_owned_table();
    if adjust_composites {
        write_table.num_glyphs = num_glyphs;
        write_table.max_composite_points = Some(max_composite_points);
        write_table.max_composite_contours = Some(max_composite_contours);
    }
    write_table.max_zones = Some(2);
    write_table.max_twilight_points = Some(max_twilight_points);
    write_table.max_storage = Some(max_storage);
    write_table.max_function_defs = Some(FunctionNumbers::MAX_FUNCTIONS as u16);
    write_table.max_instruction_defs = Some(0);
    write_table.max_stack_elements = Some(max_stack_elements);
    write_table.max_size_of_instructions = Some(max_instructions);
    write_table.max_component_elements = Some(max_components);

    let out = dump_table(&write_table)?;

    tablestore.update_table(sfnt_index, Tag::new(b"maxp"), &out);

    Ok(())
}

const TTFAUTOHINT_GLYPH_NAME: &[u8] = b".ttfautohint";
const OS2_FSTYPE_OFFSET: usize = 8;

pub(crate) fn sfnt_has_ttfautohint_glyph(
    tablestore: &TableStore,
    sfnt_index: usize,
) -> Result<bool, AutohintError> {
    // NOTE: TableStore-based implementation. If needed, this can be swapped to
    // GlyphNames::new(fontref) once a stable FontRef-from-TableStore path is wired here.
    let Some(post_table) = tablestore.get_table(sfnt_index, Tag::new(b"post")) else {
        return Ok(false);
    };

    Ok(post_table
        .windows(TTFAUTOHINT_GLYPH_NAME.len())
        .any(|w| w == TTFAUTOHINT_GLYPH_NAME))
}

pub(crate) fn sfnt_has_legal_permission(
    tablestore: &TableStore,
    sfnt_index: usize,
) -> Result<bool, AutohintError> {
    let Some(os2_table) = tablestore.get_table(sfnt_index, Tag::new(b"OS/2")) else {
        return Ok(true);
    };

    if os2_table.len() > OS2_FSTYPE_OFFSET + 1 && os2_table[OS2_FSTYPE_OFFSET + 1] == 0x02 {
        return Ok(false);
    }

    Ok(true)
}

pub(crate) fn num_faces_in_font_binary(data: &[u8]) -> Result<u32, AutohintError> {
    let count = match FileRef::new(data)? {
        FileRef::Font(_) => 1,
        FileRef::Collection(collection) => collection.len(),
    };
    Ok(count)
}

pub(crate) fn num_glyphs_in_font_binary_at_index(
    data: &[u8],
    index: u32,
) -> Result<u16, AutohintError> {
    let fontref = FontRef::from_index(data, index)?;
    let maxp = fontref.maxp()?;
    Ok(maxp.num_glyphs())
}

pub(crate) fn units_per_em_in_font_binary_at_index(
    data: &[u8],
    index: u32,
) -> Result<u16, AutohintError> {
    let fontref = FontRef::from_index(data, index)?;
    let head = fontref.head()?;
    Ok(head.units_per_em())
}
