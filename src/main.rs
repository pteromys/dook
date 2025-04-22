// Prior art:
//     https://github.com/simonw/symbex
//     https://github.com/newlinedotco/cq
//     git grep -W
//     https://dandavison.github.io/delta/grep.html
//     https://docs.github.com/en/repositories/working-with-files/using-files/navigating-code-on-github#precise-and-search-based-navigation

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
        let file_info = searches::ParsedFile::from_filename(
            &dump_target,
            &mut language_loader,
            &merged_config,
        )?;
        dumptree::dump_tree(
            &file_info.tree,
            file_info.source_code.as_slice(),
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
    let stdin = parse_stdin(&cli, &mut language_loader, &merged_config);
    let use_stdin = stdin.is_some();
    if use_stdin && cli.verbose >= 1 {
        if let Some(stdin) = stdin.as_ref() {
            write_output_line(
                &mut pager,
                format!(
                    "V: parsed stdin as {:?} in {:?}",
                    stdin.parsed.as_ref().unwrap().language_name,
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
        loop {
            let path = match filenames.pop_front() {
                None => break,
                Some(path) => path,
            };
            if cli.verbose >= 1 {
                match &path {
                    None => write_output_line(&mut pager, "V: parsing stdin".as_bytes())?,
                    Some(path) => {
                        write_output_line(&mut pager, format!("V: parsing {:?}", path).as_bytes())?
                    }
                }
            }
            // infer syntax
            let parse_start = std::time::Instant::now();
            let tmp_parsed_file: searches::ParsedFile; // storage for file_info if created on the fly
            let file_info = match &path {
                None => stdin.as_ref()
                    .expect("oops we weren't given --stdin but somehow we queued stdin to search anyway")
                    .parsed.as_ref().map_err(|e| e.clone())?,
                Some(path) => match searches::ParsedFile::from_filename(
                    path,
                    &mut language_loader,
                    &merged_config,
                ) {
                    Err(e) => {
                        let error_message = format!("Skipping {:?}: {}", &path, e);
                        write_output_line(&mut pager, error_message.as_bytes())?;
                        continue;
                    }
                    Ok(f) => {
                        tmp_parsed_file = f;
                        &tmp_parsed_file
                    }
                },
            };
            if cli.verbose >= 1 {
                write_output_line(
                    &mut pager,
                    format!(
                        "V: parsed as {:?} in {:?}",
                        file_info.language_name,
                        parse_start.elapsed()
                    )
                    .as_bytes(),
                )?;
            }
            // get corresponding search patterns
            let language_info = match query_compiler
                .get_language_info(file_info.language_name, &mut language_loader)
            {
                Ok(language_info) => language_info,
                Err(e) => {
                    write_output_line(&mut pager, e.to_string().as_bytes())?;
                    continue;
                }
            };
            // search with tree_sitter
            if cli.only_names {
                for name in searches::find_names(
                    file_info.source_code.as_slice(),
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
                    file_info.source_code.as_slice(),
                    &file_info.tree,
                    &language_info,
                    local_pattern,
                    true,
                );
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
                        _ if pager.is_abandoned() => return Ok(std::process::ExitCode::SUCCESS),
                        // for other errors, print and continue
                        Err(e) => {
                            let error_message = format!("Error reading {:?}: {}", path, e);
                            pager.write_line(error_message.as_bytes())?;
                        }
                        Ok(_) => (),
                    }
                    recurse_defs.extend(search_result.recurse_names.into_iter().filter(|name| {
                        local_patterns.iter().all(|pattern| !pattern.is_match(name))
                    }));
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
                .arg(format!(
                    "-l{}",
                    stdin.parsed.as_ref().unwrap().language_name_str
                ))
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
    parsed: Result<searches::ParsedFile, searches::FileParseError>,
}

#[cfg(not(feature = "stdin"))]
fn parse_stdin(_: &Cli, _: &mut loader::Loader, _: &config::Config) -> Option<Stdin> {
    None
}

#[cfg(feature = "stdin")]
fn parse_stdin(
    cli: &Cli,
    language_loader: &mut loader::Loader,
    merged_config: &config::Config,
) -> Option<Stdin> {
    use std::io::Read;

    if !cli.stdin {
        return None;
    }
    let mut bytes = vec![];
    let parsed = match std::io::stdin().read_to_end(&mut bytes) {
        Err(e) => Err(searches::FileParseError::UnreadableFile(
            searches::UnreadableFileError {
                message: e.to_string(),
                path: None,
            },
        )),
        Ok(_) if bytes.is_empty() => Err(searches::FileParseError::EmptyStdin(())),
        Ok(_) => searches::ParsedFile::from_bytes(bytes.clone(), language_loader, merged_config),
    };
    Some(Stdin { bytes, parsed })
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
