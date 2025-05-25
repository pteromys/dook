use dook::inputs;
use dook::main_search;
use dook::searches;
use dook::{app_dirs, default_config_path};
use dook::{downloads_policy_path, get_downloads_policy, DownloadsPolicy};
use dook::{
    ConfigLoader, ConfigParseError, LanguageName, Loader, LoaderError, QueryCompiler,
    QueryCompilerError,
};
use enum_derive_2018::EnumFromInner;
use etcetera::AppStrategy;

mod dumptree;
mod outputs;
mod run_grep;
mod uncase;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum EnablementLevel {
    #[default]
    Auto,
    Never,
    Always,
}

impl From<EnablementLevel> for env_logger::fmt::WriteStyle {
    fn from(value: EnablementLevel) -> Self {
        match value {
            EnablementLevel::Auto => Self::Auto,
            EnablementLevel::Never => Self::Never,
            EnablementLevel::Always => Self::Always,
        }
    }
}

#[derive(clap::Parser, Debug)]
/// dook: Definition lookup in your code.
struct Cli {
    /// Regex to match against symbol names. Required unless using --dump.
    pattern: Option<String>,

    /// Config directory (default: ~/.config/dook)
    #[arg(
        short,
        long,
        required = false,
        help = format!("Config directory (default: {})", match default_config_path() {
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
        help = format!("Use only the parsers already downloaded to {:?} {}", match app_dirs() {
            Ok(d) => d.cache_dir().join("sources"),
            Err(_) => std::path::PathBuf::new(),
        }, "(alias for --download=no)")
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
    wrap: outputs::WrapMode,

    #[arg(
        long,
        value_enum,
        required = false,
        help = format!(
            "What to do if we need to download a parser (default: {} from {})",
            get_downloads_policy(),
            match downloads_policy_path() {
                None => "built-in".to_string(),
                Some(path) => format!("{path:?}"),
            })
    )]
    download: Option<DownloadsPolicy>,

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

    /// 1x = ignore lower vs upper; 2x = interconvert camelCase etc
    #[arg(short, long, action = clap::ArgAction::Count)]
    ignore_case: u8,

    /// Print unstructured messages about progress, for diagnostics.
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

macro_attr_2018::macro_attr! {
    #[derive(Debug, EnumFromInner!)]
    enum DookError {
        CliParse(&'static str),
        Regex(regex::Error),
        ConfigParse(ConfigParseError),
        Input(inputs::Error),
        FileParse(searches::FileParseError),
        LoaderError(LoaderError),
        QueryCompilerError(QueryCompilerError),
        HomeDirError(etcetera::HomeDirError),
        RipGrepError(run_grep::RipGrepError),
        PagerWriteError(outputs::PagerWriteError),
        NotRecaseable(uncase::NotRecaseable),
    }
}

impl std::fmt::Display for DookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DookError::CliParse(e) => write!(f, "{}", e),
            DookError::Regex(e) => write!(f, "{}", e),
            DookError::ConfigParse(e) => write!(f, "{}", e),
            DookError::Input(e) => write!(f, "{}", e),
            DookError::FileParse(e) => write!(f, "{}", e),
            DookError::LoaderError(e) => write!(f, "{}", e),
            DookError::QueryCompilerError(e) => write!(f, "{}", e),
            DookError::HomeDirError(e) => write!(f, "{}", e),
            DookError::RipGrepError(e) => write!(f, "{}", e),
            DookError::PagerWriteError(e) => write!(f, "{}", e),
            DookError::NotRecaseable(e) => write!(f, "{}", e),
        }
    }
}

fn main() -> Result<std::process::ExitCode, DookError> {
    match main_inner() {
        // if stdout is gone, let's just leave quietly
        Err(DookError::PagerWriteError(outputs::PagerWriteError::BrokenPipe)) => {
            Ok(std::process::ExitCode::from(141))
        }
        // on error, print a message and then exit 1
        Err(e) => {
            log::error!("{e}");
            Ok(std::process::ExitCode::FAILURE)
        }
        // forward Ok unmodified
        result => result,
    }
}

fn main_inner() -> Result<std::process::ExitCode, DookError> {
    use clap::Parser;
    use std::io::Write;

    // grab cli args
    let cli = Cli::parse();
    let use_color = if cli.color != EnablementLevel::Auto {
        cli.color
    } else if console::colors_enabled() {
        EnablementLevel::Always
    } else {
        EnablementLevel::Never
    };

    // get terminal properties before paging forks and we lose the tty
    let is_term = console::Term::stdout().is_term();
    let output_options = outputs::OutputOptions {
        wrap: cli.wrap,
        plain: cli.plain,
        use_color: use_color == EnablementLevel::Always,
        terminal_size: console::Term::stdout().size_checked(),
    };

    // see how much approval we have to download parsers
    let downloads_policy = match cli.offline {
        true => DownloadsPolicy::No,
        false => cli.download.unwrap_or_else(get_downloads_policy),
    };
    let downloads_policy = if downloads_policy == DownloadsPolicy::Ask && !is_term {
        DownloadsPolicy::No
    } else {
        downloads_policy
    };

    // set up output
    let enable_paging = match cli.paging {
        EnablementLevel::Always => true,
        EnablementLevel::Never => false,
        EnablementLevel::Auto => cli.plain < 2 && is_term,
    };
    if enable_paging && downloads_policy != DownloadsPolicy::Ask {
        let pager_command = match std::env::var_os("PAGER") {
            Some(value) => match value.into_string() {
                Ok(s) => s,
                Err(orig) => {
                    eprintln!("ignoring PAGER environment variable because it contains non-utf8: {orig:?}");
                    "less".to_string()
                }
            },
            None => "less".to_string(),
        };
        let pager_command = if pager_command == "less" {
            if cli.wrap == outputs::WrapMode::Never {
                format!("{pager_command} -RFS")
            } else {
                format!("{pager_command} -RF")
            }
        } else {
            pager_command
        };
        pager::Pager::with_pager(&pager_command).setup();
    }
    let mut stdout = std::io::stdout();

    // set logging level
    let mut logger_builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"));
    if cli.verbose >= 1 {
        logger_builder.filter_level(log::LevelFilter::Debug);
    }
    if enable_paging && downloads_policy != DownloadsPolicy::Ask {
        logger_builder.target(env_logger::Target::Stdout); // make logs visible in pager
        logger_builder.write_style(use_color.into()); // follow coloring of output passed to pager
    } else {
        logger_builder.target(env_logger::Target::Stderr); // don't mix with likely-parsed output
        logger_builder.write_style(cli.color.into()); // follow whatever --color says
    }
    logger_builder.init();

    // set up caches
    let config_loader = ConfigLoader::new(cli.config.clone().or_else(default_config_path));
    let parser_src_path = app_dirs()?.cache_dir().join("sources");
    let language_loader = Loader::new(parser_src_path, None, downloads_policy)?;
    let mut query_compiler = QueryCompiler::new(config_loader, language_loader);

    // check for dump-parse mode
    if let Some(dump_target) = cli.dump {
        let input = inputs::LoadedFile::load(dump_target)?;
        let language_info = query_compiler.get_language_info(input.language_name)?;
        let tree = searches::parse(&input.bytes, input.language_name, &language_info.language)?;
        dumptree::dump_tree(
            &tree,
            input.bytes.as_slice(),
            use_color == EnablementLevel::Always,
        )
        .map_err(outputs::PagerWriteError::from)?;
        maybe_warn_paging_vs_downloads_policy(enable_paging, downloads_policy);
        return Ok(std::process::ExitCode::SUCCESS);
    }

    // get pattern
    let raw_pattern = cli.pattern.to_owned().ok_or(DookError::CliParse(
        "pattern is required unless using --dump",
    ))?;
    let raw_pattern = if cli.ignore_case >= 2 {
        uncase::uncase(raw_pattern)?
    } else {
        raw_pattern
    };
    let mut current_pattern = regex::RegexBuilder::new(&raw_pattern)
        .case_insensitive(cli.ignore_case > 0)
        .build()?;
    // store previous patterns to break --recurse cycles
    let mut local_patterns: std::vec::Vec<regex::Regex> = vec![];

    // deduplicate names found under --only-names
    let mut print_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    // parse stdin only once, and upfront, if asked to read it
    let parse_start = std::time::Instant::now();
    let stdin = load_stdin(&cli)?;
    let use_stdin = stdin.is_some();
    if use_stdin {
        if let Some(stdin) = stdin.as_ref() {
            log::debug!(
                "parsed stdin as {} in {:?}",
                stdin.language_name,
                parse_start.elapsed(),
            );
        }
    }

    for is_first_loop in std::iter::once(true).chain(std::iter::repeat(false)) {
        let ignore_case = is_first_loop && cli.ignore_case > 0;
        // track recursion
        let mut recurse_defs: std::vec::Vec<String> = vec![];
        local_patterns.push(
            regex::RegexBuilder::new(&(String::from("^(") + current_pattern.as_str() + ")$"))
                .case_insensitive(ignore_case)
                .build()?,
        );
        // pattern to match all captured names against when searching through files
        let local_pattern = local_patterns
            .last()
            .expect("last() should exist on a vec we just pushed to");
        let search_params = main_search::SearchParams {
            local_pattern,
            current_pattern: &current_pattern,
            only_names: cli.only_names,
            recurse: cli.recurse,
        };
        // pass 0: find candidate files with ripgrep
        log::debug!("invoking ripgrep with {:?}", current_pattern);
        let mut filenames: std::collections::VecDeque<Option<std::path::PathBuf>> =
            if use_stdin && is_first_loop {
                std::collections::VecDeque::from([None])
            } else {
                let ripgrep_results =
                    run_grep::ripgrep(&current_pattern, ignore_case).filter_map(|f| match f {
                        Ok(p) => Some(Some(p)),
                        Err(e) => {
                            log::error!("{e}");
                            None
                        }
                    });
                if use_stdin {
                    std::iter::once(None).chain(ripgrep_results).collect()
                } else {
                    ripgrep_results.collect()
                }
            };
        log::debug!(
            "ripgrep found {} files",
            if use_stdin {
                filenames.len().saturating_sub(1)
            } else {
                filenames.len()
            }
        );
        // track import origins seen so far
        let mut import_origins: std::collections::HashSet<(LanguageName, String)> =
            std::collections::HashSet::new();
        while let Some(path) = filenames.pop_front() {
            let search_input = match path.as_ref() {
                Some(path) => inputs::SearchInput::Path(path),
                None => inputs::SearchInput::Loaded(stdin.as_ref().expect(
                    "oops we weren't given --stdin but somehow we queued stdin to search anyway",
                )),
            };

            // read the whole file as few times as possible
            // - only before traversing the injections tree
            // - only after we know we'll be able to do anything with the language
            // - but not accounting for changes in what we're looking for (recursion)
            log::debug!("parsing {search_input}");
            let path_input: inputs::LoadedFile;
            let loaded_file = match search_input {
                inputs::SearchInput::Loaded(f) => f,
                inputs::SearchInput::Path(path) => {
                    path_input =
                        match inputs::LoadedFile::load_if_parseable(path, &mut query_compiler) {
                            Err(inputs::Error::UnreadableFile(message)) => {
                                log::warn!("Skipping unreadable {path:?}: {message}");
                                continue;
                            }
                            Err(e) => {
                                log::warn!("Skipping {path:?}: {e}");
                                continue;
                            }
                            Ok(f) => f,
                        };
                    &path_input
                }
            };

            let result_vec = match main_search::search_one_file_and_all_subfiles(
                &search_params,
                loaded_file,
                &mut query_compiler,
            ) {
                Err(main_search::SinglePassError::Input(inputs::Error::UnreadableFile(
                    message,
                ))) => {
                    log::warn!("Skipping unreadable {search_input}: {message}");
                    continue;
                }
                Err(e) => {
                    log::warn!("Skipping {search_input}: {e}");
                    continue;
                }
                Ok(results) => results,
            };
            for main_search::SubfileResults { results, subfile } in result_vec {
                for name in results.matched_names {
                    if print_names.insert(name.clone()) {
                        writeln!(stdout, "{name}").map_err(outputs::PagerWriteError::from)?;
                    }
                }
                // It could be nice to do a single bat invocation in the
                // rare case that consecutive recursions hit the same file,
                // but printing results as they come gets in the way.
                if !results.ranges.is_empty() {
                    let range_target = match subfile.as_ref() {
                        Some(subfile) => inputs::SearchInput::Loaded(subfile),
                        None => search_input,
                    };
                    match outputs::write_ranges(range_target, &results.ranges, &output_options) {
                        // if stdout is gone, just leave quietly
                        Err(outputs::PagerWriteError::BrokenPipe) => {
                            Err(outputs::PagerWriteError::BrokenPipe)?
                        }
                        // otherwise continue, printing if there are errors
                        Err(e) => log::warn!("Error reading {range_target}: {e}"),
                        Ok(_) => (),
                    }
                }
                for name in results.recurse_names {
                    if local_patterns
                        .iter()
                        .all(|pattern| !pattern.is_match(&name))
                    {
                        recurse_defs.push(name)
                    }
                }
                // follow probable imports if we know about them
                for (language_name, import_pattern) in results.import_origins {
                    if import_origins.insert((language_name, import_pattern.clone())) {
                        log::debug!("sorting files matching {:?} to the front", import_pattern);
                        filenames
                            .make_contiguous()
                            .sort_by_cached_key(|path| match path {
                                None => 0,
                                Some(path) => dook::dep_resolution::dissimilarity(
                                    language_name,
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
        log::debug!("recursion candidates: {:?}", recurse_defs);
        if cli.recurse && !cli.only_names && recurse_defs.len() == 1 {
            current_pattern = regex::Regex::new(&regex::escape(&recurse_defs[0])).unwrap();
        } else {
            break;
        }
    }

    maybe_warn_paging_vs_downloads_policy(enable_paging, downloads_policy);

    // yeah yeah whatever
    Ok(std::process::ExitCode::SUCCESS)
}

fn maybe_warn_paging_vs_downloads_policy(enable_paging: bool, downloads_policy: DownloadsPolicy) {
    if enable_paging && downloads_policy == DownloadsPolicy::Ask {
        log::warn!(
            "{}{}",
            "Paging was disabled so we could ask to download new parsers if the need arose.",
            " To enable paging, use --download=yes or --download=no.",
        );
        if let Some(settings_path) = downloads_policy_path() {
            log::warn!("Or write YES or NO to {settings_path:#?}");
        }
    }
}

#[cfg(not(feature = "stdin"))]
fn load_stdin(_: &Cli) -> Result<Option<inputs::LoadedFile>, inputs::Error> {
    Ok(None)
}

#[cfg(feature = "stdin")]
fn load_stdin(cli: &Cli) -> Result<Option<inputs::LoadedFile>, inputs::Error> {
    if !cli.stdin {
        Ok(None)
    } else {
        inputs::LoadedFile::load_stdin().map(Some)
    }
}
