use skrifa::Tag;
use write_fonts::tables::maxp::Maxp;

use crate::{error::AutohintError, font::Font, opcodes::FunctionNumbers};

pub(crate) fn update_maxp_table_dehint(font: &mut Font) -> Result<(), AutohintError> {
    let write_table: &mut Maxp = &mut font.maxp;
    write_table.max_zones = Some(0);
    write_table.max_twilight_points = Some(0);
    write_table.max_storage = Some(0);
    write_table.max_function_defs = Some(0);
    write_table.max_instruction_defs = Some(0);
    write_table.max_stack_elements = Some(0);
    write_table.max_size_of_instructions = Some(0);

    Ok(())
}

pub(crate) fn update_maxp_table_hinted(
    font: &mut Font,
    adjust_composites: bool,
    num_glyphs: u16,
) -> Result<(), AutohintError> {
    if font.get_processed(Tag::new(b"maxp")) {
        return Ok(());
    }

    let max_components = font.final_maxp_data.max_component_elements;
    let max_composite_points = font.final_maxp_data.max_composite_points;
    let max_composite_contours = font.final_maxp_data.max_composite_contours;
    let max_twilight_points = font.final_maxp_data.max_twilight_points;
    let max_storage = font.final_maxp_data.max_storage;
    let max_stack_elements = font.final_maxp_data.max_stack_elements;
    let max_instructions = font.final_maxp_data.max_size_of_instructions;

    let write_table: &mut Maxp = &mut font.maxp;
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

    Ok(())
}
