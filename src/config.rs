// TODOs
//     support fenced code blocks in markdown and rst
//         likely to require regrouping
//     tree_sitter 0.22 will support alternation of node types, allowing better concision
//     tree_sitter 0.22 will support context_cursor.set_max_start_depth(0)

const DEFAULT_CONFIG: &str = include_str!("dook.json");

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, strum::EnumIter)]
pub enum LanguageName {
    Rust,
    Python,
    Js,
    Ts,
    Tsx,
    C,
    CPlusPlus,
    Go,
}

merde::derive! {
    impl (Serialize, Deserialize) for enum LanguageName
    string_like {
        "rust" => Rust,
        "python" => Python,
        "js" => Js,
        "ts" => Ts,
        "tsx" => Tsx,
        "c" => C,
        "cplusplus" => CPlusPlus,
        "go" => Go,
    }
}

impl LanguageName {
    pub fn get_language(self) -> tree_sitter::Language {
        match self {
            LanguageName::Rust => tree_sitter_rust::language(),
            LanguageName::Python => tree_sitter_python::language(),
            LanguageName::Js => tree_sitter_javascript::language(),
            LanguageName::Ts => tree_sitter_typescript::language_typescript(),
            LanguageName::Tsx => tree_sitter_typescript::language_tsx(),
            LanguageName::C => tree_sitter_c::language(),
            LanguageName::CPlusPlus => tree_sitter_cpp::language(),
            LanguageName::Go => tree_sitter_go::language(),
        }
    }
}

#[derive(Debug, PartialEq)]
enum MultiLineString {
    One(String),
    Many(std::vec::Vec<String>),
}

impl merde::Serialize for MultiLineString {
    async fn serialize<'se>(
        &'se self,
        s: &'se mut dyn merde::DynSerializer,
    ) -> Result<(), merde::MerdeError<'static>> {
        match self {
            MultiLineString::One(v) => {
                s.write(merde::Event::Str(merde::CowStr::copy_from_str(v)))
                    .await
            }
            MultiLineString::Many(vs) => {
                s.write(merde::Event::ArrayStart(merde::ArrayStart {
                    size_hint: Some(vs.len()),
                }))
                .await?;
                for v in vs {
                    v.serialize(s).await?;
                }
                s.write(merde::Event::ArrayEnd).await
            }
        }
    }
}

impl<'de> merde::Deserialize<'de> for MultiLineString {
    async fn deserialize(
        de: &mut dyn merde::DynDeserializer<'de>,
    ) -> Result<Self, merde::MerdeError<'de>> {
        match de.next().await? {
            merde::Event::Str(v) => Ok(MultiLineString::One(v.repeat(1))), // there's probably a better way to clone CowStr to String
            merde::Event::ArrayStart(_) => {
                let mut vs: Vec<String> = Vec::new();
                loop {
                    match de.next().await? {
                        merde::Event::ArrayEnd => break,
                        merde::Event::Str(v) => vs.push(v.repeat(1)),
                        ev => Err(merde::MerdeError::UnexpectedEvent {
                            got: merde::EventType::from(&ev),
                            expected: &[merde::EventType::Str],
                            help: Some(String::from(
                                "multiline string must be a string or an array of strings",
                            )),
                        })?,
                    }
                }
                Ok(MultiLineString::Many(vs))
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

impl From<&MultiLineString> for String {
    fn from(mls: &MultiLineString) -> Self {
        match mls {
            MultiLineString::One(s) => s.clone(),
            MultiLineString::Many(v) => v.join("\n"),
        }
    }
}

#[derive(Debug, PartialEq)]
struct LanguageConfig {
    match_patterns: std::vec::Vec<MultiLineString>,
    sibling_patterns: std::vec::Vec<String>,
    parent_patterns: std::vec::Vec<String>,
    parent_exclusions: std::vec::Vec<String>,
    recurse_patterns: Option<std::vec::Vec<MultiLineString>>,
    comments: Option<Vec<String>>,
}

merde::derive! {
    impl (Deserialize) for struct LanguageConfig { match_patterns, sibling_patterns, parent_patterns, parent_exclusions, recurse_patterns, comments }
}

#[derive(Debug, PartialEq)]
pub struct Config(std::collections::HashMap<LanguageName, LanguageConfig>);

merde::derive! {
    impl (Deserialize) for struct Config transparent
}

impl Config {
    pub fn load(explicit_path: Option<std::ffi::OsString>) -> std::io::Result<Option<Self>> {
        use merde::IntoStatic;
        let file_contents = match explicit_path {
            // explicitly requested file paths expose any errors reading
            Some(p) => std::fs::read(std::path::PathBuf::from(p))?,
            // the default file path is more forgiving...
            None => match directories::ProjectDirs::from("com", "melonisland", "dook") {
                // if we have no idea how to find it, just give up
                None => return Ok(None),
                Some(d) => {
                    let default_path = d.config_dir().join("dook.json");
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
        let deserialize_result: Result<Config, _> = merde::json::from_str(contents_lowercase);
        match deserialize_result {
            Ok(c) => Ok(Some(c.into_static())),
            Err(e) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.into_static(),
            )),
        }
    }

    pub fn load_default() -> Self {
        merde::json::from_str(&DEFAULT_CONFIG.to_ascii_lowercase()).unwrap()
    }

    pub fn get_language_info(
        &self,
        language_name: LanguageName,
    ) -> Option<Result<LanguageInfo, tree_sitter::QueryError>> {
        let Self(config_map) = self;
        let language_config = config_map.get(&language_name)?;
        let language = language_name.get_language();
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
            language,
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
    pub sibling_patterns: std::vec::Vec<u16>,
    pub parent_patterns: std::vec::Vec<u16>,
    pub parent_exclusions: std::vec::Vec<u16>,
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
        language: tree_sitter::Language,
        match_patterns: I1,
        sibling_patterns: I2,
        parent_patterns: I3,
        parent_exclusions: I4,
        recurse_patterns: I5,
    ) -> Result<Self, tree_sitter::QueryError> {
        fn compile_queries<Item: AsRef<str>, II: IntoIterator<Item = Item>>(
            language: tree_sitter::Language,
            sources: II,
        ) -> Result<std::vec::Vec<tree_sitter::Query>, tree_sitter::QueryError> {
            sources
                .into_iter()
                .map(|source| tree_sitter::Query::new(language, source.as_ref()))
                .collect()
        }
        fn resolve_node_types<Item: AsRef<str>, II: IntoIterator<Item = Item>>(
            language: tree_sitter::Language,
            node_type_names: II,
        ) -> Result<std::vec::Vec<u16>, tree_sitter::QueryError> {
            node_type_names
                .into_iter()
                .map(|node_type_name| {
                    match language.id_for_node_kind(node_type_name.as_ref(), true) {
                        0 => Err(tree_sitter::QueryError {
                            row: 0,
                            column: 0,
                            offset: 0,
                            message: format!("unknown node type: {:?}", node_type_name.as_ref()),
                            kind: tree_sitter::QueryErrorKind::NodeType,
                        }),
                        n => Ok(n),
                    }
                })
                .collect()
        }
        fn resolve_field_names<Item: AsRef<str>, II: IntoIterator<Item = Item>>(
            language: tree_sitter::Language,
            field_names: II,
        ) -> Result<std::vec::Vec<u16>, tree_sitter::QueryError> {
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
    fn default_patterns_are_loadable() {
        use strum::IntoEnumIterator;
        let default_config = Config::load_default();
        for language_name in LanguageName::iter() {
            default_config.get_language_info(language_name);
        }
    }
}
