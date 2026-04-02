use crate::tablestore::TableStore;
use write_fonts::dump_table;
use write_fonts::from_obj::ToOwnedTable;
use write_fonts::read::{FontData, FontRead};
use write_fonts::tables::post::Post;
use write_fonts::types::{GlyphId16, Tag, Version16Dot16};

pub(crate) fn update_post(tablestore: &mut TableStore, sfnt_index: usize) {
    if tablestore.get_processed(sfnt_index, Tag::new(b"post")) {
        println!("`post` table alread processed, skipping update");
        return;
    }
    if let Some(table) = tablestore.get_table(sfnt_index, Tag::new(b"post")) {
        let bytes = FontData::new(table);
        let read_table = write_fonts::read::tables::post::Post::read(bytes).unwrap();
        let mut write_table: Post = read_table.to_owned_table();
        match write_table.version {
            Version16Dot16::VERSION_2_5 => {
                write_table.num_glyphs = write_table.num_glyphs.map(|x| x + 1);
                tablestore.update_table(
                    sfnt_index,
                    Tag::new(b"post"),
                    &dump_table(&write_table).unwrap(),
                );
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
                tablestore.update_table(
                    sfnt_index,
                    Tag::new(b"post"),
                    &dump_table(&new_table).unwrap(),
                );
            }
            _ => {}
        }
    }
}
