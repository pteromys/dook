use crate::language_name::LanguageName;
use crate::{config, loader, range_union};

pub struct ParsedFile {
    pub language_name: LanguageName,
    pub source_code: std::vec::Vec<u8>,
    pub tree: tree_sitter::Tree,
}

impl ParsedFile {
    pub fn from_filename(
        path: &std::ffi::OsString,
        language_loader: &mut loader::Loader,
        config: &config::Config,
    ) -> Result<ParsedFile, std::io::Error> {
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
            "Rust" => LanguageName::Rust,
            "Python" => LanguageName::Python,
            "JavaScript" => LanguageName::Js,
            "TypeScript" => LanguageName::Ts,
            "TSX" => LanguageName::Tsx,
            "C" => LanguageName::C,
            "C++" => LanguageName::CPlusPlus,
            "Go" => LanguageName::Go,
            other_language => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    other_language,
                ))
            }
        };
        let language = language_loader
            .get_language(config.get_parser_source(language_name).unwrap())
            .unwrap()
            .unwrap();
        let source_code = std::fs::read(path)?;
        Self::from_bytes(source_code, language_name, &language)
    }

    pub fn from_bytes(
        source_code: Vec<u8>,
        language_name: LanguageName,
        language: &tree_sitter::Language,
    ) -> Result<ParsedFile, std::io::Error> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(language)
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

pub fn end_point_to_end_line(p: tree_sitter::Point) -> usize {
    if p.column == 0 {
        p.row
    } else {
        p.row.saturating_add(1)
    }
}

pub fn find_definition(
    source_code: &[u8],
    tree: &tree_sitter::Tree,
    language_info: &config::LanguageInfo,
    pattern: &regex::Regex,
    recurse: bool,
) -> (range_union::RangeUnion, std::vec::Vec<String>) {
    use tree_sitter::StreamingIterator;
    let mut result: range_union::RangeUnion = Default::default();
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut recurse_cursor = tree_sitter::QueryCursor::new();
    let mut recurse_names: std::vec::Vec<String> = std::vec::Vec::new();
    //let mut context_cursor = tree_sitter::QueryCursor::new();
    //context_cursor.set_max_start_depth(0);
    for node_query in language_info.match_patterns.iter() {
        let name_idx = node_query.capture_index_for_name("name").unwrap();
        let def_idx = node_query.capture_index_for_name("def").unwrap();
        let mut matches = cursor
            .matches(node_query, tree.root_node(), source_code)
            .filter(|query_match| {
                query_match.captures.iter().any(|capture| {
                    capture.index == name_idx
                        && pattern.is_match(
                            std::str::from_utf8(&source_code[capture.node.byte_range()]).unwrap(),
                        )
                })
            });
        while let Some(query_match) = matches.next() {
            for capture in query_match
                .captures
                .iter()
                .filter(|capture| capture.index == def_idx)
            {
                let mut node = capture.node;
                result.push(
                    node.range().start_point.row..end_point_to_end_line(node.range().end_point),
                );
                // find names to look up for recursion
                if recurse {
                    for recurse_query in language_info.recurse_patterns.iter() {
                        let recurse_name_idx = node_query.capture_index_for_name("name").unwrap();
                        let mut recurse_matches =
                            recurse_cursor.matches(recurse_query, node, source_code);
                        while let Some(recurse_match) = recurse_matches.next() {
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
                            ..end_point_to_end_line(sibling.range().end_point);
                        if let Some(r) = last_ambiguously_attached_sibling_range {
                            result.push(r);
                        }
                        last_ambiguously_attached_sibling_range = Some(new_sibling_range);
                        node = sibling;
                    } else {
                        if let Some(r) = last_ambiguously_attached_sibling_range {
                            let sibling_end_line = end_point_to_end_line(sibling.range().end_point);
                            if sibling_end_line < r.end {
                                result.push(sibling_end_line.max(r.start)..r.end);
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
                        let context_end = language_info
                            .parent_exclusions
                            .iter()
                            .filter_map(|field_id| {
                                parent
                                    .child_by_field_id((*field_id).get())
                                    .and_then(|c| c.prev_sibling())
                            })
                            .map(|c| c.range().end_point)
                            .min()
                            .unwrap_or(parent.range().end_point);
                        result.push(context_start..end_point_to_end_line(context_end));
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
