use crate::language_name::LanguageName;
use crate::{config, range_union};

pub struct ParsedFile {
    pub language_name: LanguageName,
    pub tree: tree_sitter::Tree,
}

#[derive(Debug, Clone)]
pub enum FileParseError {
    UnknownLanguage,
    UnsupportedLanguage(String),
    FailedToAttachLanguage {
        // probably version mismatch
        language_name: LanguageName,
        message: String,
    },
    UnreadableFile(String),
    EmptyStdin,
    InvalidFileRange {
        range: tree_sitter::Range,
        message: String,
    },
}

#[rustfmt::skip]
impl std::fmt::Display for FileParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileParseError::UnknownLanguage => write!(f, "unknown language"),
            FileParseError::UnsupportedLanguage(language_name)
                => write!(f, "unsupported language {:?}", language_name),
            FileParseError::FailedToAttachLanguage { language_name, message}
                => write!(f, "language {:?} incompatible with parser: {:?}", language_name, message),
            FileParseError::UnreadableFile(message) => write!(f, "{}", message),
            FileParseError::EmptyStdin => write!(f, "stdin is empty"),
            FileParseError::InvalidFileRange { range, message }
                => write!(f, "tree_sitter rejected range restriction {:?}: {}", range, message),
        }
    }
}

pub fn detect_language_from_path(path: &std::path::Path) -> Result<LanguageName, FileParseError> {
    use std::str::FromStr;
    let language_name_str = hyperpolyglot::detect(path)
        .map_err(|e| FileParseError::UnreadableFile(e.to_string()))?
        .ok_or(FileParseError::UnknownLanguage)?
        .language();
    LanguageName::from_str(language_name_str)
        .map_err(|_| FileParseError::UnsupportedLanguage(language_name_str.to_owned()))
}

#[cfg(feature = "stdin")]
pub fn detect_language_from_bytes(bytes: &[u8]) -> Result<LanguageName, FileParseError> {
    use std::str::FromStr;
    let language_name_str = hyperpolyglot::detectors::classify(
        std::str::from_utf8(bytes).map_err(|e| FileParseError::UnreadableFile(e.to_string()))?,
        &[],
    );
    LanguageName::from_str(language_name_str)
        .map_err(|_| FileParseError::UnsupportedLanguage(language_name_str.to_owned()))
}

#[cfg(not(feature = "stdin"))]
pub fn detect_language_from_bytes(_: &[u8]) -> Result<LanguageName, FileParseError> {
    Err(FileParseError::UnknownLanguage)
}

impl ParsedFile {
    pub fn from_bytes_and_language(
        source_code: &[u8],
        language_name: LanguageName,
        language: &tree_sitter::Language,
    ) -> Result<Self, FileParseError> {
        Self::from_bytes_and_language_ranged(source_code, language_name, language, None)
    }

    pub fn from_bytes_and_language_ranged(
        source_code: &[u8],
        language_name: LanguageName,
        language: &tree_sitter::Language,
        range: Option<tree_sitter::Range>,
    ) -> Result<Self, FileParseError> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(language)
            .map_err(|e| FileParseError::FailedToAttachLanguage {
                language_name,
                message: e.to_string(),
            })?;
        if let Some(range) = range {
            parser
                .set_included_ranges(&[range])
                .map_err(|e| FileParseError::InvalidFileRange {
                    range,
                    message: e.to_string(),
                })?;
        }
        let tree = parser
            .parse(source_code, None)
            .expect("parse() should have returned a tree if parser.set_language() was called");
        Ok(Self {
            language_name,
            tree,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub ranges: range_union::RangeUnion,
    pub recurse_names: Vec<String>,
    pub import_origins: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct InjectionRange {
    pub range: tree_sitter::Range,
    pub language_hint: Option<String>,
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
    let mut matches = cursor.matches(
        &language_info.definition_query.query,
        tree.root_node(),
        source_code,
    );
    while let Some(query_match) = matches.next() {
        names.extend(query_match.captures.iter().filter_map(|capture| {
            if capture.index != language_info.definition_query.index_name {
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
    let mut context_cursor = tree_sitter::QueryCursor::new();
    context_cursor.set_max_start_depth(Some(0));
    let mut matches = cursor
        .matches(
            &language_info.definition_query.query,
            tree.root_node(),
            source_code,
        )
        .filter(|query_match| {
            query_match.captures.iter().any(|capture| {
                capture.index == language_info.definition_query.index_name
                    && pattern.is_match(
                        std::str::from_utf8(&source_code[capture.node.byte_range()]).unwrap(),
                    )
            })
        });
    while let Some(query_match) = matches.next() {
        for capture in query_match
            .captures
            .iter()
            .filter(|capture| capture.index == language_info.definition_query.index_def)
        {
            let mut node = capture.node;
            ranges
                .push(node.range().start_point.row..end_point_to_end_line(node.range().end_point));
            // find names to look up for recursion
            if recurse {
                if let Some(recurse_query) = &language_info.recurse_query {
                    let mut recurse_matches =
                        recurse_cursor.matches(&recurse_query.query, node, source_code);
                    while let Some(recurse_match) = recurse_matches.next() {
                        for recurse_capture in
                            recurse_match.captures.iter().filter(|recurse_capture| {
                                recurse_capture.index == recurse_query.index_name
                            })
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
            let mut last_ambiguously_attached_sibling_range: Option<std::ops::Range<usize>> = None;
            while let Some(sibling) = node.prev_sibling() {
                if match std::num::NonZero::new(sibling.kind_id()) {
                    None => false,
                    Some(kind_id) => language_info.sibling_node_types.contains(&kind_id),
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
            if let Some(parent_query) = &language_info.parent_query {
                while let Some(parent) = node.parent() {
                    // TODO interval arithmetic
                    let mut parent_matches =
                        context_cursor.matches(&parent_query.query, parent, source_code);
                    let context_start = parent.range().start_point.row;
                    let mut context_end = parent.range().end_point;
                    let mut matched = false;
                    while let Some(parent_match) = parent_matches.next() {
                        for capture in parent_match
                            .captures
                            .iter()
                            .filter(|c| Some(c.index) == parent_query.index_exclude)
                        {
                            if let Some(prev) = capture.node.prev_sibling() {
                                context_end = context_end.min(prev.range().end_point);
                            }
                        }
                        matched = true;
                    }
                    if matched {
                        ranges.push(context_start..end_point_to_end_line(context_end));
                    }
                    node = parent;
                }
            }
        }
    }
    let mut import_origins: Vec<String> = vec![];
    if let Some(import_query) = &language_info.import_query {
        cursor
            .matches(&import_query.query, tree.root_node(), source_code)
            .filter(|query_match| {
                query_match.captures.iter().any(|capture| {
                    capture.index == import_query.index_name
                        && pattern.is_match(
                            std::str::from_utf8(&source_code[capture.node.byte_range()]).unwrap(),
                        )
                })
            })
            .for_each(|query_match| {
                import_origins.extend(
                    query_match
                        .captures
                        .iter()
                        .filter(|capture| capture.index == import_query.index_origin)
                        .map(|capture| {
                            std::str::from_utf8(&source_code[capture.node.byte_range()])
                                .unwrap()
                                .to_owned()
                        }),
                )
            });
    }
    recurse_names.sort();
    recurse_names.dedup();
    SearchResult {
        ranges,
        recurse_names,
        import_origins,
    }
}

pub fn find_injections(
    source_code: &[u8],
    tree: &tree_sitter::Tree,
    language_info: &config::LanguageInfo,
    pattern: &regex::Regex,
) -> Vec<InjectionRange> {
    use tree_sitter::StreamingIterator;
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut injections: Vec<InjectionRange> = vec![];
    if let Some(injection_query) = &language_info.injection_query {
        cursor
            .matches(&injection_query.query, tree.root_node(), source_code)
            .for_each(|query_match| {
                let pattern_index = query_match.pattern_index;
                let language_hint = match injection_query
                    .language_hints_by_pattern_index
                    .get(pattern_index)
                {
                    None => None,
                    Some(config::InjectionLanguageHint::Absent) => None,
                    Some(config::InjectionLanguageHint::Fixed(s)) => Some(s.as_ref()),
                    Some(config::InjectionLanguageHint::Capture(capture_index)) => query_match
                        .captures
                        .get(*capture_index)
                        .and_then(|c| std::str::from_utf8(&source_code[c.node.byte_range()]).ok()),
                };
                injections.extend(
                    query_match
                        .captures
                        .iter()
                        .filter(|capture| {
                            if capture.index != injection_query.index_range {
                                return false;
                            }
                            let Ok(substring) =
                                std::str::from_utf8(&source_code[capture.node.byte_range()])
                            else {
                                return false;
                            };
                            pattern.is_match(substring)
                        })
                        .map(|capture| InjectionRange {
                            range: capture.node.range(),
                            language_hint: language_hint.map(|s| s.to_owned()),
                        }),
                )
            });
    }
    injections
}
