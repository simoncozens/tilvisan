use write_fonts::types::Tag;

use crate::tablestore::TableStore;

pub(crate) fn update_hmtx(tablestore: &mut TableStore, sfnt_index: usize) {
    if tablestore.get_processed(sfnt_index, Tag::new(b"hmtx")) {
        println!("`hmtx` table alread processed, skipping update");
        return;
    }
    if let Some(table) = tablestore.get_table(sfnt_index, Tag::new(b"hmtx")) {
        let mut bytes = table.to_vec();
        // Append two zero bytes to the end of the `hmtx` table
        bytes.extend_from_slice(&[0x00, 0x00]);
        tablestore.update_table(sfnt_index, Tag::new(b"hmtx"), &bytes);
    }
}
