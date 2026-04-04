use chrono::{DateTime, TimeZone, Utc};
use skrifa::raw::{FontData, FontRead};
use write_fonts::{
    from_obj::ToOwnedTable,
    tables::head::{Flags, Head},
    types::{LongDateTime, Tag},
};

use crate::{error::AutohintError, tablestore::TableStore};
// The TrueType epoch (1st January 1904) as a Unix timestamp.
// Equivalent to Utc.with_ymd_and_hms(1904, 1, 1, 0, 0, 0).unwrap().timestamp()
const MACINTOSH_EPOCH: i64 = -2082844800;

fn seconds_since_mac_epoch(datetime: DateTime<Utc>) -> i64 {
    let mac_epoch = Utc.timestamp_opt(MACINTOSH_EPOCH, 0).unwrap();
    datetime.signed_duration_since(mac_epoch).num_seconds()
}
pub(crate) fn update_head(tablestore: &mut TableStore) -> Result<(), AutohintError> {
    // Do this unconditionally on save.
    if let Some(head) = tablestore.get_table(Tag::new(b"head")) {
        let head_data = FontData::new(head);
        let read_head = skrifa::raw::tables::head::Head::read(head_data)?;
        let mut write_head: Head = read_head.to_owned_table();
        write_head.flags = write_head
            .flags
            .union(Flags::INSTRUCTIONS_MAY_DEPEND_ON_POINT_SIZE)
            .difference(Flags::INSTRUCTIONS_MAY_ALTER_ADVANCE_WIDTH);
        write_head.modified = LongDateTime::new(seconds_since_mac_epoch(Utc::now()));
        let dumped = write_fonts::dump_table(&write_head)?;
        tablestore.update_table(Tag::new(b"head"), &dumped);
        tablestore.set_processed(Tag::new(b"head"), true);
    }
    Ok(())
}
