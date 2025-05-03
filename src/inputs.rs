use crate::language_name::LanguageName;

#[derive(Copy, Clone)]
pub enum SearchInput<'a> {
    Path(&'a std::path::PathBuf),
    Loaded(&'a LoadedFile),
}

impl std::fmt::Display for SearchInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loaded(_) => write!(f, "input"),
            Self::Path(path) => write!(f, "{path:?}"),
        }
    }
}

pub struct LoadedFile {
    pub bytes: Vec<u8>,
    pub language_name: LanguageName,
}

#[derive(Debug, Clone)]
pub enum Error {
    UnknownLanguage,
    UnsupportedLanguage(String),
    UnreadableFile(String),
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
            Self::EmptyStdin => write!(f, "stdin is empty")
        }
    }
}

impl LoadedFile {
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        Ok(Self {
            language_name: detect_language_from_path(path.as_ref())?,
            bytes: std::fs::read(path.as_ref())
                .map_err(|e| Error::UnreadableFile(e.to_string()))?,
        })
    }

    pub fn load_stdin() -> Result<Self, Error> {
        use std::io::Read;
        let mut bytes = vec![];
        let language_name = match std::io::stdin().read_to_end(&mut bytes) {
            Err(e) => Err(Error::UnreadableFile(e.to_string())),
            Ok(_) if bytes.is_empty() => Err(Error::EmptyStdin),
            Ok(_) => detect_language_from_bytes(&bytes),
        }?;
        Ok(LoadedFile {
            bytes,
            language_name,
        })
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

#[cfg(feature = "stdin")]
pub fn detect_language_from_bytes(bytes: &[u8]) -> Result<LanguageName, Error> {
    use std::str::FromStr;
    let language_name_str = hyperpolyglot::detectors::classify(
        std::str::from_utf8(bytes).map_err(|e| Error::UnreadableFile(e.to_string()))?,
        &[],
    );
    LanguageName::from_str(language_name_str)
        .map_err(|_| Error::UnsupportedLanguage(language_name_str.to_owned()))
}

#[cfg(not(feature = "stdin"))]
pub fn detect_language_from_bytes(_: &[u8]) -> Result<LanguageName, Error> {
    Err(Error::UnknownLanguage)
}
