use crate::config;

#[derive(Default)]
pub struct RangeUnion {
    ends_by_start: std::collections::BTreeMap<usize, usize>,
}

impl RangeUnion {
    pub fn extend(&mut self, ranges: impl AsRef<[std::ops::Range<usize>]>) {
        for range in ranges.as_ref() {
            self.add(range);
        }
    }

    fn add(&mut self, range: &std::ops::Range<usize>) {
        self.ends_by_start
            .entry(range.start)
            .and_modify(|e| *e = (*e).max(range.end))
            .or_insert(range.end);
    }

    // TODO rewrite as iterator
    // TODO fill in single-line gaps
    pub fn as_ranges(&self) -> std::vec::Vec<std::ops::Range<usize>> {
        let mut result: std::vec::Vec<std::ops::Range<usize>> = std::vec::Vec::new();
        let mut earliest_open_start: Option<usize> = None;
        let mut farthest_end: usize = 0;
        for (start, end) in self.ends_by_start.iter() {
            match earliest_open_start {
                None => {
                    earliest_open_start = Some(*start);
                    farthest_end = *end;
                }
                Some(prev_start) => {
                    if *start <= farthest_end {
                        farthest_end = farthest_end.max(*end);
                    } else {
                        result.push(prev_start..farthest_end);
                        earliest_open_start = Some(*start);
                        farthest_end = *end;
                    }
                }
            }
        }
        if let Some(prev_start) = earliest_open_start {
            result.push(prev_start..farthest_end);
        }
        result
    }
}

pub fn find_definition(
    source_code: &[u8],
    tree: &tree_sitter::Tree,
    language_info: &config::LanguageInfo,
    pattern: &regex::Regex,
) -> std::vec::Vec<std::ops::Range<usize>> {
    let mut result: std::vec::Vec<std::ops::Range<usize>> = std::vec::Vec::new();
    let mut cursor = tree_sitter::QueryCursor::new();
    //let mut context_cursor = tree_sitter::QueryCursor::new();
    //context_cursor.set_max_start_depth(0);
    for node_query in language_info.match_patterns.iter() {
        let name_idx = node_query.capture_index_for_name("name").unwrap();
        let def_idx = node_query.capture_index_for_name("def").unwrap();
        for query_match in cursor
            .matches(node_query, tree.root_node(), source_code)
            .filter(|query_match| {
                query_match.captures.iter().any(|capture| {
                    capture.index == name_idx
                        && pattern.is_match(
                            std::str::from_utf8(&source_code[capture.node.byte_range()]).unwrap(),
                        )
                })
            })
        {
            for capture in query_match
                .captures
                .iter()
                .filter(|capture| capture.index == def_idx)
            {
                result.push(
                    capture.node.range().start_point.row
                        ..capture.node.range().end_point.row.saturating_add(1),
                );
                let mut node = capture.node;
                // include preceding neighbors as context while they remain relevant
                // such as comments, python decorators, rust attributes, and c++ template arguments
                while let Some(sibling) = node.prev_sibling() {
                    if language_info.sibling_patterns.contains(&sibling.kind_id()) {
                        result.push(
                            sibling.range().start_point.row
                                ..sibling.range().end_point.row.saturating_add(1),
                        );
                        node = sibling;
                    } else {
                        break;
                    }
                }
                // then include a header line from each relevant ancestor
                while let Some(parent) = node.parent() {
                    // TODO interval arithmetic
                    if language_info.parent_patterns.contains(&parent.kind_id()) {
                        let context_start = parent.range().start_point.row;
                        let context_end = context_start.max(
                            language_info
                                .parent_exclusions
                                .iter()
                                .filter_map(|field_id| parent.child_by_field_id(*field_id))
                                .map(|c| {
                                    c.range().start_point.row.saturating_sub(1)
                                    // TODO only subtract if exclusion is start of line?
                                })
                                .min()
                                .unwrap_or(parent.range().end_point.row),
                        );
                        result.push(context_start..context_end.saturating_add(1));
                    }
                    node = parent;
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const PYTHON_SOURCE: &[u8] = include_bytes!("../test_cases/python.py");

    fn union_ranges(
        ranges: impl AsRef<[std::ops::Range<usize>]>,
    ) -> std::vec::Vec<std::ops::Range<usize>> {
        let mut union: RangeUnion = Default::default();
        union.extend(ranges);
        union.as_ranges()
    }

    #[test]
    fn python_examples() {
        // these ranges are 0-indexed and bat line numbers are 1-indexed so generate them with `nl -ba -v0`
        let cases = [
            ("one", vec![11..34]),
            ("two", vec![13..15]),
            ("three", vec![13..14, 15..16]),
            ("four", vec![13..14, 17..24]),
            ("five", vec![13..14, 21..22]),
            ("six", vec![13..14, 25..34]),
            ("seven", vec![36..43]),
            ("eight", vec![44..45]),
            // nine and ten are function parameters split across multiple lines;
            // I assume you want the whole signature because it'll be either short enough to not be a pain
            // or long enough to need further clarification if you only see one line from it.
            ("nine", vec![13..14, 26..33]),
            ("ten", vec![13..14, 26..33]),
            ("int", vec![]),
            ("abc", vec![39..41]),
            ("xyz", vec![39..40, 41..42]),
            ("def", vec![46..48]),
        ];
        let config = config::Config::load_default();
        let language_info = config
            .get_language_info(config::LanguageName::Python)
            .unwrap()
            .unwrap();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(language_info.language).unwrap();
        let tree = parser.parse(PYTHON_SOURCE, None).unwrap();
        for (query, expect_ranges) in cases {
            let pattern = regex::Regex::new(&(String::from("^") + query + "$")).unwrap();
            let result = find_definition(PYTHON_SOURCE, &tree, &language_info, &pattern);
            assert_eq!(union_ranges(result).as_slice(), expect_ranges);
        }
    }
}
