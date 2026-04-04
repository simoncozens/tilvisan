use write_fonts::types::Tag;

use crate::c_font::Font;

pub(crate) fn update_hmtx(font: &mut Font) {
    if font.get_processed(Tag::new(b"hmtx")) {
        println!("`hmtx` table alread processed, skipping update");
        return;
    }
    if let Some(table) = font.get_table(Tag::new(b"hmtx")) {
        let mut bytes = table.to_vec();
        // Append two zero bytes to the end of the `hmtx` table
        bytes.extend_from_slice(&[0x00, 0x00]);
        font.update_table(Tag::new(b"hmtx"), &bytes);
    }
}
