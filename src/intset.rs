#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntRange {
    pub start: i32,
    pub end: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeExpr {
    Unlimited,
    RightLimited(i32),
    LeftLimited(i32),
    Single(i32),
    Range(i32, i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntSet {
    ranges: Vec<IntRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    InvalidBounds,
    OutOfRange,
    NonAscendingOrOverlapping,
}

impl IntSet {
    pub fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    pub fn ranges(&self) -> &[IntRange] {
        &self.ranges
    }

    #[cfg(test)]
    pub fn contains(&self, value: i32) -> bool {
        for range in &self.ranges {
            if value < range.start {
                return false;
            }
            if value <= range.end {
                return true;
            }
        }
        false
    }

    pub fn iter_values(&self) -> IntSetValueIter<'_> {
        IntSetValueIter {
            ranges: &self.ranges,
            range_idx: 0,
            current: None,
        }
    }

    pub fn from_exprs(exprs: &[RangeExpr], min: i32, max: i32) -> Result<Self, BuildError> {
        if min > max {
            return Err(BuildError::InvalidBounds);
        }

        let mut set = Self::new();
        let mut prev_end: Option<i32> = None;

        for expr in exprs {
            let (mut start, mut end) = match *expr {
                RangeExpr::Unlimited => (min, max),
                RangeExpr::RightLimited(v) => (min, v),
                RangeExpr::LeftLimited(v) => (v, max),
                RangeExpr::Single(v) => (v, v),
                RangeExpr::Range(a, b) => (a, b),
            };

            if start > end {
                core::mem::swap(&mut start, &mut end);
            }

            if start < min || end > max {
                return Err(BuildError::OutOfRange);
            }

            if let Some(prev) = prev_end {
                if start <= prev {
                    return Err(BuildError::NonAscendingOrOverlapping);
                }
            }

            if let Some(last) = set.ranges.last_mut() {
                if start == last.end + 1 {
                    last.end = end;
                    prev_end = Some(end);
                    continue;
                }
            }

            set.ranges.push(IntRange { start, end });
            prev_end = Some(end);
        }

        Ok(set)
    }
}

pub struct IntSetValueIter<'a> {
    ranges: &'a [IntRange],
    range_idx: usize,
    current: Option<i32>,
}

impl Iterator for IntSetValueIter<'_> {
    type Item = i32;

    fn next(&mut self) -> Option<Self::Item> {
        let range = self.ranges.get(self.range_idx)?;

        match self.current {
            None => {
                self.current = Some(range.start);
                Some(range.start)
            }
            Some(value) if value < range.end => {
                let next_value = value + 1;
                self.current = Some(next_value);
                Some(next_value)
            }
            Some(_) => {
                self.range_idx += 1;
                self.current = None;
                self.next()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BuildError, IntSet, RangeExpr};

    #[test]
    fn from_exprs_merges_adjacent() {
        let exprs = [
            RangeExpr::Single(3),
            RangeExpr::Range(4, 6),
            RangeExpr::Single(9),
        ];
        let set = IntSet::from_exprs(&exprs, 0, 10).expect("valid set");

        assert_eq!(set.ranges().len(), 2);
        assert_eq!(set.ranges()[0].start, 3);
        assert_eq!(set.ranges()[0].end, 6);
        assert_eq!(set.ranges()[1].start, 9);
        assert_eq!(set.ranges()[1].end, 9);
    }

    #[test]
    fn from_exprs_rejects_overlap_or_non_ascending() {
        let overlap = [RangeExpr::Range(3, 7), RangeExpr::Range(6, 9)];
        assert_eq!(
            IntSet::from_exprs(&overlap, 0, 20),
            Err(BuildError::NonAscendingOrOverlapping)
        );

        let descending = [RangeExpr::Range(10, 12), RangeExpr::Range(2, 3)];
        assert_eq!(
            IntSet::from_exprs(&descending, 0, 20),
            Err(BuildError::NonAscendingOrOverlapping)
        );
    }

    #[test]
    fn contains_and_iter_values_work() {
        let exprs = [RangeExpr::Range(2, 4), RangeExpr::Single(7)];
        let set = IntSet::from_exprs(&exprs, 0, 20).expect("valid set");

        assert!(set.contains(2));
        assert!(set.contains(4));
        assert!(!set.contains(5));

        let values: Vec<i32> = set.iter_values().collect();
        assert_eq!(values, vec![2, 3, 4, 7]);
    }

    #[test]
    fn respects_bounded_forms() {
        let exprs = [RangeExpr::RightLimited(4), RangeExpr::LeftLimited(10)];
        let set = IntSet::from_exprs(&exprs, 2, 12).expect("valid set");
        let values: Vec<i32> = set.iter_values().collect();

        assert_eq!(values, vec![2, 3, 4, 10, 11, 12]);
    }

    #[test]
    fn out_of_range_is_rejected() {
        let exprs = [RangeExpr::Single(100)];
        assert_eq!(
            IntSet::from_exprs(&exprs, 0, 50),
            Err(BuildError::OutOfRange)
        );
    }
}
