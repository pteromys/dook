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
            Ok(_) => detect_language_from_bytes(&bytes, None),
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

    let head = &bytes[..51200]; // hyperpolyglot::MAX_CONTENT_SIZE_BYTES
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
