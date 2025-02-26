pub struct Loader {
    cache: std::collections::HashMap<ParserSource, Option<std::rc::Rc<tree_sitter::Language>>>,
    loader: tree_sitter_loader::Loader,
    sources_dir: std::path::PathBuf,
    offline: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ParserSource {
    AbsolutePath(String), // tree-sitter-loader will recompile if parser.c is newer than .so
    GitSource(GitSource), // clone/fetch/checkout/whatever, then handle like AbsolutePath
    TarballSource(TarballSource), // recompile if .tar is newer than .so
}

merde::derive! {
    impl (Serialize, Deserialize) for enum ParserSource
    externally_tagged {
        "path" => AbsolutePath,
        "git" => GitSource,
        "tarball" => TarballSource,
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

impl Loader {
    pub fn new(
        sources_dir: std::path::PathBuf,
        parser_lib_path: Option<std::path::PathBuf>,
        offline: bool,
    ) -> Self {
        Self {
            cache: std::collections::HashMap::new(),
            loader: match parser_lib_path {
                None => tree_sitter_loader::Loader::new().unwrap(),
                Some(parser_lib_path) => {
                    tree_sitter_loader::Loader::with_parser_lib_path(parser_lib_path)
                }
            },
            sources_dir,
            offline,
        }
    }

    pub fn get_language(
        &mut self,
        source: &ParserSource,
    ) -> anyhow::Result<Option<std::rc::Rc<tree_sitter::Language>>> {
        Ok(match self.cache.entry(source.clone()) {
            std::collections::hash_map::Entry::Occupied(e) => e.get().clone(),
            std::collections::hash_map::Entry::Vacant(e) => e
                .insert(Some(std::rc::Rc::new(get_language(
                    &mut self.loader,
                    source,
                    &self.sources_dir,
                    self.offline,
                )?)))
                .clone(),
        })
    }
}

fn get_language(
    loader: &mut tree_sitter_loader::Loader,
    source: &ParserSource,
    sources_dir: &std::path::Path,
    offline: bool,
) -> anyhow::Result<tree_sitter::Language> {
    match source {
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
            if !offline {
                std::fs::create_dir_all(&local_repo)?;
                git_clone(&git.clone, &git.commit, &local_repo)?;
            }
            let src_path = match &git.subdirectory {
                None => local_repo,
                Some(sub) => local_repo.join(sub),
            };
            load_language_at_path(loader, &src_path, false)
        }
        ParserSource::TarballSource(tarball) => {
            if let Some(language) = load_language_if_tarball_older(loader, tarball, sources_dir) {
                if tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION <= language.abi_version()
                    && language.abi_version() <= tree_sitter::LANGUAGE_VERSION
                {
                    return Ok(language);
                }
            }
            let tarball_path = sources_dir.join(&tarball.name).with_extension("tar");
            download_tarball(&tarball.url, &tarball.sha256hex, &tarball_path, offline)?;
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
) -> anyhow::Result<tree_sitter::Language> {
    if !force_rebuild {
        let language = loader
            .load_language_at_path(tree_sitter_loader::CompileConfig::new(src_path, None, None))?;
        if tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION <= language.abi_version()
            && language.abi_version() <= tree_sitter::LANGUAGE_VERSION
        {
            return Ok(language);
        }
    }
    loader.force_rebuild(true);
    let result =
        loader.load_language_at_path(tree_sitter_loader::CompileConfig::new(src_path, None, None));
    loader.force_rebuild(false);
    result
}

fn load_language_if_tarball_older(
    loader: &tree_sitter_loader::Loader,
    tarball: &TarballSource,
    sources_dir: &std::path::Path,
) -> Option<tree_sitter::Language> {
    //let ParserSource::TarballSource(tarball) = source else { return None };
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

fn git_clone(
    repo_url: &str,
    checkoutable: &str,
    dest_path: &std::path::Path,
) -> anyhow::Result<()> {
    use os_str_bytes::OsStrBytes;
    use os_str_bytes::OsStrBytesExt;

    // clone if we don't have a repo
    // TODO set GIT_HTTP_USER_AGENT to "git/$(git version | cut -d' ' -f3) (dook X.Y.Z)"
    // some servers discriminate so it might be necessary to fallback to default user agent
    // but in the case of reactive blocks we should fix the provoking bug rather than circumvent
    let origin_url_output = git(dest_path, ["remote", "get-url", "origin"])?;
    if !origin_url_output.status.success() {
        let clone_output = std::process::Command::new("git")
            .args(["clone", "--filter=blob:none", repo_url])
            .arg(dest_path) // blob:none if likely to reuse, tree:0 if disposable
            .stderr(std::process::Stdio::inherit())
            .output()?;
        if !clone_output.status.success() {
            return Err(anyhow::anyhow!(
                "Attempt to clone {:?} to {:?} exited {}",
                repo_url,
                dest_path,
                clone_output.status
            ));
        }
    } else {
        // fail if we have the wrong remote (we could clobber but let's make the user delete it manually)
        let existing_remote_url = std::ffi::OsStr::from_io_bytes(&origin_url_output.stdout)
            .unwrap_or_else(|| std::ffi::OsStr::new(""))
            .trim_end_matches("\n")
            .trim_end_matches("\r");
        if existing_remote_url != repo_url {
            return Err(anyhow::anyhow!(
                "repo exists at {:?} but points at {:?} instead of {:?}",
                dest_path,
                existing_remote_url,
                repo_url
            ));
        }
    }

    // fetch if we don't have the rev
    if !git(
        dest_path,
        [
            "rev-parse",
            "--quiet",
            "--verify",
            &(String::from(checkoutable) + "^{commit}"),
        ],
    )?
    .status
    .success()
    {
        let fetch_output = git(dest_path, ["fetch"])?;
        if !fetch_output.status.success() {
            return Err(anyhow::anyhow!(
                "Attempt to fetch remote in {:?} exited {}",
                dest_path,
                fetch_output.status
            ));
        }
    }

    // checkout if HEAD is not the rev
    let current_head_output = git(dest_path, ["rev-parse", "--quiet", "--verify", "HEAD"])?;
    if !current_head_output.status.success() {
        return Err(anyhow::anyhow!(
            "Attempt to read version of {:?} exited {}",
            dest_path,
            current_head_output.status
        ));
    }
    let current_head = std::ffi::OsStr::from_io_bytes(&current_head_output.stdout)
        .ok_or_else(|| anyhow::anyhow!("Version of {:?} not decodable", dest_path))?
        .trim_end_matches("\n")
        .trim_end_matches("\r");
    if current_head != checkoutable {
        let checkout_output = git(dest_path, ["checkout", checkoutable])?;
        if !checkout_output.status.success() {
            return Err(anyhow::anyhow!(
                "Attempt to checkout {:?} to {:?} exited {}",
                repo_url,
                checkoutable,
                checkout_output.status
            ));
        }
    }

    Ok(())
}

fn git<I, S>(repo_root: &std::path::Path, args: I) -> std::io::Result<std::process::Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .stderr(std::process::Stdio::inherit())
        .output()
}

fn download_tarball(
    tarball_url: &str,
    sha256hex: &str,
    tarball_path: &std::path::Path,
    offline: bool, // error out instead if we'd have to download
) -> anyhow::Result<()> {
    let mut expected: [u8; 32] = [0; 32];
    base16ct::mixed::decode(sha256hex, &mut expected)
        .map_err(|_| anyhow::anyhow!("Not a 256-bit hex value: {:?}", sha256hex))?;

    // if not offline, check hash. if no match (or no file), download again
    let redownload = !offline
        && match hash_file_at_path(tarball_path) {
            Ok(existing_hash) => existing_hash.as_slice() != expected,
            Err(_) => true,
        };
    if redownload {
        let curl_output = std::process::Command::new("curl")
            .args(["--output"])
            .arg(tarball_path)
            .args(["--no-clobber", "-LsS", tarball_url])
            .stderr(std::process::Stdio::inherit())
            .output()?;
        if !curl_output.status.success() {
            return Err(anyhow::anyhow!(
                "Attempt to download {:?} exited {}",
                tarball_url,
                curl_output.status
            ));
        }
    }

    // check hash before returning if we haven't already
    if redownload || offline {
        let recomputed = hash_file_at_path(tarball_path)?;
        if recomputed.as_slice() != expected {
            let mut recomputed_hex_buf: [u8; 64] = [0; 64];
            return Err(anyhow::anyhow!(
                "tarball hash was {:?} but expected {:?}",
                base16ct::lower::encode_str(recomputed.as_slice(), &mut recomputed_hex_buf)
                    .unwrap(),
                sha256hex
            ));
        }
    }

    Ok(())
}

fn extract_tarball(tarball_path: &std::path::Path) -> anyhow::Result<tempfile::TempDir> {
    // extract into temporary directory
    let output_dir = tempfile::tempdir()?;
    let tar_output = std::process::Command::new("tar")
        .arg("-C")
        .arg(output_dir.path())
        .arg("-xmkf")
        .arg(tarball_path)
        .stderr(std::process::Stdio::inherit())
        .output()?;
    if !tar_output.status.success() {
        return Err(anyhow::anyhow!(
            "Attempt to extract {:?} exited {}",
            tarball_path,
            tar_output.status
        ));
    }

    Ok(output_dir)
}

fn hash_file_at_path(path: &std::path::Path) -> anyhow::Result<digest::Output<sha2::Sha256>> {
    use digest::Digest;
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
    Ok(hasher.finalize())
}

/// Load a Language from a shared library. Pasted from tree-sitter-loader 0.25.2,
/// from the end of tree_sitter_loader::Loader::load_language_at_path_with_name.
fn unsafe_load<P>(dll_path: &P, language_name: &str) -> anyhow::Result<tree_sitter::Language>
where
    P: AsRef<std::ffi::OsStr>,
{
    use anyhow::Context;
    let library = unsafe { libloading::Library::new(dll_path) }
        .with_context(|| format!("Error opening dynamic library {:?}", dll_path.as_ref()))?;
    let language_fn_name = format!("tree_sitter_{}", language_name.replace("-", "_"));
    let language = unsafe {
        let language_fn = library
            .get::<libloading::Symbol<unsafe extern "C" fn() -> tree_sitter::Language>>(
                language_fn_name.as_bytes(),
            )
            .with_context(|| format!("Failed to load symbol {language_fn_name}"))?;
        language_fn()
    };
    std::mem::forget(library);
    Ok(language)
}
