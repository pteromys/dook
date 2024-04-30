// Prior art:
//     symbex
//     git grep -W
//     https://dandavison.github.io/delta/grep.html
//     https://docs.github.com/en/repositories/working-with-files/using-files/navigating-code-on-github#precise-and-search-based-navigation

extern crate bytes;
extern crate clap;

mod dumptree;
mod paging;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum EnablementLevel {
    Auto,
    Never,
    Always,
}

impl Default for EnablementLevel {
    fn default() -> Self {
        EnablementLevel::Auto
    }
}

#[derive(clap::Parser, Debug)]
/// Find a definition.
struct Cli {
    #[arg(long, value_enum, default_value_t)]
    color: EnablementLevel,

    #[arg(long, value_enum, default_value_t)]
    paging: EnablementLevel,

    /// Apply no styling; specify twice to also disable paging.
    #[arg(short, long, action = clap::ArgAction::Count)]
    plain: u8,

    /// Regex to match against symbol names.
    pattern: regex::Regex,

    /// Dump the syntax tree of every matched file, for debugging extraction queries.
    #[arg(long)]
    dump: bool,
}

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
enum LanguageName {
    RUST,
    PYTHON,
    TS,
    TSX,
}

struct LanguageInfo {
    language: tree_sitter::Language,
    match_patterns: std::vec::Vec<tree_sitter::Query>,
    sibling_patterns: std::vec::Vec<u16>,
    parent_patterns: std::vec::Vec<u16>,
    parent_exclusions: std::vec::Vec<u16>,
}

impl LanguageInfo {
    pub fn new<
        Item: AsRef<str>,
        I1: IntoIterator<Item = Item>,
        I2: IntoIterator<Item = Item>,
        I3: IntoIterator<Item = Item>,
        I4: IntoIterator<Item = Item>,
    >(
        language: tree_sitter::Language,
        match_patterns: I1,
        sibling_patterns: I2,
        parent_patterns: I3,
        parent_exclusions: I4,
    ) -> Result<Self, tree_sitter::QueryError> {
        fn compile_queries<Item: AsRef<str>, II: IntoIterator<Item = Item>>(
            language: tree_sitter::Language,
            sources: II,
        ) -> Result<std::vec::Vec<tree_sitter::Query>, tree_sitter::QueryError> {
            let mut last_error: std::option::Option<tree_sitter::QueryError> = None;
            let result = sources
                .into_iter()
                .map_while(
                    |source| match tree_sitter::Query::new(language, source.as_ref()) {
                        Ok(q) => Some(q),
                        Err(e) => {
                            last_error = Some(e);
                            None
                        }
                    },
                )
                .collect();
            match last_error {
                Some(e) => Err(e),
                None => Ok(result),
            }
        }
        fn resolve_node_types<Item: AsRef<str>, II: IntoIterator<Item = Item>>(
            language: tree_sitter::Language,
            node_type_names: II,
        ) -> Result<std::vec::Vec<u16>, tree_sitter::QueryError> {
            let mut last_unresolved: std::option::Option<String> = None;
            let result = node_type_names
                .into_iter()
                .map_while(|node_type_name| {
                    match language.id_for_node_kind(node_type_name.as_ref(), true) {
                        0 => {
                            last_unresolved = Some(String::from(node_type_name.as_ref()));
                            None
                        }
                        n => Some(n),
                    }
                })
                .collect();
            match last_unresolved {
                Some(e) => Err(tree_sitter::QueryError {
                    row: 0,
                    column: 0,
                    offset: 0,
                    message: format!("unknown node type: {:?}", e),
                    kind: tree_sitter::QueryErrorKind::NodeType,
                }),
                None => Ok(result),
            }
        }
        fn resolve_field_names<Item: AsRef<str>, II: IntoIterator<Item = Item>>(
            language: tree_sitter::Language,
            field_names: II,
        ) -> Result<std::vec::Vec<u16>, tree_sitter::QueryError> {
            let mut last_unresolved: std::option::Option<String> = None;
            let result = field_names
                .into_iter()
                .map_while(
                    |field_name| match language.field_id_for_name(field_name.as_ref()) {
                        None => {
                            last_unresolved = Some(String::from(field_name.as_ref()));
                            None
                        }
                        Some(n) => Some(n),
                    },
                )
                .collect();
            match last_unresolved {
                Some(e) => Err(tree_sitter::QueryError {
                    row: 0,
                    column: 0,
                    offset: 0,
                    message: format!("unknown field name: {:?}", e),
                    kind: tree_sitter::QueryErrorKind::Field,
                }),
                None => Ok(result),
            }
        }
        let match_patterns = compile_queries(language, match_patterns);
        let sibling_patterns = resolve_node_types(language, sibling_patterns);
        let parent_patterns = resolve_node_types(language, parent_patterns);
        let parent_exclusions = resolve_field_names(language, parent_exclusions);
        match (
            match_patterns,
            sibling_patterns,
            parent_patterns,
            parent_exclusions,
        ) {
            (
                Ok(match_patterns),
                Ok(sibling_patterns),
                Ok(parent_patterns),
                Ok(parent_exclusions),
            ) => Ok(Self {
                language,
                match_patterns,
                sibling_patterns,
                parent_patterns,
                parent_exclusions,
            }),
            (Err(e), _, _, _) => Err(e),
            (_, Err(e), _, _) => Err(e),
            (_, _, Err(e), _) => Err(e),
            (_, _, _, Err(e)) => Err(e),
        }
    }
}

// TODO 2: support fenced code blocks in markdown and rst
//     likely to require regrouping
fn get_language_info(language_name: LanguageName) -> Result<LanguageInfo, tree_sitter::QueryError> {
    match language_name {
        LanguageName::RUST => LanguageInfo::new(
            tree_sitter_rust::language(),
            [
                "[
                    (function_item name: (_) @name)
                    (function_signature_item name: (_) @name)
                    (let_declaration pattern: [
                        (identifier) @name
                    ])
                    (const_item name: (_) @name)
                    (enum_item name: (_) @name)
                    (impl_item type: (_) @name)
                    (impl_item trait: (_) @name)
                    (impl_item trait: (generic_type type: (_) @name))
                    (impl_item trait: (generic_type type: (scoped_identifier name: (_) @name)))
                    (impl_item trait: (generic_type type: (scoped_type_identifier name: (_) @name)))
                    (impl_item trait: (scoped_type_identifier name: (_) @name))
                    (macro_definition name: (_) @name)
                    (mod_item name: (_) @name)
                    (static_item name: (_) @name)
                    (struct_item name: (_) @name)
                    (trait_item name: (_) @name)
                    (type_item name: (_) @name)
                    (union_item name: (_) @name)
                ] @def",
                // TODO tree_sitter 0.22 will support alternation of node types, allowing better concision:
                //"([function_item function_signature_item attribute_item inner_attribute_item let_declaration const_item enum_item impl_item macro_definition mod_item static_item struct_item trait_item type_item union_item]
                //    name: (_) @name) @def",
            ],
            ["line_comment", "block_comment"],
            ["function_item", "impl_item"],
            ["body"],
        ),
        LanguageName::PYTHON => LanguageInfo::new(
            tree_sitter_python::language(),
            ["[
                    (class_definition name: (_) @name) @def
                    (function_definition name: (_) @name) @def
                    (assignment left: (_) @name) @def
                    (parameters (identifier) @name @def)
                    (lambda_parameters (identifier) @name @def)
                    (typed_parameter . (identifier) @name) @def
                    (default_parameter name: (_) @name) @def
                    (typed_default_parameter name: (_) @name) @def
                ]"],
            ["decorator", "comment"],
            ["class_definition", "function_definition"],
            ["body"],
        ),
        LanguageName::TS => LanguageInfo::new(
            tree_sitter_typescript::language_typescript(),
            [
                "[
                    (function_signature name: (_) @name)
                    (method_signature name: (_) @name)
                    (abstract_method_signature name: (_) @name)
                    (abstract_class_declaration name: (_) @name)
                    (module name: (_) @name)
                    (type_alias_declaration name: (_) @name)
                    (interface_declaration name: (_) @name)
                ] @def",
                // TODO tree_sitter 0.22
                //"([function_signature method_signature abstract_method_signature abstract_class_declaration module interface_declaration]
                //    name: (_) @name) @def",
            ],
            ["comment"],
            [],
            [],
        ),
        LanguageName::TSX => LanguageInfo::new(
            tree_sitter_typescript::language_tsx(),
            [
                "[
                    (function_signature name: (_) @name)
                    (function_declaration name: (_) @name)
                    (method_signature name: (_) @name)
                    (method_definition name: (_) @name)
                    (abstract_method_signature name: (_) @name)
                    (abstract_class_declaration name: (_) @name)
                    (module name: (_) @name)
                    (variable_declarator name: (_) @name)
                    (class_declaration name: (_) @name)
                    (type_alias_declaration name: (_) @name)
                    (interface_declaration name: (_) @name)
                ] @def",
                // TODO tree_sitter 0.22
                //"([function_signature method_signature abstract_method_signature abstract_class_declaration module interface_declaration]
                //    name: (_) @name) @def"
            ],
            ["comment"],
            [
                "function_declaration",
                "method_definition",
                "class_declaration",
            ],
            ["body"],
        ),
    }
}

fn main() -> std::io::Result<std::process::ExitCode> {
    use clap::Parser;
    use std::io::Write;

    // grab cli args
    let cli = Cli::parse();
    let local_pattern =
        regex::Regex::new(&(String::from("^") + cli.pattern.as_str() + "$")).unwrap();

    // first-pass search with ripgrep
    let mut rg = std::process::Command::new("rg");
    let filenames = match rg
        .arg("-l")
        .arg("-0")
        .arg(cli.pattern.as_str())
        .arg("./")
        .stderr(std::process::Stdio::inherit())
        .output()
    {
        Err(e) => return Err(e),
        Ok(output) => {
            if !output.status.success() {
                if let Some(e) = output.status.code() {
                    return Ok(std::process::ExitCode::from(e as u8)); // truncate to 8 bits
                }
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("{}", output.status),
                ));
            }
            output
                .stdout
                .split(|x| *x == 0)
                .map(bytes::Bytes::copy_from_slice)
                .filter(|f| f.len() > 0)
                .collect::<std::vec::Vec<bytes::Bytes>>()
        }
    };

    // infer syntax, then search with tree_sitter
    // TODO 0: add more languages
    // TODO 1: sniff syntax by content
    //     maybe https://github.com/sharkdp/bat/blob/master/src/syntax_mapping.rs
    let mut print_ranges: std::collections::HashMap<
        bytes::Bytes,
        std::vec::Vec<std::ops::Range<usize>>,
    > = std::collections::HashMap::new();
    for path in filenames {
        let language_name = if path.ends_with(b".rs") {
            LanguageName::RUST
        } else if path.ends_with(b".py") {
            LanguageName::PYTHON
        } else if path.ends_with(b".ts") {
            LanguageName::TS
        } else if path.ends_with(b".tsx") {
            LanguageName::TSX
        } else {
            continue;
        };
        let language_info = get_language_info(language_name);
        match language_info {
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{}", e),
                ));
            }
            Ok(language_info) => {
                let mut parser = tree_sitter::Parser::new();
                parser.set_language(language_info.language).expect("Darn");
                let source_code = std::fs::read(String::from_utf8(path.to_vec()).unwrap())?;
                let tree = parser.parse(&source_code, None).unwrap();
                if cli.dump {
                    dumptree::dump_tree(&tree, source_code.as_slice());
                } else {
                    let mut cursor = tree_sitter::QueryCursor::new();
                    //let mut context_cursor = tree_sitter::QueryCursor::new();
                    //context_cursor.set_max_start_depth(0);
                    // TODO merge tree with ripgrep results more efficiently
                    for node_query in language_info.match_patterns {
                        let name_idx = node_query.capture_index_for_name("name").unwrap();
                        let def_idx = node_query.capture_index_for_name("def").unwrap();
                        for query_match in cursor
                            .matches(&node_query, tree.root_node(), source_code.as_slice())
                            .filter(|query_match| {
                                query_match.captures.iter().any(|capture| {
                                    capture.index == name_idx
                                        && local_pattern.is_match(
                                            std::str::from_utf8(
                                                &source_code[capture.node.byte_range()],
                                            )
                                            .unwrap(),
                                        )
                                })
                            })
                        {
                            for capture in query_match
                                .captures
                                .iter()
                                .filter(|capture| capture.index == def_idx)
                            {
                                let target_ranges = print_ranges
                                    .entry(path.clone())
                                    .or_insert_with(std::vec::Vec::new);
                                target_ranges.push(
                                    capture.node.range().start_point.row
                                        ..capture.node.range().end_point.row,
                                );
                                let mut node = capture.node;
                                loop {
                                    if let Some(sibling) = node.prev_sibling() {
                                        if language_info
                                            .sibling_patterns
                                            .contains(&sibling.kind_id())
                                        {
                                            target_ranges.push(
                                                sibling.range().start_point.row
                                                    ..sibling.range().end_point.row,
                                            );
                                            node = sibling;
                                            continue;
                                        }
                                    }
                                    if let Some(parent) = node.parent() {
                                        // TODO interval arithmetic
                                        if language_info.parent_patterns.contains(&parent.kind_id())
                                        {
                                            let context_start = parent.range().start_point.row;
                                            let context_end = context_start.max(
                                                language_info
                                                    .parent_exclusions
                                                    .iter()
                                                    .filter_map(|field_id| {
                                                        parent.child_by_field_id(*field_id)
                                                    })
                                                    .map(|c| {
                                                        c.range().start_point.row.saturating_sub(1)
                                                    })
                                                    .min()
                                                    .unwrap_or(parent.range().end_point.row),
                                            );
                                            target_ranges.push(context_start..context_end);
                                        }
                                        node = parent;
                                    } else {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // set up paging if requested
    let enable_paging = if cli.paging != EnablementLevel::Auto {
        cli.paging == EnablementLevel::Always
    } else {
        cli.plain < 2 && console::Term::stdout().is_term()
    };
    let mut pager = paging::MaybePager::new(enable_paging);
    let bat_color = if cli.color != EnablementLevel::Auto {
        cli.color
    } else if console::colors_enabled() {
        EnablementLevel::Always
    } else {
        EnablementLevel::Never
    };
    let bat_size = console::Term::stdout().size_checked();
    for (path, ranges) in print_ranges.iter() {
        let mut cmd = std::process::Command::new("bat");
        let cmd = cmd
            .arg("--paging=never")
            .arg(format!("--color={:?}", bat_color).to_lowercase());
        let cmd = match bat_size {
            Some((_rows, cols)) => cmd.arg(format!("--terminal-width={}", cols)),
            None => cmd,
        };
        let cmd = cmd
            .args(
                ranges
                    .into_iter()
                    .map(|x| format!("--line-range={}:{}", x.start + 1, x.end + 1)),
            )
            .arg(std::str::from_utf8(path).unwrap());
        let output = cmd.stderr(std::process::Stdio::inherit()).output().unwrap();
        if let Err(e) = pager.write_all(&output.stdout) {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                // stdout is gone so let's just leave quietly
                return Ok(std::process::ExitCode::SUCCESS);
            }
            break;
        }
    }
    // wait for pager
    match pager.wait() {
        Ok(0) => (),
        Ok(status) => println!("Pager exited {}", status),
        Err(e) => println!("Pager died or vanished: {}", e),
    }

    // yeah yeah whatever
    Ok(std::process::ExitCode::SUCCESS)
}
