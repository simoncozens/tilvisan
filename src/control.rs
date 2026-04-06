use crate::{
    control_index::ResolvedControlEntry,
    intset::{BuildError as IntSetBuildError, IntSet, RangeExpr},
    AutohintError,
};
use core::ops::Range;
use logos::Logos;
use skrifa::{raw::tables::glyf::Glyph, GlyphId, GlyphNames};
use std::collections::HashMap;
use write_fonts::read::{FontRef, ReadError, TableProvider};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlyphRef {
    Index(u32),
    Name(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptSelector {
    Script(String),
    Any,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NumberSetElem {
    Unlimited,
    RightLimited(i32),
    LeftLimited(i32),
    Single(i32),
    Range(i32, i32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NumberSetAst {
    pub elems: Vec<NumberSetElem>,
}

impl NumberSetAst {
    pub fn parse(input: &str) -> Result<Self, AutohintError> {
        let mut elems = Vec::new();

        for part in input.split(',') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }

            elems.push(parse_number_set_elem_text(trimmed)?);
        }

        Ok(NumberSetAst { elems })
    }

    pub fn canonicalize(&self, min: i32, max: i32) -> Result<String, IntSetBuildError> {
        let exprs: Vec<RangeExpr> = self
            .elems
            .iter()
            .map(|elem| match elem {
                NumberSetElem::Unlimited => RangeExpr::Unlimited,
                NumberSetElem::RightLimited(v) => RangeExpr::RightLimited(*v),
                NumberSetElem::LeftLimited(v) => RangeExpr::LeftLimited(*v),
                NumberSetElem::Single(v) => RangeExpr::Single(*v),
                NumberSetElem::Range(a, b) => RangeExpr::Range(*a, *b),
            })
            .collect();

        let set = IntSet::from_exprs(&exprs, min, max)?;
        let mut out = String::new();

        for range in set.ranges() {
            if !out.is_empty() {
                out.push_str(", ");
            }

            if range.start == range.end {
                out.push_str(&range.start.to_string());
            } else if range.start <= min && range.end >= max {
                out.push('-');
            } else if range.start <= min {
                out.push_str(&format!("-{}", range.end));
            } else if range.end >= max {
                out.push_str(&format!("{}-", range.start));
            } else {
                out.push_str(&format!("{}-{}", range.start, range.end));
            }
        }

        Ok(out)
    }

    fn to_intset(&self, min: i32, max: i32) -> Result<IntSet, AutohintError> {
        let exprs: Vec<RangeExpr> = self
            .elems
            .iter()
            .map(|elem| match elem {
                NumberSetElem::Unlimited => RangeExpr::Unlimited,
                NumberSetElem::RightLimited(v) => RangeExpr::RightLimited(*v),
                NumberSetElem::LeftLimited(v) => RangeExpr::LeftLimited(*v),
                NumberSetElem::Single(v) => RangeExpr::Single(*v),
                NumberSetElem::Range(a, b) => RangeExpr::Range(*a, *b),
            })
            .collect();

        IntSet::from_exprs(&exprs, min, max)
            .map_err(|_| AutohintError::ValidationError("invalid number set".to_string()))
    }
}

fn parse_number_set_elem_text(raw: &str) -> Result<NumberSetElem, AutohintError> {
    let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return Err(AutohintError::ControlFileParseError {
            message: "empty number-set element".to_string(),
            line: 1,
            column: 1,
        });
    }

    if compact == "-" {
        return Ok(NumberSetElem::Unlimited);
    }

    if let Some(rest) = compact.strip_prefix('-') {
        let n = parse_non_negative_i32(rest)?;
        return Ok(NumberSetElem::RightLimited(n));
    }

    if let Some(rest) = compact.strip_suffix('-') {
        let n = parse_non_negative_i32(rest)?;
        return Ok(NumberSetElem::LeftLimited(n));
    }

    let dash_count = compact.bytes().filter(|&b| b == b'-').count();
    if dash_count == 0 {
        let n = parse_non_negative_i32(&compact)?;
        return Ok(NumberSetElem::Single(n));
    }
    if dash_count == 1 {
        let (a, b) = compact.split_once('-').expect("single dash checked");
        let start = parse_non_negative_i32(a)?;
        let end = parse_non_negative_i32(b)?;
        return Ok(NumberSetElem::Range(start, end));
    }

    Err(AutohintError::ControlFileParseError {
        message: format!("invalid range `{}`", raw),
        line: 1,
        column: 1,
    })
}

fn parse_non_negative_i32(s: &str) -> Result<i32, AutohintError> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AutohintError::ControlFileParseError {
            message: format!("invalid number `{}`", s),
            line: 1,
            column: 1,
        });
    }

    s.parse::<i32>()
        .map_err(|_| AutohintError::ControlFileParseError {
            message: format!("number out of range `{}`", s),
            line: 1,
            column: 1,
        })
}

#[derive(Debug, Clone, PartialEq)]
pub enum GlyphSetElem {
    Single(GlyphRef),
    Range(GlyphRef, GlyphRef),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PointMode {
    Touch,
    Point,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SegmentDirection {
    Left,
    Right,
    NoDir,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControlEntryAst {
    Delta {
        font_idx: i32,
        glyph: GlyphRef,
        mode: PointMode,
        points: NumberSetAst,
        x_shift: f64,
        y_shift: f64,
        ppems: NumberSetAst,
    },
    SegmentDirection {
        font_idx: i32,
        glyph: GlyphRef,
        dir: SegmentDirection,
        points: NumberSetAst,
        offsets: Option<(i32, i32)>,
    },
    StyleAdjust {
        font_idx: i32,
        script: String,
        feature: String,
        glyphs: Vec<GlyphSetElem>,
    },
    StemWidthAdjust {
        font_idx: i32,
        script: ScriptSelector,
        feature: String,
        widths: Vec<i32>,
    },
}

/* see the section `Managing exceptions' in chapter 6 */
/* (`The TrueType Instruction Set') of the OpenType reference */
/* how `delta_shift' works */

pub const CONTROL_DELTA_SHIFT: i32 = 3; /* 1/8px */
pub const CONTROL_DELTA_FACTOR: i32 = 1 << CONTROL_DELTA_SHIFT;
pub const CONTROL_DELTA_SHIFT_MAX: f64 = (1.0 / CONTROL_DELTA_FACTOR as f64) * 8.0;
pub const CONTROL_DELTA_SHIFT_MIN: f64 = -CONTROL_DELTA_SHIFT_MAX;
pub const CONTROL_DELTA_PPEM_MIN: i32 = 6;
pub const CONTROL_DELTA_PPEM_MAX: i32 = 53;
pub const CONTROL_WIDTH_MIN: i32 = 1;
pub const CONTROL_WIDTH_MAX: i32 = 65535;
pub const CONTROL_MAX_WIDTHS: usize = 16;

pub trait ControlSemanticProvider {
    fn num_fonts(&self) -> usize;
    fn num_glyphs(&self, font_idx: i32) -> Option<usize>;
    fn glyph_index_by_name(&self, font_idx: i32, glyph_name: &str) -> Option<GlyphId>;
    fn glyph_point_count(&self, font_idx: i32, glyph_idx: GlyphId) -> Option<usize>;

    fn is_valid_script(&self, _script: &str) -> bool {
        true
    }

    fn is_valid_feature(&self, feature: &str) -> bool {
        crate::features::is_known_feature(feature)
    }

    fn is_valid_style(&self, _script: &str, _feature: &str) -> bool {
        true
    }

    fn is_valid_wildcard_feature(&self, feature: &str) -> bool {
        self.is_valid_feature(feature)
    }
}

pub fn validate_control_entries<P: ControlSemanticProvider>(
    entries: &[ControlEntryAst],
    provider: &P,
) -> Result<(), AutohintError> {
    for (i, entry) in entries.iter().enumerate() {
        let entry_index = i + 1;

        let font_idx = match entry {
            ControlEntryAst::Delta { font_idx, .. }
            | ControlEntryAst::SegmentDirection { font_idx, .. }
            | ControlEntryAst::StyleAdjust { font_idx, .. }
            | ControlEntryAst::StemWidthAdjust { font_idx, .. } => *font_idx,
        };

        validate_font_idx(provider, font_idx, entry_index)?;

        match entry {
            ControlEntryAst::Delta {
                glyph,
                points,
                x_shift,
                y_shift,
                ppems,
                ..
            } => {
                let glyph_idx = resolve_glyph(provider, font_idx, glyph, entry_index)?;
                let point_count =
                    provider
                        .glyph_point_count(font_idx, glyph_idx)
                        .ok_or_else(|| {
                            semantic_error(
                                entry_index,
                                format!(
                                    "unable to get point count for glyph index {} in font {}",
                                    glyph_idx, font_idx
                                ),
                            )
                        })?;
                if point_count == 0 {
                    return Err(semantic_error(
                        entry_index,
                        format!("glyph index {} has no outline points", glyph_idx),
                    ));
                }

                if *x_shift < CONTROL_DELTA_SHIFT_MIN || *x_shift > CONTROL_DELTA_SHIFT_MAX {
                    return Err(semantic_error(
                        entry_index,
                        format!(
                            "x shift {} is out of range [{}, {}]",
                            x_shift, CONTROL_DELTA_SHIFT_MIN, CONTROL_DELTA_SHIFT_MAX
                        ),
                    ));
                }
                if *y_shift < CONTROL_DELTA_SHIFT_MIN || *y_shift > CONTROL_DELTA_SHIFT_MAX {
                    return Err(semantic_error(
                        entry_index,
                        format!(
                            "y shift {} is out of range [{}, {}]",
                            y_shift, CONTROL_DELTA_SHIFT_MIN, CONTROL_DELTA_SHIFT_MAX
                        ),
                    ));
                }

                validate_ordered_number_set(
                    points,
                    0,
                    point_count as i32 - 1,
                    entry_index,
                    "point set",
                )?;
                validate_ordered_number_set(
                    ppems,
                    CONTROL_DELTA_PPEM_MIN,
                    CONTROL_DELTA_PPEM_MAX,
                    entry_index,
                    "ppem set",
                )?;
            }
            ControlEntryAst::SegmentDirection {
                glyph,
                points,
                offsets,
                ..
            } => {
                let glyph_idx = resolve_glyph(provider, font_idx, glyph, entry_index)?;
                let point_count =
                    provider
                        .glyph_point_count(font_idx, glyph_idx)
                        .ok_or_else(|| {
                            semantic_error(
                                entry_index,
                                format!(
                                    "unable to get point count for glyph index {} in font {}",
                                    glyph_idx, font_idx
                                ),
                            )
                        })?;
                if point_count == 0 {
                    return Err(semantic_error(
                        entry_index,
                        format!("glyph index {} has no outline points", glyph_idx),
                    ));
                }

                validate_ordered_number_set(
                    points,
                    0,
                    point_count as i32 - 1,
                    entry_index,
                    "point set",
                )?;

                if let Some((left, right)) = offsets {
                    if !fits_i16(*left) || !fits_i16(*right) {
                        return Err(semantic_error(
                            entry_index,
                            format!("segment offsets ({}, {}) are out of i16 range", left, right),
                        ));
                    }
                }
            }
            ControlEntryAst::StyleAdjust {
                script,
                feature,
                glyphs,
                ..
            } => {
                if !provider.is_valid_script(script) {
                    return Err(semantic_error(
                        entry_index,
                        format!("invalid script `{}`", script),
                    ));
                }
                if !provider.is_valid_feature(feature) {
                    return Err(semantic_error(
                        entry_index,
                        format!("invalid feature `{}`", feature),
                    ));
                }
                if !provider.is_valid_style(script, feature) {
                    return Err(semantic_error(
                        entry_index,
                        format!("invalid style combination `{}`/`{}`", script, feature),
                    ));
                }

                validate_glyph_set_non_overlapping(provider, font_idx, glyphs, entry_index)?;
            }
            ControlEntryAst::StemWidthAdjust {
                script,
                feature,
                widths,
                ..
            } => {
                match script {
                    ScriptSelector::Any => {
                        if !provider.is_valid_wildcard_feature(feature) {
                            return Err(semantic_error(
                                entry_index,
                                format!("invalid wildcard feature `{}`", feature),
                            ));
                        }
                    }
                    ScriptSelector::Script(script_name) => {
                        if !provider.is_valid_script(script_name) {
                            return Err(semantic_error(
                                entry_index,
                                format!("invalid script `{}`", script_name),
                            ));
                        }
                        if !provider.is_valid_feature(feature) {
                            return Err(semantic_error(
                                entry_index,
                                format!("invalid feature `{}`", feature),
                            ));
                        }
                        if !provider.is_valid_style(script_name, feature) {
                            return Err(semantic_error(
                                entry_index,
                                format!(
                                    "invalid style combination `{}`/`{}`",
                                    script_name, feature
                                ),
                            ));
                        }
                    }
                }

                if widths.len() > CONTROL_MAX_WIDTHS {
                    return Err(semantic_error(
                        entry_index,
                        format!(
                            "too many widths: got {}, max {}",
                            widths.len(),
                            CONTROL_MAX_WIDTHS
                        ),
                    ));
                }

                for w in widths {
                    if *w < CONTROL_WIDTH_MIN || *w > CONTROL_WIDTH_MAX {
                        return Err(semantic_error(
                            entry_index,
                            format!(
                                "width {} is out of range [{}, {}]",
                                w, CONTROL_WIDTH_MIN, CONTROL_WIDTH_MAX
                            ),
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

fn validate_font_idx<P: ControlSemanticProvider>(
    provider: &P,
    font_idx: i32,
    entry_index: usize,
) -> Result<(), AutohintError> {
    if font_idx < 0 || font_idx as usize >= provider.num_fonts() {
        return Err(semantic_error(
            entry_index,
            format!("invalid font index {}", font_idx),
        ));
    }
    Ok(())
}

fn resolve_glyph<P: ControlSemanticProvider>(
    provider: &P,
    font_idx: i32,
    glyph: &GlyphRef,
    entry_index: usize,
) -> Result<GlyphId, AutohintError> {
    match glyph {
        GlyphRef::Index(idx) => {
            let num_glyphs = provider.num_glyphs(font_idx).ok_or_else(|| {
                semantic_error(
                    entry_index,
                    format!("unable to get glyph count for font {}", font_idx),
                )
            })?;
            if *idx as usize >= num_glyphs {
                return Err(semantic_error(
                    entry_index,
                    format!(
                        "glyph index {} out of range for font {} (num_glyphs {})",
                        idx, font_idx, num_glyphs
                    ),
                ));
            }
            Ok(GlyphId::new(*idx))
        }
        GlyphRef::Name(name) => provider
            .glyph_index_by_name(font_idx, name)
            .ok_or_else(|| semantic_error(entry_index, format!("invalid glyph name `{}`", name))),
    }
}

fn validate_ordered_number_set(
    set: &NumberSetAst,
    min: i32,
    max: i32,
    entry_index: usize,
    label: &str,
) -> Result<(), AutohintError> {
    let exprs: Vec<RangeExpr> = set
        .elems
        .iter()
        .map(|elem| match elem {
            NumberSetElem::Unlimited => RangeExpr::Unlimited,
            NumberSetElem::RightLimited(v) => RangeExpr::RightLimited(*v),
            NumberSetElem::LeftLimited(v) => RangeExpr::LeftLimited(*v),
            NumberSetElem::Single(v) => RangeExpr::Single(*v),
            NumberSetElem::Range(a, b) => RangeExpr::Range(*a, *b),
        })
        .collect();

    match IntSet::from_exprs(&exprs, min, max) {
        Ok(_) => Ok(()),
        Err(IntSetBuildError::InvalidBounds) => Err(semantic_error(
            entry_index,
            format!("{} has an invalid allowed range [{}, {}]", label, min, max),
        )),
        Err(IntSetBuildError::OutOfRange) => Err(semantic_error(
            entry_index,
            format!(
                "{} has a range out of allowed range [{}, {}]",
                label, min, max
            ),
        )),
        Err(IntSetBuildError::NonAscendingOrOverlapping) => Err(semantic_error(
            entry_index,
            format!("{} has overlapping or non-ascending ranges", label),
        )),
    }
}

fn validate_glyph_set_non_overlapping<P: ControlSemanticProvider>(
    provider: &P,
    font_idx: i32,
    glyphs: &[GlyphSetElem],
    entry_index: usize,
) -> Result<(), AutohintError> {
    let mut ranges: Vec<(GlyphId, GlyphId)> = Vec::new();

    for elem in glyphs {
        let (mut start, mut end) = match elem {
            GlyphSetElem::Single(g) => {
                let idx = resolve_glyph(provider, font_idx, g, entry_index)?;
                (idx, idx)
            }
            GlyphSetElem::Range(left, right) => (
                resolve_glyph(provider, font_idx, left, entry_index)?,
                resolve_glyph(provider, font_idx, right, entry_index)?,
            ),
        };

        if start > end {
            core::mem::swap(&mut start, &mut end);
        }

        for (a, b) in &ranges {
            if !(end.to_u32() < a.to_u32() || start.to_u32() > b.to_u32()) {
                return Err(semantic_error(
                    entry_index,
                    "glyph set has overlapping ranges".to_string(),
                ));
            }
        }

        ranges.push((start, end));
    }

    Ok(())
}

fn fits_i16(v: i32) -> bool {
    i16::try_from(v).is_ok()
}

fn semantic_error(entry_index: usize, message: String) -> AutohintError {
    AutohintError::ControlFileValidationError {
        entry_index,
        message,
    }
}

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
enum TokKind {
    #[regex(r"[ \t\f\r]+", logos::skip)]
    _Whitespace,

    #[regex(r"\\\r?\n", logos::skip)]
    _LineContinuation,

    #[regex(r"#[^\n]*", logos::skip)]
    _Comment,

    #[regex(r";|\n+")]
    Eoe,

    #[token("@")]
    At,
    #[token(",")]
    Comma,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,

    #[regex(r"[0-9]*\.[0-9]+|[0-9]+\.[0-9]*")]
    Real,

    #[regex(r"0|[1-9][0-9]*|0[xX][0-9a-fA-F]+|0[0-7]+")]
    Integer,

    #[regex(r"[A-Za-z._][A-Za-z0-9._]*")]
    Name,
}

#[derive(Debug, Clone)]
struct SpannedTok {
    kind: TokKind,
    text: String,
    span: Range<usize>,
}

fn line_col(input: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (idx, ch) in input.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn parse_i32_token(text: &str) -> Result<i32, String> {
    if text == "0" {
        return Ok(0);
    }
    let (digits, radix) =
        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            (rest, 16)
        } else if text.starts_with('0') {
            (text, 8)
        } else {
            (text, 10)
        };

    i32::from_str_radix(digits, radix).map_err(|_| format!("integer out of range: {text}"))
}

pub fn parse_control(input: &str) -> Result<Vec<ControlEntryAst>, AutohintError> {
    let mut tokens = Vec::new();
    let mut lexer = TokKind::lexer(input);
    while let Some(tok) = lexer.next() {
        match tok {
            Ok(kind) => {
                let span = lexer.span();
                let text = input[span.clone()].to_string();
                tokens.push(SpannedTok { kind, text, span });
            }
            Err(_) => {
                let span = lexer.span();
                let (line, column) = line_col(input, span.start);
                return Err(AutohintError::ControlFileParseError {
                    message: format!("invalid character `{}`", &input[span]),
                    line,
                    column,
                });
            }
        }
    }

    let mut parser = Parser {
        input,
        tokens,
        pos: 0,
    };
    parser.parse_entries()
}

struct Parser<'a> {
    input: &'a str,
    tokens: Vec<SpannedTok>,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn parse_entries(&mut self) -> Result<Vec<ControlEntryAst>, AutohintError> {
        let mut out = Vec::new();

        while self.peek_kind().is_some() {
            self.skip_eoe();
            if self.peek_kind().is_none() {
                break;
            }
            out.push(self.parse_entry()?);
            self.skip_eoe();
        }

        Ok(out)
    }

    fn parse_entry(&mut self) -> Result<ControlEntryAst, AutohintError> {
        let font_idx = self.parse_optional_font_idx()?;

        if self.looks_like_style_adjust() {
            return self.parse_style_adjust(font_idx);
        }

        if self.looks_like_width_adjust() {
            return self.parse_width_adjust(font_idx);
        }

        self.parse_glyph_adjust(font_idx)
    }

    fn parse_style_adjust(&mut self, font_idx: i32) -> Result<ControlEntryAst, AutohintError> {
        let script = self.expect_name("expected script tag")?;
        let feature = self.expect_name("expected feature tag")?;
        self.expect(TokKind::At, "expected `@` before glyph list")?;
        let glyphs = self.parse_glyph_set()?;

        Ok(ControlEntryAst::StyleAdjust {
            font_idx,
            script,
            feature,
            glyphs,
        })
    }

    fn parse_width_adjust(&mut self, font_idx: i32) -> Result<ControlEntryAst, AutohintError> {
        let script = if self.match_kind(TokKind::Star) {
            ScriptSelector::Any
        } else {
            ScriptSelector::Script(self.expect_name("expected script tag or `*`")?)
        };

        let feature = self.expect_name("expected feature tag")?;
        if !self.match_keyword(&["width", "w"]) {
            return self.error_here("expected `width` or `w`");
        }

        let mut widths = Vec::new();
        widths.push(self.expect_integer("expected width value")?);
        while self.match_kind(TokKind::Comma) {
            widths.push(self.expect_integer("expected width value after comma")?);
        }

        Ok(ControlEntryAst::StemWidthAdjust {
            font_idx,
            script,
            feature,
            widths,
        })
    }

    fn parse_glyph_adjust(&mut self, font_idx: i32) -> Result<ControlEntryAst, AutohintError> {
        let glyph = self.parse_glyph_ref()?;

        if self.match_keyword(&["point", "p"]) || self.match_keyword(&["touch", "t"]) {
            let mode = if self.prev_was_keyword(&["point", "p"]) {
                PointMode::Point
            } else {
                PointMode::Touch
            };
            let points = self.parse_number_set()?;
            let x_shift = if self.match_keyword(&["xshift", "x"]) {
                self.expect_real_or_int_signed("expected x shift value")?
            } else {
                0.0
            };
            let y_shift = if self.match_keyword(&["yshift", "y"]) {
                self.expect_real_or_int_signed("expected y shift value")?
            } else {
                0.0
            };
            self.expect(TokKind::At, "expected `@` before ppem set")?;
            let ppems = self.parse_number_set()?;

            return Ok(ControlEntryAst::Delta {
                font_idx,
                glyph,
                mode,
                points,
                x_shift,
                y_shift,
                ppems,
            });
        }

        if self.match_keyword(&["left", "l"])
            || self.match_keyword(&["right", "r"])
            || self.match_keyword(&["nodir", "n"])
        {
            let dir = if self.prev_was_keyword(&["left", "l"]) {
                SegmentDirection::Left
            } else if self.prev_was_keyword(&["right", "r"]) {
                SegmentDirection::Right
            } else {
                SegmentDirection::NoDir
            };

            let points = self.parse_number_set()?;
            let offsets = if self.match_kind(TokKind::LParen) {
                let left = self.expect_signed_integer("expected left offset")?;
                self.expect(TokKind::Comma, "expected comma between offsets")?;
                let right = self.expect_signed_integer("expected right offset")?;
                self.expect(TokKind::RParen, "expected `)` after offsets")?;
                Some((left, right))
            } else {
                None
            };

            return Ok(ControlEntryAst::SegmentDirection {
                font_idx,
                glyph,
                dir,
                points,
                offsets,
            });
        }

        self.error_here("expected one of `point/touch`, `left/right`, or `nodir`")
    }

    fn parse_number_set(&mut self) -> Result<NumberSetAst, AutohintError> {
        let mut elems = Vec::new();
        elems.push(self.parse_number_set_elem()?);
        while self.match_kind(TokKind::Comma) {
            elems.push(self.parse_number_set_elem()?);
        }
        Ok(NumberSetAst { elems })
    }

    fn parse_number_set_elem(&mut self) -> Result<NumberSetElem, AutohintError> {
        if self.match_kind(TokKind::Minus) {
            if self.peek_kind() == Some(TokKind::Integer) {
                let end = self.expect_integer("expected integer after `-`")?;
                return Ok(NumberSetElem::RightLimited(end));
            }
            return Ok(NumberSetElem::Unlimited);
        }

        let start = self.expect_integer("expected integer or range")?;
        if self.match_kind(TokKind::Minus) {
            if self.peek_kind() == Some(TokKind::Integer) {
                let end = self.expect_integer("expected integer after `-`")?;
                return Ok(NumberSetElem::Range(start, end));
            }
            return Ok(NumberSetElem::LeftLimited(start));
        }

        Ok(NumberSetElem::Single(start))
    }

    fn parse_glyph_set(&mut self) -> Result<Vec<GlyphSetElem>, AutohintError> {
        let mut elems = Vec::new();
        elems.push(self.parse_glyph_set_elem()?);
        while self.match_kind(TokKind::Comma) {
            elems.push(self.parse_glyph_set_elem()?);
        }
        Ok(elems)
    }

    fn parse_glyph_set_elem(&mut self) -> Result<GlyphSetElem, AutohintError> {
        let start = self.parse_glyph_ref()?;
        if self.match_kind(TokKind::Minus) {
            let end = self.parse_glyph_ref()?;
            Ok(GlyphSetElem::Range(start, end))
        } else {
            Ok(GlyphSetElem::Single(start))
        }
    }

    fn parse_glyph_ref(&mut self) -> Result<GlyphRef, AutohintError> {
        match self.peek_kind() {
            Some(TokKind::Integer) => {
                let n = self.expect_integer("expected glyph index")?;
                if n < 0 {
                    return self.error_here("glyph index cannot be negative");
                }
                Ok(GlyphRef::Index(n as u32))
            }
            Some(TokKind::Name) => {
                let s = self.expect_name("expected glyph name")?;
                Ok(GlyphRef::Name(s))
            }
            _ => self.error_here("expected glyph id (name or index)"),
        }
    }

    fn parse_optional_font_idx(&mut self) -> Result<i32, AutohintError> {
        if !self.should_consume_font_idx() {
            return Ok(0);
        }
        self.expect_integer("expected font index")
    }

    fn should_consume_font_idx(&self) -> bool {
        if self.peek_kind() != Some(TokKind::Integer) {
            return false;
        }

        let t1 = self.peek_n_kind(1);
        let t2 = self.peek_n_kind(2);
        let t3 = self.peek_n_kind(3);

        if matches!(t1, Some(TokKind::Name | TokKind::Integer))
            && matches!(self.peek_n_keyword(2), Some(k) if is_any_of(k, &["point","p","touch","t","left","l","right","r","nodir","n"]))
        {
            return true;
        }

        if matches!(t1, Some(TokKind::Name))
            && matches!(t2, Some(TokKind::Name))
            && (matches!(t3, Some(TokKind::At))
                || matches!(self.peek_n_keyword(3), Some(k) if is_any_of(k, &["width","w"])))
        {
            return true;
        }

        if t1 == Some(TokKind::Star)
            && matches!(t2, Some(TokKind::Name))
            && matches!(self.peek_n_keyword(3), Some(k) if is_any_of(k, &["width","w"]))
        {
            return true;
        }

        false
    }

    fn looks_like_style_adjust(&self) -> bool {
        matches!(self.peek_kind(), Some(TokKind::Name))
            && matches!(self.peek_n_kind(1), Some(TokKind::Name))
            && self.peek_n_kind(2) == Some(TokKind::At)
    }

    fn looks_like_width_adjust(&self) -> bool {
        (self.peek_kind() == Some(TokKind::Name) || self.peek_kind() == Some(TokKind::Star))
            && self.peek_n_kind(1) == Some(TokKind::Name)
            && matches!(self.peek_n_keyword(2), Some(k) if is_any_of(k, &["width", "w"]))
    }

    fn expect_integer(&mut self, msg: &str) -> Result<i32, AutohintError> {
        let tok = self.expect(TokKind::Integer, msg)?;
        parse_i32_token(&tok.text).map_err(|e| self.error_at(tok.span.start, e))
    }

    fn expect_signed_integer(&mut self, msg: &str) -> Result<i32, AutohintError> {
        let sign = if self.match_kind(TokKind::Plus) {
            1
        } else if self.match_kind(TokKind::Minus) {
            -1
        } else {
            1
        };
        Ok(sign * self.expect_integer(msg)?)
    }

    fn expect_real_or_int_signed(&mut self, msg: &str) -> Result<f64, AutohintError> {
        let sign = if self.match_kind(TokKind::Plus) {
            1.0
        } else if self.match_kind(TokKind::Minus) {
            -1.0
        } else {
            1.0
        };

        if self.peek_kind() == Some(TokKind::Real) {
            let tok = self.bump().expect("checked by peek_kind");
            let v: f64 = tok.text.parse().map_err(|_| {
                self.error_at(tok.span.start, format!("invalid real: {}", tok.text))
            })?;
            Ok(sign * v)
        } else if self.peek_kind() == Some(TokKind::Integer) {
            Ok(sign * self.expect_integer(msg)? as f64)
        } else {
            self.error_here(msg)
        }
    }

    fn expect_name(&mut self, msg: &str) -> Result<String, AutohintError> {
        match self.peek_kind() {
            Some(TokKind::Name) => Ok(self.bump().expect("checked by peek_kind").text),
            _ => self.error_here(msg),
        }
    }

    fn expect(&mut self, kind: TokKind, msg: &str) -> Result<SpannedTok, AutohintError> {
        if self.peek_kind() == Some(kind) {
            Ok(self.bump().expect("checked by peek_kind"))
        } else {
            self.error_here(msg)
        }
    }

    fn bump(&mut self) -> Option<SpannedTok> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn match_kind(&mut self, kind: TokKind) -> bool {
        if self.peek_kind() == Some(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn skip_eoe(&mut self) {
        while self.peek_kind() == Some(TokKind::Eoe) {
            self.pos += 1;
        }
    }

    fn peek_kind(&self) -> Option<TokKind> {
        self.tokens.get(self.pos).map(|t| t.kind)
    }

    fn peek_n_kind(&self, n: usize) -> Option<TokKind> {
        self.tokens.get(self.pos + n).map(|t| t.kind)
    }

    fn prev_was_keyword(&self, words: &[&str]) -> bool {
        if self.pos == 0 {
            return false;
        }
        let Some(tok) = self.tokens.get(self.pos - 1) else {
            return false;
        };
        tok.kind == TokKind::Name && is_any_of(tok.text.as_str(), words)
    }

    fn peek_n_keyword(&self, n: usize) -> Option<&str> {
        let tok = self.tokens.get(self.pos + n)?;
        if tok.kind == TokKind::Name {
            Some(tok.text.as_str())
        } else {
            None
        }
    }

    fn match_keyword(&mut self, words: &[&str]) -> bool {
        let Some(tok) = self.tokens.get(self.pos) else {
            return false;
        };
        if tok.kind == TokKind::Name && is_any_of(tok.text.as_str(), words) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn error_here<T>(&self, message: &str) -> Result<T, AutohintError> {
        let offset = self
            .tokens
            .get(self.pos)
            .map(|t| t.span.start)
            .unwrap_or(self.input.len());
        Err(self.error_at(offset, message.to_string()))
    }

    fn error_at(&self, offset: usize, message: String) -> AutohintError {
        let (line, column) = line_col(self.input, offset);
        AutohintError::ControlFileParseError {
            message,
            line,
            column,
        }
    }
}

fn is_any_of(s: &str, words: &[&str]) -> bool {
    words.iter().any(|w| s.eq_ignore_ascii_case(w))
}

/// Simple provider that does minimal validation for control parsing.
/// Used when we don't have access to real font data yet.
pub(crate) struct MinimalProvider {
    num_sfnts: usize,
}

impl MinimalProvider {
    pub(crate) fn new(num_sfnts: usize) -> Self {
        Self { num_sfnts }
    }
}

impl ControlSemanticProvider for MinimalProvider {
    fn num_fonts(&self) -> usize {
        self.num_sfnts
    }

    fn num_glyphs(&self, _font_idx: i32) -> Option<usize> {
        // Return a large number to allow most glyphs; real validation happens in C
        Some(0x10000)
    }

    fn glyph_index_by_name(&self, _font_idx: i32, _glyph_name: &str) -> Option<GlyphId> {
        // Defer glyph name resolution to C code; for now return None to force
        // index-based references or error handling in C
        None
    }

    fn glyph_point_count(&self, _font_idx: i32, _glyph_idx: GlyphId) -> Option<usize> {
        // Return a reasonable default; C code will validate actual point counts
        Some(256)
    }
}

pub(crate) struct SkrifaProvider {
    bytes: Vec<u8>,
    glyph_names: HashMap<String, GlyphId>,
}
impl SkrifaProvider {
    pub(crate) fn new(font_bytes: Vec<u8>) -> Result<Self, ReadError> {
        let fontref = FontRef::new(&font_bytes)?;
        let glyphnames = GlyphNames::new(&fontref);
        let glyph_names = glyphnames
            .iter()
            .map(|(id, name)| (name.to_string(), id))
            .collect();

        Ok(Self {
            bytes: font_bytes,
            glyph_names,
        })
    }

    #[allow(clippy::unwrap_used)] // we checked at construction that this unwrap is safe
    fn font(&self) -> FontRef<'_> {
        FontRef::new(&self.bytes).unwrap()
    }
}

impl ControlSemanticProvider for SkrifaProvider {
    fn num_fonts(&self) -> usize {
        1
    }

    fn num_glyphs(&self, _font_idx: i32) -> Option<usize> {
        Some(self.font().maxp().ok()?.num_glyphs().into())
    }

    fn glyph_index_by_name(&self, _font_idx: i32, glyph_name: &str) -> Option<GlyphId> {
        self.glyph_names.get(glyph_name).copied()
    }

    fn glyph_point_count(&self, _font_idx: i32, glyph_idx: GlyphId) -> Option<usize> {
        let loca = self.font().loca(None).ok()?;
        let glyf = self.font().glyf().ok()?;
        let glyph = loca.get_glyf((glyph_idx.to_u32()).into(), &glyf).ok()?;
        match glyph {
            None => Some(0),
            Some(Glyph::Simple(g)) => Some(g.num_points()),
            Some(Glyph::Composite(g)) => Some(g.components().count() + 4),
        }
    }
}

pub(crate) fn parse_control_entries<P: crate::control::ControlSemanticProvider>(
    input: &str,
    provider: &P,
) -> Result<Vec<ResolvedControlEntry>, AutohintError> {
    let entries = crate::control::parse_control(input)?;
    crate::control::validate_control_entries(&entries, provider)?;

    let mut out = Vec::new();

    for (idx, entry) in entries.iter().enumerate() {
        let line_number = (idx + 1) as i32;

        match entry {
            ControlEntryAst::Delta {
                font_idx,
                glyph,
                mode,
                points,
                x_shift,
                y_shift,
                ppems,
            } => {
                let glyph_idx = resolve_glyph_ref(*font_idx, glyph, provider, idx + 1)?;
                let point_count = provider
                    .glyph_point_count(*font_idx, glyph_idx)
                    .ok_or_else(|| AutohintError::ControlFileValidationError {
                        entry_index: idx + 1,
                        message: format!(
                            "unable to get point count for glyph index {} in font {}",
                            glyph_idx, font_idx
                        ),
                    })?;
                let points = points.to_intset(0, point_count as i32 - 1)?;
                let ppems = ppems.to_intset(
                    crate::control::CONTROL_DELTA_PPEM_MIN,
                    crate::control::CONTROL_DELTA_PPEM_MAX,
                )?;

                out.push(ResolvedControlEntry::Delta {
                    font_idx: *font_idx,
                    glyph_idx,
                    before_iup: matches!(mode, PointMode::Touch),
                    points,
                    ppems,
                    x_shift: (*x_shift * crate::control::CONTROL_DELTA_FACTOR as f64).round()
                        as i32,
                    y_shift: (*y_shift * crate::control::CONTROL_DELTA_FACTOR as f64).round()
                        as i32,
                    line_number,
                });
            }
            ControlEntryAst::SegmentDirection {
                font_idx,
                glyph,
                dir,
                points,
                offsets,
            } => {
                let glyph_idx = resolve_glyph_ref(*font_idx, glyph, provider, idx + 1)?;
                let point_count = provider
                    .glyph_point_count(*font_idx, glyph_idx)
                    .ok_or_else(|| AutohintError::ControlFileValidationError {
                        entry_index: idx + 1,
                        message: format!(
                            "unable to get point count for glyph index {} in font {}",
                            glyph_idx, font_idx
                        ),
                    })?;
                let points = points.to_intset(0, point_count as i32 - 1)?;
                let (left_offset, right_offset) = offsets.unwrap_or((0, 0));

                out.push(ResolvedControlEntry::SegmentDirection {
                    font_idx: *font_idx,
                    glyph_idx,
                    points,
                    dir: match dir {
                        SegmentDirection::Left => -1,
                        SegmentDirection::Right => 1,
                        SegmentDirection::NoDir => 4,
                    },
                    left_offset,
                    right_offset,
                    line_number,
                });
            }
            ControlEntryAst::StyleAdjust {
                font_idx,
                script,
                feature,
                glyphs,
            } => {
                let mut glyph_indices = Vec::new();
                for glyph_elem in glyphs {
                    match glyph_elem {
                        GlyphSetElem::Single(g) => {
                            glyph_indices.push(resolve_glyph_ref(*font_idx, g, provider, idx + 1)?);
                        }
                        GlyphSetElem::Range(_, _) => {
                            return Err(AutohintError::ControlFileValidationError {
                                entry_index: idx + 1,
                                message: "glyph ranges in StyleAdjust not yet supported"
                                    .to_string(),
                            });
                        }
                    }
                }

                // Resolve script/feature directly to a Skrifa style index.
                let resolved_style =
                    crate::globals::resolve_script_feature_to_style_index(script, feature)
                        .ok_or_else(|| AutohintError::ControlFileValidationError {
                            entry_index: idx + 1,
                            message: format!(
                                "unknown or unsupported style: {}/{}",
                                script, feature
                            ),
                        })?;

                out.push(ResolvedControlEntry::StyleAdjust {
                    font_idx: *font_idx,
                    style: resolved_style as u16,
                    glyph_indices,
                });
            }
            ControlEntryAst::StemWidthAdjust { .. } => {
                out.push(ResolvedControlEntry::StemWidthAdjust);
            }
        }
    }

    Ok(out)
}

pub(crate) fn resolve_glyph_ref<P: crate::control::ControlSemanticProvider>(
    font_idx: i32,
    glyph: &GlyphRef,
    provider: &P,
    entry_index: usize,
) -> Result<GlyphId, AutohintError> {
    match glyph {
        GlyphRef::Index(idx) => Ok(GlyphId::new(*idx)),
        GlyphRef::Name(name) => provider.glyph_index_by_name(font_idx, name).ok_or_else(|| {
            AutohintError::ControlFileValidationError {
                entry_index,
                message: format!("invalid glyph name `{}`", name),
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    struct MockProvider {
        num_fonts: usize,
        glyph_counts: HashMap<i32, usize>,
        glyph_names: HashMap<(i32, String), GlyphId>,
        point_counts: HashMap<(i32, GlyphId), usize>,
        valid_scripts: HashSet<String>,
        valid_styles: HashSet<(String, String)>,
    }

    impl MockProvider {
        fn basic() -> Self {
            let mut glyph_counts = HashMap::new();
            glyph_counts.insert(0, 100);

            let mut glyph_names = HashMap::new();
            glyph_names.insert((0, "A".to_string()), GlyphId::new(1));
            glyph_names.insert((0, "Q".to_string()), GlyphId::new(2));
            glyph_names.insert((0, "Aacute".to_string()), GlyphId::new(3));
            glyph_names.insert((0, "zero.sups".to_string()), GlyphId::new(10));
            glyph_names.insert((0, "nine.sups".to_string()), GlyphId::new(19));
            glyph_names.insert((0, "a.sups".to_string()), GlyphId::new(20));
            glyph_names.insert((0, "o.sups".to_string()), GlyphId::new(21));

            let mut point_counts = HashMap::new();
            point_counts.insert((0, GlyphId::new(1)), 64);
            point_counts.insert((0, GlyphId::new(2)), 64);
            point_counts.insert((0, GlyphId::new(3)), 64);
            point_counts.insert((0, GlyphId::new(10)), 64);
            point_counts.insert((0, GlyphId::new(19)), 64);
            point_counts.insert((0, GlyphId::new(20)), 64);
            point_counts.insert((0, GlyphId::new(21)), 64);

            let valid_scripts = ["cyrl".to_string(), "latn".to_string()]
                .into_iter()
                .collect();
            let valid_styles = [
                ("cyrl".to_string(), "sups".to_string()),
                ("latn".to_string(), "dflt".to_string()),
            ]
            .into_iter()
            .collect();

            Self {
                num_fonts: 1,
                glyph_counts,
                glyph_names,
                point_counts,
                valid_scripts,
                valid_styles,
            }
        }
    }

    impl ControlSemanticProvider for MockProvider {
        fn num_fonts(&self) -> usize {
            self.num_fonts
        }

        fn num_glyphs(&self, font_idx: i32) -> Option<usize> {
            self.glyph_counts.get(&font_idx).copied()
        }

        fn glyph_index_by_name(&self, font_idx: i32, glyph_name: &str) -> Option<GlyphId> {
            self.glyph_names
                .get(&(font_idx, glyph_name.to_string()))
                .copied()
        }

        fn glyph_point_count(&self, font_idx: i32, glyph_idx: GlyphId) -> Option<usize> {
            self.point_counts.get(&(font_idx, glyph_idx)).copied()
        }

        fn is_valid_script(&self, script: &str) -> bool {
            self.valid_scripts.contains(script)
        }

        fn is_valid_style(&self, script: &str, feature: &str) -> bool {
            self.valid_styles
                .contains(&(script.to_string(), feature.to_string()))
        }
    }

    #[test]
    fn parses_delta_entry() {
        let input = "0 Aacute touch 2-4 yshift 0.25 @ 12,13";
        let ast = parse_control(input).expect("parse succeeds");

        assert_eq!(ast.len(), 1);
        match &ast[0] {
            ControlEntryAst::Delta {
                font_idx,
                glyph,
                mode,
                points,
                x_shift,
                y_shift,
                ppems,
            } => {
                assert_eq!(*font_idx, 0);
                assert_eq!(glyph, &GlyphRef::Name("Aacute".to_string()));
                assert_eq!(mode, &PointMode::Touch);
                assert_eq!(points.elems, vec![NumberSetElem::Range(2, 4)]);
                assert_eq!(*x_shift, 0.0);
                assert_eq!(*y_shift, 0.25);
                assert_eq!(
                    ppems.elems,
                    vec![NumberSetElem::Single(12), NumberSetElem::Single(13)]
                );
            }
            _ => panic!("unexpected AST entry"),
        }
    }

    #[test]
    fn parses_segment_direction_with_offsets() {
        let input = "Q left 38 (-70,20)";
        let ast = parse_control(input).expect("parse succeeds");

        assert_eq!(ast.len(), 1);
        match &ast[0] {
            ControlEntryAst::SegmentDirection {
                font_idx,
                glyph,
                dir,
                points,
                offsets,
            } => {
                assert_eq!(*font_idx, 0);
                assert_eq!(glyph, &GlyphRef::Name("Q".to_string()));
                assert_eq!(dir, &SegmentDirection::Left);
                assert_eq!(points.elems, vec![NumberSetElem::Single(38)]);
                assert_eq!(*offsets, Some((-70, 20)));
            }
            _ => panic!("unexpected AST entry"),
        }
    }

    #[test]
    fn parses_style_adjust() {
        let input = "cyrl sups @ zero.sups-nine.sups, a.sups, o.sups";
        let ast = parse_control(input).expect("parse succeeds");

        assert_eq!(ast.len(), 1);
        match &ast[0] {
            ControlEntryAst::StyleAdjust {
                font_idx,
                script,
                feature,
                glyphs,
            } => {
                assert_eq!(*font_idx, 0);
                assert_eq!(script, "cyrl");
                assert_eq!(feature, "sups");
                assert_eq!(glyphs.len(), 3);
            }
            _ => panic!("unexpected AST entry"),
        }
    }

    #[test]
    fn parses_width_adjust_with_wildcard() {
        let input = "* dflt width 100, 80";
        let ast = parse_control(input).expect("parse succeeds");

        assert_eq!(ast.len(), 1);
        match &ast[0] {
            ControlEntryAst::StemWidthAdjust {
                font_idx,
                script,
                feature,
                widths,
            } => {
                assert_eq!(*font_idx, 0);
                assert_eq!(script, &ScriptSelector::Any);
                assert_eq!(feature, "dflt");
                assert_eq!(widths, &vec![100, 80]);
            }
            _ => panic!("unexpected AST entry"),
        }
    }

    #[test]
    fn handles_comments_separators_and_continuations() {
        let input = "# comment\nQ left 38;\\\nA point 1 @ 12\n";
        let ast = parse_control(input).expect("parse succeeds");
        assert_eq!(ast.len(), 2);
    }

    #[test]
    fn reports_unexpected_token() {
        let input = "A width 100";
        let err = parse_control(input).expect_err("should fail");
        assert!(err.to_string().contains("expected"));
        assert!(err.to_string().contains("at line 1"));
    }

    #[test]
    fn validates_delta_entry_semantics() {
        let input = "Aacute touch 2-4 yshift 0.25 @ 12,13";
        let ast = parse_control(input).expect("parse succeeds");
        let provider = MockProvider::basic();

        validate_control_entries(&ast, &provider).expect("validation succeeds");
    }

    #[test]
    fn rejects_invalid_ppem_value() {
        let input = "A point 2 @ 5";
        let ast = parse_control(input).expect("parse succeeds");
        let provider = MockProvider::basic();

        let err = validate_control_entries(&ast, &provider).expect_err("validation should fail");
        assert!(err.to_string().contains("ppem set"));
    }

    #[test]
    fn rejects_overlapping_glyph_ranges() {
        let input = "cyrl sups @ zero.sups-nine.sups, nine.sups";
        let ast = parse_control(input).expect("parse succeeds");
        let provider = MockProvider::basic();

        let err = validate_control_entries(&ast, &provider).expect_err("validation should fail");
        assert!(err.to_string().contains("overlapping"));
    }

    #[test]
    fn rejects_too_many_widths() {
        let input = "* dflt width 1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17";
        let ast = parse_control(input).expect("parse succeeds");
        let provider = MockProvider::basic();

        let err = validate_control_entries(&ast, &provider).expect_err("validation should fail");
        assert!(err.to_string().contains("too many widths"));
    }

    #[test]
    fn rejects_invalid_font_index() {
        let input = "1 A point 1 @ 12";
        let ast = parse_control(input).expect("parse succeeds");
        let provider = MockProvider::basic();

        let err = validate_control_entries(&ast, &provider).expect_err("validation should fail");
        assert!(err.to_string().contains("invalid font index"));
    }

    #[test]
    fn parses_plain_number_set_syntax() {
        let set = NumberSetAst::parse(" 1-3, , 8 , 10- ").expect("parse succeeds");
        assert_eq!(
            set.elems,
            vec![
                NumberSetElem::Range(1, 3),
                NumberSetElem::Single(8),
                NumberSetElem::LeftLimited(10)
            ]
        );
    }

    #[test]
    fn canonicalizes_number_set_like_numberset_show() {
        let set = NumberSetAst::parse("-8, 9, 10-12, 20-").expect("parse succeeds");
        let shown = set.canonicalize(6, 20).expect("canonicalize succeeds");
        assert_eq!(shown, "-12, 20");
    }
}
