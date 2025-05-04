use crate::downloads_policy::{can_download, DownloadsPolicy};
use crate::language_name::LanguageName;

// Structs

pub struct Loader {
    cache: std::collections::HashMap<ParserSource, Option<std::rc::Rc<tree_sitter::Language>>>,
    loader: tree_sitter_loader::Loader,
    sources_dir: std::path::PathBuf,
    downloads_policy: DownloadsPolicy,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ParserSource {
    AbsolutePath(String), // tree-sitter-loader will recompile if parser.c is newer than .so
    GitSource(GitSource), // clone/fetch/checkout/whatever, then handle like AbsolutePath
    TarballSource(TarballSource), // recompile if .tar is newer than .so
    Static(String),       // use built-in
}

merde::derive! {
    impl (Serialize, Deserialize) for enum ParserSource
    externally_tagged {
        "path" => AbsolutePath,
        "git" => GitSource,
        "tarball" => TarballSource,
        "static" => Static,
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct GitSource {
    clone: String,
    commit: String,
    subdirectory: Option<String>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct GitSource { clone, commit, subdirectory }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TarballSource {
    name: String, // finally loads tree_sitter_{name} from {name}.so
    url: String,
    sha256hex: String,
    subdirectory: String,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct TarballSource { name, url, sha256hex, subdirectory }
}

// Errors

#[derive(Debug)]
pub enum LoaderError {
    ChildProcessFailed {
        verb: String,
        source: CalledProcessError,
    },
    CannotMakeDirectoryForGit {
        source: std::io::Error,
        repo_url: String,
        repo_path: std::path::PathBuf,
    },
    GitHasWrongRemote {
        repo_path: std::path::PathBuf,
        desired_repo_url: String,
        existing_repo_url: std::ffi::OsString,
    },
    GitHeadIsInvalid {
        repo_path: std::path::PathBuf,
        head: Vec<u8>,
    },
    CannotMakeDirectoryForTarball {
        err: std::io::Error,
        tarball_path: std::path::PathBuf,
    },
    ExpectedHashIsInvalid {
        err: base16ct::Error,
        tarball_url: String,
        expected_sha256hex: String,
    },
    TarballIsUnreadable {
        err: std::io::Error,
        tarball_path: std::path::PathBuf,
    },
    TarballHasWrongHash {
        tarball_url: String,
        expected_hash: String,
        recomputed_hash: String,
    },
    DllIsUnreadable {
        dll_path: std::ffi::OsString,
        source: libloading::Error,
    },
    DllSymbolIsMissing {
        source: libloading::Error,
        dll_path: std::ffi::OsString,
        symbol_name: String,
    },
    CannotFindAppDirectory {
        source: Box<dyn DebuggableDisplayable>,
    },
    CompileFailed {
        source: Box<dyn DebuggableDisplayable>,
        src_path: std::path::PathBuf,
    },
    LanguageWasNotBuiltIn(String),
    NotAllowedToDownload(String),
}

// this is just here because anyhow::Error doesn't claim to implement std::error::Error;
// once tree-sitter-loader moves to anyhow >= 1.0.98 we can use into_boxed_dyn_error()
pub trait DebuggableDisplayable: std::fmt::Display + std::fmt::Debug {}
impl<T> DebuggableDisplayable for T where T: std::fmt::Display + std::fmt::Debug {}

#[rustfmt::skip] // keep compact
impl std::fmt::Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChildProcessFailed { verb, source }
                => write!(f, "Attempt to {} {} ({:?})",
                          verb, source.source, source.command),
            Self::CannotMakeDirectoryForGit { repo_url, repo_path, source }
                => write!(f, "Could not make directory at {:?} to checkout {:?}: {}",
                          repo_url, repo_path, source),
            Self::GitHasWrongRemote { repo_path, desired_repo_url, existing_repo_url }
                => write!(f, "Repository at {:?} points at {:?} instead of {:?}",
                          repo_path, existing_repo_url, desired_repo_url),
            Self::GitHeadIsInvalid { repo_path, head }
                => write!(f, "Current revision {:?} not parseable as utf-8 in {:?}",
                          head, repo_path),
            Self::CannotMakeDirectoryForTarball { tarball_path, err }
                => write!(f, "Could not make temporary directory to extract {:?}: {}",
                          tarball_path, err),
            Self::ExpectedHashIsInvalid { tarball_url, expected_sha256hex, err }
                => write!(f, "Hash for {:?} not a 256-bit hex value: {:?}: {}",
                          tarball_url, expected_sha256hex, err),
            Self::TarballIsUnreadable { tarball_path, err }
                => write!(f, "Downloaded {:?} is unreadble: {}",
                          tarball_path, err),
            Self::TarballHasWrongHash { tarball_url, expected_hash, recomputed_hash }
                => write!(f, "Hash for {:?} was {:?} but expected {:?}",
                          tarball_url, recomputed_hash, expected_hash),
            Self::DllIsUnreadable { dll_path, source }
                => write!(f, "Error opening dynamic library {:?}: {}",
                          dll_path, source),
            Self::DllSymbolIsMissing { dll_path, symbol_name, source }
                => write!(f, "Could not find {:?} in {:?}: {}",
                          symbol_name, dll_path, source),
            Self::CannotFindAppDirectory { source }
                => write!(f, "tree-sitter-loader failed to load: {}",
                          *source),
            Self::CompileFailed { src_path, source }
                => write!(f, "Could not compile grammar at {:?}: {}",
                          src_path, *source),
            Self::LanguageWasNotBuiltIn(language_name)
                => write!(f, "Support for language {:?} was not enabled at compile time",
                          language_name),
            Self::NotAllowedToDownload(url)
                => write!(f, "User did not allow us to download from {:?}",
                          url),
        }
    }
}

#[derive(Debug)]
pub struct CalledProcessError {
    command: String,
    source: CalledProcessErrorSource,
}

#[derive(Debug)]
pub enum CalledProcessErrorSource {
    Io(std::io::Error),
    ExitStatus(std::process::ExitStatus),
}

impl std::fmt::Display for CalledProcessErrorSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "failed to run: {}", e),
            Self::ExitStatus(e) => write!(f, "exited {}", e),
        }
    }
}

impl From<std::io::Error> for CalledProcessErrorSource {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<std::process::ExitStatus> for CalledProcessErrorSource {
    fn from(value: std::process::ExitStatus) -> Self {
        Self::ExitStatus(value)
    }
}

// Public API

impl Loader {
    pub fn new(
        sources_dir: std::path::PathBuf,
        parser_lib_path: Option<std::path::PathBuf>,
        downloads_policy: DownloadsPolicy,
    ) -> Result<Self, LoaderError> {
        Ok(Self {
            cache: std::collections::HashMap::new(),
            loader: match parser_lib_path {
                None => tree_sitter_loader::Loader::new().map_err(|e| {
                    LoaderError::CannotFindAppDirectory {
                        source: Box::new(e),
                    }
                })?,
                Some(parser_lib_path) => {
                    tree_sitter_loader::Loader::with_parser_lib_path(parser_lib_path)
                }
            },
            sources_dir,
            downloads_policy,
        })
    }

    pub fn get_language(
        &mut self,
        source: &ParserSource,
    ) -> Result<Option<std::rc::Rc<tree_sitter::Language>>, LoaderError> {
        Ok(match self.cache.entry(source.clone()) {
            std::collections::hash_map::Entry::Occupied(e) => e.get().clone(),
            std::collections::hash_map::Entry::Vacant(e) => e
                .insert(Some(std::rc::Rc::new(get_language(
                    &mut self.loader,
                    source,
                    &self.sources_dir,
                    self.downloads_policy,
                )?)))
                .clone(),
        })
    }
}

fn get_language(
    loader: &mut tree_sitter_loader::Loader,
    source: &ParserSource,
    sources_dir: &std::path::Path,
    downloads_policy: DownloadsPolicy,
) -> Result<tree_sitter::Language, LoaderError> {
    use std::str::FromStr;
    match source {
        ParserSource::Static(language_name) => {
            if let Ok(LanguageName::PYTHON) = LanguageName::from_str(language_name.as_ref()) {
                if let Some(language) = get_builtin_language_python() {
                    return Ok(language);
                }
            }
            Err(LoaderError::LanguageWasNotBuiltIn(language_name.to_owned()))
        }
        ParserSource::AbsolutePath(src_path) => {
            load_language_at_path(loader, std::path::Path::new(src_path), false)
        }
        ParserSource::GitSource(git) => {
            let repo_name = match git.clone.rsplit_once('/') {
                Some((_, right)) => right,
                None => match git.clone.split_once(':') {
                    Some((_, right)) => right,
                    None => &git.clone,
                },
            };
            let local_repo = sources_dir.join(repo_name);
            std::fs::create_dir_all(&local_repo).map_err(|e| {
                LoaderError::CannotMakeDirectoryForGit {
                    repo_url: git.clone.to_owned(),
                    repo_path: local_repo.to_owned(),
                    source: e,
                }
            })?;
            git_clone(&git.clone, &git.commit, &local_repo, downloads_policy)?;
            let src_path = match &git.subdirectory {
                None => local_repo,
                Some(sub) => local_repo.join(sub),
            };
            load_language_at_path(loader, &src_path, false)
        }
        ParserSource::TarballSource(tarball) => {
            let tarball_path = sources_dir.join(&tarball.name).with_extension("tar");
            download_tarball(
                &tarball.url,
                &tarball.sha256hex,
                &tarball_path,
                downloads_policy,
            )?;
            if let Some(language) = load_language_if_tarball_older(loader, tarball, sources_dir) {
                if tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION <= language.abi_version()
                    && language.abi_version() <= tree_sitter::LANGUAGE_VERSION
                {
                    return Ok(language);
                }
            }
            let tarball_root = extract_tarball(&tarball_path)?;
            let src_path = if tarball.subdirectory == "." {
                tarball_root.path().to_path_buf()
            } else {
                tarball_root.path().join(&tarball.subdirectory)
            };
            load_language_at_path(loader, &src_path, true)
        }
    }
}

fn load_language_at_path(
    loader: &mut tree_sitter_loader::Loader,
    src_path: &std::path::Path,
    force_rebuild: bool,
) -> Result<tree_sitter::Language, LoaderError> {
    if !force_rebuild {
        if let Ok(language) = loader
            .load_language_at_path(tree_sitter_loader::CompileConfig::new(src_path, None, None))
        {
            if tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION <= language.abi_version()
                && language.abi_version() <= tree_sitter::LANGUAGE_VERSION
            {
                return Ok(language);
            }
        }
    }
    loader.force_rebuild(true);
    let result = loader
        .load_language_at_path(tree_sitter_loader::CompileConfig::new(src_path, None, None))
        .map_err(|e| LoaderError::CompileFailed {
            src_path: src_path.to_owned(),
            source: Box::new(e),
        });
    loader.force_rebuild(false);
    result
}

fn load_language_if_tarball_older(
    loader: &tree_sitter_loader::Loader,
    tarball: &TarballSource,
    sources_dir: &std::path::Path,
) -> Option<tree_sitter::Language> {
    let tarball_path = sources_dir.join(&tarball.name).with_extension("tar");
    let dll_path = loader
        .parser_lib_path
        .join(&tarball.name)
        .with_extension(std::env::consts::DLL_EXTENSION);
    let Ok(dll_metadata) = std::fs::metadata(&dll_path) else {
        return None;
    };
    let Ok(tarball_metadata) = std::fs::metadata(&tarball_path) else {
        return None;
    };
    let Ok(dll_timestamp) = dll_metadata.modified() else {
        return None;
    };
    let Ok(tarball_timestamp) = tarball_metadata.modified() else {
        return None;
    };
    if tarball_timestamp >= dll_timestamp {
        return None;
    }
    let Ok(language) = unsafe_load(&dll_path, &tarball.name) else {
        return None;
    };
    Some(language)
}

// primitives

fn stdout_if_success(mut command: std::process::Command) -> Result<Vec<u8>, CalledProcessError> {
    let output = command.output();
    match output {
        Ok(o) if o.status.success() => Ok(o.stdout),
        Ok(o) => Err(CalledProcessError {
            command: format!("{:?}", command),
            source: o.status.into(),
        }),
        Err(e) => Err(CalledProcessError {
            command: format!("{:?}", command),
            source: e.into(),
        }),
    }
}

fn git_clone(
    repo_url: &str,
    checkoutable: &str,
    dest_path: &std::path::Path,
    downloads_policy: DownloadsPolicy,
) -> Result<(), LoaderError> {
    use os_str_bytes::OsStrBytes;
    use os_str_bytes::OsStrBytesExt;

    // clone if we don't have a repo
    // TODO set GIT_HTTP_USER_AGENT to "git/$(git version | cut -d' ' -f3) (dook X.Y.Z)"
    // some servers discriminate so it might be necessary to fallback to default user agent
    // but in the case of reactive blocks we should fix the provoking bug rather than circumvent
    if let Ok(origin_url_bytes) = git(dest_path, ["remote", "get-url", "origin"]) {
        // fail if we have the wrong remote (we could clobber but let's make the user delete it manually)
        let existing_remote_url = std::ffi::OsStr::from_io_bytes(&origin_url_bytes)
            .unwrap_or_else(|| std::ffi::OsStr::new(""))
            .trim_end_matches("\n")
            .trim_end_matches("\r");
        if existing_remote_url != repo_url {
            return Err(LoaderError::GitHasWrongRemote {
                repo_path: dest_path.to_owned(),
                desired_repo_url: repo_url.to_owned(),
                existing_repo_url: existing_remote_url.to_owned(),
            });
        }
    } else {
        if !can_download(repo_url, downloads_policy) {
            return Err(LoaderError::NotAllowedToDownload(repo_url.to_owned()));
        }
        let mut command = std::process::Command::new("git");
        command
            .args(["clone", "--filter=blob:none", repo_url])
            .arg(dest_path) // blob:none if likely to reuse, tree:0 if disposable
            .stderr(std::process::Stdio::inherit());
        stdout_if_success(command).map_err(|e| LoaderError::ChildProcessFailed {
            verb: format!("clone {:?} to {:?}", repo_url, dest_path),
            source: e,
        })?;
    }

    // fetch if we don't have the rev
    if git(
        dest_path,
        [
            "rev-parse",
            "--quiet",
            "--verify",
            &(String::from(checkoutable) + "^{commit}"),
        ],
    )
    .is_err()
    {
        if !can_download(repo_url, downloads_policy) {
            return Err(LoaderError::NotAllowedToDownload(repo_url.to_owned()));
        }
        git(dest_path, ["fetch"]).map_err(|e| LoaderError::ChildProcessFailed {
            verb: format!("fetch {:?} to {:?}", repo_url, dest_path),
            source: e,
        })?;
    }

    // checkout if HEAD is not the rev
    let current_head_bytes =
        git(dest_path, ["rev-parse", "--quiet", "--verify", "HEAD"]).map_err(|e| {
            LoaderError::ChildProcessFailed {
                verb: format!("determine HEAD in {:?}", dest_path),
                source: e,
            }
        })?;
    let current_head = std::ffi::OsStr::from_io_bytes(&current_head_bytes)
        .ok_or_else(|| LoaderError::GitHeadIsInvalid {
            repo_path: dest_path.to_owned(),
            head: current_head_bytes.clone(),
        })?
        .trim_end_matches("\n")
        .trim_end_matches("\r");
    if current_head != checkoutable {
        git(dest_path, ["checkout", checkoutable]).map_err(|e| {
            LoaderError::ChildProcessFailed {
                verb: format!("checkout {:?} to {:?}", repo_url, checkoutable),
                source: e,
            }
        })?;
    }

    Ok(())
}

fn git<I, S>(repo_root: &std::path::Path, args: I) -> Result<Vec<u8>, CalledProcessError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = std::process::Command::new("git");
    command
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .stderr(std::process::Stdio::inherit());
    stdout_if_success(command)
}

fn download_tarball(
    tarball_url: &str,
    sha256hex: &str,
    tarball_path: &std::path::Path,
    downloads_policy: DownloadsPolicy,
) -> Result<(), LoaderError> {
    let mut expected: [u8; 32] = [0; 32];
    base16ct::mixed::decode(sha256hex, &mut expected).map_err(|e| {
        LoaderError::ExpectedHashIsInvalid {
            tarball_url: tarball_url.to_owned(),
            expected_sha256hex: sha256hex.to_owned(),
            err: e,
        }
    })?;

    // if not offline, check hash. if no match (or no file), download again
    let offline = downloads_policy == DownloadsPolicy::No;
    let redownload = !offline
        && match hash_file_at_path(tarball_path) {
            Ok(existing_hash) => existing_hash.as_slice() != expected,
            Err(_) => true,
        };
    if redownload {
        if !can_download(tarball_url, downloads_policy) {
            return Err(LoaderError::NotAllowedToDownload(tarball_url.to_owned()));
        }
        let mut command = std::process::Command::new("curl");
        command
            .args(["--output"])
            .arg(tarball_path)
            .args(["-LsS", tarball_url])
            .stderr(std::process::Stdio::inherit());
        stdout_if_success(command).map_err(|e| LoaderError::ChildProcessFailed {
            verb: format!("download {:?}", tarball_url),
            source: e,
        })?;
    }

    // check hash before returning if we haven't already
    if redownload || offline {
        let recomputed =
            hash_file_at_path(tarball_path).map_err(|e| LoaderError::TarballIsUnreadable {
                tarball_path: tarball_path.to_owned(),
                err: e,
            })?;
        if recomputed.as_slice() != expected {
            let mut recomputed_hex_buf: Vec<u8> = vec![0; 2 * recomputed.len()];
            return Err(LoaderError::TarballHasWrongHash {
                tarball_url: tarball_url.to_owned(),
                expected_hash: sha256hex.to_owned(),
                recomputed_hash: base16ct::lower::encode_str(
                    recomputed.as_slice(),
                    &mut recomputed_hex_buf,
                )
                .expect("sorry I set the wrong buffer size for base16ct::lower::encode_str")
                .to_owned(),
            });
        }
    }

    Ok(())
}

fn extract_tarball(tarball_path: &std::path::Path) -> Result<tempfile::TempDir, LoaderError> {
    // extract into temporary directory
    let output_dir =
        tempfile::tempdir().map_err(|e| LoaderError::CannotMakeDirectoryForTarball {
            tarball_path: tarball_path.to_owned(),
            err: e,
        })?;
    let mut command = std::process::Command::new("tar");
    command
        .arg("-C")
        .arg(output_dir.path())
        .arg("-xmkf")
        .arg(tarball_path)
        .stderr(std::process::Stdio::inherit());
    stdout_if_success(command).map_err(|e| LoaderError::ChildProcessFailed {
        verb: format!("extract {:?}", tarball_path),
        source: e,
    })?;

    Ok(output_dir)
}

fn hash_file_at_path(path: &std::path::Path) -> std::io::Result<digest::Output<sha2::Sha256>> {
    use digest::Digest;
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
    Ok(hasher.finalize())
}

/// Load a Language from a shared library. Pasted from tree-sitter-loader 0.25.2,
/// from the end of tree_sitter_loader::Loader::load_language_at_path_with_name.
fn unsafe_load<P>(dll_path: &P, language_name: &str) -> Result<tree_sitter::Language, LoaderError>
where
    P: AsRef<std::ffi::OsStr>,
{
    let library = unsafe { libloading::Library::new(dll_path) }.map_err(|e| {
        LoaderError::DllIsUnreadable {
            dll_path: dll_path.as_ref().to_owned(),
            source: e,
        }
    })?;
    let language_fn_name = format!("tree_sitter_{}", language_name.replace("-", "_"));
    let language = unsafe {
        let language_fn = library
            .get::<libloading::Symbol<unsafe extern "C" fn() -> tree_sitter::Language>>(
                language_fn_name.as_bytes(),
            )
            .map_err(|e| LoaderError::DllSymbolIsMissing {
                dll_path: dll_path.as_ref().to_owned(),
                symbol_name: language_fn_name,
                source: e,
            })?;
        language_fn()
    };
    // prevent `library` from unloading since it'd invalidate `language`
    std::mem::forget(library);
    Ok(language)
}

// Statically compiled languages

#[cfg(not(feature = "static_python"))]
fn get_builtin_language_python() -> Option<tree_sitter::Language> {
    None
}

#[cfg(feature = "static_python")]
fn get_builtin_language_python() -> Option<tree_sitter::Language> {
    Some(tree_sitter_python::LANGUAGE.into())
}
