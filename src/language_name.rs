use crate::language_aliases::LANGUAGE_CANONICAL_NAMES;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LanguageName(&'static str);

pub struct UnknownLanguageError {}

impl LanguageName {
    // these are almost enums right?
    pub const PYTHON: Self = Self("Python");
    pub const RUST: Self = Self("Rust");
    pub const JAVASCRIPT: Self = Self("JavaScript");
    pub const TYPESCRIPT: Self = Self("TypeScript");
    pub const TSX: Self = Self("TSX");
    pub const C: Self = Self("C");
    pub const CPLUSPLUS: Self = Self("C++");
    pub const GO: Self = Self("Go");
    pub const MARKDOWN: Self = Self("Markdown");
    pub const HTML: Self = Self("HTML");
    pub const CYTHON: Self = Self("Cython");
    pub const TEX: Self = Self("TeX");
    pub const YAML: Self = Self("YAML");

    /// Convert language names from the strings we used in the v1 and v2 config format
    pub fn from_legacy(s: &str) -> Result<Self, UnknownLanguageError> {
        Ok(match s.to_lowercase().as_ref() {
            // hyperpolyglot names
            "rust" => Self::RUST,
            "python" => Self::PYTHON,
            "javascript" => Self::JAVASCRIPT,
            "typescript" => Self::TYPESCRIPT,
            "tsx" => Self::TSX,
            "c" => Self::C,
            "c++" => Self::CPLUSPLUS,
            "go" => Self::GO,
            "markdown" => Self::MARKDOWN,
            // names from our config format v1 and v2
            "js" => Self::JAVASCRIPT,
            "ts" => Self::TYPESCRIPT,
            "cplusplus" => Self::CPLUSPLUS,
            _ => return Err(UnknownLanguageError {}),
        })
    }
}

impl std::str::FromStr for LanguageName {
    type Err = UnknownLanguageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = match LANGUAGE_CANONICAL_NAMES.get(&s.to_lowercase()) {
            Some(s) => *s,
            None => s,
        };
        if let Ok(hyperpolyglot_language) = hyperpolyglot::Language::try_from(s) {
            return Ok(Self(hyperpolyglot_language.name));
        }
        Err(UnknownLanguageError {})
    }
}

impl std::fmt::Display for LanguageName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self(language_name_str) = self;
        write!(f, "{}", language_name_str)
    }
}

impl AsRef<str> for LanguageName {
    fn as_ref(&self) -> &'static str {
        let Self(inner) = self;
        inner
    }
}
