use skrifa::GlyphId;

use crate::{c_font::Font, intset::IntSet, AutohintError};
use std::collections::{BTreeMap, HashMap};

const TA_DIGIT: u16 = 0x8000;

#[derive(Debug, Clone)]
pub(crate) enum ResolvedControlEntry {
    Delta {
        font_idx: i32,
        glyph_idx: GlyphId,
        before_iup: bool,
        points: IntSet,
        ppems: IntSet,
        x_shift: i32,
        y_shift: i32,
        line_number: i32,
    },
    SegmentDirection {
        font_idx: i32,
        glyph_idx: GlyphId,
        points: IntSet,
        dir: i32,
        left_offset: i32,
        right_offset: i32,
        line_number: i32,
    },
    StyleAdjust {
        font_idx: i32,
        style: u16,
        glyph_indices: Vec<GlyphId>,
    },
    StemWidthAdjust,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DeltaRule {
    pub(crate) before_iup: u8,
    pub(crate) point_idx: i32,
    pub(crate) ppem: i32,
    pub(crate) x_shift: i32,
    pub(crate) y_shift: i32,
    pub(crate) line_number: i32,
}

#[derive(Debug, Default)]
pub struct ControlIndex {
    delta_rules: HashMap<GlyphId, Vec<DeltaRule>>,
    coverage_rules: Vec<(GlyphId, u16)>,
}

#[derive(Default)]
pub(crate) struct ControlState {
    pub(crate) entries: Vec<ResolvedControlEntry>,
    pub(crate) index: Option<ControlIndex>,
}

impl ControlState {
    pub(crate) fn set_entries(&mut self, entries: Vec<ResolvedControlEntry>) {
        self.entries = entries;
        self.index = None;
    }

    pub(crate) fn has_index(&self) -> bool {
        self.index.is_some()
    }

    pub(crate) fn index(&self) -> Option<&ControlIndex> {
        self.index.as_ref()
    }
}

impl ControlIndex {
    fn from_control_entries(entries: &[ResolvedControlEntry]) -> Self {
        let mut delta_by_key: BTreeMap<(i32, GlyphId, i32, i32), DeltaRule> = BTreeMap::new();
        let mut coverage_by_key: BTreeMap<(i32, GlyphId), u16> = BTreeMap::new();

        for entry in entries {
            match entry {
                ResolvedControlEntry::Delta {
                    font_idx,
                    glyph_idx,
                    before_iup,
                    points,
                    ppems,
                    x_shift,
                    y_shift,
                    line_number,
                } => {
                    for ppem in ppems.iter_values() {
                        for point in points.iter_values() {
                            let key = (*font_idx, *glyph_idx, ppem, point);
                            let rule = DeltaRule {
                                before_iup: u8::from(*before_iup),
                                point_idx: point,
                                ppem,
                                x_shift: *x_shift,
                                y_shift: *y_shift,
                                line_number: *line_number,
                            };
                            delta_by_key.insert(key, rule);
                        }
                    }
                }
                ResolvedControlEntry::SegmentDirection {
                    font_idx,
                    glyph_idx,
                    points,
                    dir,
                    left_offset,
                    right_offset,
                    line_number,
                } => {
                    let _ = (
                        font_idx,
                        glyph_idx,
                        points,
                        dir,
                        left_offset,
                        right_offset,
                        line_number,
                    );
                }
                ResolvedControlEntry::StyleAdjust {
                    font_idx,
                    style,
                    glyph_indices,
                } => {
                    for glyph_idx in glyph_indices {
                        coverage_by_key.insert((*font_idx, *glyph_idx), *style);
                    }
                }
                ResolvedControlEntry::StemWidthAdjust => {}
            }
        }

        let mut index = Self::default();

        for ((font_idx, glyph_idx, _ppem, _point), rule) in delta_by_key {
            index.delta_rules.entry(glyph_idx).or_default().push(rule);
        }

        for ((font_idx, glyph_idx), style) in coverage_by_key {
            index.coverage_rules.push((glyph_idx, style));
        }

        index
    }
}

pub(crate) fn delta_rules_for_glyph(
    index: &ControlIndex,
    font_idx: i32,
    glyph_idx: GlyphId,
) -> Vec<DeltaRule> {
    index
        .delta_rules
        .get(&(glyph_idx))
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn control_apply_coverage(font: &mut Font) {
    let Some(index) = font.control.index() else {
        return;
    };

    let rules = &index.coverage_rules;

    let rules_copy = rules.clone();

    let sfnt = &mut font.sfnt;
    let glyph_styles = &mut sfnt.glyph_styles;
    for &(glyph_idx, style) in &rules_copy {
        let glyph_idx = glyph_idx.to_u32() as usize;
        if glyph_idx >= glyph_styles.len() {
            continue;
        }

        glyph_styles[glyph_idx] &= TA_DIGIT;
        glyph_styles[glyph_idx] |= style;
    }
}

pub(crate) fn control_build_tree_rs(font: &mut Font) -> Result<(), AutohintError> {
    font.control.index = None;

    if font.control.entries.is_empty() {
        return Ok(());
    }

    let entries = &font.control.entries;
    let index = ControlIndex::from_control_entries(entries);

    font.control.index = Some(index);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_style_coverage_last_wins() {
        let entries = vec![
            ResolvedControlEntry::StyleAdjust {
                font_idx: 0,
                style: 100,
                glyph_indices: vec![GlyphId::new(5)],
            },
            ResolvedControlEntry::StyleAdjust {
                font_idx: 0,
                style: 200,
                glyph_indices: vec![GlyphId::new(5)],
            },
        ];

        let index = ControlIndex::from_control_entries(&entries);
        let rules = index.coverage_rules;
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0], (GlyphId::new(5), 200));
    }
}
