use crate::{
    font::Font,
    info::{process_name_post, process_name_record},
    AutohintError,
};

pub(crate) fn update_name_table(font: &mut Font) -> Result<(), AutohintError> {
    let Some(mut write_table) = font.name.take() else {
        return Ok(());
    };

    // Build a Vec<u8> buffer for each name record.
    let mut record_bufs: Vec<Vec<u8>> = write_table
        .name_record
        .iter()
        .map(|r| r.string.to_string().into_bytes())
        .collect();
    let idata = &mut font.info_data;
    let args = &font.args;

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
    font.name = Some(write_table);
    Ok(())
}
