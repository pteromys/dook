use crate::loader;
use crate::config::{ConfigLoader, ConfigParseError, LanguageConfig};
use crate::LanguageName;

pub struct QueryCompiler {
    config_loader: ConfigLoader,
    language_loader: loader::Loader,
    cache: std::collections::HashMap<LanguageName, Option<std::rc::Rc<LanguageInfo>>>,
}

pub struct LanguageInfo {
    pub language: tree_sitter::Language,
    pub definition_query: DefinitionQuery,
    pub sibling_node_types: std::vec::Vec<std::num::NonZero<u16>>,
    pub parent_query: Option<ParentQuery>,
    pub recurse_query: Option<RecurseQuery>,
    pub import_query: Option<ImportQuery>,
    pub injection_query: Option<InjectionQuery>,
    // stuff not exposed to config because it's too special-cased or churning
    pub name_transform: Option<Box<NameTransform>>,
}

pub type NameTransform = dyn Fn(&str) -> &str;

pub struct DefinitionQuery {
    pub query: tree_sitter::Query,
    pub index_name: u32,
    pub index_def: u32,
}

pub struct ParentQuery {
    pub query: tree_sitter::Query,
    pub index_exclude: Option<u32>,
}

pub struct RecurseQuery {
    pub query: tree_sitter::Query,
    pub index_name: u32,
}

pub struct ImportQuery {
    pub query: tree_sitter::Query,
    pub index_name: u32,
    pub index_origin: u32,
}

pub struct InjectionQuery {
    pub query: tree_sitter::Query,
    pub index_range: u32,
    pub language_hints_by_pattern_index: Vec<InjectionLanguageHint>,
}

#[derive(Clone)]
pub enum InjectionLanguageHint {
    Absent,
    Fixed(String),
    Capture(usize),
}

#[derive(Debug)]
pub enum QueryCompilerError {
    HasFailedBefore(LanguageName),
    GetLanguageInfoError(LanguageName, GetLanguageInfoError),
}

#[derive(Debug)]
pub enum GetLanguageInfoError {
    ConfigParseError(ConfigParseError),
    LanguageIsNotInConfig(LanguageName),
    ParserNotConfigured,
    LoaderError(loader::LoaderError),
    QueryCompileFailed {
        query_source: String,
        query_error: tree_sitter::QueryError,
    },
    UnrecognizedNodeType(String),
    RequiredCaptureMissing {
        query_source: String,
        capture_name: &'static str,
        config_field: &'static str,
    },
    DefinitionQueryMissing,
}

#[rustfmt::skip] // keep compact
impl std::fmt::Display for QueryCompilerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HasFailedBefore(language_name)
                => write!(f, "skipping due to previous error for language {language_name}"),
            Self::GetLanguageInfoError(language_name, e)
                => write!(f, "in {language_name}: {e}"),
        }
    }
}

#[rustfmt::skip] // keep compact
impl std::fmt::Display for GetLanguageInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigParseError(e)
                => write!(f, "failed to load config: {e}"),
            Self::LanguageIsNotInConfig(language_name)
                => write!(f, "language {language_name} not found in any config"),
            Self::ParserNotConfigured
                => write!(f, "no parser configured for language or any of its ancestors"),
            Self::LoaderError(e)
                => write!(f, "failed to load parser: {e}"),
            Self::QueryCompileFailed { query_source, query_error }
                => write!(f, "cannot compile query {:?}: {}", query_source, query_error),
            Self::UnrecognizedNodeType(node_type)
                => write!(f, "{:?} is not a node type the parser recognizes", node_type),
            Self::RequiredCaptureMissing { query_source, capture_name, config_field }
                => write!(f, "{} requires capturing @{} not found in {:?}",
                          config_field, capture_name, query_source),
            Self::DefinitionQueryMissing
                => write!(f, "no config defines definition_query for this language"),
        }
    }
}

impl QueryCompiler {
    pub fn new(config_loader: ConfigLoader, language_loader: loader::Loader) -> Self {
        Self {
            config_loader,
            language_loader,
            cache: std::collections::HashMap::new(),
        }
    }

    pub fn get_language_info(
        &mut self,
        language_name: LanguageName,
    ) -> Result<std::rc::Rc<LanguageInfo>, QueryCompilerError> {
        use std::str::FromStr;
        let parent_language = match self.cache.entry(language_name) {
            std::collections::hash_map::Entry::Occupied(entry) => {
                return entry
                    .get()
                    .clone()
                    .ok_or(QueryCompilerError::HasFailedBefore(language_name))
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                match get_language_info_uncached(
                    language_name,
                    &mut self.config_loader,
                    &mut self.language_loader,
                ) {
                    Ok(x) => {
                        let result = std::rc::Rc::new(x);
                        entry.insert(Some(result.clone()));
                        return Ok(result);
                    }
                    Err(e) => {
                        let parent = hyperpolyglot::Language::try_from(language_name.as_ref())
                            .ok()
                            .and_then(|lang_hy| lang_hy.group)
                            .and_then(|group| LanguageName::from_str(group).ok());
                        let Some(parent) = parent else {
                            entry.insert(None);
                            return Err(QueryCompilerError::GetLanguageInfoError(language_name, e));
                        };
                        log::warn!(
                            "failed to load {language_name} so falling back to {parent}: {e}"
                        );
                        parent
                    }
                }
            }
        };
        match self.get_language_info(parent_language) {
            Ok(result) => {
                self.cache.insert(language_name, Some(result.clone()));
                Ok(result)
            }
            Err(e) => {
                self.cache.insert(language_name, None);
                Err(e)
            }
        }
    }
}

fn get_language_info_uncached(
    language_name: LanguageName,
    config_loader: &mut ConfigLoader,
    language_loader: &mut loader::Loader,
) -> Result<LanguageInfo, GetLanguageInfoError> {
    let language_config = config_loader.load_config(language_name)
        .map_err(GetLanguageInfoError::ConfigParseError)?;
    let parser_source = language_config
        .parser
        .as_ref()
        .ok_or(GetLanguageInfoError::ParserNotConfigured)?;
    let language = language_loader
        .get_language(parser_source)
        .map_err(GetLanguageInfoError::LoaderError)?;
    LanguageInfo::new(language, language_name, &language_config)
}

impl LanguageInfo {
    pub fn new(
        language: tree_sitter::Language,
        language_name: LanguageName,
        config: &LanguageConfig,
    ) -> Result<Self, GetLanguageInfoError> {
        fn compile_query(
            language: &tree_sitter::Language,
            query_source: &str,
        ) -> Result<tree_sitter::Query, GetLanguageInfoError> {
            tree_sitter::Query::new(language, query_source).map_err(|e| {
                GetLanguageInfoError::QueryCompileFailed {
                    query_source: query_source.to_owned(),
                    query_error: e,
                }
            })
        }
        fn get_capture_index(
            query: &tree_sitter::Query,
            capture_name: &'static str,
            query_source: &str,
            config_field: &'static str,
        ) -> Result<u32, GetLanguageInfoError> {
            query.capture_index_for_name(capture_name).ok_or_else(|| {
                GetLanguageInfoError::RequiredCaptureMissing {
                    query_source: query_source.to_owned(),
                    capture_name,
                    config_field,
                }
            })
        }
        fn resolve_node_types<Item: AsRef<str>, II: IntoIterator<Item = Item>>(
            language: &tree_sitter::Language,
            node_type_names: II,
        ) -> Result<std::vec::Vec<std::num::NonZero<u16>>, GetLanguageInfoError> {
            node_type_names
                .into_iter()
                .map(|node_type_name| {
                    std::num::NonZero::new(language.id_for_node_kind(node_type_name.as_ref(), true))
                        .ok_or_else(|| {
                            GetLanguageInfoError::UnrecognizedNodeType(
                                node_type_name.as_ref().to_owned(),
                            )
                        })
                })
                .collect()
        }
        let definition_query = match &config.definition_query {
            None => Err(GetLanguageInfoError::DefinitionQueryMissing)?,
            Some(query_source) => {
                let query = compile_query(&language, query_source.as_ref())?;
                DefinitionQuery {
                    index_name: get_capture_index(
                        &query,
                        "name",
                        query_source.as_ref(),
                        "definition_query",
                    )?,
                    index_def: get_capture_index(
                        &query,
                        "def",
                        query_source.as_ref(),
                        "definition_query",
                    )?,
                    query,
                }
            }
        };
        let parent_query = match &config.parent_query {
            None => None,
            Some(query_source) => {
                let query = compile_query(&language, query_source.as_ref())?;
                Some(ParentQuery {
                    index_exclude: query.capture_index_for_name("exclude"),
                    query,
                })
            }
        };
        let recurse_query = match &config.recurse_query {
            None => None,
            Some(query_source) => {
                let query = compile_query(&language, query_source.as_ref())?;
                Some(RecurseQuery {
                    index_name: get_capture_index(
                        &query,
                        "name",
                        query_source.as_ref(),
                        "recurse_query",
                    )?,
                    query,
                })
            }
        };
        let import_query = match &config.import_query {
            None => None,
            Some(query_source) => {
                let query = compile_query(&language, query_source.as_ref())?;
                Some(ImportQuery {
                    index_name: get_capture_index(
                        &query,
                        "name",
                        query_source.as_ref(),
                        "import_query",
                    )?,
                    index_origin: get_capture_index(
                        &query,
                        "origin",
                        query_source.as_ref(),
                        "import_query",
                    )?,
                    query,
                })
            }
        };
        let injection_query = match &config.injection_query {
            None => None,
            Some(query_source) => {
                let query = compile_query(&language, query_source.as_ref())?;
                let mut language_hints_by_pattern_index: Vec<InjectionLanguageHint> =
                    vec![InjectionLanguageHint::Absent; query.pattern_count()];
                for (pattern_index, language_hint) in language_hints_by_pattern_index
                    .iter_mut()
                    .enumerate()
                    .take(query.pattern_count())
                {
                    for prop in query.property_settings(pattern_index) {
                        if &*prop.key == "injection.language" {
                            if let Some(value) = prop.value.as_ref() {
                                *language_hint = InjectionLanguageHint::Fixed((*value).to_string());
                            }
                            if let Some(capture_index) = prop.capture_id {
                                *language_hint = InjectionLanguageHint::Capture(capture_index);
                            }
                        }
                    }
                }
                Some(InjectionQuery {
                    index_range: get_capture_index(
                        &query,
                        "injection.content",
                        query_source.as_ref(),
                        "injection_query",
                    )?,
                    language_hints_by_pattern_index,
                    query,
                })
            }
        };
        Ok(Self {
            definition_query,
            sibling_node_types: match &config.sibling_node_types {
                None => vec![],
                Some(v) => resolve_node_types(&language, v)?,
            },
            parent_query,
            recurse_query,
            import_query,
            injection_query,
            language,
            name_transform: match language_name {
                LanguageName::TEX => Some(Box::new(|n| n.trim_start_matches("\\"))),
                LanguageName::YAML => Some(Box::new(|n| n.trim_matches(['\'', '"']))),
                _ => None,
            },
        })
    }
}
