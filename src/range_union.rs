#[derive(Default, Debug, Clone)]
pub struct RangeUnion {
    ends_by_start: std::collections::BTreeMap<usize, usize>,
}

impl RangeUnion {
    pub fn push(&mut self, range: impl std::borrow::Borrow<std::ops::Range<usize>>) {
        let range = range.borrow();
        self.ends_by_start
            .entry(range.start)
            .and_modify(|e| *e = (*e).max(range.end))
            .or_insert(range.end);
    }

    pub fn extend(&mut self, ranges: impl IntoIterator<Item = std::ops::Range<usize>>) {
        for range in ranges {
            self.push(range);
        }
    }

    pub fn iter_filling_gaps(&self, gap_size: usize) -> RangeUnionIterator {
        let mut iterator = self.ends_by_start.iter();
        let first_interval = iterator.next();
        RangeUnionIterator {
            position: iterator,
            current_interval: first_interval,
            fill_gaps: gap_size,
        }
    }

    pub fn iter(&self) -> RangeUnionIterator {
        self.iter_filling_gaps(0)
    }

    pub fn is_empty(&self) -> bool {
        self.ends_by_start.is_empty()
    }
}

impl<'it> IntoIterator for &'it RangeUnion {
    type Item = std::ops::Range<usize>;
    type IntoIter = RangeUnionIterator<'it>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct RangeUnionIterator<'it> {
    position: std::collections::btree_map::Iter<'it, usize, usize>,
    current_interval: Option<(&'it usize, &'it usize)>,
    fill_gaps: usize,
}

impl Iterator for RangeUnionIterator<'_> {
    type Item = std::ops::Range<usize>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.current_interval {
            None => None,
            Some((&first_start, &first_end)) => {
                let mut farthest_end: usize = first_end;
                loop {
                    self.current_interval = self.position.next();
                    match self.current_interval {
                        None => break Some(first_start..farthest_end),
                        Some((&start, &end)) => {
                            if start <= farthest_end + self.fill_gaps {
                                farthest_end = farthest_end.max(end);
                            } else {
                                break Some(first_start..farthest_end);
                            }
                        }
                    }
                }
            }
        }
    }
}
