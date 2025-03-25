// TODOs
//     support fenced code blocks in markdown and rst
//         likely to require regrouping
//     tree_sitter 0.22 will support alternation of node types, allowing better concision
//     tree_sitter 0.22 will support context_cursor.set_max_start_depth(0)
use crate::language_name::LanguageName;
use crate::loader;

const DEFAULT_CONFIG: &str = include_str!("dook.yml");

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
struct LanguageConfig {
    parser: Option<loader::ParserSource>,
    match_patterns: std::vec::Vec<MultiLineString>,
    sibling_patterns: std::vec::Vec<String>,
    parent_patterns: std::vec::Vec<String>,
    parent_exclusions: std::vec::Vec<String>,
    recurse_patterns: Option<std::vec::Vec<MultiLineString>>,
    comments: Option<Vec<String>>,
}

merde::derive! {
    impl (Deserialize) for struct LanguageConfig { parser, match_patterns, sibling_patterns, parent_patterns, parent_exclusions, recurse_patterns, comments }
}

#[derive(Debug, PartialEq)]
pub struct ConfigV1(std::collections::HashMap<LanguageName, LanguageConfig>);

merde::derive! {
    impl (Deserialize) for struct ConfigV1 transparent
}

#[derive(Debug, PartialEq)]
pub struct ConfigV2 {
    version: u64,
    languages: std::collections::HashMap<LanguageName, LanguageConfig>,
}

merde::impl_into_static!(struct ConfigV2 { version, languages });

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
                        de.put_back(merde::Event::Str(key))?;
                        let key: LanguageName = de.t().await?;
                        result.languages.insert(key, de.t().await?);
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

pub enum ConfigFormat {
    V1,
    V2,
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

pub use ConfigV2 as Config;

impl Config {
    pub fn load(explicit_path: Option<std::ffi::OsString>) -> std::io::Result<Option<Self>> {
        use etcetera::AppStrategy;
        use merde::IntoStatic;
        let file_contents = match explicit_path {
            // explicitly requested file paths expose any errors reading
            Some(p) => std::fs::read(std::path::PathBuf::from(p))?,
            // the default file path is more forgiving...
            None => match dirs() {
                // if we have no idea how to find it, just give up
                Err(_) => return Ok(None),
                Ok(d) => {
                    let default_path = d.config_dir().join("dook.yml");
                    match std::fs::read(&default_path) {
                        // unwrap the contents if we successfully read it
                        Ok(contents) => contents,
                        Err(e) => match e.kind() {
                            // silently eat NotFound
                            std::io::ErrorKind::NotFound => return Ok(None),
                            // log other errors but don't let them stop us from trying to work in a degraded environment
                            _ => {
                                log::warn!("Error reading config at {:?}, falling back to built-in default: {:?}", default_path, e);
                                return Ok(None);
                            }
                        },
                    }
                }
            },
        }.to_ascii_lowercase();
        let contents_lowercase = std::str::from_utf8(&file_contents)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let deserialize_result = Self::load_from_str(contents_lowercase);
        match deserialize_result {
            Ok(c) => Ok(Some(c.into_static())),
            Err(e) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.into_static(),
            )),
        }
    }

    fn load_from_str(contents_lowercase: &str) -> Result<Self, merde::MerdeError> {
        // first pass to hunt for the config version
        let config_format: ConfigFormat = merde::yaml::from_str(contents_lowercase)?;
        // second pass depending on version
        match config_format {
            ConfigFormat::V1 => {
                let ConfigV1(language_configs) = merde::yaml::from_str(contents_lowercase)?;
                Ok(ConfigV2 {
                    version: 2,
                    languages: language_configs,
                })
            }
            ConfigFormat::V2 => merde::yaml::from_str::<ConfigV2>(contents_lowercase),
        }
    }

    pub fn load_default() -> Self {
        Self::load_from_str(&DEFAULT_CONFIG.to_ascii_lowercase()).unwrap()
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
                    if !language_config.match_patterns.is_empty() {
                        dest_config.match_patterns = language_config.match_patterns.clone();
                    }
                    if !language_config.sibling_patterns.is_empty() {
                        dest_config.sibling_patterns = language_config.sibling_patterns.clone();
                    }
                    if !language_config.parent_patterns.is_empty() {
                        dest_config.parent_patterns = language_config.parent_patterns.clone();
                    }
                    if !language_config.parent_exclusions.is_empty() {
                        dest_config.parent_exclusions = language_config.parent_exclusions.clone();
                    }
                    if let Some(recurse_patterns) = language_config.recurse_patterns {
                        dest_config.recurse_patterns = Some(recurse_patterns.clone());
                    }
                    dest_config
                }
            };
        }
        self
    }

    pub fn get_parser_source(&self, language_name: LanguageName) -> Option<&loader::ParserSource> {
        self.languages.get(&language_name)?.parser.as_ref()
    }

    pub fn get_language_info(
        &self,
        language_name: LanguageName,
        loader: &mut loader::Loader,
    ) -> Option<Result<LanguageInfo, tree_sitter::QueryError>> {
        let language_config = self.languages.get(&language_name)?;
        let language = loader
            .get_language(language_config.parser.as_ref().unwrap())
            .unwrap()
            .unwrap();
        let match_patterns: std::vec::Vec<String> = language_config
            .match_patterns
            .iter()
            .map(String::from)
            .collect();
        let recurse_patterns: std::vec::Vec<String> = language_config
            .recurse_patterns
            .as_ref()
            .map(|v| v.iter().map(String::from).collect())
            .unwrap_or_default();
        Some(LanguageInfo::new(
            &language,
            match_patterns,
            &language_config.sibling_patterns,
            &language_config.parent_patterns,
            &language_config.parent_exclusions,
            recurse_patterns,
        ))
    }
}

pub struct LanguageInfo {
    pub match_patterns: std::vec::Vec<tree_sitter::Query>,
    pub sibling_patterns: std::vec::Vec<std::num::NonZero<u16>>,
    pub parent_patterns: std::vec::Vec<std::num::NonZero<u16>>,
    pub parent_exclusions: std::vec::Vec<std::num::NonZero<u16>>,
    pub recurse_patterns: std::vec::Vec<tree_sitter::Query>,
}

impl LanguageInfo {
    pub fn new<
        Item1: AsRef<str>,
        Item2: AsRef<str>,
        Item3: AsRef<str>,
        Item4: AsRef<str>,
        Item5: AsRef<str>,
        I1: IntoIterator<Item = Item1>,
        I2: IntoIterator<Item = Item2>,
        I3: IntoIterator<Item = Item3>,
        I4: IntoIterator<Item = Item4>,
        I5: IntoIterator<Item = Item5>,
    >(
        language: &tree_sitter::Language,
        match_patterns: I1,
        sibling_patterns: I2,
        parent_patterns: I3,
        parent_exclusions: I4,
        recurse_patterns: I5,
    ) -> Result<Self, tree_sitter::QueryError> {
        fn compile_queries<Item: AsRef<str>, II: IntoIterator<Item = Item>>(
            language: &tree_sitter::Language,
            sources: II,
        ) -> Result<std::vec::Vec<tree_sitter::Query>, tree_sitter::QueryError> {
            sources
                .into_iter()
                .map(|source| tree_sitter::Query::new(language, source.as_ref()))
                .collect()
        }
        fn resolve_node_types<Item: AsRef<str>, II: IntoIterator<Item = Item>>(
            language: &tree_sitter::Language,
            node_type_names: II,
        ) -> Result<std::vec::Vec<std::num::NonZero<u16>>, tree_sitter::QueryError> {
            node_type_names
                .into_iter()
                .map(|node_type_name| {
                    match std::num::NonZero::new(
                        language.id_for_node_kind(node_type_name.as_ref(), true),
                    ) {
                        None => Err(tree_sitter::QueryError {
                            row: 0,
                            column: 0,
                            offset: 0,
                            message: format!("unknown node type: {:?}", node_type_name.as_ref()),
                            kind: tree_sitter::QueryErrorKind::NodeType,
                        }),
                        Some(n) => Ok(n),
                    }
                })
                .collect()
        }
        fn resolve_field_names<Item: AsRef<str>, II: IntoIterator<Item = Item>>(
            language: &tree_sitter::Language,
            field_names: II,
        ) -> Result<std::vec::Vec<std::num::NonZero<u16>>, tree_sitter::QueryError> {
            field_names
                .into_iter()
                .map(|field_name| {
                    language
                        .field_id_for_name(field_name.as_ref())
                        .ok_or_else(|| tree_sitter::QueryError {
                            row: 0,
                            column: 0,
                            offset: 0,
                            message: format!("unknown field name: {:?}", field_name.as_ref()),
                            kind: tree_sitter::QueryErrorKind::Field,
                        })
                })
                .collect()
        }
        Ok(Self {
            match_patterns: compile_queries(language, match_patterns)?,
            sibling_patterns: resolve_node_types(language, sibling_patterns)?,
            parent_patterns: resolve_node_types(language, parent_patterns)?,
            parent_exclusions: resolve_field_names(language, parent_exclusions)?,
            recurse_patterns: compile_queries(language, recurse_patterns)?,
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
}
