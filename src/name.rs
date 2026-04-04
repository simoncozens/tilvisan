use skrifa::{
    raw::{FontData, FontRead},
    Tag,
};
use write_fonts::{dump_table, from_obj::ToOwnedTable, tables::name::Name};

use crate::{
    args::Args,
    info::{process_name_post, process_name_record, InfoData},
    tablestore::TableStore,
    AutohintError,
};

pub(crate) fn update_name_table(
    tablestore: &mut TableStore,
    idata: &mut InfoData,
    args: &Args,
) -> Result<(), AutohintError> {
    if tablestore.get_processed(Tag::new(b"name")) {
        return Ok(());
    }

    let Some(table) = tablestore.get_table(Tag::new(b"name")) else {
        return Ok(());
    };

    let bytes = FontData::new(table);
    let Ok(read_table) = write_fonts::read::tables::name::Name::read(bytes) else {
        // Keep behavior compatible with the C mutator: invalid `name` is non-fatal.
        return Ok(());
    };

    let mut write_table: Name = read_table.to_owned_table();

    // Build a Vec<u8> buffer for each name record.
    let mut record_bufs: Vec<Vec<u8>> = write_table
        .name_record
        .iter()
        .map(|r| r.string.to_string().into_bytes())
        .collect();

    // Process each record.  We always lie and claim Latin1 (platform 1,
    // encoding 0, language 0) to match the original callback convention.
    for (idx, record) in write_table.name_record.iter().enumerate() {
        process_name_record(
            1, // platform_id (Latin1)
            0, // encoding_id
            0, // language_id
            record.name_id.to_u16(),
            idx,
            &mut record_bufs[idx],
            idata,
            args,
        );
    }

    // Apply family-suffix post-processing.
    process_name_post(idata, &mut record_bufs, &args.family_suffix);

    // Write the (possibly modified) buffers back into the table.
    for (record, buf) in write_table.name_record.iter_mut().zip(record_bufs.iter()) {
        record.string = String::from_utf8_lossy(buf).into_owned().into();
    }

    let Ok(out) = dump_table(&write_table) else {
        return Ok(());
    };

    tablestore.update_table(Tag::new(b"name"), &out);
    Ok(())
}
