// Prior art:
//     https://github.com/simonw/symbex
//     https://github.com/newlinedotco/cq
//     git grep -W
//     https://dandavison.github.io/delta/grep.html
//     https://docs.github.com/en/repositories/working-with-files/using-files/navigating-code-on-github#precise-and-search-based-navigation

use crate::language_name::LanguageName;
use enum_derive_2018::EnumFromInner;
use etcetera::AppStrategy;

mod config;
mod dep_resolution;
mod dumptree;
mod language_name;
mod loader;
mod paging;
mod range_union;
mod searches;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum EnablementLevel {
    #[default]
    Auto,
    Never,
    Always,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum WrapMode {
    #[default]
    Auto,
    Never,
    Character,
}

#[derive(clap::Parser, Debug)]
/// dook: Definition lookup in your code.
struct Cli {
    /// Regex to match against symbol names. Required unless using --dump.
    pattern: Option<regex::Regex>,

    /// Config file path (default: ~/.config/dook/dook.yml)
    #[arg(
        short,
        long,
        required = false,
        help = format!("Config file path (default: {})", match config::default_config_path() {
            None => String::from("unset"),
            Some(p) => format!("{:?}", p),
        })
    )]
    config: Option<std::path::PathBuf>,

    /// Read from standard input instead of searching current directory
    /// (makes language detection slower)
    #[cfg(feature = "stdin")]
    #[arg(long)]
    stdin: bool,

    #[arg(
        long,
        help = format!("Use only the parsers already downloaded to {:?}", match config::dirs() {
            Ok(d) => d.cache_dir().join("sources"),
            Err(_) => std::path::PathBuf::new(),
        })
    )]
    offline: bool,

    #[arg(long, value_enum, default_value_t)]
    color: EnablementLevel,

    #[arg(long, value_enum, default_value_t)]
    paging: EnablementLevel,

    #[arg(
        long,
        value_enum,
        default_value_t,
        default_value_if("_chop_long_lines", clap::builder::ArgPredicate::IsPresent, "never")
    )]
    wrap: WrapMode,

    /// Alias for --wrap=never.
    #[arg(short = 'S', long)]
    _chop_long_lines: bool,

    /// Apply no styling; specify twice to also disable paging.
    #[arg(short, long, action = clap::ArgAction::Count)]
    plain: u8,

    /// Recurse if the definition contains exactly one function or constructor call.
    #[arg(short, long)]
    recurse: bool,

    /// Don't recurse (default).
    #[arg(long, overrides_with = "recurse")]
    _no_recurse: bool,

    /// Dump the syntax tree of the specified file, for debugging extraction queries.
    #[arg(long, required = false)]
    dump: Option<std::path::PathBuf>,

    /// Print only names matching the pattern, probably for shell completions.
    #[arg(long)]
    only_names: bool,

    /// Print unstructured messages about progress, for diagnostics.
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

macro_attr_2018::macro_attr! {
    #[derive(Debug, EnumFromInner!)]
    enum DookError {
        IoError(std::io::Error),
        FileParse(searches::FileParseError),
        LoaderError(loader::LoaderError),
        HomeDirError(etcetera::HomeDirError),
        RipGrepError(RipGrepError),
        PagerWriteError(PagerWriteError),
    }
}

impl std::fmt::Display for DookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DookError::FileParse(e) => write!(f, "{}", e),
            DookError::IoError(e) => write!(f, "{}", e),
            DookError::LoaderError(e) => write!(f, "{}", e),
            DookError::HomeDirError(e) => write!(f, "{}", e),
            DookError::RipGrepError(e) => write!(f, "{}", e),
            DookError::PagerWriteError(e) => write!(f, "{}", e),
        }
    }
}

fn main() -> Result<std::process::ExitCode, DookError> {
    match main_inner() {
        // if stdout is gone, let's just leave quietly
        Err(DookError::PagerWriteError(PagerWriteError::BrokenPipe(_))) => {
            Ok(std::process::ExitCode::SUCCESS)
        }
        result => result,
    }
}

fn main_inner() -> Result<std::process::ExitCode, DookError> {
    use clap::Parser;
    use std::str::FromStr;

    env_logger::init();

    // grab cli args
    let cli = Cli::parse();
    let use_color = if cli.color != EnablementLevel::Auto {
        cli.color
    } else if console::colors_enabled() {
        EnablementLevel::Always
    } else {
        EnablementLevel::Never
    };

    // load config
    let custom_config = config::Config::load(&cli.config)?;
    let default_config = config::Config::load_default();
    let merged_config = match custom_config {
        None => default_config,
        Some(custom_config) => default_config.merge(custom_config),
    };

    let parser_src_path = config::dirs()?.cache_dir().join("sources");
    let mut language_loader = loader::Loader::new(parser_src_path, None, cli.offline)?;
    let mut query_compiler = config::QueryCompiler::new(&merged_config);

    // check for dump-parse mode
    if let Some(dump_target) = cli.dump {
        let file_bytes = std::fs::read(&dump_target)?;
        let language_name = searches::detect_language_from_path(&dump_target)?;
        let parser_source = merged_config
            .get_parser_source(language_name)
            .ok_or_else(|| {
                searches::FileParseError::UnsupportedLanguage(language_name.to_string())
            })?;
        let language = language_loader.get_language(parser_source)?.unwrap();
        let file_info =
            searches::ParsedFile::from_bytes_and_language(&file_bytes, language_name, &language)?;
        dumptree::dump_tree(
            &file_info.tree,
            file_bytes.as_slice(),
            use_color == EnablementLevel::Always,
        );
        return Ok(std::process::ExitCode::SUCCESS);
    }

    // get pattern
    let mut current_pattern = cli
        .pattern
        .as_ref()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "pattern is required unless using --dump",
            )
        })?
        .to_owned();
    // store previous patterns to break --recurse cycles
    let mut local_patterns: std::vec::Vec<regex::Regex> = vec![];

    // set up output
    let enable_paging = if cli.paging != EnablementLevel::Auto {
        cli.paging == EnablementLevel::Always
    } else {
        cli.plain < 2 && console::Term::stdout().is_term()
    };
    let mut pager = paging::MaybePager::new(enable_paging, cli.wrap == WrapMode::Never);
    let bat_size = console::Term::stdout().size_checked();

    // deduplicate names found under --only-names
    let mut print_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    // parse stdin only once, and upfront, if asked to read it
    let parse_start = std::time::Instant::now();
    let stdin = parse_stdin(&cli)?;
    let use_stdin = stdin.is_some();
    if use_stdin && cli.verbose >= 1 {
        if let Some(stdin) = stdin.as_ref() {
            write_output_line(
                &mut pager,
                format!(
                    "V: parsed stdin as {} in {:?}",
                    stdin.language_name,
                    parse_start.elapsed(),
                )
                .as_bytes(),
            )?;
        }
    }

    for is_first_loop in std::iter::once(true).chain(std::iter::repeat(false)) {
        // track recursion
        let mut recurse_defs: std::vec::Vec<String> = vec![];
        local_patterns.push(
            regex::Regex::new(&(String::from("^(") + current_pattern.as_str() + ")$"))
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?,
        );
        // pattern to match all captured names against when searching through files
        let local_pattern = local_patterns
            .last()
            .expect("last() should exist on a vec we just pushed to");
        // pass 0: find candidate files with ripgrep
        if cli.verbose >= 1 {
            write_output_line(
                &mut pager,
                format!("V: invoking ripgrep with {:?}", local_pattern).as_bytes(),
            )?;
        }
        let mut filenames: std::collections::VecDeque<Option<std::path::PathBuf>> =
            if use_stdin && is_first_loop {
                std::collections::VecDeque::from([None])
            } else {
                let ripgrep_results = ripgrep(&current_pattern).filter_map(|f| match f {
                    Ok(p) => Some(Some(p)),
                    Err(e) => {
                        eprintln!("{}", e);
                        None
                    }
                });
                if use_stdin {
                    std::iter::once(None).chain(ripgrep_results).collect()
                } else {
                    ripgrep_results.collect()
                }
            };
        if cli.verbose >= 1 {
            let file_count = if use_stdin {
                filenames.len().saturating_sub(1)
            } else {
                filenames.len()
            };
            write_output_line(
                &mut pager,
                format!("V: ripgrep found {} files", file_count).as_bytes(),
            )?;
        }
        // track import origins seen so far
        let mut import_origins: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        while let Some(path) = filenames.pop_front() {
            if cli.verbose >= 1 {
                match &path {
                    None => write_output_line(&mut pager, "V: parsing stdin".as_bytes())?,
                    Some(path) => {
                        write_output_line(&mut pager, format!("V: parsing {:?}", path).as_bytes())?
                    }
                }
            }

            // read the whole file as few times as possible:
            // - only before traversing the injections tree
            // - only after we know we'll be able to do anything with the language
            let file_bytes: Vec<u8>;
            let (file_bytes, root_language) = match &path {
                None => {
                    let stdin = stdin.as_ref().expect("oops we weren't given --stdin but somehow we queued stdin to search anyway");
                    (stdin.bytes.as_slice(), stdin.language_name)
                }
                Some(path) => {
                    let language_name = match searches::detect_language_from_path(path) {
                        Ok(language_name) => language_name,
                        Err(e) => {
                            let error_message = format!("Skipping {:?}: {}", &path, e);
                            write_output_line(&mut pager, error_message.as_bytes())?;
                            continue;
                        }
                    };
                    file_bytes = match std::fs::read(path) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            let error_message = format!("Skipping unreadable {:?}: {}", &path, e);
                            write_output_line(&mut pager, error_message.as_bytes())?;
                            continue;
                        }
                    };
                    (file_bytes.as_slice(), language_name)
                }
            };
            // parse the whole file, then injections
            let mut injections: Vec<Option<searches::InjectionRange>> = vec![None];
            while let Some(injection) = injections.pop() {
                // determine language
                let parse_start = std::time::Instant::now();
                let language_name = match &injection {
                    None => root_language,
                    Some(injection) => {
                        match injection
                            .language_hint
                            .as_ref()
                            .and_then(|hint| LanguageName::from_str(hint).ok())
                        {
                            Some(hinted) => hinted,
                            None => match searches::detect_language_from_bytes(
                                &file_bytes[injection.range.start_byte..injection.range.end_byte],
                            ) {
                                Ok(detected) => detected,
                                Err(e) => {
                                    let error_message = format!(
                                        "Skipping embedded document at {:?}:{:?}: {}",
                                        &path, injection.range, e
                                    );
                                    write_output_line(&mut pager, error_message.as_bytes())?;
                                    continue;
                                }
                            },
                        }
                    }
                };
                // get language parser
                let parser_source =
                    merged_config
                        .get_parser_source(language_name)
                        .ok_or_else(|| {
                            searches::FileParseError::UnsupportedLanguage(language_name.to_string())
                        })?;
                let language = match language_loader.get_language(parser_source)? {
                    None => {
                        let error_message = format!(
                            "Skipping {:?} for previously failed language {}",
                            path, language_name
                        );
                        write_output_line(&mut pager, error_message.as_bytes())?;
                        continue;
                    }
                    Some(language) => language,
                };
                // get search patterns
                let language_info = match query_compiler.get_language_info(language_name, &language)
                {
                    Ok(language_info) => language_info,
                    Err(e) => {
                        write_output_line(&mut pager, e.to_string().as_bytes())?;
                        continue;
                    }
                };
                let file_info = match searches::ParsedFile::from_bytes_and_language_ranged(
                    file_bytes,
                    language_name,
                    &language,
                    injection.clone().map(|i| i.range),
                ) {
                    Ok(f) => f,
                    Err(e) => {
                        let error_message = format!("Skipping {:?}: {}", &path, e);
                        write_output_line(&mut pager, error_message.as_bytes())?;
                        continue;
                    }
                };

                // TODO expand this to sit between previous steps too
                if cli.verbose >= 1 {
                    write_output_line(
                        &mut pager,
                        format!(
                            "V: parsed {:?} as {:?} in {:?}",
                            injection.clone().map(|i| i.range),
                            file_info.language_name,
                            parse_start.elapsed()
                        )
                        .as_bytes(),
                    )?;
                }

                // search with tree_sitter
                if cli.only_names {
                    for name in searches::find_names(
                        file_bytes,
                        &file_info.tree,
                        &language_info,
                        local_pattern,
                    ) {
                        if print_names.insert(name.clone()) {
                            write_output_line(&mut pager, name.as_bytes())?;
                        }
                    }
                } else {
                    let search_result = searches::find_definition(
                        file_bytes,
                        &file_info.tree,
                        &language_info,
                        local_pattern,
                        true,
                    );
                    if cli.verbose >= 1 {
                        write_output_line(
                            &mut pager,
                            format!("V: search results = {:?}", search_result,).as_bytes(),
                        )?;
                    }
                    if !search_result.ranges.is_empty() {
                        // It could be nice to do a single bat invocation in the
                        // rare case that consecutive recursions hit the same file,
                        // but printing results as they come gets in the way.
                        match write_ranges(
                            &mut pager,
                            path.as_ref(),
                            &search_result.ranges,
                            &cli,
                            use_color,
                            bat_size,
                            stdin.as_ref(),
                        ) {
                            // if stdout is gone, just leave quietly
                            Err(PagerWriteError::BrokenPipe(_)) => {
                                return Ok(std::process::ExitCode::SUCCESS)
                            }
                            _ if pager.is_abandoned() => {
                                return Ok(std::process::ExitCode::SUCCESS)
                            }
                            // for other errors, print and continue
                            Err(e) => {
                                let error_message = format!("Error reading {:?}: {}", path, e);
                                pager.write_line(error_message.as_bytes())?;
                            }
                            Ok(_) => (),
                        }
                        recurse_defs.extend(search_result.recurse_names.into_iter().filter(
                            |name| local_patterns.iter().all(|pattern| !pattern.is_match(name)),
                        ));
                    }

                    // follow probable imports if we know about them
                    for import_pattern in search_result.import_origins {
                        if import_origins.insert(import_pattern.clone()) {
                            if cli.verbose >= 1 {
                                write_output_line(
                                    &mut pager,
                                    format!(
                                        "V: sorting files matching {:?} to the front",
                                        import_pattern
                                    )
                                    .as_bytes(),
                                )?;
                            }

                            filenames
                                .make_contiguous()
                                .sort_by_cached_key(|path| match path {
                                    None => 0,
                                    Some(path) => dep_resolution::dissimilarity(
                                        file_info.language_name,
                                        &import_pattern,
                                        path,
                                    ),
                                });
                        }
                    }
                }

                let new_injections = searches::find_injections(
                    file_bytes,
                    &file_info.tree,
                    &language_info,
                    &current_pattern,
                );
                if cli.verbose >= 1 {
                    write_output_line(
                        &mut pager,
                        format!("V: injections found: {:?}", new_injections).as_bytes(),
                    )?;
                }
                injections.extend(new_injections.into_iter().map(Some));
            }
        }

        // recursion
        recurse_defs.dedup();
        if cli.verbose >= 1 {
            write_output_line(
                &mut pager,
                format!("V: recursion candidates: {:?}", recurse_defs).as_bytes(),
            )?;
        }
        if cli.recurse && !cli.only_names && recurse_defs.len() == 1 {
            current_pattern = regex::Regex::new(&regex::escape(&recurse_defs[0])).unwrap();
        } else {
            break;
        }
    }

    // wait for pager
    match pager.wait() {
        Ok(0) => (),
        Ok(status) => eprintln!("Pager exited {}", status),
        Err(e) => eprintln!("Pager died or vanished: {}", e),
    }

    // yeah yeah whatever
    Ok(std::process::ExitCode::SUCCESS)
}

#[derive(Debug)]
enum PagerWriteError {
    IoError(std::io::Error),
    BrokenPipe(()),
    ReaderDied(std::process::ExitStatus),
}

impl std::fmt::Display for PagerWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PagerWriteError::IoError(e) => write!(f, "{}", e),
            PagerWriteError::BrokenPipe(_) => write!(f, "broken pipe (someone closed our output)"),
            PagerWriteError::ReaderDied(status) => {
                write!(f, "formatting excerpt exited {}", status)
            }
        }
    }
}

impl From<std::io::Error> for PagerWriteError {
    fn from(value: std::io::Error) -> Self {
        match value.kind() {
            std::io::ErrorKind::BrokenPipe => Self::BrokenPipe(()),
            _ => Self::IoError(value),
        }
    }
}

fn write_output_line(pager: &mut paging::MaybePager, line: &[u8]) -> Result<(), PagerWriteError> {
    match pager.write_line(line) {
        Ok(x) => Ok(x),
        Err(e) => {
            if pager.is_abandoned() {
                Err(PagerWriteError::BrokenPipe(()))
            } else {
                Err(e)?
            }
        }
    }
}

// this function signature is a disaster
fn write_ranges(
    pager: &mut paging::MaybePager,
    path: Option<&std::path::PathBuf>,
    ranges: &range_union::RangeUnion,
    cli: &Cli,
    use_color: EnablementLevel,
    bat_size: Option<(u16, u16)>,
    stdin: Option<&Stdin>,
) -> Result<(), PagerWriteError> {
    use std::io::Write;

    let mut cmd = std::process::Command::new("bat");
    let cmd = cmd
        .arg("--paging=never")
        .arg(format!("--wrap={:?}", cli.wrap).to_lowercase())
        .arg(format!("--color={:?}", use_color).to_lowercase());
    let cmd = match bat_size {
        Some((_rows, cols)) => cmd.arg(format!("--terminal-width={}", cols)),
        None => cmd,
    };
    let cmd = match cli.plain {
        0 => cmd,
        _ => cmd.arg("--plain"),
    };
    let cmd = cmd
        .args(
            ranges
                .iter_filling_gaps(1) // snip indicator - 8< - takes 1 line anyway
                .map(|x| format!("--line-range={}:{}", x.start + 1, x.end)), // bat end is inclusive
        )
        .stderr(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::piped());
    let mut child = match path {
        Some(path) => cmd.arg(path).spawn(),
        None => {
            let stdin = stdin.expect("oops we weren't given --stdin but somehow we claim we have results from stdin anyway");
            let mut child = cmd
                .arg(format!("-l{}", stdin.language_name,))
                .stdin(std::process::Stdio::piped())
                .spawn();
            if let Ok(child) = &mut child {
                let mut child_stdin = child
                    .stdin
                    .take()
                    .expect("BUG: should have launched bat with stdin=piped()");
                let stdin_clone = stdin.bytes.clone();
                std::thread::spawn(move || {
                    let result = child_stdin.write_all(&stdin_clone);
                    if paging::is_broken_pipe(&result) {
                    } else {
                        result.unwrap()
                    }
                });
            }
            child
        }
    }?;
    // std::io::copy uses more efficient syscalls than explicitly reading and writing
    let mut child_stdout = child
        .stdout
        .take()
        .expect("BUG: should have launched bat with stdout=piped()");
    let copy_result = std::io::copy(&mut child_stdout, pager);
    let wait_result = child.wait();
    copy_result?;
    let exit_status = wait_result?;
    if exit_status.success() {
        Ok(())
    } else {
        Err(PagerWriteError::ReaderDied(exit_status))
    }
}

struct Stdin {
    bytes: Vec<u8>,
    language_name: LanguageName,
}

#[cfg(not(feature = "stdin"))]
fn parse_stdin(_: &Cli) -> Result<Option<Stdin>, searches::FileParseError> {
    Ok(None)
}

#[cfg(feature = "stdin")]
fn parse_stdin(cli: &Cli) -> Result<Option<Stdin>, searches::FileParseError> {
    use std::io::Read;

    if !cli.stdin {
        return Ok(None);
    }
    let mut bytes = vec![];
    let language_name = match std::io::stdin().read_to_end(&mut bytes) {
        Err(e) => Err(searches::FileParseError::UnreadableFile(e.to_string())),
        Ok(_) if bytes.is_empty() => Err(searches::FileParseError::EmptyStdin),
        Ok(_) => searches::detect_language_from_bytes(&bytes),
    }?;
    Ok(Some(Stdin {
        bytes,
        language_name,
    }))
}

#[derive(Debug)]
enum RipGrepError {
    NotLaunched(std::io::Error),
    ReadFailed(std::io::Error),
    FileNameUnparseable(Vec<u8>),
}

#[rustfmt::skip] // keep compact
impl std::fmt::Display for RipGrepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RipGrepError::NotLaunched(e)
                => write!(f, "failed to run ripgrep: {}", e),
            RipGrepError::ReadFailed(e)
                => write!(f, "failed to read ripgrep output: {}", e),
            RipGrepError::FileNameUnparseable(filename)
                => write!(f, "ripgrep returned unreadable filename: {:?}", filename),
        }
    }
}

fn ripgrep(
    pattern: &regex::Regex,
) -> Box<dyn Iterator<Item = Result<std::path::PathBuf, RipGrepError>>> {
    use os_str_bytes::OsStrBytes;
    use std::io::BufRead;

    // first-pass search with ripgrep
    let mut rg = std::process::Command::new("rg");
    let mut child = match rg
        .args(["-l", "--sort=path", "-0"])
        .arg(pattern.as_str())
        .arg("./")
        .stderr(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return Box::new(std::iter::once(Err(RipGrepError::NotLaunched(e)))),
    };
    let child_stdout = child.stdout.take().unwrap();
    let rg_lines = std::io::BufReader::new(child_stdout).split(0);
    Box::new(rg_lines.filter_map(|x| match x {
        Err(e) => Some(Err(RipGrepError::ReadFailed(e))),
        Ok(x) => match std::ffi::OsStr::from_io_bytes(&x) {
            None => Some(Err(RipGrepError::FileNameUnparseable(x))),
            Some(y) => {
                if y.is_empty() {
                    None
                } else {
                    Some(Ok(std::path::PathBuf::from(y)))
                }
            }
        },
    }))
}
