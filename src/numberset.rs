use crate::{
    intset::{BuildError, IntSet, RangeExpr},
    AutohintError,
};

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

    pub fn canonicalize(&self, min: i32, max: i32) -> Result<String, BuildError> {
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

    #[cfg(feature = "control")]
    pub(crate) fn to_intset(&self, min: i32, max: i32) -> Result<IntSet, AutohintError> {
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

#[cfg(test)]
mod tests {
    use super::{NumberSetAst, NumberSetElem};

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
