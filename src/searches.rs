use crate::{config, range_union};

pub struct ParsedFile {
    pub language_name: config::LanguageName,
    pub source_code: std::vec::Vec<u8>,
    pub tree: tree_sitter::Tree,
}

impl ParsedFile {
    pub fn from_filename(path: &std::ffi::OsString) -> Option<ParsedFile> {
        // TODO 0: add more languages
        // TODO 1: sniff syntax by content
        //     maybe use shebangs
        //     maybe https://github.com/sharkdp/bat/blob/master/src/syntax_mapping.rs
        // TODO 2: group by language and do a second pass with language-specific regexes?
        use os_str_bytes::OsStrBytesExt;
        let language_name = if path.ends_with(".rs") {
            config::LanguageName::Rust
        } else if path.ends_with(".py") || path.ends_with(".pyx") {
            config::LanguageName::Python
        } else if path.ends_with(".js") {
            config::LanguageName::Js
        } else if path.ends_with(".ts") {
            config::LanguageName::Ts
        } else if path.ends_with(".tsx") {
            config::LanguageName::Tsx
        } else if path.ends_with(".c") || path.ends_with(".h") {
            config::LanguageName::C
        } else if path.ends_with(".cpp")
            || path.ends_with(".hpp")
            || path.ends_with(".cxx")
            || path.ends_with(".hxx")
            || path.ends_with(".C")
            || path.ends_with(".H")
        {
            config::LanguageName::CPlusPlus
        } else if path.ends_with(".go") {
            config::LanguageName::Go
        } else {
            return None;
        };
        let source_code = std::fs::read(path).unwrap(); // TODO transmit error
        Self::from_bytes(source_code, language_name)
    }

    pub fn from_bytes(
        source_code: Vec<u8>,
        language_name: config::LanguageName,
    ) -> Option<ParsedFile> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(language_name.get_language()).unwrap();
        let tree = parser.parse(&source_code, None).unwrap();
        Some(ParsedFile {
            language_name,
            source_code,
            tree,
        })
    }
}

pub fn find_definition(
    source_code: &[u8],
    tree: &tree_sitter::Tree,
    language_info: &config::LanguageInfo,
    pattern: &regex::Regex,
    recurse: bool,
) -> (range_union::RangeUnion, std::vec::Vec<String>) {
    let mut result: range_union::RangeUnion = Default::default();
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut recurse_cursor = tree_sitter::QueryCursor::new();
    let mut recurse_names: std::vec::Vec<String> = std::vec::Vec::new();
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
                let mut node = capture.node;
                result.push(
                    node.range().start_point.row..node.range().end_point.row.saturating_add(1),
                );
                // find names to look up for recursion
                if recurse {
                    for recurse_query in language_info.recurse_patterns.iter() {
                        let recurse_name_idx = node_query.capture_index_for_name("name").unwrap();
                        for recurse_match in
                            recurse_cursor.matches(recurse_query, node, source_code)
                        {
                            for recurse_capture in recurse_match
                                .captures
                                .iter()
                                .filter(|recurse_capture| recurse_capture.index == recurse_name_idx)
                            {
                                let recurse_name = std::str::from_utf8(
                                    &source_code[recurse_capture.node.byte_range()],
                                )
                                .unwrap();
                                recurse_names.push(String::from(recurse_name));
                            }
                        }
                    }
                }
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
    recurse_names.sort();
    recurse_names.dedup();
    (result, recurse_names)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PYTHON_SOURCE: &[u8] = include_bytes!("../test_cases/python.py");

    #[test]
    fn python_examples() {
        // these ranges are 0-indexed and bat line numbers are 1-indexed so generate them with `nl -ba -v0`
        #[rustfmt::skip]
        let cases = [
            ("one", vec![11..34], vec!["hecks"]), // hm I don't like this
            ("two", vec![13..15], vec![]),
            ("three", vec![13..14, 15..16], vec![]),
            ("four", vec![13..14, 17..24], vec![]),
            ("five", vec![13..14, 21..22], vec![]),
            ("six", vec![13..14, 25..34], vec!["hecks"]),
            ("seven", vec![40..47], vec![]),
            ("eight", vec![48..49], vec![]),
            // nine and ten are function parameters split across multiple lines;
            // I assume you want the whole signature because it'll be either short enough to not be a pain
            // or long enough to need further clarification if you only see one line from it.
            ("nine", vec![13..14, 26..33], vec![]),
            ("ten", vec![13..14, 26..33], vec![]),
            ("int", vec![], vec![]),
            ("abc", vec![43..45], vec![]),
            ("xyz", vec![43..44, 45..46], vec![]),
            ("def", vec![51..53], vec![]),
            ("factorial", vec![55..57], vec!["permutations"]),
            ("permutations", vec![59..63], vec!["permutations"]),
            ("combinations", vec![65..67], vec!["factorial", "permutations"]),
            ("combinations2", vec![69..71], vec!["factorial"]),
        ];
        let config = config::Config::load_default();
        let language_info = config
            .get_language_info(config::LanguageName::Python)
            .unwrap()
            .unwrap();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(tree_sitter_python::language()).unwrap();
        let tree = parser.parse(PYTHON_SOURCE, None).unwrap();
        for (query, expect_ranges, expect_recurses) in cases {
            let pattern = regex::Regex::new(&(String::from("^") + query + "$")).unwrap();
            let (result, recurses) =
                find_definition(PYTHON_SOURCE, &tree, &language_info, &pattern, true);
            let result_vec: Vec<_> = result.iter().collect();
            assert_eq!(result_vec, expect_ranges);
            assert_eq!(recurses, expect_recurses);
        }
    }
}
