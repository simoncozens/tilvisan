use skrifa::Tag;
use write_fonts::{
    tables::{
        glyf::Glyph,
        gpos::{AnchorTable, PositionLookup},
    },
    NullableOffsetMarker,
};

use crate::{font::Font, glyf::ScaledGlyph, AutohintError};

fn update_anchor(anchor: &mut AnchorTable, glyph: Option<&ScaledGlyph>) {
    let Some(ScaledGlyph {
        glyf: Glyph::Composite(_),
        pointsums,
        ..
    }) = glyph
    else {
        return;
    };
    if let AnchorTable::Format2(anchor_format2) = anchor {
        let anchor_point = anchor_format2.anchor_point;
        let mut i: u16 = 0;

        for &pointsum in pointsums {
            if anchor_point < pointsum {
                break;
            }
            i = i.saturating_add(1);
        }

        anchor_format2.anchor_point = anchor_point.saturating_add(i);
    }
}

fn update_nullable_anchor(
    anchor: &mut NullableOffsetMarker<AnchorTable>,
    glyph: Option<&ScaledGlyph>,
) {
    if let Some(anchor) = anchor.as_mut() {
        update_anchor(anchor, glyph);
    }
}

pub(crate) fn update_gpos(font: &mut Font) -> Result<(), AutohintError> {
    if font.get_processed(Tag::new(b"GPOS")) {
        return Ok(());
    }

    let data = font.glyf_data.as_ref().ok_or(AutohintError::NullPointer)?;
    let glyphs = &data.glyphs;

    let Some(mut write_table) = font.gpos.take() else {
        return Ok(());
    };

    for lookup in write_table.lookup_list.lookups.iter_mut() {
        match &mut **lookup {
            PositionLookup::Single(_) => {}
            PositionLookup::Pair(_) => {}
            PositionLookup::Cursive(lookup) => {
                for subtable in lookup.subtables.iter_mut() {
                    let coverage = subtable
                        .coverage
                        .iter()
                        .map(|x| glyphs.get(x.to_u32() as usize))
                        .collect::<Vec<_>>();
                    for (entry_exit, glyph) in subtable.entry_exit_record.iter_mut().zip(coverage) {
                        update_nullable_anchor(&mut entry_exit.entry_anchor, glyph);
                        update_nullable_anchor(&mut entry_exit.exit_anchor, glyph);
                    }
                }
            }
            PositionLookup::MarkToBase(lookup) => {
                for subtable in lookup.subtables.iter_mut() {
                    let mark_coverage = subtable
                        .mark_coverage
                        .iter()
                        .map(|x| glyphs.get(x.to_u32() as usize))
                        .collect::<Vec<_>>();
                    for (mark_record, glyph) in subtable
                        .mark_array
                        .mark_records
                        .iter_mut()
                        .zip(mark_coverage)
                    {
                        update_anchor(&mut mark_record.mark_anchor, glyph);
                    }

                    let base_coverage = subtable
                        .base_coverage
                        .iter()
                        .map(|x| glyphs.get(x.to_u32() as usize))
                        .collect::<Vec<_>>();
                    for (base_record, glyph) in subtable
                        .base_array
                        .base_records
                        .iter_mut()
                        .zip(base_coverage)
                    {
                        for anchor in base_record.base_anchors.iter_mut() {
                            update_nullable_anchor(anchor, glyph);
                        }
                    }
                }
            }
            PositionLookup::MarkToLig(lookup) => {
                for subtable in lookup.subtables.iter_mut() {
                    let coverage = subtable
                        .ligature_coverage
                        .iter()
                        .map(|x| glyphs.get(x.to_u32() as usize))
                        .collect::<Vec<_>>();
                    for (lig_record, glyph) in subtable
                        .ligature_array
                        .ligature_attaches
                        .iter_mut()
                        .zip(coverage)
                    {
                        for comp in lig_record.component_records.iter_mut() {
                            for anchor in comp.ligature_anchors.iter_mut() {
                                update_nullable_anchor(anchor, glyph);
                            }
                        }
                    }
                    let mark_coverage = subtable
                        .mark_coverage
                        .iter()
                        .map(|x| glyphs.get(x.to_u32() as usize))
                        .collect::<Vec<_>>();
                    for (mark_record, glyph) in subtable
                        .mark_array
                        .mark_records
                        .iter_mut()
                        .zip(mark_coverage)
                    {
                        update_anchor(&mut mark_record.mark_anchor, glyph);
                    }
                }
            }
            PositionLookup::MarkToMark(lookup) => {
                for subtable in lookup.subtables.iter_mut() {
                    let mark1_coverage = subtable
                        .mark1_coverage
                        .iter()
                        .map(|x| glyphs.get(x.to_u32() as usize))
                        .collect::<Vec<_>>();
                    for (mark1_record, glyph) in subtable
                        .mark1_array
                        .mark_records
                        .iter_mut()
                        .zip(mark1_coverage)
                    {
                        update_anchor(&mut mark1_record.mark_anchor, glyph);
                    }

                    let mark2_coverage = subtable
                        .mark2_coverage
                        .iter()
                        .map(|x| glyphs.get(x.to_u32() as usize))
                        .collect::<Vec<_>>();
                    for (mark2_record, glyph) in subtable
                        .mark2_array
                        .mark2_records
                        .iter_mut()
                        .zip(mark2_coverage)
                    {
                        for anchor in mark2_record.mark2_anchors.iter_mut() {
                            update_nullable_anchor(anchor, glyph);
                        }
                    }
                }
            }
            PositionLookup::Contextual(_) => {}
            PositionLookup::ChainContextual(_) => {}
            PositionLookup::Extension(_) => {}
        }
    }

    font.gpos = Some(write_table);
    font.set_processed(Tag::new(b"GPOS"));

    Ok(())
}
