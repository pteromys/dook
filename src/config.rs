use crate::language_name::LanguageName;
use crate::loader;

const DEFAULT_CONFIG: &str = include_str!("dook.yml");

pub fn default_config_path() -> Option<std::path::PathBuf> {
    use etcetera::AppStrategy;
    dirs().map(|d| d.config_dir().join("dook.yml")).ok()
}

pub fn dirs() -> Result<impl etcetera::AppStrategy, etcetera::HomeDirError> {
    etcetera::choose_app_strategy(etcetera::AppStrategyArgs {
        top_level_domain: "com".to_string(),
        author: "melonisland".to_string(),
        app_name: "dook".to_string(),
    })
}

#[derive(Clone, Debug, PartialEq)]
struct MultiLineString(String);

impl AsRef<str> for MultiLineString {
    fn as_ref(&self) -> &str {
        let Self(inner) = self;
        inner
    }
}

impl From<&MultiLineString> for String {
    fn from(mls: &MultiLineString) -> Self {
        let MultiLineString(inner) = mls;
        inner.clone()
    }
}

impl<'de> merde::Deserialize<'de> for MultiLineString {
    async fn deserialize(
        de: &mut dyn merde::DynDeserializer<'de>,
    ) -> Result<Self, merde::MerdeError<'de>> {
        match de.next().await? {
            merde::Event::Str(v) => Ok(MultiLineString(String::from(v))),
            merde::Event::ArrayStart(_) => {
                let mut vs: Vec<String> = Vec::new();
                loop {
                    match de.next().await? {
                        merde::Event::ArrayEnd => break,
                        merde::Event::Str(v) => vs.push(String::from(v)),
                        ev => Err(merde::MerdeError::UnexpectedEvent {
                            got: merde::EventType::from(&ev),
                            expected: &[merde::EventType::Str],
                            help: Some(String::from(
                                "multiline string must be a string or an array of strings",
                            )),
                        })?,
                    }
                }
                Ok(MultiLineString(vs.join("\n")))
            }
            ev => Err(merde::MerdeError::UnexpectedEvent {
                got: merde::EventType::from(&ev),
                expected: &[merde::EventType::Str, merde::EventType::ArrayStart],
                help: Some(String::from(
                    "multiline string must be a string or an array of strings",
                )),
            })?,
        }
    }
}

#[derive(Debug, PartialEq)]
struct LanguageConfigV1 {
    parser: Option<loader::ParserSource>,
    match_patterns: std::vec::Vec<MultiLineString>,
    sibling_patterns: std::vec::Vec<String>,
    parent_patterns: std::vec::Vec<String>,
    parent_exclusions: std::vec::Vec<String>,
    recurse_patterns: Option<std::vec::Vec<MultiLineString>>,
    import_patterns: Option<std::vec::Vec<MultiLineString>>,
    comments: Option<Vec<String>>,
}

merde::derive! {
    impl (Deserialize) for struct LanguageConfigV1 { parser, match_patterns, sibling_patterns, parent_patterns, parent_exclusions, recurse_patterns, import_patterns, comments }
}

#[derive(Debug, PartialEq)]
struct ConfigV1(std::collections::HashMap<String, LanguageConfigV1>);

merde::derive! {
    impl (Deserialize) for struct ConfigV1 transparent
}

#[derive(Debug, PartialEq)]
struct ConfigV2 {
    version: u64,
    languages: std::collections::HashMap<String, LanguageConfigV1>,
}

#[derive(Debug, PartialEq, Default)]
pub struct LanguageConfigV3 {
    parser: Option<loader::ParserSource>,
    extends: Option<String>,
    definition_query: Option<String>,
    sibling_node_types: Option<std::vec::Vec<String>>,
    parent_query: Option<String>,
    recurse_query: Option<String>,
    import_query: Option<String>,
    injection_query: Option<String>,
}

merde::derive! {
    impl (Deserialize) for struct LanguageConfigV3 { parser, extends, definition_query, sibling_node_types, parent_query, recurse_query, import_query, injection_query }
}

#[derive(Debug, PartialEq)]
struct ConfigV3 {
    version: u64,
    languages: std::collections::HashMap<String, LanguageConfigV3>,
}

merde::impl_into_static!(struct ConfigV3 { version, languages });

fn join_strs(v: Vec<String>, sep: &str) -> String {
    v.iter()
        .flat_map(|s| [sep, s].into_iter())
        .skip(1)
        .collect()
}

impl From<ConfigV1> for ConfigV2 {
    fn from(value: ConfigV1) -> Self {
        let ConfigV1(language_map) = value;
        Self {
            version: 1,
            languages: language_map,
        }
    }
}

impl From<ConfigV2> for ConfigV3 {
    fn from(value: ConfigV2) -> Self {
        Self {
            version: value.version,
            languages: value
                .languages
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
        }
    }
}

impl From<LanguageConfigV1> for LanguageConfigV3 {
    fn from(value: LanguageConfigV1) -> Self {
        Self {
            parser: value.parser,
            extends: None,
            definition_query: match value.match_patterns.len() {
                0 => None,
                _ => Some(join_strs(
                    value.match_patterns.iter().map(|s| s.into()).collect(),
                    "\n",
                )),
            },
            sibling_node_types: Some(value.sibling_patterns),
            parent_query: match value.parent_patterns.len() {
                0 => None,
                _ => Some(join_strs(
                    value
                        .parent_patterns
                        .iter()
                        .map(|node_name| format!("({})", node_name))
                        .collect(),
                    "\n",
                )),
            },
            recurse_query: value
                .recurse_patterns
                .map(|v| join_strs(v.iter().map(|s| s.into()).collect(), "\n")),
            import_query: value
                .import_patterns
                .map(|v| join_strs(v.iter().map(|s| s.into()).collect(), "\n")),
            injection_query: None,
        }
    }
}

impl<'de> merde::Deserialize<'de> for ConfigV2 {
    async fn deserialize(
        de: &mut dyn merde::DynDeserializer<'de>,
    ) -> Result<Self, merde::MerdeError<'de>> {
        use merde::DynDeserializerExt;
        let mut result = ConfigV2 {
            version: 2,
            languages: std::collections::HashMap::new(),
        };
        de.next().await?.into_map_start()?;
        loop {
            match de.next().await? {
                merde::Event::Str(key) => {
                    if key == "_version" {
                        result.version = u64::try_from(de.next().await?.into_i64()?)
                            .map_err(|_| merde::MerdeError::OutOfRange)?;
                    } else {
                        result.languages.insert(key.to_string(), de.t().await?);
                    }
                }
                merde::Event::MapEnd => return Ok(result),
                e => {
                    return Err(merde::MerdeError::UnexpectedEvent {
                        got: merde::EventType::from(&e),
                        expected: &[merde::EventType::Str],
                        help: None,
                    })
                }
            }
        }
    }
}

impl<'de> merde::Deserialize<'de> for ConfigV3 {
    async fn deserialize(
        de: &mut dyn merde::DynDeserializer<'de>,
    ) -> Result<Self, merde::MerdeError<'de>> {
        use merde::DynDeserializerExt;
        let mut result = ConfigV3 {
            version: 3,
            languages: std::collections::HashMap::new(),
        };
        de.next().await?.into_map_start()?;
        loop {
            match de.next().await? {
                merde::Event::Str(key) => {
                    if key == "_version" {
                        result.version = u64::try_from(de.next().await?.into_i64()?)
                            .map_err(|_| merde::MerdeError::OutOfRange)?;
                    } else {
                        result.languages.insert(key.to_string(), de.t().await?);
                    }
                }
                merde::Event::MapEnd => return Ok(result),
                e => {
                    return Err(merde::MerdeError::UnexpectedEvent {
                        got: merde::EventType::from(&e),
                        expected: &[merde::EventType::Str],
                        help: None,
                    })
                }
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ConfigFormat {
    V1,
    V2,
    V3,
}

impl<'de> merde::Deserialize<'de> for ConfigFormat {
    async fn deserialize(
        de: &mut dyn merde::DynDeserializer<'de>,
    ) -> Result<Self, merde::MerdeError<'de>> {
        use merde::DynDeserializerExt;
        de.next().await?.into_map_start()?;
        loop {
            match de.next().await? {
                merde::Event::Str(key) => {
                    if key == "_version" {
                        return match de.next().await?.into_i64()? {
                            2 => Ok(ConfigFormat::V2),
                            3 => Ok(ConfigFormat::V3),
                            _ => Err(merde::MerdeError::OutOfRange),
                        };
                    }
                    let _: merde::Value<'de> = de.t().await?;
                }
                merde::Event::MapEnd => return Ok(ConfigFormat::V1),
                _ => break,
            }
        }
        Err(merde::MerdeError::MissingProperty(
            merde::CowStr::copy_from_str("_version"),
        ))
    }
}

#[derive(Debug, PartialEq)]
pub struct Config {
    version: ConfigFormat,
    languages: std::collections::HashMap<LanguageName, LanguageConfigV3>,
}

#[derive(Debug)]
pub enum ConfigParseError {
    Deserialize(merde::MerdeError<'static>),
    UnknownLanguage(String),
    NotUtf8(std::str::Utf8Error),
    UnreadableFile(std::io::Error),
}

impl From<merde::MerdeError<'_>> for ConfigParseError {
    fn from(value: merde::MerdeError<'_>) -> Self {
        use merde::IntoStatic;
        Self::Deserialize(value.into_static())
    }
}

impl From<std::str::Utf8Error> for ConfigParseError {
    fn from(value: std::str::Utf8Error) -> Self {
        Self::NotUtf8(value)
    }
}

#[rustfmt::skip]
impl std::fmt::Display for ConfigParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deserialize(e)
                => write!(f, "{}", e),
            Self::UnknownLanguage(language)
                => write!(f, "unknown language: {}", language),
            Self::NotUtf8(e)
                => write!(f, "{}", e),
            Self::UnreadableFile(e)
                => write!(f, "{}", e),
        }
    }
}

impl TryFrom<ConfigV3> for Config {
    type Error = ConfigParseError;
    fn try_from(value: ConfigV3) -> Result<Self, Self::Error> {
        use std::str::FromStr;
        let mut languages: std::collections::HashMap<LanguageName, LanguageConfigV3> =
            std::collections::HashMap::new();
        for (language_name, config) in value.languages {
            if let Ok(language_name) = LanguageName::from_str(language_name.as_ref()) {
                languages.insert(language_name, config);
                continue;
            }
            if value.version <= 2 {
                if let Ok(language_name) = LanguageName::from_legacy(language_name.as_ref()) {
                    languages.insert(language_name, config);
                    continue;
                }
            }
            return Err(ConfigParseError::UnknownLanguage(language_name));
        }
        Ok(Self {
            version: ConfigFormat::V3,
            languages,
        })
    }
}

impl Config {
    /// Used by integration tests for limited access to private self.languages
    #[allow(unused)]
    pub fn configured_languages(&self) -> impl Iterator<Item = &LanguageName> {
        self.languages.keys()
    }

    pub fn load(
        explicit_path: &Option<impl AsRef<std::path::Path>>,
    ) -> Result<Option<Self>, ConfigParseError> {
        let config_bytes = match explicit_path {
            // explicitly requested file paths expose any errors reading
            Some(p) => std::fs::read(p.as_ref()).map_err(ConfigParseError::UnreadableFile)?,
            // the default file path is more forgiving
            None => match default_config_path() {
                None => return Ok(None), // if there's no default path, just return None
                Some(default_path) => match std::fs::read(&default_path) {
                    // unwrap the contents if we successfully read it
                    Ok(contents) => contents,
                    Err(e) => {
                        // silently eat NotFound---user never created config file
                        // log other errors but don't let them stop us from trying to work in a degraded environment
                        if e.kind() != std::io::ErrorKind::NotFound {
                            log::warn!("Error reading config at {:?}, falling back to built-in default: {:?}", default_path, e);
                        }
                        return Ok(None);
                    }
                },
            },
        };
        let config_str = std::str::from_utf8(&config_bytes)?;
        let deserialize_result = Self::load_from_str(config_str);
        deserialize_result.map(Some)
    }

    fn load_from_str(config_str: &str) -> Result<Self, ConfigParseError> {
        // first pass to hunt for the config version
        let config_format: ConfigFormat = merde::yaml::from_str(config_str)?;
        // second pass depending on version
        let v3 = match config_format {
            ConfigFormat::V1 => {
                let v1 = merde::yaml::from_str::<ConfigV1>(config_str)?;
                let v2: ConfigV2 = v1.into();
                v2.into()
            }
            ConfigFormat::V2 => {
                let v2 = merde::yaml::from_str::<ConfigV2>(config_str)?;
                v2.into()
            }
            ConfigFormat::V3 => merde::yaml::from_str::<ConfigV3>(config_str)?,
        };
        v3.try_into()
    }

    pub fn load_default() -> Self {
        let mut result = Self::load_from_str(DEFAULT_CONFIG)
            .expect("default_patterns_are_loadable test should have caught this");
        if cfg!(feature = "static_python") {
            result
                .languages
                .get_mut(&LanguageName::PYTHON)
                .expect("default_patterns_are_loadable test should have caught this")
                .parser = Some(loader::ParserSource::Static("Python".to_string()));
        }
        result
    }

    pub fn merge(mut self, overrides: Self) -> Self {
        for (language_name, language_config) in overrides.languages {
            match self.languages.entry(language_name) {
                std::collections::hash_map::Entry::Vacant(e) => e.insert(language_config),
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    let dest_config = e.get_mut();
                    if let Some(parser) = language_config.parser {
                        dest_config.parser = Some(parser.clone());
                    }
                    if let Some(x) = language_config.definition_query {
                        dest_config.definition_query = Some(x.clone());
                    }
                    if let Some(x) = language_config.sibling_node_types {
                        dest_config.sibling_node_types = Some(x.clone());
                    }
                    if let Some(x) = language_config.parent_query {
                        dest_config.parent_query = Some(x.clone());
                    }
                    if let Some(x) = language_config.recurse_query {
                        dest_config.recurse_query = Some(x.clone());
                    }
                    if let Some(x) = language_config.import_query {
                        dest_config.import_query = Some(x.clone());
                    }
                    if let Some(x) = language_config.injection_query {
                        dest_config.injection_query = Some(x.clone());
                    }
                    dest_config
                }
            };
        }
        self
    }
}

impl LanguageConfigV3 {
    pub fn rebase(&mut self, base: &LanguageConfigV3) {
        fn combine_queries(base: Option<&String>, extension: Option<String>) -> Option<String> {
            let (is_concat, extension) = match extension.as_ref() {
                None => (true, None),
                Some(extension) => {
                    let extension = extension.trim_start();
                    match extension.split_at_checked(3) {
                        Some(("...", rest)) => (true, Some(rest)),
                        _ => (false, Some(extension)),
                    }
                }
            };
            match base {
                None => extension.map(|s| s.to_owned()),
                Some(base) => match extension {
                    None => Some(base.clone()),
                    Some(extension) if !is_concat => Some(extension.to_owned()),
                    Some(extension) => Some(base.clone() + extension),
                },
            }
        }
        if self.parser.is_none() {
            self.parser = base.parser.clone();
        }
        self.extends = base.extends.clone();

        // combine queries
        self.definition_query =
            combine_queries(base.definition_query.as_ref(), self.definition_query.take());

        // combine sibling nodes
        let is_concat = self
            .sibling_node_types
            .as_ref()
            .and_then(|v| v.first())
            .map(|s| s.as_str())
            == Some("...");
        if is_concat {
            self.sibling_node_types.as_mut().map(|v| v.swap_remove(0));
        }
        if let Some(base_sibs) = base.sibling_node_types.as_ref() {
            match self.sibling_node_types.as_mut() {
                Some(sibs) if is_concat => {
                    sibs.extend_from_slice(base_sibs);
                }
                None => {
                    self.sibling_node_types = Some(base_sibs.clone());
                }
                _ => (),
            }
        }
        self.parent_query = combine_queries(base.parent_query.as_ref(), self.parent_query.take());
        self.recurse_query =
            combine_queries(base.recurse_query.as_ref(), self.recurse_query.take());
        self.import_query = combine_queries(base.import_query.as_ref(), self.import_query.take());
        self.injection_query =
            combine_queries(base.injection_query.as_ref(), self.injection_query.take());
    }
}

pub struct QueryCompiler {
    config: Config,
    language_loader: loader::Loader,
    cache: std::collections::HashMap<LanguageName, Option<std::rc::Rc<LanguageInfo>>>,
}

#[derive(Debug)]
pub enum QueryCompilerError {
    HasFailedBefore(LanguageName),
    GetLanguageInfoError(LanguageName, GetLanguageInfoError),
}

#[derive(Debug)]
pub enum GetLanguageInfoError {
    LanguageIsNotInConfig(LanguageName),
    ParserNotConfigured,
    LoaderError(loader::LoaderError),
    ExtendsUnknownLanguage(LanguageName, String),
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
            Self::LanguageIsNotInConfig(language_name)
                => write!(f, "language {language_name} not found in any config"),
            Self::ParserNotConfigured
                => write!(f, "no parser configured for language or any of its ancestors"),
            Self::LoaderError(e)
                => write!(f, "failed to load parser: {e}"),
            Self::ExtendsUnknownLanguage(language_name, extends)
                => write!(f, "{language_name} extends unknown language {extends:#?}"),
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
    pub fn new(config: Config, language_loader: loader::Loader) -> Self {
        Self {
            config,
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
                    &self.config,
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
    config: &Config,
    language_loader: &mut loader::Loader,
) -> Result<LanguageInfo, GetLanguageInfoError> {
    let mut language_config = LanguageConfigV3 {
        extends: Some(language_name.to_string()),
        ..Default::default()
    };
    while let Some(extends) = language_config.extends.as_ref() {
        use std::str::FromStr;
        let base_language = LanguageName::from_str(extends).map_err(|_| {
            GetLanguageInfoError::ExtendsUnknownLanguage(language_name, extends.to_owned())
        })?;
        let base_config = config
            .languages
            .get(&base_language)
            .ok_or(GetLanguageInfoError::LanguageIsNotInConfig(base_language))?;
        language_config.rebase(base_config);
    }
    let parser_source = language_config
        .parser
        .as_ref()
        .ok_or(GetLanguageInfoError::ParserNotConfigured)?;
    let language = language_loader
        .get_language(parser_source)
        .map_err(GetLanguageInfoError::LoaderError)?;
    LanguageInfo::new(language, language_name, &language_config)
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

impl LanguageInfo {
    pub fn new(
        language: tree_sitter::Language,
        language_name: LanguageName,
        config: &LanguageConfigV3,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_vs_v2() {
        let v1 = Config::load_from_str(
            r#"{"python": {
            "match_patterns": [],
            "sibling_patterns": [],
            "parent_patterns": [],
            "parent_exclusions": []
        }}"#,
        )
        .unwrap();
        let v2 = Config::load_from_str(
            r#"{
            "_version": 2,
            "python": {
                "match_patterns": [],
                "sibling_patterns": [],
                "parent_patterns": [],
                "parent_exclusions": []
            }
        }"#,
        )
        .unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn v2_vs_v3() {
        let v2 = Config::load_from_str(
            r#"{
            "_version": 2,
            "pYtHOn": {
                "match_patterns": ["(function_definition name: (_) @name) @def"],
                "sibling_patterns": [],
                "parent_patterns": [],
                "parent_exclusions": []
            }
        }"#,
        )
        .unwrap();
        let v3 = Config::load_from_str(
            r#"{
            "_version": 3,
            "Python": {
                "definition_query": "(function_definition name: (_) @name) @def",
                "sibling_node_types": [],
                "parent_query": null,
            }
        }"#,
        )
        .unwrap();
        assert_eq!(v2, v3);
    }
}
