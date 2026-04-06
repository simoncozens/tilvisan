use std::collections::BTreeSet;

use skrifa::GlyphId;

use crate::{
    bytecode::Bytecode,
    font::Font,
    glyf::{extract_unscaled_outline, ScaledGlyph},
    loader::build_subglyph_shifter_bytecode,
    opcodes::{
        CvtLocations, FunctionNumbers, StorageAreaLocations, CALL, CVT_SCALING_VALUE_OFFSET, EIF,
        ELSE, IF, LT, MPPEM, NPUSHB, NPUSHW, PUSHB_1, PUSHB_2, PUSHW_1, WCVTP,
    },
    AutohintError,
};
use skrifa::outline::{ExportedHintPlan, ExportedHintRecord};

use crate::style::{StyleIndex, STYLE_COUNT};

const ADDITIONAL_STACK_ELEMENTS: u16 = 20;
const TA_DIR_NONE: i32 = 4;

fn compute_cvt_blue_offsets(font: &Font, ta_style: StyleIndex) -> Option<(u16, u16)> {
    if ta_style.as_usize() >= STYLE_COUNT {
        return None;
    }

    // Bounds check and get GlyfData pointer.
    let glyf_data = font.glyf_data.as_ref()?;
    let data = glyf_data
        .style_offsets
        .get(&ta_style)
        .or_else(|| glyf_data.style_offsets.values().next())?;

    let base = (CvtLocations::cvtl_max_runtime as u32)
        .checked_add(3u32.checked_mul(glyf_data.num_used_styles())?)?
        .checked_add(data.cvt_offset)?;

    let cvt_blue_refs_offset_u32 = base
        .checked_add(1)?
        .checked_add(data.horz_width_size)?
        .checked_add(1)?
        .checked_add(data.vert_width_size)?;

    let cvt_blue_shoots_offset_u32 = cvt_blue_refs_offset_u32.checked_add(data.blue_zone_size)?;

    let cvt_blue_refs_offset = u16::try_from(cvt_blue_refs_offset_u32).ok()?;
    let cvt_blue_shoots_offset = u16::try_from(cvt_blue_shoots_offset_u32).ok()?;

    Some((cvt_blue_refs_offset, cvt_blue_shoots_offset))
}

fn get_glyph(font: &Font, glyph_idx: GlyphId) -> Option<&ScaledGlyph> {
    // Bounds check and get GlyfData pointer.
    font.glyf_data.as_ref()?;

    let glyf_data = font.glyf_data.as_ref()?;
    glyf_data.glyphs.get(glyph_idx.to_u32() as usize)
}

fn put_glyph(font: &mut Font, glyph_idx: GlyphId, glyph: ScaledGlyph) {
    // Bounds check and get GlyfData pointer.

    let Some(glyf_data) = font.glyf_data.as_mut() else {
        return;
    };
    if let Some(slot) = glyf_data.glyphs.get_mut(glyph_idx.to_u32() as usize) {
        *slot = glyph;
    }
}

fn log_debug_heading(label: &str, underline_char: char) {
    let underline = underline_char.to_string().repeat(label.len());
    log::debug!("{label}\n{underline}\n\n");
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct RecorderOnPoint {
    edge: u16,
    point: u16,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct RecorderBetweenPoint {
    before_edge: u16,
    after_edge: u16,
    point: u16,
}

struct RustRecorder<'a> {
    glyph: &'a ScaledGlyph,
    ip_before_points: BTreeSet<u16>,
    ip_after_points: BTreeSet<u16>,
    ip_on_points: BTreeSet<RecorderOnPoint>,
    ip_between_points: BTreeSet<RecorderBetweenPoint>,
    segment_map: Vec<u16>,
    segment_map_capacity: u16,
    use_replay_axis: bool,
    replay_first_indices: Vec<u16>,
    replay_last_indices: Vec<u16>,
    replay_axis_num_segments: u16,
    replay_axis_max_segments: u16,
    replay_axis_num_edges: u16,
    replay_axis_max_edges: u16,
    replay_axis_major_dir: i32,
    replay_segment_edge_raw: Vec<u16>,
    replay_edge_first_raw: Vec<u16>,
    replay_edge_serif_raw: Vec<u16>,
    replay_edge_flags: Vec<u8>,
    replay_edge_best_blue_idx: Vec<u16>,
    replay_edge_best_blue_is_shoot: Vec<bool>,
    wrap_around_segments: Vec<u16>,
    num_wrap_around_segments: u16,
    replay_edge_segment_offsets: Vec<u32>,
    replay_edge_segment_indices: Vec<u16>,
    hints_record_buffer: Bytecode,
    hints_record_size: u16,
    hints_record_num_actions: u32,
    num_stack_elements: u16,
}

impl<'a> RustRecorder<'a> {
    fn new(glyph: &'a ScaledGlyph) -> Self {
        Self {
            glyph,
            ip_before_points: BTreeSet::new(),
            ip_after_points: BTreeSet::new(),
            ip_on_points: BTreeSet::new(),
            ip_between_points: BTreeSet::new(),
            segment_map: Vec::new(),
            segment_map_capacity: 0,
            use_replay_axis: false,
            replay_first_indices: Vec::new(),
            replay_last_indices: Vec::new(),
            replay_axis_num_segments: 0,
            replay_axis_max_segments: 0,
            replay_axis_num_edges: 0,
            replay_axis_max_edges: 0,
            replay_axis_major_dir: 0,
            replay_segment_edge_raw: Vec::new(),
            replay_edge_first_raw: Vec::new(),
            replay_edge_serif_raw: Vec::new(),
            replay_edge_flags: Vec::new(),
            replay_edge_best_blue_idx: Vec::new(),
            replay_edge_best_blue_is_shoot: Vec::new(),
            wrap_around_segments: Vec::new(),
            num_wrap_around_segments: 0,
            replay_edge_segment_offsets: Vec::new(),
            replay_edge_segment_indices: Vec::new(),
            hints_record_buffer: Bytecode::new(),
            hints_record_size: 0,
            hints_record_num_actions: 0,
            num_stack_elements: 0,
        }
    }

    fn clear(&mut self) {
        self.ip_before_points.clear();
        self.ip_after_points.clear();
        self.ip_on_points.clear();
        self.ip_between_points.clear();
        self.use_replay_axis = false;
        self.replay_first_indices.clear();
        self.replay_last_indices.clear();
        self.replay_axis_num_segments = 0;
        self.replay_axis_max_segments = 0;
        self.replay_axis_num_edges = 0;
        self.replay_axis_max_edges = 0;
        self.replay_axis_major_dir = 0;
        self.replay_segment_edge_raw.clear();
        self.replay_edge_first_raw.clear();
        self.replay_edge_serif_raw.clear();
        self.replay_edge_flags.clear();
        self.replay_edge_best_blue_idx.clear();
        self.replay_edge_best_blue_is_shoot.clear();
        self.num_wrap_around_segments = 0;
        self.replay_edge_segment_offsets.clear();
        self.replay_edge_segment_indices.clear();
    }

    fn ensure_segment_map_capacity(&mut self, num_segments: u16) -> bool {
        let new_len = num_segments as usize + 1;
        if new_len > self.segment_map.len() {
            if self
                .segment_map
                .try_reserve_exact(new_len.saturating_sub(self.segment_map.len()))
                .is_err()
            {
                return false;
            }

            self.segment_map.resize(new_len, 0xFFFF);
        }

        if num_segments > self.segment_map_capacity {
            self.segment_map_capacity = num_segments;
        }

        true
    }

    fn get_segment_map_entry(&self, idx: u16) -> u16 {
        self.segment_map
            .get(idx as usize)
            .copied()
            .unwrap_or(0xFFFF)
    }

    fn glyph_ref(&self) -> Option<&ScaledGlyph> {
        Some(self.glyph)
    }

    fn glyph_num_components(&self) -> u16 {
        let Some(glyph) = self.glyph_ref() else {
            return 0;
        };
        glyph.num_components()
    }

    fn glyph_pointsums_len(&self) -> u16 {
        let Some(glyph) = self.glyph_ref() else {
            return 0;
        };
        glyph.pointsums_len()
    }

    fn glyph_pointsum(&self, idx: u16) -> u16 {
        let Some(glyph) = self.glyph_ref() else {
            return 0;
        };
        glyph.pointsum(idx)
    }

    fn adjust_point_index(&self, idx: u32, hint_composites: bool) -> u32 {
        if !hint_composites || self.glyph_num_components() == 0 {
            return idx;
        }

        let mut i: u16 = 0;
        while i < self.glyph_pointsums_len() {
            if idx < self.glyph_pointsum(i) as u32 {
                break;
            }
            i += 1;
        }

        idx + i as u32
    }

    fn set_use_replay_axis(&mut self, enabled: bool) {
        self.use_replay_axis = enabled;
    }

    fn set_replay_indices(&mut self, first: &[u16], last: &[u16]) -> bool {
        if first.len() != last.len() {
            return false;
        }

        self.replay_first_indices.clear();
        self.replay_last_indices.clear();

        if self
            .replay_first_indices
            .try_reserve_exact(first.len())
            .is_err()
        {
            return false;
        }
        if self
            .replay_last_indices
            .try_reserve_exact(last.len())
            .is_err()
        {
            return false;
        }

        self.replay_first_indices.extend_from_slice(first);
        self.replay_last_indices.extend_from_slice(last);
        true
    }

    fn replay_first_index(&self, idx: u16) -> u16 {
        self.replay_first_indices
            .get(idx as usize)
            .copied()
            .unwrap_or(0)
    }

    fn replay_last_index(&self, idx: u16) -> u16 {
        self.replay_last_indices
            .get(idx as usize)
            .copied()
            .unwrap_or(0)
    }

    fn get_replay_axis_num_edges(&self) -> u16 {
        self.replay_axis_num_edges
    }

    fn ensure_wrap_around_capacity(&mut self, count: u16) -> bool {
        let needed = count as usize;

        if needed > self.wrap_around_segments.len() {
            if self
                .wrap_around_segments
                .try_reserve_exact(needed.saturating_sub(self.wrap_around_segments.len()))
                .is_err()
            {
                return false;
            }
            self.wrap_around_segments.resize(needed, 0);
        }

        true
    }

    fn num_wrap_around_segments(&self) -> u16 {
        self.num_wrap_around_segments
    }

    fn initialize_replay_segment_metadata(&mut self) -> bool {
        let num_segments = self.replay_axis_num_segments;
        let mut mapped_values = Vec::new();
        let mut wrap_values = Vec::new();

        if mapped_values
            .try_reserve_exact(num_segments as usize + 1)
            .is_err()
            || wrap_values
                .try_reserve_exact(num_segments as usize)
                .is_err()
        {
            return false;
        }

        {
            let mut mapped_idx: u16 = 0;

            for raw_idx in 0..num_segments as usize {
                let edge_idx = self
                    .replay_segment_edge_raw
                    .get(raw_idx)
                    .copied()
                    .unwrap_or(0xFFFF);

                let mapped = if edge_idx == 0xFFFF {
                    0xFFFF
                } else {
                    let cur = mapped_idx;
                    mapped_idx = match mapped_idx.checked_add(1) {
                        Some(val) => val,
                        None => return false,
                    };
                    cur
                };

                mapped_values.push(mapped);

                let raw_idx = raw_idx as u16;
                if mapped != 0xFFFF
                    && self.replay_first_index(raw_idx) > self.replay_last_index(raw_idx)
                {
                    wrap_values.push(mapped);
                }
            }

            mapped_values.push(mapped_idx);
        }

        if !self.ensure_segment_map_capacity(num_segments) {
            return false;
        }

        for (i, value) in mapped_values.into_iter().enumerate() {
            self.segment_map[i] = value;
        }

        let wrap_count = match u16::try_from(wrap_values.len()) {
            Ok(val) => val,
            Err(_) => return false,
        };

        if !self.ensure_wrap_around_capacity(wrap_count) {
            return false;
        }

        if wrap_count != 0 {
            self.wrap_around_segments[..wrap_values.len()].copy_from_slice(&wrap_values);
        }
        self.num_wrap_around_segments = wrap_count;

        true
    }

    fn replay_edge_segment_index_count(&self, edge_idx: u16) -> usize {
        let i = edge_idx as usize;
        if i + 1 >= self.replay_edge_segment_offsets.len() {
            return 0;
        }

        let start = self.replay_edge_segment_offsets[i] as usize;
        let end = self.replay_edge_segment_offsets[i + 1] as usize;
        end.saturating_sub(start)
    }

    fn dump_replay_edge_segment_indices(&self, edge_idx: u16, out: &mut [u16]) -> usize {
        let i = edge_idx as usize;
        if i + 1 >= self.replay_edge_segment_offsets.len() {
            return 0;
        }

        let start = self.replay_edge_segment_offsets[i] as usize;
        let end = self.replay_edge_segment_offsets[i + 1] as usize;
        let count = end.saturating_sub(start);

        if count > out.len() {
            return 0;
        }

        for (dst, raw_idx) in out
            .iter_mut()
            .zip(self.replay_edge_segment_indices[start..end].iter())
        {
            *dst = self.get_segment_map_entry(*raw_idx);
        }

        count
    }

    fn edge_flags_by_idx(&self, edge_idx: u16) -> Option<u8> {
        self.replay_edge_flags.get(edge_idx as usize).copied()
    }

    fn edge_best_blue_idx_by_idx(&self, edge_idx: u16) -> Option<u16> {
        self.replay_edge_best_blue_idx
            .get(edge_idx as usize)
            .copied()
    }

    fn edge_best_blue_is_shoot_by_idx(&self, edge_idx: u16) -> bool {
        self.replay_edge_best_blue_is_shoot
            .get(edge_idx as usize)
            .copied()
            .unwrap_or(false)
    }

    fn edge_serif_idx_by_idx(&self, edge_idx: u16) -> Option<u16> {
        self.replay_edge_serif_raw.get(edge_idx as usize).copied()
    }

    fn edge_first_mapped_segment_index(&self, edge_idx: u16) -> u16 {
        let Some(raw_seg_idx_u16) = self.replay_edge_first_raw.get(edge_idx as usize).copied()
        else {
            return 0xFFFF;
        };

        if raw_seg_idx_u16 == 0xFFFF {
            return 0xFFFF;
        }

        self.get_segment_map_entry(raw_seg_idx_u16)
    }

    fn num_active_segments(&self) -> u16 {
        self.get_segment_map_entry(self.replay_axis_num_segments)
    }

    fn fill_active_segment_point_indices(
        &self,
        first_out: &mut [u32],
        last_out: &mut [u32],
    ) -> bool {
        let num_segments = self.replay_axis_num_segments as usize;
        let num_active = self.num_active_segments() as usize;

        if num_active == 0 {
            return true;
        }

        if first_out.len() < num_active || last_out.len() < num_active {
            return false;
        }

        for raw_idx in 0..num_segments {
            let raw_idx_u16 = raw_idx as u16;
            let mapped_idx = self.get_segment_map_entry(raw_idx_u16);

            if mapped_idx == 0xFFFF {
                continue;
            }

            let mapped = mapped_idx as usize;
            if mapped >= first_out.len() || mapped >= last_out.len() {
                return false;
            }

            first_out[mapped] = self.replay_first_index(raw_idx_u16) as u32;
            last_out[mapped] = self.replay_last_index(raw_idx_u16) as u32;
        }

        true
    }

    fn between_edge_pair_count(&self) -> usize {
        let mut count = 0;
        let mut prev_pair = None;

        for entry in &self.ip_between_points {
            let pair = (entry.before_edge, entry.after_edge);
            if prev_pair != Some(pair) {
                count += 1;
                prev_pair = Some(pair);
            }
        }

        count
    }

    fn between_point_count_for_pair(&self, before_edge_idx: u16, after_edge_idx: u16) -> usize {
        let start = RecorderBetweenPoint {
            before_edge: before_edge_idx,
            after_edge: after_edge_idx,
            point: u16::MIN,
        };
        let end = RecorderBetweenPoint {
            before_edge: before_edge_idx,
            after_edge: after_edge_idx,
            point: u16::MAX,
        };

        self.ip_between_points.range(start..=end).count()
    }

    fn emit_point_hints_bytes(
        &self,
        hint_composites: bool,
    ) -> Result<(Vec<u8>, u32), AutohintError> {
        const ACTION_OFFSET: u8 = FunctionNumbers::bci_action_ip_before as u8;
        const TA_IP_BEFORE: u8 = 0;
        const TA_IP_AFTER: u8 = 1;
        const TA_IP_ON: u8 = 2;
        const TA_IP_BETWEEN: u8 = 3;

        let mut out = Bytecode::new();
        let mut num_actions: u32 = 0;

        let num_edges = self.get_replay_axis_num_edges();

        if !self.ip_before_points.is_empty() {
            if num_edges == 0 {
                return Err(AutohintError::NullPointer);
            }

            num_actions += 1;

            let edge_first_idx = self.edge_first_mapped_segment_index(0);

            out.push_word((TA_IP_BEFORE + ACTION_OFFSET) as u32);
            out.push_word(edge_first_idx as u32);
            let before_len = match u16::try_from(self.ip_before_points.len()) {
                Ok(v) => v,
                Err(_) => return Err(AutohintError::NullPointer),
            };
            out.push_word(before_len as u32);

            for point in &self.ip_before_points {
                let adjusted = self.adjust_point_index(*point as u32, hint_composites) as u16;
                out.push_word(adjusted as u32);
            }
        }

        if !self.ip_after_points.is_empty() {
            if num_edges == 0 {
                return Err(AutohintError::NullPointer);
            }

            num_actions += 1;

            let edge_idx = num_edges - 1;
            let edge_first_idx = self.edge_first_mapped_segment_index(edge_idx);

            out.push_word((TA_IP_AFTER + ACTION_OFFSET) as u32);
            out.push_word(edge_first_idx as u32);
            let after_len = match u16::try_from(self.ip_after_points.len()) {
                Ok(v) => v,
                Err(_) => return Err(AutohintError::NullPointer),
            };
            out.push_word(after_len as u32);

            for point in &self.ip_after_points {
                let adjusted = self.adjust_point_index(*point as u32, hint_composites) as u16;
                out.push_word(adjusted as u32);
            }
        }

        if !self.ip_on_points.is_empty() {
            let mut group_count: u32 = 0;
            let mut prev_edge = None;

            for entry in &self.ip_on_points {
                if prev_edge != Some(entry.edge) {
                    group_count += 1;
                    prev_edge = Some(entry.edge);
                }
            }

            num_actions += 1;

            out.push_word((TA_IP_ON + ACTION_OFFSET) as u32);
            let group_count_u16 = match u16::try_from(group_count) {
                Ok(v) => v,
                Err(_) => return Err(AutohintError::NullPointer),
            };
            out.push_word(group_count_u16 as u32);

            let mut it = self.ip_on_points.iter().peekable();
            while let Some(first) = it.next() {
                let edge_idx = first.edge;
                let edge_first_idx = self.edge_first_mapped_segment_index(edge_idx);

                let mut points_count: u32 = 1;
                let mut points = vec![first.point];

                while let Some(next) = it.peek() {
                    if next.edge != edge_idx {
                        break;
                    }

                    points.push(next.point);
                    points_count += 1;
                    let _ = it.next();
                }

                out.push_word(edge_first_idx as u32);
                let points_count_u16 = match u16::try_from(points_count) {
                    Ok(v) => v,
                    Err(_) => return Err(AutohintError::NullPointer),
                };
                out.push_word(points_count_u16 as u32);

                for point in points {
                    let adjusted = self.adjust_point_index(point as u32, hint_composites) as u16;
                    out.push_word(adjusted as u32);
                }
            }
        }

        let pair_count = self.between_edge_pair_count();
        if pair_count > 0 {
            num_actions += 1;

            out.push_word((TA_IP_BETWEEN + ACTION_OFFSET) as u32);
            let pair_count_u16 = match u16::try_from(pair_count) {
                Ok(v) => v,
                Err(_) => return Err(AutohintError::NullPointer),
            };
            out.push_word(pair_count_u16 as u32);

            let mut prev_pair = None;
            for entry in &self.ip_between_points {
                let pair = (entry.before_edge, entry.after_edge);
                if prev_pair == Some(pair) {
                    continue;
                }
                prev_pair = Some(pair);

                let before_first_idx = self.edge_first_mapped_segment_index(entry.before_edge);
                let after_first_idx = self.edge_first_mapped_segment_index(entry.after_edge);
                let point_count =
                    self.between_point_count_for_pair(entry.before_edge, entry.after_edge);

                out.push_word(after_first_idx as u32);
                out.push_word(before_first_idx as u32);
                let point_count_u16 = match u16::try_from(point_count) {
                    Ok(v) => v,
                    Err(_) => return Err(AutohintError::NullPointer),
                };
                out.push_word(point_count_u16 as u32);

                let start = RecorderBetweenPoint {
                    before_edge: entry.before_edge,
                    after_edge: entry.after_edge,
                    point: u16::MIN,
                };
                let end = RecorderBetweenPoint {
                    before_edge: entry.before_edge,
                    after_edge: entry.after_edge,
                    point: u16::MAX,
                };

                for point_entry in self.ip_between_points.range(start..=end) {
                    let adjusted =
                        self.adjust_point_index(point_entry.point as u32, hint_composites) as u16;
                    out.push_word(adjusted as u32);
                }
            }
        }

        Ok((out.into_iter().collect(), num_actions))
    }
}

#[derive(Copy, Clone)]
pub struct RecorderMarshaledAction {
    pub edge1_first_idx: u16,
    pub edge2_first_idx: u16,
    pub edge3_first_idx: u16,
    pub lower_bound_first_idx: u16,
    pub upper_bound_first_idx: u16,
    pub primary_is_round: bool,
    pub secondary_is_serif: bool,
    pub cvt_idx: u16,
    pub segment_edge_indices: [u16; 2],
    pub num_segment_edges: u16,
}

struct ReplayProcessResult {
    bytecode: Bytecode,
    did_emit_action: bool,
}

struct GlyphSegmentsBytecode {
    bytecode: Bytecode,
    num_segments: u16,
    num_args: u16,
}

impl RecorderMarshaledAction {
    fn init() -> Self {
        Self {
            edge1_first_idx: 0xFFFF,
            edge2_first_idx: 0xFFFF,
            edge3_first_idx: 0xFFFF,
            lower_bound_first_idx: 0xFFFF,
            upper_bound_first_idx: 0xFFFF,
            primary_is_round: false,
            secondary_is_serif: false,
            cvt_idx: 0,
            segment_edge_indices: [0xFFFF, 0xFFFF],
            num_segment_edges: 0,
        }
    }
}

/// Build point-hints bytecode and store the result into the recorder's
/// internal hints-record buffer, replacing `hints_record_num_actions`.
fn recorder_build_point_hints(
    recorder: &mut RustRecorder,
    hint_composites: bool,
) -> Result<(), AutohintError> {
    let (emitted, num_actions) = recorder.emit_point_hints_bytes(hint_composites)?;
    recorder.hints_record_buffer.extend_bytes(&emitted);
    recorder.hints_record_num_actions = num_actions;
    Ok(())
}

/// Clear the hints-record bytecode buffer and reset the action counter,
/// preserving `hints_record_size`.  Called before building point hints for
/// the same ppem value whose action hints were already recorded.
fn recorder_reset_hints_record(recorder: &mut RustRecorder) {
    recorder.hints_record_buffer = Bytecode::new();
    recorder.hints_record_num_actions = 0;
}

fn marshal_action_fields(
    recorder: &RustRecorder,
    action: u32,
    arg1_edge_idx: u16,
    arg2_edge_idx: u16,
    arg3_edge_idx: u16,
    lower_bound_edge_idx: u16,
    upper_bound_edge_idx: u16,
    cvt_blue_refs_offset: u16,
    cvt_blue_shoots_offset: u16,
) -> Option<RecorderMarshaledAction> {
    const TA_EDGE_ROUND: u8 = 1 << 0;
    const TA_EDGE_SERIF: u8 = 1 << 1;

    const TA_BLUE: u32 = 4;
    const TA_BLUE_ANCHOR: u32 = 5;
    const TA_ANCHOR: u32 = 6;
    const TA_ADJUST: u32 = 10;
    const TA_LINK: u32 = 22;
    const TA_STEM: u32 = 26;
    const TA_SERIF: u32 = 38;
    const TA_SERIF_ANCHOR: u32 = 45;
    const TA_SERIF_LINK1: u32 = 52;
    const TA_SERIF_LINK2: u32 = 59;

    let mut m = RecorderMarshaledAction::init();

    let maybe_bound_first_idx = |edge_idx: u16| -> u16 {
        if edge_idx == 0xFFFF {
            0xFFFF
        } else {
            recorder.edge_first_mapped_segment_index(edge_idx)
        }
    };

    match action {
        TA_LINK => {
            let base_flags = recorder.edge_flags_by_idx(arg1_edge_idx)?;
            let stem_flags = recorder.edge_flags_by_idx(arg2_edge_idx)?;

            m.edge1_first_idx = recorder.edge_first_mapped_segment_index(arg1_edge_idx);
            m.edge2_first_idx = recorder.edge_first_mapped_segment_index(arg2_edge_idx);
            m.primary_is_round = (base_flags & TA_EDGE_ROUND) != 0;
            m.secondary_is_serif = (stem_flags & TA_EDGE_SERIF) != 0;
            m.segment_edge_indices[0] = arg2_edge_idx;
            m.num_segment_edges = 1;
        }

        TA_ANCHOR => {
            let edge_flags = recorder.edge_flags_by_idx(arg1_edge_idx)?;
            let edge2_flags = recorder.edge_flags_by_idx(arg2_edge_idx)?;

            m.edge1_first_idx = recorder.edge_first_mapped_segment_index(arg1_edge_idx);
            m.edge2_first_idx = recorder.edge_first_mapped_segment_index(arg2_edge_idx);
            m.primary_is_round = (edge_flags & TA_EDGE_ROUND) != 0;
            m.secondary_is_serif = (edge2_flags & TA_EDGE_SERIF) != 0;
            m.segment_edge_indices[0] = arg1_edge_idx;
            m.num_segment_edges = 1;
        }

        TA_ADJUST => {
            let edge_flags = recorder.edge_flags_by_idx(arg1_edge_idx)?;
            let edge2_flags = recorder.edge_flags_by_idx(arg2_edge_idx)?;

            m.edge1_first_idx = recorder.edge_first_mapped_segment_index(arg1_edge_idx);
            m.edge2_first_idx = recorder.edge_first_mapped_segment_index(arg2_edge_idx);
            m.edge3_first_idx = maybe_bound_first_idx(lower_bound_edge_idx);
            m.primary_is_round = (edge_flags & TA_EDGE_ROUND) != 0;
            m.secondary_is_serif = (edge2_flags & TA_EDGE_SERIF) != 0;
            m.segment_edge_indices[0] = arg1_edge_idx;
            m.num_segment_edges = 1;
        }

        TA_BLUE_ANCHOR => {
            let best_blue_idx = recorder.edge_best_blue_idx_by_idx(arg1_edge_idx)?;
            m.edge1_first_idx = recorder.edge_first_mapped_segment_index(arg1_edge_idx);
            m.edge2_first_idx = recorder.edge_first_mapped_segment_index(arg2_edge_idx);
            m.cvt_idx = if recorder.edge_best_blue_is_shoot_by_idx(arg1_edge_idx) {
                cvt_blue_shoots_offset.saturating_add(best_blue_idx)
            } else {
                cvt_blue_refs_offset.saturating_add(best_blue_idx)
            };
            m.segment_edge_indices[0] = arg1_edge_idx;
            m.num_segment_edges = 1;
        }

        TA_STEM => {
            let edge_flags = recorder.edge_flags_by_idx(arg1_edge_idx)?;
            let edge2_flags = recorder.edge_flags_by_idx(arg2_edge_idx)?;

            m.edge1_first_idx = recorder.edge_first_mapped_segment_index(arg1_edge_idx);
            m.edge2_first_idx = recorder.edge_first_mapped_segment_index(arg2_edge_idx);
            m.edge3_first_idx = maybe_bound_first_idx(lower_bound_edge_idx);
            m.primary_is_round = (edge_flags & TA_EDGE_ROUND) != 0;
            m.secondary_is_serif = (edge2_flags & TA_EDGE_SERIF) != 0;
            m.segment_edge_indices[0] = arg1_edge_idx;
            m.segment_edge_indices[1] = arg2_edge_idx;
            m.num_segment_edges = 2;
        }

        TA_BLUE => {
            let best_blue_idx = recorder.edge_best_blue_idx_by_idx(arg1_edge_idx)?;

            m.edge1_first_idx = recorder.edge_first_mapped_segment_index(arg1_edge_idx);
            m.cvt_idx = if recorder.edge_best_blue_is_shoot_by_idx(arg1_edge_idx) {
                cvt_blue_shoots_offset.saturating_add(best_blue_idx)
            } else {
                cvt_blue_refs_offset.saturating_add(best_blue_idx)
            };
            m.segment_edge_indices[0] = arg1_edge_idx;
            m.num_segment_edges = 1;
        }

        TA_SERIF => {
            let base_edge_idx = recorder.edge_serif_idx_by_idx(arg1_edge_idx)?;
            if base_edge_idx == 0xFFFF {
                return None;
            }

            m.edge1_first_idx = recorder.edge_first_mapped_segment_index(arg1_edge_idx);
            m.edge2_first_idx = recorder.edge_first_mapped_segment_index(base_edge_idx);
            m.lower_bound_first_idx = maybe_bound_first_idx(lower_bound_edge_idx);
            m.upper_bound_first_idx = maybe_bound_first_idx(upper_bound_edge_idx);
            m.segment_edge_indices[0] = arg1_edge_idx;
            m.num_segment_edges = 1;
        }

        TA_SERIF_ANCHOR | TA_SERIF_LINK2 => {
            recorder.edge_flags_by_idx(arg1_edge_idx)?;

            m.edge1_first_idx = recorder.edge_first_mapped_segment_index(arg1_edge_idx);
            m.lower_bound_first_idx = maybe_bound_first_idx(lower_bound_edge_idx);
            m.upper_bound_first_idx = maybe_bound_first_idx(upper_bound_edge_idx);
            m.segment_edge_indices[0] = arg1_edge_idx;
            m.num_segment_edges = 1;
        }

        TA_SERIF_LINK1 => {
            if recorder.edge_flags_by_idx(arg1_edge_idx).is_none()
                || recorder.edge_flags_by_idx(arg2_edge_idx).is_none()
                || recorder.edge_flags_by_idx(arg3_edge_idx).is_none()
            {
                return None;
            }

            m.edge1_first_idx = recorder.edge_first_mapped_segment_index(arg2_edge_idx);
            m.edge2_first_idx = recorder.edge_first_mapped_segment_index(arg1_edge_idx);
            m.edge3_first_idx = recorder.edge_first_mapped_segment_index(arg3_edge_idx);
            m.lower_bound_first_idx = maybe_bound_first_idx(lower_bound_edge_idx);
            m.upper_bound_first_idx = maybe_bound_first_idx(upper_bound_edge_idx);
            m.segment_edge_indices[0] = arg1_edge_idx;
            m.num_segment_edges = 1;
        }

        _ => return None,
    }

    Some(m)
}

fn hints_recorder_marshal_and_emit_action(
    recorder: &RustRecorder,
    action: u32,
    arg1_edge_idx: u16,
    arg2_edge_idx: u16,
    arg3_edge_idx: u16,
    lower_bound_edge_idx: u16,
    upper_bound_edge_idx: u16,
    cvt_blue_refs_offset: u16,
    cvt_blue_shoots_offset: u16,
    top_to_bottom_hinting: bool,
) -> Result<Bytecode, AutohintError> {
    let Some(m) = marshal_action_fields(
        recorder,
        action,
        arg1_edge_idx,
        arg2_edge_idx,
        arg3_edge_idx,
        lower_bound_edge_idx,
        upper_bound_edge_idx,
        cvt_blue_refs_offset,
        cvt_blue_shoots_offset,
    ) else {
        // Keep C behavior: unsupported actions emit nothing and are not an error.
        return Ok(Bytecode::new());
    };

    let mut segment_indices1 = Vec::<u16>::new();
    let mut segment_indices2 = Vec::<u16>::new();

    if m.num_segment_edges >= 1 {
        let edge_idx = m.segment_edge_indices[0];
        let count = recorder.replay_edge_segment_index_count(edge_idx);
        if count > 0 {
            if segment_indices1.try_reserve_exact(count).is_err() {
                return Err(AutohintError::OutOfMemory);
            }
            segment_indices1.resize(count, 0);
            if recorder.dump_replay_edge_segment_indices(edge_idx, &mut segment_indices1) != count {
                return Err(AutohintError::NullPointer);
            }
        }
    }

    if m.num_segment_edges >= 2 {
        let edge_idx = m.segment_edge_indices[1];
        let count = recorder.replay_edge_segment_index_count(edge_idx);
        if count > 0 {
            if segment_indices2.try_reserve_exact(count).is_err() {
                return Err(AutohintError::OutOfMemory);
            }
            segment_indices2.resize(count, 0);
            if recorder.dump_replay_edge_segment_indices(edge_idx, &mut segment_indices2) != count {
                return Err(AutohintError::NullPointer);
            }
        }
    }

    let wraps_len = core::cmp::min(
        recorder.num_wrap_around_segments as usize,
        recorder.wrap_around_segments.len(),
    );
    let wraps = &recorder.wrap_around_segments[..wraps_len];

    let num_segments = recorder.get_segment_map_entry(recorder.replay_axis_num_segments);

    let emitted = emit_marshaled_action_bytes(
        action,
        m.edge1_first_idx,
        m.edge2_first_idx,
        m.edge3_first_idx,
        m.lower_bound_first_idx,
        m.upper_bound_first_idx,
        m.primary_is_round,
        m.secondary_is_serif,
        m.cvt_idx,
        top_to_bottom_hinting,
        &segment_indices1,
        &segment_indices2,
        wraps,
        num_segments,
    )?;

    if emitted.is_empty() {
        return Ok(Bytecode::new());
    }

    Ok(Bytecode(emitted))
}

fn recorder_replay_process_hint_record(
    recorder: &mut RustRecorder,
    rec: &ExportedHintRecord,
    glyph_num_points: u32,
    cvt_blue_refs_offset: u16,
    cvt_blue_shoots_offset: u16,
    top_to_bottom_hinting: bool,
) -> Result<ReplayProcessResult, AutohintError> {
    const TA_DIMENSION_VERT: u8 = 1;
    const TA_IP_BEFORE: u8 = 0;
    const TA_IP_AFTER: u8 = 1;
    const TA_IP_ON: u8 = 2;
    const TA_IP_BETWEEN: u8 = 3;
    const TA_BLUE: u8 = 4;
    const TA_ANCHOR: u8 = 6;
    const TA_ADJUST: u8 = 10;
    const TA_LINK: u8 = 22;
    const TA_STEM: u8 = 26;
    const TA_SERIF: u8 = 38;
    const TA_SERIF_ANCHOR: u8 = 45;
    const TA_SERIF_LINK1: u8 = 52;
    const TA_SERIF_LINK2: u8 = 59;
    const TA_BOUND: u8 = 66;

    if rec.dim != TA_DIMENSION_VERT {
        return Ok(ReplayProcessResult {
            bytecode: Bytecode::new(),
            did_emit_action: false,
        });
    }

    let num_edges = recorder.get_replay_axis_num_edges();

    let point_valid = rec.point_ix != 0xFFFF && (rec.point_ix as u32) < glyph_num_points;
    let edge_valid = rec.edge_ix != 0xFFFF && rec.edge_ix < num_edges;
    let edge2_valid = rec.edge2_ix != 0xFFFF && rec.edge2_ix < num_edges;
    let edge3_valid = rec.edge3_ix != 0xFFFF && rec.edge3_ix < num_edges;
    let lower_bound_idx = if rec.lower_bound_ix < num_edges {
        rec.lower_bound_ix
    } else {
        0xFFFF
    };
    let upper_bound_idx = if rec.upper_bound_ix < num_edges {
        rec.upper_bound_ix
    } else {
        0xFFFF
    };

    match rec.action {
        TA_IP_BEFORE => {
            if point_valid {
                recorder.ip_before_points.insert(rec.point_ix);
            }
            return Ok(ReplayProcessResult {
                bytecode: Bytecode::new(),
                did_emit_action: false,
            });
        }
        TA_IP_AFTER => {
            if point_valid {
                recorder.ip_after_points.insert(rec.point_ix);
            }
            return Ok(ReplayProcessResult {
                bytecode: Bytecode::new(),
                did_emit_action: false,
            });
        }
        TA_IP_ON => {
            if point_valid && edge_valid {
                recorder.ip_on_points.insert(RecorderOnPoint {
                    edge: rec.edge_ix,
                    point: rec.point_ix,
                });
            }
            return Ok(ReplayProcessResult {
                bytecode: Bytecode::new(),
                did_emit_action: false,
            });
        }
        TA_IP_BETWEEN => {
            if point_valid && edge_valid && edge2_valid {
                recorder.ip_between_points.insert(RecorderBetweenPoint {
                    before_edge: rec.edge_ix,
                    after_edge: rec.edge2_ix,
                    point: rec.point_ix,
                });
            }
            return Ok(ReplayProcessResult {
                bytecode: Bytecode::new(),
                did_emit_action: false,
            });
        }
        TA_BLUE => {
            if !edge_valid || !edge2_valid {
                return Ok(ReplayProcessResult {
                    bytecode: Bytecode::new(),
                    did_emit_action: false,
                });
            }
        }
        TA_ANCHOR | TA_ADJUST | TA_LINK | TA_STEM => {
            if !edge_valid || !edge2_valid {
                return Ok(ReplayProcessResult {
                    bytecode: Bytecode::new(),
                    did_emit_action: false,
                });
            }
        }
        TA_SERIF | TA_SERIF_ANCHOR | TA_SERIF_LINK2 => {
            if !edge_valid {
                return Ok(ReplayProcessResult {
                    bytecode: Bytecode::new(),
                    did_emit_action: false,
                });
            }
        }
        TA_SERIF_LINK1 => {
            if !edge_valid || !edge2_valid || !edge3_valid {
                return Ok(ReplayProcessResult {
                    bytecode: Bytecode::new(),
                    did_emit_action: false,
                });
            }
        }
        TA_BOUND => {
            return Ok(ReplayProcessResult {
                bytecode: Bytecode::new(),
                did_emit_action: false,
            });
        }
        _ => {
            return Ok(ReplayProcessResult {
                bytecode: Bytecode::new(),
                did_emit_action: false,
            });
        }
    }

    let emitted = hints_recorder_marshal_and_emit_action(
        recorder,
        rec.action as u32,
        rec.edge_ix,
        rec.edge2_ix,
        rec.edge3_ix,
        lower_bound_idx,
        upper_bound_idx,
        cvt_blue_refs_offset,
        cvt_blue_shoots_offset,
        top_to_bottom_hinting,
    )?;

    Ok(ReplayProcessResult {
        bytecode: emitted,
        did_emit_action: true,
    })
}

fn recorder_record_hints_for_ppem(
    recorder: &mut RustRecorder,
    font: &Font,
    glyph_idx: GlyphId,
    glyph_num_points: u32,
    ppem: u16,
    ta_style: StyleIndex,
    is_non_base: bool,
    is_digit: bool,
) -> Result<(), AutohintError> {
    // Reset the hints-record accumulator for this ppem
    {
        recorder.hints_record_buffer = Bytecode::new();
        recorder.hints_record_size = ppem;
        recorder.hints_record_num_actions = 0;
    }

    let rust_plan = crate::glyf::compute_hint_plan(
        font,
        glyph_idx,
        ta_style.as_usize(),
        is_non_base as u8,
        is_digit as u8,
        ppem,
    )?;

    if !recorder_build_replay_axis_from_plan(recorder, &rust_plan) {
        return Err(AutohintError::OutOfMemory);
    }

    let Some((cvt_blue_refs_offset, cvt_blue_shoots_offset)) =
        compute_cvt_blue_offsets(font, ta_style)
    else {
        return Err(AutohintError::NullPointer);
    };

    let top_to_bottom_hinting =
        crate::style_metadata::script_hints_top_to_bottom(ta_style.as_usize());

    for rec in &rust_plan.records {
        let result = recorder_replay_process_hint_record(
            recorder,
            rec,
            glyph_num_points,
            cvt_blue_refs_offset,
            cvt_blue_shoots_offset,
            top_to_bottom_hinting,
        )?;

        if !result.bytecode.is_empty() {
            recorder.hints_record_buffer.extend(result.bytecode);
        }

        if result.did_emit_action {
            recorder.hints_record_num_actions =
                match recorder.hints_record_num_actions.checked_add(1) {
                    Some(v) => v,
                    None => {
                        return Err(AutohintError::NullPointer);
                    }
                };
        }
    }

    Ok(())
}

fn recorder_build_replay_axis_from_plan(
    recorder: &mut RustRecorder,
    plan: &ExportedHintPlan,
) -> bool {
    let segments_src = &plan.segments;
    let edges_src = &plan.edges;
    let num_segments = segments_src.len();
    let num_edges = edges_src.len();

    let mut replay_first = Vec::new();
    let mut replay_last = Vec::new();
    if replay_first.try_reserve_exact(num_segments).is_err()
        || replay_last.try_reserve_exact(num_segments).is_err()
    {
        return false;
    }

    recorder.replay_segment_edge_raw.clear();
    if recorder
        .replay_segment_edge_raw
        .try_reserve_exact(num_segments)
        .is_err()
    {
        return false;
    }

    for src in segments_src.iter() {
        replay_first.push(src.first_ix);
        replay_last.push(src.last_ix);
        let edge_ix = if src.edge_ix != 0xFFFF && (src.edge_ix as usize) < num_edges {
            src.edge_ix
        } else {
            0xFFFF
        };
        recorder.replay_segment_edge_raw.push(edge_ix);
    }

    if !recorder.set_replay_indices(&replay_first, &replay_last) {
        return false;
    }

    recorder.replay_edge_first_raw.clear();
    recorder.replay_edge_serif_raw.clear();
    recorder.replay_edge_flags.clear();
    recorder.replay_edge_best_blue_idx.clear();
    recorder.replay_edge_best_blue_is_shoot.clear();

    if recorder
        .replay_edge_first_raw
        .try_reserve_exact(num_edges)
        .is_err()
        || recorder
            .replay_edge_serif_raw
            .try_reserve_exact(num_edges)
            .is_err()
        || recorder
            .replay_edge_flags
            .try_reserve_exact(num_edges)
            .is_err()
        || recorder
            .replay_edge_best_blue_idx
            .try_reserve_exact(num_edges)
            .is_err()
        || recorder
            .replay_edge_best_blue_is_shoot
            .try_reserve_exact(num_edges)
            .is_err()
    {
        return false;
    }

    for src in edges_src.iter() {
        let first_ix = if src.first_ix != 0xFFFF && (src.first_ix as usize) < num_segments {
            src.first_ix
        } else {
            0xFFFF
        };
        let serif_ix = if src.serif_ix != 0xFFFF && (src.serif_ix as usize) < num_edges {
            src.serif_ix
        } else {
            0xFFFF
        };
        let best_blue_idx = if src.blue_ix != 0xFFFF {
            src.blue_ix
        } else {
            0
        };

        recorder.replay_edge_first_raw.push(first_ix);
        recorder.replay_edge_serif_raw.push(serif_ix);
        recorder.replay_edge_flags.push(src.flags);
        recorder.replay_edge_best_blue_idx.push(best_blue_idx);
        recorder
            .replay_edge_best_blue_is_shoot
            .push(src.blue_is_shoot != 0);
    }

    {
        let mut offsets = Vec::with_capacity(num_edges + 1);
        let mut indices = Vec::with_capacity(num_segments);

        offsets.push(0);
        for edge in edges_src {
            if edge.first_ix == 0xFFFF || (edge.first_ix as usize) >= num_segments {
                offsets.push(indices.len() as u32);
                continue;
            }

            let start = edge.first_ix;
            let mut cur = start;
            let mut guard = 0usize;

            loop {
                indices.push(cur);
                guard += 1;

                if guard > num_segments {
                    break;
                }

                let next = segments_src[cur as usize].edge_next_ix;
                if next == 0xFFFF || (next as usize) >= num_segments {
                    break;
                }
                cur = next;
                if cur == start {
                    break;
                }
            }

            offsets.push(indices.len() as u32);
        }

        recorder.replay_edge_segment_offsets = offsets;
        recorder.replay_edge_segment_indices = indices;
    }

    recorder.replay_axis_num_segments = num_segments as u16;
    recorder.replay_axis_max_segments = u16::try_from(num_segments).unwrap_or(u16::MAX);
    recorder.replay_axis_num_edges = num_edges as u16;
    recorder.replay_axis_max_edges = u16::try_from(num_edges).unwrap_or(u16::MAX);
    recorder.replay_axis_major_dir = TA_DIR_NONE;
    recorder.set_use_replay_axis(true);

    if !recorder.initialize_replay_segment_metadata() {
        return false;
    }

    true
}

struct HintsRecordEntry {
    size: u32,
    num_actions: u32,
    buf: Vec<u8>,
}

pub struct HintsRecordArray {
    records: Vec<HintsRecordEntry>,
}

impl HintsRecordArray {
    fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    fn is_different(&self, buf: &[u8]) -> bool {
        match self.records.last() {
            None => true,
            Some(last) => last.buf.as_slice() != buf,
        }
    }

    fn push(&mut self, size: u32, num_actions: u32, buf: &[u8]) {
        self.records.push(HintsRecordEntry {
            size,
            num_actions,
            buf: buf.to_vec(),
        });
    }

    fn len(&self) -> usize {
        self.records.len()
    }

    /// True when there is exactly one record and it has no hint actions
    /// (i.e. the glyph can be handled by the scaler alone).
    fn is_empty_singleton(&self) -> bool {
        self.records.len() == 1 && self.records[0].num_actions == 0
    }

    fn emit(&self, optimize: bool) -> Result<(Bytecode, u16), ()> {
        let mut out = Bytecode::new();
        let mut max_stack_elements = 0u16;

        if self.records.is_empty() {
            return Ok((out, 0));
        }

        for i in 0..(self.records.len() - 1) {
            let curr = &self.records[i];
            let next = &self.records[i + 1];

            out.push_u8(MPPEM);
            if next.size > 0xFF {
                out.push_u8(PUSHW_1);

                out.push_word(next.size);
            } else {
                out.push_u8(PUSHB_1);
                out.push_u8(next.size as u8);
            }
            out.push_u8(LT);
            out.push_u8(IF);

            let n = emit_hints_record_into(&mut out, curr.buf.as_slice(), optimize)?;
            if n > max_stack_elements {
                max_stack_elements = n;
            }

            out.push_u8(ELSE);
        }

        let last = &self.records[self.records.len() - 1];
        let n = emit_hints_record_into(&mut out, last.buf.as_slice(), optimize)?;
        if n > max_stack_elements {
            max_stack_elements = n;
        }

        out.extend(std::iter::repeat_n(EIF, self.records.len() - 1));

        Ok((out, max_stack_elements))
    }
}

fn emit_hints_record_into(out: &mut Bytecode, words_be: &[u8], optimize: bool) -> Result<u16, ()> {
    if !words_be.len().is_multiple_of(2) {
        return Err(());
    }

    let num_arguments = words_be.len() / 2;
    let mut need_words = false;
    for i in (0..words_be.len()).step_by(2) {
        if words_be[i] != 0 {
            need_words = true;
            break;
        }
    }

    let mut i = 0usize;
    while i < num_arguments {
        let num_args = (num_arguments - i).min(255);
        if need_words {
            if optimize && num_args <= 8 {
                out.push_u8(PUSHW_1 - 1 + num_args as u8);
            } else {
                out.push_u8(NPUSHW);
                out.push_u8(num_args as u8);
            }

            for j in 0..num_args {
                let src_word_idx = num_arguments - 1 - (i + j);
                let byte_ix = src_word_idx * 2;
                out.push_u8(words_be[byte_ix]);
                out.push_u8(words_be[byte_ix + 1]);
            }
        } else {
            if optimize && num_args <= 8 {
                out.push_u8(PUSHB_1 - 1 + num_args as u8);
            } else {
                out.push_u8(NPUSHB);
                out.push_u8(num_args as u8);
            }

            for j in 0..num_args {
                let src_word_idx = num_arguments - 1 - (i + j);
                let byte_ix = src_word_idx * 2;
                out.push_u8(words_be[byte_ix + 1]);
            }
        }

        i += 255;
    }

    Ok(u16::try_from(num_arguments).unwrap_or(u16::MAX))
}

pub(crate) fn build_glyph_instructions(font: &mut Font, idx: GlyphId) -> Result<(), AutohintError> {
    let font_ref = font;

    // Bounds check and get sfnt/GlyfData pointers.
    if font_ref.glyf_data.is_none() {
        return Err(AutohintError::NullPointer);
    }

    let glyf_num_glyphs = font_ref
        .glyf_data
        .as_ref()
        .map(|g| g.num_glyphs)
        .ok_or(AutohintError::NullPointer)?;

    let mut glyph_ref: ScaledGlyph = get_glyph(font_ref, idx)
        .ok_or(AutohintError::NullPointer)?
        .clone();

    let sfnt_ref = &font_ref.sfnt;
    let idx_usize = idx.to_u32() as usize;
    if idx_usize >= sfnt_ref.glyph_styles.len() || idx.to_u32() >= glyf_num_glyphs as u32 {
        return Err(AutohintError::NullPointer);
    }

    let gstyle = sfnt_ref.glyph_styles[idx_usize];
    let fallback_style =
        crate::orchestrate::fallback_style_for_script(font_ref.args.fallback_script) as usize;
    let mut sfnt_max_storage = sfnt_ref.max_storage;
    let mut sfnt_max_stack_elements = sfnt_ref.max_stack_elements;
    let mut sfnt_max_twilight_points = sfnt_ref.max_twilight_points;
    let mut sfnt_max_instructions = sfnt_ref.max_instructions;

    if font_ref.args.debug {
        log_debug_heading(&format!("glyph {}", idx), '=');
    }

    let ta_style = StyleIndex::new(gstyle.style_index as usize)?;
    let mut use_gstyle_data = true;

    let mut is_composite_glyph = glyph_ref.num_components() != 0;
    let mut is_empty_glyph = !is_composite_glyph && glyph_ref.num_contours() == 0;
    let mut glyph_num_points = glyph_ref.num_points() as u32;

    if let Ok(info) = crate::loader::load_glyph_info(font_ref, idx) {
        is_composite_glyph = info.kind == 2;
        is_empty_glyph = info.kind == 0;
        glyph_num_points = info.num_points as u32;
    }

    if is_empty_glyph {
        return Ok(());
    }

    let mut bytecode = Bytecode::new();

    if is_composite_glyph {
        let subglyph = match build_subglyph_shifter_bytecode(font_ref, idx) {
            Ok(v) => v,
            Err(_) => return Err(AutohintError::LoaderInvalidArgument),
        };
        bytecode.extend(subglyph);
        use_gstyle_data = false;
    } else if font_ref.args.fallback_scaling {
        if ta_style.as_usize() == fallback_style {
            let recorder = RustRecorder::new(&glyph_ref);
            let (emitted, num_args) =
                build_glyph_scaler_bytecode(&recorder, font_ref, idx, font_ref.args.composites)?;
            bytecode.extend(emitted);

            let num_storage = StorageAreaLocations::sal_segment_offset as u16;
            if num_storage > sfnt_max_storage {
                sfnt_max_storage = num_storage;
            }

            let num_stack_elements = ADDITIONAL_STACK_ELEMENTS.saturating_add(num_args as u16);
            if num_stack_elements > sfnt_max_stack_elements {
                sfnt_max_stack_elements = num_stack_elements;
            }

            use_gstyle_data = false;
        } else {
            let mut recorder = RustRecorder::new(&glyph_ref);
            let mut action_hints_records = HintsRecordArray::new();
            let mut point_hints_records = HintsRecordArray::new();

            for size in font_ref.args.hinting_range_min..=font_ref.args.hinting_range_max {
                recorder.clear();

                if font_ref.args.debug {
                    log_debug_heading(&format!("size {}", size), '-');
                }

                recorder_record_hints_for_ppem(
                    &mut recorder,
                    font_ref,
                    idx,
                    glyph_num_points,
                    size as u16,
                    ta_style,
                    gstyle.is_non_base,
                    gstyle.is_digit,
                )?;

                if action_hints_records.is_different(recorder.hints_record_buffer.as_slice()) {
                    action_hints_records.push(
                        recorder.hints_record_size as u32,
                        recorder.hints_record_num_actions,
                        recorder.hints_record_buffer.as_slice(),
                    )
                }

                recorder_reset_hints_record(&mut recorder);
                recorder_build_point_hints(&mut recorder, font_ref.args.composites)?;

                if point_hints_records.is_different(recorder.hints_record_buffer.as_slice()) {
                    point_hints_records.push(
                        recorder.hints_record_size as u32,
                        recorder.hints_record_num_actions,
                        recorder.hints_record_buffer.as_slice(),
                    )
                }
            }

            if action_hints_records.is_empty_singleton() {
                let (emitted, num_args) = build_glyph_scaler_bytecode(
                    &recorder,
                    font_ref,
                    idx,
                    font_ref.args.composites,
                )?;
                bytecode.extend(emitted);

                let num_storage = StorageAreaLocations::sal_segment_offset as u16;
                if num_storage > sfnt_max_storage {
                    sfnt_max_storage = num_storage;
                }

                let num_stack_elements = ADDITIONAL_STACK_ELEMENTS.saturating_add(num_args as u16);
                if num_stack_elements > sfnt_max_stack_elements {
                    sfnt_max_stack_elements = num_stack_elements;
                }

                use_gstyle_data = false;
            } else {
                let optimize = action_hints_records.len() > 1;

                let pos0 = bytecode.as_slice().len();
                let (point_bytes, point_stack) = match point_hints_records.emit(optimize) {
                    Ok(v) => v,
                    Err(_) => return Err(AutohintError::NullPointer),
                };
                bytecode.extend_bytes(point_bytes.as_slice());
                if point_stack > recorder.num_stack_elements {
                    recorder.num_stack_elements = point_stack;
                }

                let saved_stack = recorder.num_stack_elements;
                recorder.num_stack_elements = 0;

                let pos1 = bytecode.as_slice().len();
                let (action_bytes, action_stack) = match action_hints_records.emit(optimize) {
                    Ok(v) => v,
                    Err(_) => return Err(AutohintError::NullPointer),
                };
                bytecode.extend_bytes(action_bytes.as_slice());
                if action_stack > recorder.num_stack_elements {
                    recorder.num_stack_elements = action_stack;
                }
                recorder.num_stack_elements =
                    recorder.num_stack_elements.saturating_add(saved_stack);

                let mut first_indices = Vec::<u32>::new();
                let mut last_indices = Vec::<u32>::new();
                let num_active_segments = recorder.num_active_segments() as usize;
                if num_active_segments > 0 {
                    if first_indices
                        .try_reserve_exact(num_active_segments)
                        .is_err()
                        || last_indices.try_reserve_exact(num_active_segments).is_err()
                    {
                        return Err(AutohintError::OutOfMemory);
                    }
                    first_indices.resize(num_active_segments, 0);
                    last_indices.resize(num_active_segments, 0);

                    if !recorder
                        .fill_active_segment_point_indices(&mut first_indices, &mut last_indices)
                    {
                        return Err(AutohintError::NullPointer);
                    }
                }

                let style_id = font_ref
                    .glyf_data
                    .as_ref()
                    .and_then(|g| g.style_offsets.get(&ta_style))
                    .map(|d| d.slot)
                    .unwrap_or(0xFFFF);
                let pos2 = bytecode.as_slice().len();
                let segment_result = build_glyph_segments_bytecode(
                    &recorder,
                    font_ref,
                    idx,
                    font_ref.args.composites,
                    style_id,
                    &first_indices,
                    &last_indices,
                    recorder.num_wrap_around_segments(),
                    optimize,
                )?;
                bytecode.extend(segment_result.bytecode);

                let num_storage = (StorageAreaLocations::sal_segment_offset as u16)
                    .saturating_add(segment_result.num_segments.saturating_mul(3));
                if num_storage > sfnt_max_storage {
                    sfnt_max_storage = num_storage;
                }

                let num_twilight_points = segment_result.num_segments.saturating_mul(2);
                if num_twilight_points > sfnt_max_twilight_points {
                    sfnt_max_twilight_points = num_twilight_points;
                }

                let num_stack_elements = ADDITIONAL_STACK_ELEMENTS
                    .saturating_add(recorder.num_stack_elements)
                    .saturating_add(segment_result.num_args);
                if num_stack_elements > sfnt_max_stack_elements {
                    sfnt_max_stack_elements = num_stack_elements;
                }

                if action_hints_records.len() == 1 && !bytecode.optimize_push([pos0, pos1, pos2]) {
                    bytecode.truncate(pos0);
                }
            }
        }
    } else {
        let mut recorder = RustRecorder::new(&glyph_ref);
        let mut action_hints_records = HintsRecordArray::new();
        let mut point_hints_records = HintsRecordArray::new();

        for size in font_ref.args.hinting_range_min..=font_ref.args.hinting_range_max {
            recorder.clear();

            if font_ref.args.debug {
                log_debug_heading(&format!("size {}", size), '-');
            }

            recorder_record_hints_for_ppem(
                &mut recorder,
                font_ref,
                idx,
                glyph_num_points,
                size as u16,
                ta_style,
                gstyle.is_non_base,
                gstyle.is_digit,
            )?;

            if action_hints_records.is_different(recorder.hints_record_buffer.as_slice()) {
                action_hints_records.push(
                    recorder.hints_record_size as u32,
                    recorder.hints_record_num_actions,
                    recorder.hints_record_buffer.as_slice(),
                )
            }

            recorder_reset_hints_record(&mut recorder);
            recorder_build_point_hints(&mut recorder, font_ref.args.composites)?;

            if point_hints_records.is_different(recorder.hints_record_buffer.as_slice()) {
                point_hints_records.push(
                    recorder.hints_record_size as u32,
                    recorder.hints_record_num_actions,
                    recorder.hints_record_buffer.as_slice(),
                )
            }
        }

        if action_hints_records.is_empty_singleton() {
            let (emitted, num_args) =
                build_glyph_scaler_bytecode(&recorder, font_ref, idx, font_ref.args.composites)?;
            bytecode.extend(emitted);

            let num_storage = StorageAreaLocations::sal_segment_offset as u16;
            if num_storage > sfnt_max_storage {
                sfnt_max_storage = num_storage;
            }

            let num_stack_elements = ADDITIONAL_STACK_ELEMENTS.saturating_add(num_args as u16);
            if num_stack_elements > sfnt_max_stack_elements {
                sfnt_max_stack_elements = num_stack_elements;
            }

            use_gstyle_data = false;
        } else {
            let optimize = action_hints_records.len() > 1;

            let pos0 = bytecode.as_slice().len();
            let (point_bytes, point_stack) = match point_hints_records.emit(optimize) {
                Ok(v) => v,
                Err(_) => return Err(AutohintError::NullPointer),
            };
            bytecode.extend_bytes(point_bytes.as_slice());
            if point_stack > recorder.num_stack_elements {
                recorder.num_stack_elements = point_stack;
            }

            let saved_stack = recorder.num_stack_elements;
            recorder.num_stack_elements = 0;

            let pos1 = bytecode.as_slice().len();
            let (action_bytes, action_stack) = match action_hints_records.emit(optimize) {
                Ok(v) => v,
                Err(_) => return Err(AutohintError::NullPointer),
            };
            bytecode.extend_bytes(action_bytes.as_slice());
            if action_stack > recorder.num_stack_elements {
                recorder.num_stack_elements = action_stack;
            }
            recorder.num_stack_elements = recorder.num_stack_elements.saturating_add(saved_stack);

            let mut first_indices = Vec::<u32>::new();
            let mut last_indices = Vec::<u32>::new();
            let num_active_segments = recorder.num_active_segments() as usize;
            if num_active_segments > 0 {
                if first_indices
                    .try_reserve_exact(num_active_segments)
                    .is_err()
                    || last_indices.try_reserve_exact(num_active_segments).is_err()
                {
                    return Err(AutohintError::OutOfMemory);
                }
                first_indices.resize(num_active_segments, 0);
                last_indices.resize(num_active_segments, 0);

                if !recorder
                    .fill_active_segment_point_indices(&mut first_indices, &mut last_indices)
                {
                    return Err(AutohintError::NullPointer);
                }
            }

            let style_id = font_ref
                .glyf_data
                .as_ref()
                .and_then(|g| g.style_offsets.get(&ta_style))
                .map(|d| d.slot)
                .unwrap_or(0xFFFF);
            let pos2 = bytecode.as_slice().len();
            let segment_result = build_glyph_segments_bytecode(
                &recorder,
                font_ref,
                idx,
                font_ref.args.composites,
                style_id,
                &first_indices,
                &last_indices,
                recorder.num_wrap_around_segments(),
                optimize,
            )?;
            bytecode.extend(segment_result.bytecode);

            let num_storage = (StorageAreaLocations::sal_segment_offset as u16)
                .saturating_add(segment_result.num_segments.saturating_mul(3));
            if num_storage > sfnt_max_storage {
                sfnt_max_storage = num_storage;
            }

            let num_twilight_points = segment_result.num_segments.saturating_mul(2);
            if num_twilight_points > sfnt_max_twilight_points {
                sfnt_max_twilight_points = num_twilight_points;
            }

            let num_stack_elements = ADDITIONAL_STACK_ELEMENTS
                .saturating_add(recorder.num_stack_elements)
                .saturating_add(segment_result.num_args);
            if num_stack_elements > sfnt_max_stack_elements {
                sfnt_max_stack_elements = num_stack_elements;
            }

            if action_hints_records.len() == 1 && !bytecode.optimize_push([pos0, pos1, pos2]) {
                bytecode.truncate(pos0);
            }
        }
    }

    if font_ref.control.has_index() {
        let (emitted, max_stack) =
            crate::c_api::build_delta_exceptions(font_ref.control.index(), idx, &mut glyph_ref);
        if max_stack > sfnt_max_stack_elements.into() {
            sfnt_max_stack_elements = max_stack as u16;
        }
        bytecode.extend(emitted);
    }

    if use_gstyle_data && gstyle.is_non_base {
        glyph_ref.append_ignore_std_width();
        bytecode.extend_bytes(&[PUSHB_2, CvtLocations::cvtl_ignore_std_width as u8, 0, WCVTP]);
    }

    if bytecode.as_slice().len() > u16::MAX as usize {
        return Err(AutohintError::NullPointer);
    }
    let ins_len = bytecode.as_slice().len() as u16;
    if ins_len > sfnt_max_instructions {
        sfnt_max_instructions = ins_len;
    }

    {
        let sfnt_mut = &mut font_ref.sfnt;
        sfnt_mut.max_storage = sfnt_max_storage;
        sfnt_mut.max_stack_elements = sfnt_max_stack_elements;
        sfnt_mut.max_twilight_points = sfnt_max_twilight_points;
        sfnt_mut.max_instructions = sfnt_max_instructions;
    }

    glyph_ref.set_instructions(bytecode.as_slice());
    // Put the glyph back
    put_glyph(font_ref, idx, glyph_ref);
    Ok(())
}

/// Build scaler bytecode for one glyph.
///
/// Returns 0 on success, 0x23 (FT_Err_Invalid_Table) on invalid input/data,
/// 0x50 (FT_Err_Out_Of_Memory) on allocation failure.
fn build_glyph_scaler_bytecode(
    recorder: &RustRecorder,
    font: &Font,
    glyph_id: GlyphId,
    hint_composites: bool,
) -> Result<(Bytecode, usize), AutohintError> {
    let outline = match extract_unscaled_outline(font, glyph_id) {
        Ok(v) => v,
        Err(_) => return Err(AutohintError::NullPointer),
    };

    let (points, contours) = match outline {
        Some((points, _tags, contours)) => (points, contours),
        None => (Vec::new(), Vec::new()),
    };

    let num_contours = contours.len();
    let num_args = 2usize.saturating_mul(num_contours).saturating_add(2usize);

    let mut args = Vec::<u32>::new();
    if args.try_reserve_exact(num_args).is_err() {
        return Err(AutohintError::OutOfMemory);
    }
    args.resize(num_args, 0);

    let mut write_at = num_args;
    let mut push_rev = |value: u32| {
        write_at -= 1;
        args[write_at] = value;
    };

    let has_components = recorder.glyph_num_components() > 0;
    let composite_scaler = hint_composites && has_components;

    if composite_scaler {
        push_rev(FunctionNumbers::bci_scale_composite_glyph as u8 as u32);
    } else {
        push_rev(FunctionNumbers::bci_scale_glyph as u8 as u32);
    }
    push_rev(num_contours as u32);

    let mut start: u32 = 0;
    let mut end: u32 = 0;

    for contour_end in contours.iter().copied() {
        end = contour_end as u32;
        if end < start || end as usize >= points.len() {
            return Err(AutohintError::NullPointer);
        }

        let mut min = start;
        let mut max = start;
        for q in start..=end {
            if points[q as usize].y < points[min as usize].y {
                min = q;
            }
            if points[q as usize].y > points[max as usize].y {
                max = q;
            }
        }

        let adjust = |idx: u32| -> u32 { recorder.adjust_point_index(idx, hint_composites) };

        if min > max {
            push_rev(adjust(max));
            push_rev(adjust(min));
        } else {
            push_rev(adjust(min));
            push_rev(adjust(max));
        }

        start = end + 1;
    }

    let mut need_words = num_args > 0xFF;
    if end > 0xFF {
        need_words = true;
    }

    let mut bytecode = Bytecode::new();
    if bytecode.push(&args, need_words, true).is_err() {
        return Err(AutohintError::NullPointer);
    }
    bytecode.push_u8(CALL);
    Ok((bytecode, num_args))
}

/// Build segments bytecode for one glyph.
///
/// `first_indices` and `last_indices` contain active segments in traversal
/// order (already filtered by segment map).
fn build_glyph_segments_bytecode(
    recorder: &RustRecorder,
    font: &Font,
    glyph_id: GlyphId,
    hint_composites: bool,
    style_id: u32,
    first_indices: &[u32],
    last_indices: &[u32],
    num_wrap_around_segments: u16,
    optimize: bool,
) -> Result<GlyphSegmentsBytecode, AutohintError> {
    if first_indices.len() != last_indices.len() {
        return Err(AutohintError::InvalidTable);
    }

    let segment_len = first_indices.len();
    let first = first_indices;
    let last = last_indices;

    let outline = match extract_unscaled_outline(font, glyph_id) {
        Ok(v) => v,
        Err(_) => return Err(AutohintError::InvalidTable),
    };

    let contours = match outline {
        Some((_points, _tags, contours)) => contours,
        None => Vec::new(),
    };

    let adjust = |idx: u32| -> u32 { recorder.adjust_point_index(idx, hint_composites) };

    let mut base = 0u32;
    let mut num_packed_segments = 0usize;
    for i in 0..segment_len {
        let first_idx = adjust(first[i]);
        let last_idx = adjust(last[i]);

        if first_idx < base || first_idx - base >= 16 {
            break;
        }
        if first_idx > last_idx || last_idx - first_idx >= 16 {
            break;
        }
        if num_packed_segments == 9 {
            break;
        }

        num_packed_segments += 1;
        base = last_idx;
    }

    let num_segments_usize = segment_len.saturating_add(num_wrap_around_segments as usize);
    if num_segments_usize > u16::MAX as usize {
        return Err(AutohintError::InvalidTable);
    }
    let num_segments = num_segments_usize as u16;

    let num_args = num_packed_segments
        .saturating_add(
            2usize.saturating_mul(num_segments_usize.saturating_sub(num_packed_segments)),
        )
        .saturating_add(2usize.saturating_mul(num_wrap_around_segments as usize))
        .saturating_add(3);
    if num_args > u16::MAX as usize {
        return Err(AutohintError::InvalidTable);
    }

    let mut args = Vec::<u32>::new();
    if args.try_reserve_exact(num_args).is_err() {
        return Err(AutohintError::OutOfMemory);
    }
    args.resize(num_args, 0);

    let mut write_at = num_args;
    let mut push_rev = |value: u32| {
        write_at -= 1;
        args[write_at] = value;
    };

    let mut need_words = num_segments > 0xFF;

    let has_components = recorder.glyph_num_components() > 0;
    let create_segments_fn = if hint_composites && has_components {
        FunctionNumbers::bci_create_segments_composite_0 as u8 as u32
    } else {
        FunctionNumbers::bci_create_segments_0 as u8 as u32
    };

    push_rev(create_segments_fn + num_packed_segments as u32);
    push_rev(CVT_SCALING_VALUE_OFFSET(style_id as u8) as u32);
    push_rev(num_segments as u32);

    base = 0;
    let mut n = 0usize;
    for i in 0..segment_len {
        if n >= num_packed_segments {
            break;
        }

        let first_idx = adjust(first[i]);
        let last_idx = adjust(last[i]);
        let low_nibble = first_idx - base;
        let high_nibble = last_idx - first_idx;
        push_rev(16 * high_nibble + low_nibble);
        base = last_idx;
        n += 1;
    }

    for i in n..segment_len {
        let first_idx = first[i];
        let last_idx = last[i];

        push_rev(adjust(first_idx));
        push_rev(adjust(last_idx));

        if first_idx > last_idx {
            let contour = contours
                .iter()
                .copied()
                .enumerate()
                .find(|(_, end)| first_idx <= *end as u32);
            let Some((contour_idx, end)) = contour else {
                return Err(AutohintError::InvalidTable);
            };

            push_rev(adjust(end as u32));
            if end > 0xFF {
                need_words = true;
            }

            let contour_start = if contour_idx == 0 {
                0
            } else {
                contours[contour_idx - 1] as u32 + 1
            };
            push_rev(adjust(contour_start));
        }

        if last_idx > 0xFF {
            need_words = true;
        }
    }

    for i in 0..segment_len {
        let first_idx = first[i];
        let last_idx = last[i];

        if first_idx > last_idx {
            let contour = contours
                .iter()
                .copied()
                .enumerate()
                .find(|(_, end)| first_idx <= *end as u32);
            let Some((contour_idx, _)) = contour else {
                return Err(AutohintError::InvalidTable);
            };

            let contour_start = if contour_idx == 0 {
                0
            } else {
                contours[contour_idx - 1] as u32 + 1
            };

            push_rev(adjust(contour_start));
            push_rev(adjust(last_idx));
        }
    }

    let mut bytecode = Bytecode::new();
    if bytecode.push(&args, need_words, optimize).is_err() {
        return Err(AutohintError::InvalidTable);
    }
    bytecode.push_u8(CALL);

    Ok(GlyphSegmentsBytecode {
        bytecode,
        num_segments,
        num_args: num_args.try_into().unwrap_or(u16::MAX),
    })
}

fn emit_action_header(
    action: u32,
    edge1_first_idx: u16,
    edge2_first_idx: u16,
    edge3_first_idx: u16,
    lower_bound_idx: u16,
    upper_bound_idx: u16,
    primary_is_round: bool,
    secondary_is_serif: bool,
    cvt_idx: u16,
    top_to_bottom_hinting: bool,
) -> Result<Vec<u8>, AutohintError> {
    // Must match the C TA_Action enum base-variant values (see tahints.h).
    const TA_BLUE: u32 = 4;
    const TA_BLUE_ANCHOR: u32 = 5;
    const TA_ANCHOR: u32 = 6;
    const TA_ADJUST: u32 = 10;
    const TA_LINK: u32 = 22;
    const TA_STEM: u32 = 26;
    const TA_SERIF: u32 = 38;
    const TA_SERIF_ANCHOR: u32 = 45;
    const TA_SERIF_LINK1: u32 = 52;
    const TA_SERIF_LINK2: u32 = 59;

    const ACTION_OFFSET: u8 = FunctionNumbers::bci_action_ip_before as u8;

    fn push_u16(buf: &mut Vec<u8>, value: u16) {
        buf.push((value >> 8) as u8);
        buf.push((value & 0xFF) as u8);
    }

    let mut buf = Vec::<u8>::new();

    buf.push(0);

    match action {
        TA_LINK => {
            let action_byte = (TA_LINK as u8)
                + ACTION_OFFSET
                + (secondary_is_serif as u8)
                + 2 * (primary_is_round as u8);
            buf.push(action_byte);
            push_u16(&mut buf, edge1_first_idx);
            push_u16(&mut buf, edge2_first_idx);
        }

        TA_ANCHOR => {
            let action_byte = (TA_ANCHOR as u8)
                + ACTION_OFFSET
                + (secondary_is_serif as u8)
                + 2 * (primary_is_round as u8);
            buf.push(action_byte);
            push_u16(&mut buf, edge1_first_idx);
            push_u16(&mut buf, edge2_first_idx);
        }

        TA_ADJUST => {
            let has_bound = (edge3_first_idx != 0xFFFF) as u8;
            let bound_and_down = 4 * has_bound + 4 * (has_bound * (top_to_bottom_hinting as u8));
            let action_byte = (TA_ADJUST as u8)
                + ACTION_OFFSET
                + (secondary_is_serif as u8)
                + 2 * (primary_is_round as u8)
                + bound_and_down;
            buf.push(action_byte);
            push_u16(&mut buf, edge1_first_idx);
            push_u16(&mut buf, edge2_first_idx);
            if has_bound != 0 {
                push_u16(&mut buf, edge3_first_idx);
            }
        }

        TA_BLUE_ANCHOR => {
            buf.push((TA_BLUE_ANCHOR as u8) + ACTION_OFFSET);
            push_u16(&mut buf, edge2_first_idx);
            push_u16(&mut buf, cvt_idx);
            push_u16(&mut buf, edge1_first_idx);
        }

        TA_STEM => {
            let has_bound = (edge3_first_idx != 0xFFFF) as u8;
            let bound_and_down = 4 * has_bound + 4 * (has_bound * (top_to_bottom_hinting as u8));
            let action_byte = (TA_STEM as u8)
                + ACTION_OFFSET
                + (secondary_is_serif as u8)
                + 2 * (primary_is_round as u8)
                + bound_and_down;
            buf.push(action_byte);
            push_u16(&mut buf, edge1_first_idx);
            push_u16(&mut buf, edge2_first_idx);
            if has_bound != 0 {
                push_u16(&mut buf, edge3_first_idx);
            }
        }

        TA_BLUE => {
            buf.push((TA_BLUE as u8) + ACTION_OFFSET);
            push_u16(&mut buf, cvt_idx);
            push_u16(&mut buf, edge1_first_idx);
        }

        TA_SERIF => {
            let has_lower = (lower_bound_idx != 0xFFFF) as u8;
            let has_upper = (upper_bound_idx != 0xFFFF) as u8;
            let bound_and_down = has_lower
                + 2 * has_upper
                + 3 * ((has_lower | has_upper) * (top_to_bottom_hinting as u8));
            let action_byte = (TA_SERIF as u8) + ACTION_OFFSET + bound_and_down;
            buf.push(action_byte);
            push_u16(&mut buf, edge1_first_idx);
            push_u16(&mut buf, edge2_first_idx);
            if has_lower != 0 {
                push_u16(&mut buf, lower_bound_idx);
            }
            if has_upper != 0 {
                push_u16(&mut buf, upper_bound_idx);
            }
        }

        TA_SERIF_ANCHOR | TA_SERIF_LINK2 => {
            let has_lower = (lower_bound_idx != 0xFFFF) as u8;
            let has_upper = (upper_bound_idx != 0xFFFF) as u8;
            let bound_and_down = has_lower
                + 2 * has_upper
                + 3 * ((has_lower | has_upper) * (top_to_bottom_hinting as u8));
            let action_byte = (action as u8) + ACTION_OFFSET + bound_and_down;
            buf.push(action_byte);
            push_u16(&mut buf, edge1_first_idx);
            if has_lower != 0 {
                push_u16(&mut buf, lower_bound_idx);
            }
            if has_upper != 0 {
                push_u16(&mut buf, upper_bound_idx);
            }
        }

        TA_SERIF_LINK1 => {
            let has_lower = (lower_bound_idx != 0xFFFF) as u8;
            let has_upper = (upper_bound_idx != 0xFFFF) as u8;
            let bound_and_down = has_lower
                + 2 * has_upper
                + 3 * ((has_lower | has_upper) * (top_to_bottom_hinting as u8));
            let action_byte = (TA_SERIF_LINK1 as u8) + ACTION_OFFSET + bound_and_down;
            buf.push(action_byte);
            push_u16(&mut buf, edge1_first_idx);
            push_u16(&mut buf, edge2_first_idx);
            push_u16(&mut buf, edge3_first_idx);
            if has_lower != 0 {
                push_u16(&mut buf, lower_bound_idx);
            }
            if has_upper != 0 {
                push_u16(&mut buf, upper_bound_idx);
            }
        }

        _ => return Err(AutohintError::NullPointer),
    }

    Ok(buf)
}

fn emit_segments_payload(
    segment_indices: &[u16],
    wrap_around_segments: &[u16],
    num_segments: u16,
) -> Result<Vec<u8>, AutohintError> {
    fn push_u16(buf: &mut Vec<u8>, value: u16) {
        buf.push((value >> 8) as u8);
        buf.push((value & 0xFF) as u8);
    }

    if segment_indices.is_empty() {
        return Err(AutohintError::NullPointer);
    }

    let mut buf = Vec::<u8>::new();
    let first_seg = segment_indices[0];
    push_u16(&mut buf, first_seg);

    let mut num_segs = 0usize;
    if wrap_around_segments.contains(&first_seg) {
        num_segs += 1;
    }
    for &seg_idx in &segment_indices[1..] {
        num_segs += 1;
        if wrap_around_segments.contains(&seg_idx) {
            num_segs += 1;
        }
    }

    if num_segs > u16::MAX as usize {
        return Err(AutohintError::NullPointer);
    }
    push_u16(&mut buf, num_segs as u16);

    if let Some(wrap_pos) = wrap_around_segments
        .iter()
        .position(|&wrap| wrap == first_seg)
    {
        let synthetic = (num_segments as usize).saturating_add(wrap_pos);
        if synthetic > u16::MAX as usize {
            return Err(AutohintError::NullPointer);
        }
        push_u16(&mut buf, synthetic as u16);
    }

    for &seg_idx in &segment_indices[1..] {
        push_u16(&mut buf, seg_idx);

        if let Some(wrap_pos) = wrap_around_segments
            .iter()
            .position(|&wrap| wrap == seg_idx)
        {
            let synthetic = (num_segments as usize).saturating_add(wrap_pos);
            if synthetic > u16::MAX as usize {
                return Err(AutohintError::NullPointer);
            }
            push_u16(&mut buf, synthetic as u16);
        }
    }

    Ok(buf)
}

fn emit_marshaled_action_bytes(
    action: u32,
    edge1_first_idx: u16,
    edge2_first_idx: u16,
    edge3_first_idx: u16,
    lower_bound_idx: u16,
    upper_bound_idx: u16,
    primary_is_round: bool,
    secondary_is_serif: bool,
    cvt_idx: u16,
    top_to_bottom_hinting: bool,
    segment_indices1: &[u16],
    segment_indices2: &[u16],
    wrap_around_segments: &[u16],
    num_segments: u16,
) -> Result<Vec<u8>, AutohintError> {
    let header = emit_action_header(
        action,
        edge1_first_idx,
        edge2_first_idx,
        edge3_first_idx,
        lower_bound_idx,
        upper_bound_idx,
        primary_is_round,
        secondary_is_serif,
        cvt_idx,
        top_to_bottom_hinting,
    )?;

    let seg1 = if segment_indices1.is_empty() {
        Vec::new()
    } else {
        emit_segments_payload(segment_indices1, wrap_around_segments, num_segments)?
    };

    let seg2 = if segment_indices2.is_empty() {
        Vec::new()
    } else {
        emit_segments_payload(segment_indices2, wrap_around_segments, num_segments)?
    };

    let mut combined = Vec::with_capacity(header.len() + seg1.len() + seg2.len());
    combined.extend_from_slice(&header);
    combined.extend_from_slice(&seg1);
    combined.extend_from_slice(&seg2);
    Ok(combined)
}
