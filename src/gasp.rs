use write_fonts::types::Tag;

use crate::c_font::Font;

pub(crate) fn update_gasp(tablestore: &mut Font) {
    if tablestore.has_table(Tag::new(b"gasp")) {
        return;
    }
    let bytes = [0x00, 0x01, 0x00, 0x01, 0xFF, 0xFF, 0x00, 0x0F];
    tablestore.update_table(Tag::new(b"gasp"), &bytes);
}
