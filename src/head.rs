use chrono::{DateTime, TimeZone, Utc};
use write_fonts::{tables::head::Flags, types::LongDateTime};

use crate::{error::AutohintError, font::Font};
// The TrueType epoch (1st January 1904) as a Unix timestamp.
// Equivalent to Utc.with_ymd_and_hms(1904, 1, 1, 0, 0, 0).unwrap().timestamp()
const MACINTOSH_EPOCH: i64 = -2082844800;

fn seconds_since_mac_epoch(datetime: DateTime<Utc>) -> i64 {
    let mac_epoch = Utc.timestamp_opt(MACINTOSH_EPOCH, 0).unwrap();
    datetime.signed_duration_since(mac_epoch).num_seconds()
}
pub(crate) fn update_head(font: &mut Font) -> Result<(), AutohintError> {
    // Do this unconditionally on save.
    font.head.flags = font
        .head
        .flags
        .union(Flags::INSTRUCTIONS_MAY_DEPEND_ON_POINT_SIZE)
        .difference(Flags::INSTRUCTIONS_MAY_ALTER_ADVANCE_WIDTH);
    font.head.modified = LongDateTime::new(seconds_since_mac_epoch(Utc::now()));
    Ok(())
}
