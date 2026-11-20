use skrifa::Tag;
use write_fonts::{
    tables::{
        glyf::Glyph,
        gpos::{
            AnchorTable, BaseRecord, BaseRecord2, ComponentRecord, ComponentRecord2, CursivePos,
            CursivePosFormat1, CursivePosFormat2, EntryExitRecord, EntryExitRecord2,
            LigatureAttach, LigatureAttach2, Mark2Record, Mark2Record2, MarkBasePos,
            MarkBasePosFormat1, MarkBasePosFormat2, MarkLigPos, MarkLigPosFormat1,
            MarkLigPosFormat2, MarkMarkPos, MarkMarkPosFormat1, MarkMarkPosFormat2, MarkRecord,
            MarkRecord2, PositionLookup,
        },
        layout::CoverageTable,
    },
    NullableOffsetMarker, OffsetMarker,
};

use crate::{font::Font, glyf::ScaledGlyph, AutohintError};

/// A glyph referenced from a coverage table, or `None` if the coverage
/// contains a glyph ID outside the glyph order.
type GlyphRef<'a> = Option<&'a ScaledGlyph>;

fn update_anchor(anchor: &mut AnchorTable, glyph: GlyphRef<'_>) {
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

/// A type which owns anchors that may need adjusting for a given glyph.
///
/// The 16-bit and 24-bit variants of the GPOS subtables are distinct types
/// which differ only in their offset widths, so this trait is implemented for
/// both variants of everything below the subtable level.
trait UpdateAnchors {
    fn update_anchors(&mut self, glyph: GlyphRef<'_>);
}

impl UpdateAnchors for AnchorTable {
    fn update_anchors(&mut self, glyph: GlyphRef<'_>) {
        update_anchor(self, glyph);
    }
}

impl<T: UpdateAnchors, const N: usize> UpdateAnchors for OffsetMarker<T, N> {
    fn update_anchors(&mut self, glyph: GlyphRef<'_>) {
        (**self).update_anchors(glyph);
    }
}

impl<T: UpdateAnchors, const N: usize> UpdateAnchors for NullableOffsetMarker<T, N> {
    fn update_anchors(&mut self, glyph: GlyphRef<'_>) {
        if let Some(anchor) = self.as_mut() {
            anchor.update_anchors(glyph);
        }
    }
}

impl<T: UpdateAnchors> UpdateAnchors for Vec<T> {
    fn update_anchors(&mut self, glyph: GlyphRef<'_>) {
        for item in self.iter_mut() {
            item.update_anchors(glyph);
        }
    }
}

/// Implements `UpdateAnchors` for the records below a subtable, given the
/// fields which hold their anchors. Each record type is listed twice, once for
/// each offset width.
macro_rules! impl_update_anchors {
    ($($ty:ty { $($field:ident),+ $(,)? })*) => {
        $(impl UpdateAnchors for $ty {
            fn update_anchors(&mut self, glyph: GlyphRef<'_>) {
                $(self.$field.update_anchors(glyph);)+
            }
        })*
    };
}

impl_update_anchors! {
    EntryExitRecord { entry_anchor, exit_anchor }
    EntryExitRecord2 { entry_anchor, exit_anchor }
    MarkRecord { mark_anchor }
    MarkRecord2 { mark_anchor }
    BaseRecord { base_anchors }
    BaseRecord2 { base_anchors }
    Mark2Record { mark2_anchors }
    Mark2Record2 { mark2_anchors }
    ComponentRecord { ligature_anchors }
    ComponentRecord2 { ligature_anchors }
    LigatureAttach { component_records }
    LigatureAttach2 { component_records }
}

/// Updates the anchors of every record in `records`, matching records to
/// glyphs via their position in `coverage`.
fn update_covered<T: UpdateAnchors>(
    records: &mut [T],
    coverage: &CoverageTable,
    glyphs: &[ScaledGlyph],
) {
    let covered_glyphs = coverage
        .iter()
        .map(|glyph_id| glyphs.get(glyph_id.to_u32() as usize));
    for (record, glyph) in records.iter_mut().zip(covered_glyphs) {
        record.update_anchors(glyph);
    }
}

/// A GPOS positioning subtable whose anchor point numbers may need adjusting.
trait UpdateSubtable {
    fn update(&mut self, glyphs: &[ScaledGlyph]);
}

/// Implements `UpdateSubtable` for both offset-width variants of a subtable,
/// which have the same fields, and for the enum which wraps them.
///
/// Each `coverage => records` pair names a coverage table in the subtable and
/// the array of records indexed by it.
macro_rules! impl_update_subtable {
    (
        $enum:ident, $format1:ident, $format2:ident;
        $($coverage:ident => $($records:ident).+);+ $(;)?
    ) => {
        impl UpdateSubtable for $enum {
            fn update(&mut self, glyphs: &[ScaledGlyph]) {
                match self {
                    Self::Format1(subtable) => subtable.update(glyphs),
                    Self::Format2(subtable) => subtable.update(glyphs),
                }
            }
        }

        impl_update_subtable!(@format $format1; $($coverage => $($records).+);+);
        impl_update_subtable!(@format $format2; $($coverage => $($records).+);+);
    };

    (@format $format:ident; $($coverage:ident => $($records:ident).+);+) => {
        impl UpdateSubtable for $format {
            fn update(&mut self, glyphs: &[ScaledGlyph]) {
                $(update_covered(&mut self.$($records).+, &self.$coverage, glyphs);)+
            }
        }
    };
}

impl_update_subtable! {
    CursivePos, CursivePosFormat1, CursivePosFormat2;
    coverage => entry_exit_record;
}

impl_update_subtable! {
    MarkBasePos, MarkBasePosFormat1, MarkBasePosFormat2;
    mark_coverage => mark_array.mark_records;
    base_coverage => base_array.base_records;
}

impl_update_subtable! {
    MarkLigPos, MarkLigPosFormat1, MarkLigPosFormat2;
    ligature_coverage => ligature_array.ligature_attaches;
    mark_coverage => mark_array.mark_records;
}

impl_update_subtable! {
    MarkMarkPos, MarkMarkPosFormat1, MarkMarkPosFormat2;
    mark1_coverage => mark1_array.mark_records;
    mark2_coverage => mark2_array.mark2_records;
}

fn update_subtables<T: UpdateSubtable>(subtables: &mut [OffsetMarker<T>], glyphs: &[ScaledGlyph]) {
    for subtable in subtables {
        subtable.update(glyphs);
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

    for lookup in write_table
        .lookup_list
        .as_mut()
        .map(|x| x.lookups.iter_mut())
        .into_iter()
        .flatten()
    {
        match &mut **lookup {
            PositionLookup::Cursive(lookup) => update_subtables(&mut lookup.subtables, glyphs),
            PositionLookup::MarkToBase(lookup) => update_subtables(&mut lookup.subtables, glyphs),
            PositionLookup::MarkToLig(lookup) => update_subtables(&mut lookup.subtables, glyphs),
            PositionLookup::MarkToMark(lookup) => update_subtables(&mut lookup.subtables, glyphs),
            _ => {}
        }
    }

    font.gpos = Some(write_table);
    font.set_processed(Tag::new(b"GPOS"));

    Ok(())
}
