use crate::language_name::LanguageName;
use crate::{config, loader, range_union};

pub struct ParsedFile {
    pub path: Option<std::path::PathBuf>,
    pub language_name: LanguageName,
    pub language_name_str: String,
    pub source_code: std::vec::Vec<u8>,
    pub tree: tree_sitter::Tree,
}

impl ParsedFile {
    pub fn from_filename(
        path: &std::path::Path,
        language_loader: &mut loader::Loader,
        config: &config::Config,
    ) -> Result<ParsedFile, std::io::Error> {
        // TODO 0: add more languages
        // TODO 1: support embeds
        // TODO 2: group by language and do a second pass with language-specific regexes?
        // strings from https://github.com/monkslc/hyperpolyglot/blob/master/languages.yml
        let language_name_str = hyperpolyglot::detect(path)?
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Unsupported, format!("{:?}", path))
            })?
            .language();
        let source_code = std::fs::read(path)?;
        let result = Self::from_bytes_and_language_name(
            source_code,
            language_name_str,
            language_loader,
            config,
        );
        result.map(|mut f| {
            f.path = Some(path.to_owned());
            f
        })
    }

    #[cfg(feature = "stdin")]
    pub fn from_bytes(
        source_code: Vec<u8>,
        language_loader: &mut loader::Loader,
        config: &config::Config,
    ) -> Result<ParsedFile, std::io::Error> {
        use core::str;
        let language_name_str = hyperpolyglot::detectors::classify(
            str::from_utf8(&source_code)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Unsupported, e))?,
            &[],
        );
        Self::from_bytes_and_language_name(source_code, language_name_str, language_loader, config)
    }

    fn from_bytes_and_language_name(
        source_code: Vec<u8>,
        language_name_str: &str,
        language_loader: &mut loader::Loader,
        config: &config::Config,
    ) -> Result<ParsedFile, std::io::Error> {
        let Some(language_name) = LanguageName::from_hyperpolyglot(language_name_str) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                language_name_str,
            ));
        };
        let language = language_loader
            .get_language(config.get_parser_source(language_name).unwrap())
            .unwrap()
            .unwrap();
        let result = Self::from_bytes_and_language(source_code, language_name, &language);
        result.map(|mut f| {
            f.language_name_str = language_name_str.to_owned();
            f
        })
    }

    pub fn from_bytes_and_language(
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
            path: None,
            language_name,
            language_name_str: format!("{:?}", language_name),
            source_code,
            tree,
        })
    }
}

pub struct SearchResult {
    pub ranges: range_union::RangeUnion,
    pub recurse_names: Vec<String>,
}

pub fn end_point_to_end_line(p: tree_sitter::Point) -> usize {
    if p.column == 0 {
        p.row
    } else {
        p.row.saturating_add(1)
    }
}

pub fn find_names(
    source_code: &[u8],
    tree: &tree_sitter::Tree,
    language_info: &config::LanguageInfo,
    pattern: &regex::Regex,
) -> Vec<String> {
    use tree_sitter::StreamingIterator;
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut names: std::vec::Vec<String> = std::vec::Vec::new();
    for node_query in language_info.match_patterns.iter() {
        let name_idx = node_query.capture_index_for_name("name").unwrap();
        let mut matches = cursor.matches(node_query, tree.root_node(), source_code);
        while let Some(query_match) = matches.next() {
            names.extend(query_match.captures.iter().filter_map(|capture| {
                if capture.index != name_idx {
                    return None;
                }
                let name = std::str::from_utf8(&source_code[capture.node.byte_range()])
                    .unwrap()
                    .to_owned();
                if pattern.is_match(&name) {
                    Some(name)
                } else {
                    None
                }
            }));
        }
    }
    names.dedup(); // lol idk
    names.sort();
    names.dedup();
    names
}

pub fn find_definition(
    source_code: &[u8],
    tree: &tree_sitter::Tree,
    language_info: &config::LanguageInfo,
    pattern: &regex::Regex,
    recurse: bool,
) -> SearchResult {
    use tree_sitter::StreamingIterator;
    let mut ranges: range_union::RangeUnion = Default::default();
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
                ranges.push(
                    node.range().start_point.row..end_point_to_end_line(node.range().end_point),
                );
                // find names to look up for recursion
                if recurse {
                    for recurse_query in language_info.recurse_patterns.iter() {
                        let recurse_name_idx =
                            recurse_query.capture_index_for_name("name").unwrap();
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
                            ranges.push(r);
                        }
                        last_ambiguously_attached_sibling_range = Some(new_sibling_range);
                        node = sibling;
                    } else {
                        if let Some(r) = last_ambiguously_attached_sibling_range {
                            let sibling_end_line = end_point_to_end_line(sibling.range().end_point);
                            if sibling_end_line < r.end {
                                ranges.push(sibling_end_line.max(r.start)..r.end);
                            }
                            last_ambiguously_attached_sibling_range = None;
                        }
                        break;
                    }
                }
                if let Some(r) = last_ambiguously_attached_sibling_range {
                    ranges.push(r);
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
                        ranges.push(context_start..end_point_to_end_line(context_end));
                    }
                    node = parent;
                }
            }
        }
    }
    recurse_names.sort();
    recurse_names.dedup();
    SearchResult {
        ranges,
        recurse_names,
    }
}
