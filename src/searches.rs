use crate::{config, range_union};

pub struct ParsedFile {
    pub language_name: config::LanguageName,
    pub source_code: std::vec::Vec<u8>,
    pub tree: tree_sitter::Tree,
}

impl ParsedFile {
    pub fn from_filename(path: &std::ffi::OsString) -> Result<ParsedFile, std::io::Error> {
        // TODO 0: add more languages
        // TODO 1: support embeds
        // TODO 2: group by language and do a second pass with language-specific regexes?
        // strings from https://github.com/monkslc/hyperpolyglot/blob/master/languages.yml
        let language_name = match hyperpolyglot::detect(std::path::Path::new(path))?
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Unsupported, format!("{:?}", path))
            })?
            .language()
        {
            "Rust" => config::LanguageName::Rust,
            "Python" => config::LanguageName::Python,
            "JavaScript" => config::LanguageName::Js,
            "TypeScript" => config::LanguageName::Ts,
            "TSX" => config::LanguageName::Tsx,
            "C" => config::LanguageName::C,
            "C++" => config::LanguageName::CPlusPlus,
            "Go" => config::LanguageName::Go,
            other_language => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    other_language,
                ))
            }
        };
        let source_code = std::fs::read(path)?;
        Self::from_bytes(source_code, language_name)
    }

    pub fn from_bytes(
        source_code: Vec<u8>,
        language_name: config::LanguageName,
    ) -> Result<ParsedFile, std::io::Error> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language_name.get_language())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let tree = parser
            .parse(&source_code, None)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, ""))?;
        Ok(ParsedFile {
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
                let mut last_ambiguously_attached_sibling_range: Option<std::ops::Range<usize>> =
                    None;
                while let Some(sibling) = node.prev_sibling() {
                    if match std::num::NonZero::new(sibling.kind_id()) {
                        None => false,
                        Some(kind_id) => language_info.sibling_patterns.contains(&kind_id),
                    } {
                        let new_sibling_range = sibling.range().start_point.row
                            ..sibling.range().end_point.row.saturating_add(1);
                        if let Some(r) = last_ambiguously_attached_sibling_range {
                            result.push(r);
                        }
                        last_ambiguously_attached_sibling_range = Some(new_sibling_range);
                        node = sibling;
                    } else {
                        if let Some(r) = last_ambiguously_attached_sibling_range {
                            if sibling.range().end_point.row.saturating_add(1) < r.end {
                                result.push(
                                    sibling.range().end_point.row.saturating_add(1).max(r.start)
                                        ..r.end,
                                );
                            }
                            last_ambiguously_attached_sibling_range = None;
                        }
                        break;
                    }
                }
                if let Some(r) = last_ambiguously_attached_sibling_range {
                    result.push(r);
                }
                // then include a header line from each relevant ancestor
                while let Some(parent) = node.parent() {
                    // TODO interval arithmetic
                    if match std::num::NonZero::new(parent.kind_id()) {
                        None => false,
                        Some(kind_id) => language_info.parent_patterns.contains(&kind_id),
                    } {
                        let context_start = parent.range().start_point.row;
                        let context_end = context_start.max(
                            language_info
                                .parent_exclusions
                                .iter()
                                .filter_map(|field_id| parent.child_by_field_id((*field_id).get()))
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

    fn verify_examples(
        language_name: config::LanguageName,
        source: &[u8],
        cases: &[(&str, Vec<std::ops::Range<usize>>, Vec<&str>)],
    ) {
        let config = config::Config::load_default();
        let language_info = config.get_language_info(language_name).unwrap().unwrap();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language_name.get_language()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        for (query, expect_ranges, expect_recurses) in cases {
            let pattern = regex::Regex::new(&(String::from("^") + query + "$")).unwrap();
            let (result, recurses) = find_definition(source, &tree, &language_info, &pattern, true);
            let result_vec: Vec<_> = result.iter().collect();
            assert_eq!(result_vec, *expect_ranges);
            assert_eq!(recurses, *expect_recurses);
        }
    }

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
            ("attr", vec![73..78], vec!["setattr"]),
        ];
        verify_examples(
            config::LanguageName::Python,
            include_bytes!("../test_cases/python.py"),
            &cases,
        );
    }

    #[test]
    fn js_examples() {
        // these ranges are 0-indexed and bat line numbers are 1-indexed so generate them with `nl -ba -v0`
        #[rustfmt::skip]
        let cases = [
            ("one", vec![0..1], vec![]),  // let
            ("two", vec![1..2], vec![]),  // const
            ("three", vec![3..6], vec![]),  // function declaration
            // old-style class, prototype shenanigans
            ("four", vec![7..10, 11..17, 20..23], vec![]),
            ("f", vec![12..15], vec![]),  // object key, bare
            ("flop", vec![12..15], vec![]),  // named function expression
            ("eff", vec![15..16], vec![]),  // object key, in quotes
            ("g", vec![20..23], vec![]),  // assign to dot-property
            ("five", vec![24..29], vec![]),  // new-style class
            ("six", vec![24..26], vec![]),  // class member variable
            ("seven", vec![24..25, 27..28], vec![]),  // getter
        ];
        verify_examples(
            config::LanguageName::Js,
            include_bytes!("../test_cases/javascript.js"),
            &cases,
        );
    }

    #[test]
    fn c_examples() {
        // these ranges are 0-indexed and bat line numbers are 1-indexed so generate them with `nl -ba -v0`
        #[rustfmt::skip]
        let cases = [
            ("ONE", vec![2..4], vec![]),  // #define, which I guess includes the line ending.
            ("two", vec![5..6], vec![]),  // static const
            ("ThreeStruct", vec![7..11], vec![]),  // struct
            ("Three", vec![7..11], vec![]),  // typedef struct; see https://stackoverflow.com/a/1675446
            ("THREE_PTR", vec![12..13], vec![]),  // typedef of pointer to struct
            ("Pint", vec![14..15], vec![]),  // typedef pointer to other stuff
            ("Quart", vec![16..20], vec![]),  // struct not in a typedef
            ("four", vec![7..9], vec![]),  // member
            ("five", vec![7..8, 9..10], vec![]),  // array
            ("six", vec![21..22], vec![]),  // unreasonable levels of pointer nesting
            ("SEVEN", vec![23..25, 33..35], vec![]),  // macro
            ("second_order", vec![25..32], vec![]),  // function definition
            ("callback", vec![25..30], vec![]),  // function pointer
            ("right", vec![25..30], vec![]),  // other function parameter
        ];
        verify_examples(
            config::LanguageName::C,
            include_bytes!("../test_cases/c.c"),
            &cases,
        );
    }
}
