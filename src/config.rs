use crate::LanguageName;
use crate::loader;
use crate::multi_line_string::MultiLineString;

pub static DEFAULT_CONFIG: phf::Map<&'static str, &'static str> = phf::phf_map! {
    "C" => include_str!("../config/c.yml"),
    "C++" => include_str!("../config/cxx.yml"),
    "CSS" => include_str!("../config/css.yml"),
    "Cython" => include_str!("../config/cython.yml"),
    "GLSL" => include_str!("../config/glsl.yml"),
    "Go" => include_str!("../config/go.yml"),
    "HTML" => include_str!("../config/html.yml"),
    "JavaScript" => include_str!("../config/javascript.yml"),
    "Lua" => include_str!("../config/lua.yml"),
    "Markdown" => include_str!("../config/markdown.yml"),
    "Python" => include_str!("../config/python.yml"),
    "Rust" => include_str!("../config/rust.yml"),
    "Shell" => include_str!("../config/shell.yml"),
    "TeX" => include_str!("../config/tex.yml"),
    "TSX" => include_str!("../config/tsx.yml"),
    "TypeScript" => include_str!("../config/typescript.yml"),
    "YAML" => include_str!("../config/yaml.yml"),
};

pub fn default_config_path() -> Option<std::path::PathBuf> {
    use etcetera::AppStrategy;
    app_dirs().map(|d| d.config_dir()).ok()
}

pub fn app_dirs() -> Result<impl etcetera::AppStrategy, etcetera::HomeDirError> {
    etcetera::choose_app_strategy(etcetera::AppStrategyArgs {
        top_level_domain: "com".to_string(),
        author: "melonisland".to_string(),
        app_name: "dook".to_string(),
    })
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
struct MonolithicConfigV1(std::collections::HashMap<String, LanguageConfigV1>);

merde::derive! {
    impl (Deserialize) for struct MonolithicConfigV1 transparent
}

#[derive(Debug, PartialEq)]
struct MonolithicConfigV2 {
    version: u64,
    languages: std::collections::HashMap<String, LanguageConfigV1>,
}

#[derive(Debug, PartialEq, Default)]
struct LanguageConfigV3 {
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
struct MonolithicConfigV3 {
    version: u64,
    languages: std::collections::HashMap<String, LanguageConfigV3>,
}

#[derive(Debug, PartialEq, Default)]
pub struct LanguageConfigV4 {
    pub version: u64,
    pub parser: Option<loader::ParserSource>,
    pub extends: Option<String>,
    pub definition_query: Option<String>,
    pub sibling_node_types: Option<std::vec::Vec<String>>,
    pub parent_query: Option<String>,
    pub recurse_query: Option<String>,
    pub import_query: Option<String>,
    pub injection_query: Option<String>,
}

merde::derive! {
    impl (Deserialize) for struct LanguageConfigV4 {
        version,
        parser,
        extends,
        definition_query,
        sibling_node_types,
        parent_query,
        recurse_query,
        import_query,
        injection_query
    }
}

pub use LanguageConfigV4 as LanguageConfig;

impl From<MonolithicConfigV1> for MonolithicConfigV2 {
    fn from(value: MonolithicConfigV1) -> Self {
        let MonolithicConfigV1(language_map) = value;
        Self {
            version: 1,
            languages: language_map,
        }
    }
}

impl From<MonolithicConfigV2> for MonolithicConfigV3 {
    fn from(value: MonolithicConfigV2) -> Self {
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
                _ => Some(value.match_patterns.into_iter().map(|s| s.into()).collect::<Vec<String>>().join("\n")),
            },
            sibling_node_types: Some(value.sibling_patterns),
            parent_query: match value.parent_patterns.len() {
                0 => None,
                _ => Some(value
                        .parent_patterns
                        .iter()
                        .map(|node_name| format!("({})", node_name))
                        .collect::<Vec<String>>().join("\n")),
            },
            recurse_query: value
                .recurse_patterns
                .map(|v| v.into_iter().map(|s| s.into()).collect::<Vec<String>>().join("\n")),
            import_query: value
                .import_patterns
                .map(|v| v.into_iter().map(|s| s.into()).collect::<Vec<String>>().join("\n")),
            injection_query: None,
        }
    }
}

impl From<LanguageConfigV3> for LanguageConfigV4 {
    fn from(value: LanguageConfigV3) -> Self {
        let LanguageConfigV3 {
            parser,
            extends,
            definition_query,
            sibling_node_types,
            parent_query,
            recurse_query,
            import_query,
            injection_query,
        } = value;
        Self {
            version: 4,
            parser,
            extends,
            definition_query,
            sibling_node_types,
            parent_query,
            recurse_query,
            import_query,
            injection_query
        }
    }
}

impl<'de> merde::Deserialize<'de> for MonolithicConfigV2 {
    async fn deserialize(
        de: &mut dyn merde::DynDeserializer<'de>,
    ) -> Result<Self, merde::MerdeError<'de>> {
        use merde::DynDeserializerExt;
        let mut result = MonolithicConfigV2 {
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

impl<'de> merde::Deserialize<'de> for MonolithicConfigV3 {
    async fn deserialize(
        de: &mut dyn merde::DynDeserializer<'de>,
    ) -> Result<Self, merde::MerdeError<'de>> {
        use merde::DynDeserializerExt;
        let mut result = MonolithicConfigV3 {
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
    V4,
}

impl std::fmt::Display for ConfigFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V1 => write!(f, "1"),
            Self::V2 => write!(f, "2"),
            Self::V3 => write!(f, "3"),
            Self::V4 => write!(f, "4"),
        }
    }
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
                            4 => Ok(ConfigFormat::V4),
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
struct MonolithicConfig {
    version: ConfigFormat,
    languages: std::collections::HashMap<LanguageName, LanguageConfig>,
}

#[derive(Debug)]
pub enum ConfigParseError {
    Deserialize(merde::MerdeError<'static>),
    UnknownVersion(ConfigFormat),
    UnknownLanguage(String),
    NotUtf8(std::str::Utf8Error),
    UnreadableFile(std::io::Error),
    HasFailedBefore(LanguageName),
    ExtendsCycle(LanguageName),
    ExtendsUnknownLanguage(LanguageName, String),
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
            Self::UnknownVersion(version)
                => write!(f, "unknown version: {}", version),
            Self::UnknownLanguage(language)
                => write!(f, "unknown language: {}", language),
            Self::NotUtf8(e)
                => write!(f, "{}", e),
            Self::UnreadableFile(e)
                => write!(f, "{}", e),
            Self::HasFailedBefore(language_name)
                => write!(f, "failed to load {} config earlier", language_name),
            Self::ExtendsCycle(language_name)
                => write!(f, "\"extends\" field in {} config points into a cycle", language_name),
            Self::ExtendsUnknownLanguage(language_name, extends)
                => write!(f, "{language_name} extends unknown language {extends:#?}"),
        }
    }
}

impl TryFrom<MonolithicConfigV3> for MonolithicConfig {
    type Error = ConfigParseError;
    fn try_from(value: MonolithicConfigV3) -> Result<Self, Self::Error> {
        use std::str::FromStr;
        let mut languages: std::collections::HashMap<LanguageName, LanguageConfig> =
            std::collections::HashMap::new();
        for (language_name, config) in value.languages {
            if let Ok(language_name) = LanguageName::from_str(language_name.as_ref()) {
                languages.insert(language_name, config.into());
                continue;
            }
            if value.version <= 2 {
                if let Ok(language_name) = LanguageName::from_legacy(language_name.as_ref()) {
                    languages.insert(language_name, config.into());
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

impl MonolithicConfig {
    fn load(path: impl AsRef<std::path::Path>) -> Result<Self, ConfigParseError> {
        let config_bytes = std::fs::read(path.as_ref()).map_err(ConfigParseError::UnreadableFile)?;
        let config_str = std::str::from_utf8(&config_bytes)?;
        Self::load_from_str(config_str)
    }

    fn load_from_str(config_str: &str) -> Result<Self, ConfigParseError> {
        // first pass to hunt for the config version
        let config_format: ConfigFormat = merde::yaml::from_str(config_str)?;
        // second pass depending on version
        let v3 = match config_format {
            ConfigFormat::V1 => {
                let v1 = merde::yaml::from_str::<MonolithicConfigV1>(config_str)?;
                let v2: MonolithicConfigV2 = v1.into();
                v2.into()
            }
            ConfigFormat::V2 => {
                let v2 = merde::yaml::from_str::<MonolithicConfigV2>(config_str)?;
                v2.into()
            }
            ConfigFormat::V3 => merde::yaml::from_str::<MonolithicConfigV3>(config_str)?,
            x => return Err(ConfigParseError::UnknownVersion(x)),
        };
        v3.try_into()
    }
}

impl LanguageConfig {
    fn rebase(&mut self, base: &LanguageConfig) {
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

    fn replace(&mut self, replacements: LanguageConfig) -> &Self {
        if let Some(parser) = replacements.parser {
            self.parser = Some(parser.clone());
        }
        if let Some(x) = replacements.definition_query {
            self.definition_query = Some(x.clone());
        }
        if let Some(x) = replacements.sibling_node_types {
            self.sibling_node_types = Some(x.clone());
        }
        if let Some(x) = replacements.parent_query {
            self.parent_query = Some(x.clone());
        }
        if let Some(x) = replacements.recurse_query {
            self.recurse_query = Some(x.clone());
        }
        if let Some(x) = replacements.import_query {
            self.import_query = Some(x.clone());
        }
        if let Some(x) = replacements.injection_query {
            self.injection_query = Some(x.clone());
        }
        self
    }
}

pub struct ConfigLoader {
    config_dir: Option<std::path::PathBuf>,
    cache: std::collections::HashMap<LanguageName, ConfigCacheEntry>,
    files: Option<std::collections::HashMap<LanguageName, std::path::PathBuf>>,
    monolithic_config: Option<MonolithicConfig>,
}

enum ConfigCacheEntry {
    HasFailedBefore,
    InProgress,
    Loaded(std::rc::Rc<LanguageConfig>),
}

impl ConfigLoader {
    pub fn new(config_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            config_dir,
            cache: Default::default(),
            files: None,
            monolithic_config: None,
        }
    }

    fn get_path_to_config(
        &mut self,
        language_name: LanguageName,
    ) -> Option<std::path::PathBuf> {
        use std::str::FromStr;
        if let Some(files) = self.files.as_ref() {
            return files.get(&language_name).cloned();
        }
        let files = self.files.insert(Default::default());
        let dir_entries = match std::fs::read_dir(self.config_dir.as_ref()?) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                log::error!("{}", e);
                return None;
            }
        };
        for entry in dir_entries {
            let path = match entry {
                Err(e) => {
                    log::error!("{}", e);
                    continue;
                }
                Ok(e) => e.path(),
            };
            if path.file_name() == Some(std::ffi::OsStr::new("dook.yml")) || path.file_name() == Some(std::ffi::OsStr::new("dook.json")) {
                log::warn!("parsing legacy monolithic config at {path:#?}");
                match MonolithicConfig::load(path) {
                    Ok(config) => { self.monolithic_config = Some(config); }
                    Err(e) => { log::error!("{}", e); }
                }
                continue;
            }
            if path.extension() != Some(std::ffi::OsStr::new("yml")) { continue; }
            let Some(file_stem) = path.file_stem() else { continue; };
            let Some(name) = file_stem.to_str() else { continue; };
            if let Ok(language_name) = LanguageName::from_str(name) {
                if let Some(replaced) = files.insert(language_name, path.clone()) {
                    log::error!("multiple configs found for {language_name}: {replaced:#?}, {path:#?}");
                }
            }
        }
        files.get(&language_name).cloned()
    }

    pub fn load_config(
        &mut self,
        language_name: LanguageName,
    ) -> Result<std::rc::Rc<LanguageConfig>, ConfigParseError> {
        match self.cache.entry(language_name) {
            std::collections::hash_map::Entry::Occupied(entry) => {
                return match entry.get() {
                    ConfigCacheEntry::HasFailedBefore => Err(ConfigParseError::HasFailedBefore(language_name)),
                    ConfigCacheEntry::InProgress => Err(ConfigParseError::ExtendsCycle(language_name)),
                    ConfigCacheEntry::Loaded(config) => Ok(config.clone()),
                };
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(ConfigCacheEntry::InProgress);
            }
        }
        match self.load_config_uncached(language_name) {
            Ok(result) => {
                let result = std::rc::Rc::new(result);
                self.cache.insert(language_name, ConfigCacheEntry::Loaded(result.clone()));
                Ok(result)
            }
            Err(e) => {
                self.cache.insert(language_name, ConfigCacheEntry::HasFailedBefore);
                Err(e)
            }
        }
    }

    fn load_config_uncached(
        &mut self,
        language_name: LanguageName,
    ) -> Result<LanguageConfig, ConfigParseError> {
        use std::str::FromStr;
        let default_config = match DEFAULT_CONFIG.get(language_name.as_ref()) {
            Some(c) => Some(merde::yaml::from_str::<LanguageConfigV4>(c)?),
            None => None,
        };
        let user_config = match self.get_path_to_config(language_name) {
            Some(path) => {
                let config_bytes = std::fs::read(path).map_err(ConfigParseError::UnreadableFile)?;
                let config_str = std::str::from_utf8(&config_bytes)?;
                Some(merde::yaml::from_str::<LanguageConfigV4>(config_str)?)
            },
            None => self.monolithic_config.as_mut().and_then(|c| c.languages.remove(&language_name)),
        };
        let mut merged_config = match default_config {
            Some(mut default_config) => {
                if cfg!(feature = "static_python") && language_name == LanguageName::PYTHON {
                    default_config.parser = Some(loader::ParserSource::Static("Python".to_string()));
                }
                if let Some(user_config) = user_config {
                    default_config.replace(user_config);
                }
                default_config
            }
            None => match user_config {
                Some(user_config) => user_config,
                None => { return Err(ConfigParseError::HasFailedBefore(language_name)); }
            }
        };
        if let Some(extends) = merged_config.extends.as_ref() {
            let base_language = LanguageName::from_str(extends).map_err(|_| {
                ConfigParseError::ExtendsUnknownLanguage(language_name, extends.to_owned())
            })?;
            let base_config = self.load_config(base_language)?;
            merged_config.rebase(&base_config);
        }
        Ok(merged_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_vs_v2() {
        let v1 = MonolithicConfig::load_from_str(
            r#"{"python": {
            "match_patterns": [],
            "sibling_patterns": [],
            "parent_patterns": [],
            "parent_exclusions": []
        }}"#,
        )
        .unwrap();
        let v2 = MonolithicConfig::load_from_str(
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
        let v2 = MonolithicConfig::load_from_str(
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
        let v3 = MonolithicConfig::load_from_str(
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
