use skrifa::{
    raw::{FontData, FontRead, TableProvider},
    FontRef, Tag,
};
use write_fonts::{dump_table, from_obj::ToOwnedTable, tables::maxp::Maxp};

use crate::{error::AutohintError, font::Font, opcodes::FunctionNumbers};

pub(crate) fn update_maxp_table_dehint(font: &mut Font) -> Result<(), AutohintError> {
    if font.get_processed(Tag::new(b"maxp")) {
        return Ok(());
    }

    let Some(table) = font.get_table(Tag::new(b"maxp")) else {
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

    font.update_table(Tag::new(b"maxp"), &out);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_maxp_table_hinted(
    font: &mut Font,
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
    if font.get_processed(Tag::new(b"maxp")) {
        return Ok(());
    }

    let Some(table) = font.get_table(Tag::new(b"maxp")) else {
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

    font.update_table(Tag::new(b"maxp"), &out);

    Ok(())
}

const TTFAUTOHINT_GLYPH_NAME: &[u8] = b".ttfautohint";
const OS2_FSTYPE_OFFSET: usize = 8;

pub(crate) fn sfnt_has_ttfautohint_glyph(font: &Font) -> Result<bool, AutohintError> {
    // NOTE: font-based implementation. If needed, this can be swapped to
    // GlyphNames::new(fontref) once a stable FontRef-from-font path is wired here.
    let Some(post_table) = font.get_table(Tag::new(b"post")) else {
        return Ok(false);
    };

    Ok(post_table
        .windows(TTFAUTOHINT_GLYPH_NAME.len())
        .any(|w| w == TTFAUTOHINT_GLYPH_NAME))
}

pub(crate) fn sfnt_has_legal_permission(font: &Font) -> Result<bool, AutohintError> {
    let Some(os2_table) = font.get_table(Tag::new(b"OS/2")) else {
        return Ok(true);
    };

    if os2_table.len() > OS2_FSTYPE_OFFSET + 1 && os2_table[OS2_FSTYPE_OFFSET + 1] == 0x02 {
        return Ok(false);
    }

    Ok(true)
}

pub(crate) fn num_glyphs_in_font_binary(data: &[u8]) -> Result<u16, AutohintError> {
    let fontref = FontRef::new(data)?;
    let maxp = fontref.maxp()?;
    Ok(maxp.num_glyphs())
}

pub(crate) fn units_per_em_in_font_binary(data: &[u8]) -> Result<u16, AutohintError> {
    let fontref = FontRef::new(data)?;
    let head = fontref.head()?;
    Ok(head.units_per_em())
}
