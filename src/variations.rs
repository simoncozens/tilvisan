use std::collections::HashSet;

use skrifa::{outline::ExportedHintPlan, raw::TableProvider, GlyphId};
use write_fonts::{tables::gvar::Tent, types::F2Dot14};

use crate::{font::Font, style::StyleIndex, AutohintError};

#[derive(Debug, Clone, PartialEq, Eq)]
struct SegmentSig {
    first_ix: u16,
    last_ix: u16,
    edge_ix: u16,
    edge_next_ix: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EdgeSig {
    first_ix: u16,
    serif_ix: u16,
    blue_ix: u16,
    flags: u8,
    blue_is_shoot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordSig {
    dim: u8,
    action: u32,
    point_ix: u16,
    edge_ix: u16,
    edge2_ix: u16,
    edge3_ix: u16,
    lower_bound_ix: u16,
    upper_bound_ix: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HintPlanSignature {
    segments: Vec<SegmentSig>,
    edges: Vec<EdgeSig>,
    records: Vec<RecordSig>,
}

#[derive(Debug, Clone, Copy, Default)]
struct HintPlanDivergenceMetrics {
    segment_count_delta: usize,
    edge_count_delta: usize,
    record_count_delta: usize,
    segment_mismatch_count: usize,
    edge_mismatch_count: usize,
    record_opcode_mismatch_count: usize,
    record_operand_mismatch_count: usize,
    opcode_mismatch_with_round_flip: usize,
    opcode_mismatch_point_arity_change: usize,
    point_on_between_mismatch_with_round_flip: usize,
}

impl HintPlanDivergenceMetrics {
    fn total_score(self) -> usize {
        self.segment_count_delta
            + self.edge_count_delta
            + self.record_count_delta
            + self.segment_mismatch_count
            + self.edge_mismatch_count
            + self.record_opcode_mismatch_count
            + self.record_operand_mismatch_count
    }

    fn critical_opcode_risk(self) -> usize {
        self.point_on_between_mismatch_with_round_flip
            .saturating_mul(3)
            + self.opcode_mismatch_with_round_flip
            + self.opcode_mismatch_point_arity_change.saturating_sub(1)
    }

    fn is_critical(self) -> bool {
        if self.point_on_between_mismatch_with_round_flip > 0 {
            return true;
        }

        if self.record_count_delta > 0
            && self.record_opcode_mismatch_count >= 4
            && self.record_operand_mismatch_count >= 10
            && self.segment_mismatch_count >= 10
            && self.edge_mismatch_count >= 6
        {
            return true;
        }

        false
    }
}

fn round_bit(flags: u8) -> bool {
    const TA_EDGE_ROUND: u8 = 1 << 0;
    (flags & TA_EDGE_ROUND) != 0
}

fn is_point_ip_action(action: u32) -> bool {
    action == 2 || action == 3
}

fn is_on_between_swap(lhs: u32, rhs: u32) -> bool {
    (lhs == 2 && rhs == 3) || (lhs == 3 && rhs == 2)
}

fn remap_idx(idx: u16, map: &[u16]) -> u16 {
    if idx == 0xFFFF {
        0xFFFF
    } else {
        map.get(idx as usize).copied().unwrap_or(0xFFFF)
    }
}

fn hint_plan_signature(plan: &ExportedHintPlan) -> HintPlanSignature {
    let mut segment_order: Vec<usize> = (0..plan.segments.len()).collect();
    segment_order.sort_by_key(|&idx| {
        let seg = &plan.segments[idx];
        (
            seg.first_ix,
            seg.last_ix,
            seg.edge_ix,
            seg.edge_next_ix,
            idx,
        )
    });

    let mut segment_map = vec![0xFFFF; plan.segments.len()];
    for (canon_idx, old_idx) in segment_order.iter().copied().enumerate() {
        segment_map[old_idx] = u16::try_from(canon_idx).unwrap_or(u16::MAX);
    }

    let mut edge_order: Vec<usize> = (0..plan.edges.len()).collect();
    edge_order.sort_by_key(|&idx| {
        let edge = &plan.edges[idx];
        (
            remap_idx(edge.first_ix, &segment_map),
            edge.flags,
            edge.blue_ix,
            edge.blue_is_shoot != 0,
            edge.serif_ix,
            idx,
        )
    });

    let mut edge_map = vec![0xFFFF; plan.edges.len()];
    for (canon_idx, old_idx) in edge_order.iter().copied().enumerate() {
        edge_map[old_idx] = u16::try_from(canon_idx).unwrap_or(u16::MAX);
    }

    let segments = segment_order
        .into_iter()
        .map(|old_idx| {
            let seg = &plan.segments[old_idx];
            SegmentSig {
                first_ix: seg.first_ix,
                last_ix: seg.last_ix,
                edge_ix: remap_idx(seg.edge_ix, &edge_map),
                edge_next_ix: remap_idx(seg.edge_next_ix, &segment_map),
            }
        })
        .collect();

    let edges = edge_order
        .into_iter()
        .map(|old_idx| {
            let edge = &plan.edges[old_idx];
            EdgeSig {
                first_ix: remap_idx(edge.first_ix, &segment_map),
                serif_ix: remap_idx(edge.serif_ix, &edge_map),
                blue_ix: edge.blue_ix,
                flags: edge.flags,
                blue_is_shoot: edge.blue_is_shoot != 0,
            }
        })
        .collect();

    let records = plan
        .records
        .iter()
        .map(|rec| RecordSig {
            dim: rec.dim,
            action: rec.action as u32,
            point_ix: rec.point_ix,
            edge_ix: remap_idx(rec.edge_ix, &edge_map),
            edge2_ix: remap_idx(rec.edge2_ix, &edge_map),
            edge3_ix: remap_idx(rec.edge3_ix, &edge_map),
            lower_bound_ix: remap_idx(rec.lower_bound_ix, &edge_map),
            upper_bound_ix: remap_idx(rec.upper_bound_ix, &edge_map),
        })
        .collect();

    HintPlanSignature {
        segments,
        edges,
        records,
    }
}

fn hint_plan_divergence_metrics(
    base: &HintPlanSignature,
    other: &HintPlanSignature,
) -> HintPlanDivergenceMetrics {
    let segment_count_delta = base.segments.len().abs_diff(other.segments.len());
    let edge_count_delta = base.edges.len().abs_diff(other.edges.len());
    let record_count_delta = base.records.len().abs_diff(other.records.len());

    let segment_mismatch_count = base
        .segments
        .iter()
        .zip(other.segments.iter())
        .filter(|(l, r)| l != r)
        .count();

    let edge_mismatch_count = base
        .edges
        .iter()
        .zip(other.edges.iter())
        .filter(|(l, r)| l != r)
        .count();

    let mut round_unstable_edges = HashSet::new();
    for (idx, (l, r)) in base.edges.iter().zip(other.edges.iter()).enumerate() {
        if round_bit(l.flags) != round_bit(r.flags) {
            round_unstable_edges.insert(idx as u16);
        }
    }

    let mut record_opcode_mismatch_count = 0usize;
    let mut record_operand_mismatch_count = 0usize;
    let mut opcode_mismatch_with_round_flip = 0usize;
    let mut opcode_mismatch_point_arity_change = 0usize;
    let mut point_on_between_mismatch_with_round_flip = 0usize;

    for (l, r) in base.records.iter().zip(other.records.iter()) {
        if l.dim != r.dim || l.action != r.action {
            record_opcode_mismatch_count += 1;

            let mut has_round_flip_on_referenced_edge = false;
            let referenced_edges = [l.edge_ix, l.edge2_ix, r.edge_ix, r.edge2_ix];
            for edge_idx in referenced_edges {
                if edge_idx != 0xFFFF && round_unstable_edges.contains(&edge_idx) {
                    opcode_mismatch_with_round_flip += 1;
                    has_round_flip_on_referenced_edge = true;
                    break;
                }
            }

            if has_round_flip_on_referenced_edge && is_on_between_swap(l.action, r.action) {
                point_on_between_mismatch_with_round_flip += 1;
            }

            if is_point_ip_action(l.action) || is_point_ip_action(r.action) {
                let left_has_second_edge = l.edge2_ix != 0xFFFF;
                let right_has_second_edge = r.edge2_ix != 0xFFFF;
                if left_has_second_edge != right_has_second_edge {
                    opcode_mismatch_point_arity_change += 1;
                }
            }
        } else if l != r {
            record_operand_mismatch_count += 1;
        }
    }

    HintPlanDivergenceMetrics {
        segment_count_delta,
        edge_count_delta,
        record_count_delta,
        segment_mismatch_count,
        edge_mismatch_count,
        record_opcode_mismatch_count,
        record_operand_mismatch_count,
        opcode_mismatch_with_round_flip,
        opcode_mismatch_point_arity_change,
        point_on_between_mismatch_with_round_flip,
    }
}

fn hint_plan_divergence_details(base: &HintPlanSignature, other: &HintPlanSignature) -> String {
    let mut details = String::new();

    for (idx, (base_edge, other_edge)) in base.edges.iter().zip(other.edges.iter()).enumerate() {
        if base_edge != other_edge {
            details.push_str(&format!(
                "\n  Edge {}: base={{first_ix:{}, flags:{}, blue_ix:{}}} vs other={{first_ix:{}, flags:{}, blue_ix:{}}}",
                idx,
                base_edge.first_ix,
                base_edge.flags,
                base_edge.blue_ix,
                other_edge.first_ix,
                other_edge.flags,
                other_edge.blue_ix
            ));
        }
    }

    for (idx, (base_rec, other_rec)) in base.records.iter().zip(other.records.iter()).enumerate() {
        if base_rec.dim != other_rec.dim || base_rec.action != other_rec.action {
            details.push_str(&format!(
                "\n  Record {} OPCODE mismatch: base action={} vs other action={}",
                idx, base_rec.action, other_rec.action
            ));
            if base_rec.action != other_rec.action {
                details.push_str(&format!(
                    " (edges: base={}/{} vs other={}/{})",
                    base_rec.edge_ix, base_rec.edge2_ix, other_rec.edge_ix, other_rec.edge2_ix
                ));
            }
        } else if base_rec != other_rec {
            details.push_str(&format!(
                "\n  Record {} operand mismatch (action {}): base={{edge:{}, edge2:{}, lower:{}, upper:{}}} vs other={{edge:{}, edge2:{}, lower:{}, upper:{}}}",
                idx,
                base_rec.action,
                base_rec.edge_ix,
                base_rec.edge2_ix,
                base_rec.lower_bound_ix,
                base_rec.upper_bound_ix,
                other_rec.edge_ix,
                other_rec.edge2_ix,
                other_rec.lower_bound_ix,
                other_rec.upper_bound_ix
            ));
        }
    }

    details
}

pub(crate) fn has_stable_hint_plan_across_variations(
    font: &Font,
    glyph_idx: GlyphId,
    ta_style: StyleIndex,
    is_non_base: bool,
    is_digit: bool,
) -> Result<bool, AutohintError> {
    let locations = glyph_variations(font, glyph_idx)?;
    if locations.is_empty() {
        return Ok(true);
    }

    for size in font.args.hinting_range_min..=font.args.hinting_range_max {
        let default_plan = crate::glyf::compute_hint_plan(
            font,
            glyph_idx,
            ta_style.as_usize(),
            is_non_base as u8,
            is_digit as u8,
            size as u16,
            &[],
        )?;
        let default_sig = hint_plan_signature(&default_plan);

        for coords in &locations {
            let var_plan = crate::glyf::compute_hint_plan(
                font,
                glyph_idx,
                ta_style.as_usize(),
                is_non_base as u8,
                is_digit as u8,
                size as u16,
                coords,
            )?;
            let var_sig = hint_plan_signature(&var_plan);
            let metrics = hint_plan_divergence_metrics(&default_sig, &var_sig);
            if metrics.total_score() != 0 {
                let is_critical = metrics.is_critical();
                let risk_level = if is_critical { "CRITICAL" } else { "low risk" };

                if is_critical {
                    log::info!(
                        "glyph {} ppem {} [{}] has critically divergent canonical hint plan at variation {:?}: opcode risk {}, round-flip mismatches {}, point-arity shifts {}, point on/between + round mismatches {}, record_count_delta {}, opcode_mismatch_count {}, operand_mismatch_count {}, segment_mismatch_count {}, edge_mismatch_count {}\n{:?}",
                        font.glyph_name(glyph_idx),
                        size,
                        risk_level,
                        coords,
                        metrics.critical_opcode_risk(),
                        metrics.opcode_mismatch_with_round_flip,
                        metrics.opcode_mismatch_point_arity_change,
                        metrics.point_on_between_mismatch_with_round_flip,
                        metrics.record_count_delta,
                        metrics.record_opcode_mismatch_count,
                        metrics.record_operand_mismatch_count,
                        metrics.segment_mismatch_count,
                        metrics.edge_mismatch_count,
                        hint_plan_divergence_details(&default_sig, &var_sig)
                    );
                    return Ok(false);
                }

                if font.args.debug {
                    let details = hint_plan_divergence_details(&default_sig, &var_sig);
                    log::debug!(
                        "glyph {} ppem {} [{}] divergence details at variation {:?}: {:?}{}",
                        font.glyph_name(glyph_idx),
                        size,
                        risk_level,
                        coords,
                        metrics,
                        details
                    );
                }
            }
        }
    }

    Ok(true)
}

pub(crate) fn glyph_variations(
    font: &Font,
    gid: GlyphId,
) -> Result<Vec<Vec<F2Dot14>>, AutohintError> {
    let Some(gvd) = font.fontref.gvar()?.glyph_variation_data(gid)? else {
        return Ok(vec![]);
    };
    Ok(gvd
        .tuples()
        .map(|t| {
            t.peak()
                .values
                .iter()
                .map(|x| x.get())
                .collect::<Vec<F2Dot14>>()
        })
        .collect())
}

pub(crate) fn peaks(peaks: Vec<F2Dot14>) -> Vec<Tent> {
    peaks
        .into_iter()
        .map(|peak| Tent::new(peak, None))
        .collect()
}
