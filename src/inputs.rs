use crate::{LanguageName, QueryCompiler, QueryCompilerError};

#[derive(Copy, Clone)]
pub enum SearchInput<'a> {
    Path(&'a std::path::PathBuf),
    Loaded(&'a LoadedFile),
}

impl std::fmt::Display for SearchInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path(path) => write!(f, "{path:?}"),
            Self::Loaded(loaded) => write!(f, "{}", loaded.describe()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedFile {
    pub recipe: Option<String>,
    pub bytes: Vec<u8>,
    pub language_name: LanguageName,
}

#[derive(Debug)]
pub enum Error {
    UnknownLanguage,
    UnsupportedLanguage(String),
    UnreadableFile(String),
    UnconfiguredLanguage(QueryCompilerError),
    EmptyStdin,
}

#[rustfmt::skip]
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownLanguage => write!(f, "unknown language"),
            Self::UnsupportedLanguage(language_name)
                => write!(f, "unsupported language {:?}", language_name),
            Self::UnreadableFile(message) => write!(f, "{}", message),
            Self::UnconfiguredLanguage(e) => write!(f, "{}", e),
            Self::EmptyStdin => write!(f, "stdin is empty")
        }
    }
}

impl LoadedFile {
    /// Detect the language of a file; if successful, load it into memory.
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        let language_name = detect_language_from_path(path.as_ref())?;
        Self::load_as(path, language_name)
    }

    /// Detect the language of a file; if it's one we can parse, load it into memory.
    pub fn load_if_parseable(path: impl AsRef<std::path::Path>, query_compiler: &mut QueryCompiler) -> Result<Self, Error> {
        let language_name = detect_language_from_path(path.as_ref())?;
        if language_name != LanguageName::IPYNB {
            query_compiler.get_language_info(language_name)
                .map_err(Error::UnconfiguredLanguage)?;
        }
        Self::load_as(path, language_name)
    }

    fn load_as(path: impl AsRef<std::path::Path>, language_name: LanguageName) -> Result<Self, Error> {
        Ok(Self {
            language_name,
            bytes: std::fs::read(path.as_ref())
                .map_err(|e| Error::UnreadableFile(e.to_string()))?,
            recipe: Some(format!("cat {:#?}", path.as_ref())),
        })
    }

    pub fn load_stdin() -> Result<Self, Error> {
        use std::io::Read;
        let mut bytes = vec![];
        let language_name = match std::io::stdin().read_to_end(&mut bytes) {
            Err(e) => Err(Error::UnreadableFile(e.to_string())),
            Ok(_) if bytes.is_empty() => Err(Error::EmptyStdin),
            Ok(_) => detect_language_from_bytes(&bytes, None),
        }?;
        Ok(LoadedFile {
            recipe: None,
            bytes,
            language_name,
        })
    }

    pub fn describe(&self) -> String {
        match self.recipe.as_ref() {
            None => "input".to_string(),
            Some(recipe) => format!("{recipe:?}"),
        }
    }
}

pub fn detect_language_from_path(path: &std::path::Path) -> Result<LanguageName, Error> {
    use std::str::FromStr;
    let language_name_str = hyperpolyglot::detect(path)
        .map_err(|e| Error::UnreadableFile(e.to_string()))?
        .ok_or(Error::UnknownLanguage)?
        .language();
    LanguageName::from_str(language_name_str)
        .map_err(|_| Error::UnsupportedLanguage(language_name_str.to_owned()))
}

#[cfg(not(feature = "stdin"))]
pub fn detect_language_from_bytes(_: &[u8], _: Option<&str>) -> Result<LanguageName, Error> {
    Err(Error::UnknownLanguage)
}

#[cfg(feature = "stdin")]
pub fn detect_language_from_bytes(bytes: &[u8], hint: Option<&str>) -> Result<LanguageName, Error> {
    use std::str::FromStr;
    let language_name_str = detect_language_str_from_bytes(bytes, hint)?;
    LanguageName::from_str(language_name_str)
        .map_err(|_| Error::UnsupportedLanguage(language_name_str.to_owned()))
}

/// This is basically hyperpolyglot::detect but without the part using the file path
#[cfg(feature = "stdin")]
fn detect_language_str_from_bytes(bytes: &[u8], hint: Option<&str>) -> Result<&'static str, Error> {
    let extension = hint.map(|hint| ".".to_string() + hint);
    let extension_candidates = extension.as_ref().map(|e| hyperpolyglot::detectors::get_languages_from_extension(e)).unwrap_or_default();
    if extension_candidates.len() == 1 {
        return Ok(extension_candidates[0]);
    }

    let shebang_reader = std::io::Cursor::new(bytes);
    let shebang_candidates = hyperpolyglot::detectors::get_languages_from_shebang(shebang_reader).unwrap_or_default();
    let candidates = filter_candidates(extension_candidates, shebang_candidates);
    if candidates.len() == 1 {
        return Ok(candidates[0]);
    }

    let head = match bytes.len() {
        ..51200 => bytes, // hyperpolyglot::MAX_CONTENT_SIZE_BYTES
        _ => &bytes[..51200],
    };
    let head_end = head.iter().rposition(|b| b & 128 == 0).ok_or(Error::UnknownLanguage)?;
    let head_str = std::str::from_utf8(&head[..head_end])
        .map_err(|e| Error::UnreadableFile(e.to_string()))?;
    let candidates = match extension {
        None => candidates,
        Some(_) if candidates.is_empty() => candidates,
        Some(extension) => {
            let heuristic_candidates = hyperpolyglot::detectors::get_languages_from_heuristics(&extension, &candidates, head_str);
            filter_candidates(candidates, heuristic_candidates)
        }
    };
    if candidates.len() == 1 {
        return Ok(candidates[0]);
    }

    Ok(hyperpolyglot::detectors::classify(head_str, &candidates))
}

// cribbed from hyperpolyglot lib.rs
#[cfg(feature = "stdin")]
fn filter_candidates(old: Vec<&'static str>, new: Vec<&'static str>) -> Vec<&'static str> {
    if old.is_empty() { return new; }
    let intersection: Vec<_> = new.into_iter().filter(|s| old.contains(s)).collect();
    match intersection.len() {
        0 => old,
        _ => intersection,
    }
}
