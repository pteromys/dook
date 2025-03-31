// Prior art:
//     https://github.com/simonw/symbex
//     https://github.com/newlinedotco/cq
//     git grep -W
//     https://dandavison.github.io/delta/grep.html
//     https://docs.github.com/en/repositories/working-with-files/using-files/navigating-code-on-github#precise-and-search-based-navigation

use etcetera::AppStrategy;

mod config;
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
        help = format!("Config file path (default: {:?})", config::default_config_path())
    )]
    config: Option<std::path::PathBuf>,

    /// Read from standard input instead of searching current directory
    /// (makes language detection slower)
    #[cfg(feature = "stdin")]
    #[arg(long)]
    stdin: bool,

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
}

fn main() -> std::io::Result<std::process::ExitCode> {
    use clap::Parser;
    use std::io::Write;

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

    let mut language_loader = match config::dirs() {
        Err(_) => loader::Loader::new(
            std::path::PathBuf::new(),
            Some(std::path::PathBuf::new()),
            true,
        ),
        Ok(d) => loader::Loader::new(d.cache_dir().join("sources"), None, false),
    };
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
    let mut current_pattern = match &cli.pattern {
        Some(pattern) => pattern.to_owned(),
        None => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "pattern is required unless using --dump",
            ))
        }
    };
    let mut local_patterns: std::vec::Vec<regex::Regex> = vec![];

    // store the result here
    let mut print_ranges: Vec<(Option<std::path::PathBuf>, range_union::RangeUnion)> = Vec::new();
    // parse stdin only once, and upfront, if asked to read it
    let (use_stdin, stdin, stdin_parsed) = parse_stdin(&cli, &mut language_loader, &merged_config);
    for is_first_loop in std::iter::once(true).chain(std::iter::repeat(false)) {
        // track recursion
        let mut recurse_defs: std::vec::Vec<String> = vec![];
        local_patterns.push(
            match regex::Regex::new(&(String::from("^(") + current_pattern.as_str() + ")$")) {
                Ok(p) => p,
                Err(e) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, e)),
            },
        );
        // pattern to match all captured names against when searching through files
        let local_pattern = local_patterns.last().unwrap();
        // pass 0: find candidate files with ripgrep
        let filenames: Box<dyn Iterator<Item = Option<std::path::PathBuf>>> =
            if use_stdin && is_first_loop {
                Box::new(std::iter::once(None))
            } else {
                let ripgrep_results = ripgrep(&current_pattern)?.into_iter().map(Some);
                if use_stdin {
                    Box::new(std::iter::once(None).chain(ripgrep_results))
                } else {
                    Box::new(ripgrep_results)
                }
            };
        for path in filenames {
            // infer syntax, then search with tree_sitter
            let tmp_parsed_file: searches::ParsedFile; // storage for file_info if created on the fly
            let file_info = match path {
                None => stdin_parsed.as_ref().unwrap(),
                Some(path) => match searches::ParsedFile::from_filename(
                    &path,
                    &mut language_loader,
                    &merged_config,
                ) {
                    Err(e) => {
                        eprintln!("Skipping {:?}: {:?}", &path, e);
                        continue;
                    }
                    Ok(f) => {
                        tmp_parsed_file = f;
                        &tmp_parsed_file
                    }
                },
            };
            let language_info = query_compiler
                .get_language_info(file_info.language_name, &mut language_loader)
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!(
                            "No config contains definitions for language: {:?}",
                            file_info.language_name
                        ),
                    )
                })?
                .map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{}", e))
                })?;
            let (new_ranges, new_recurses) = searches::find_definition(
                file_info.source_code.as_slice(),
                &file_info.tree,
                &language_info,
                local_pattern,
                true,
            );
            if !new_ranges.is_empty() {
                print_ranges.push((file_info.path.to_owned(), new_ranges)); // TODO extend prev if new_ranges comes after in the same file
                recurse_defs.extend(
                    new_recurses.into_iter().filter(|name| {
                        local_patterns.iter().all(|pattern| !pattern.is_match(name))
                    }),
                );
            }
        }
        recurse_defs.dedup();
        if cli.recurse && recurse_defs.len() == 1 {
            current_pattern = regex::Regex::new(&regex::escape(&recurse_defs[0])).unwrap();
        } else {
            break;
        }
    }

    // set up paging if requested
    let enable_paging = if cli.paging != EnablementLevel::Auto {
        cli.paging == EnablementLevel::Always
    } else {
        cli.plain < 2 && console::Term::stdout().is_term()
    };
    let mut pager = paging::MaybePager::new(enable_paging, cli.wrap == WrapMode::Never);
    let bat_size = console::Term::stdout().size_checked();
    for (path, ranges) in print_ranges.iter() {
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
        let child = match path {
            Some(path) => cmd.arg(path).spawn(),
            None => {
                let mut child = cmd
                    .arg(format!(
                        "-l{}",
                        stdin_parsed.as_ref().unwrap().language_name_str
                    ))
                    .stdin(std::process::Stdio::piped())
                    .spawn();
                if let Ok(child) = &mut child {
                    let mut child_stdin = child.stdin.take().unwrap();
                    let stdin_clone = stdin.clone();
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
        };
        if let Err(e) = child.and_then(|mut child| {
            // std::io::copy uses more efficient syscalls than explicitly reading and writing
            let mut child_stdout = child.stdout.take().unwrap();
            let copy_result = std::io::copy(&mut child_stdout, &mut pager);
            let wait_result = child.wait().and_then(|exit_status| {
                if exit_status.success() {
                    Ok(())
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("reader exited {:?}", exit_status),
                    ))
                }
            });
            copy_result.map(|_| wait_result)
        }) {
            if pager.is_abandoned() {
                // stdout is gone so let's just leave quietly
                return Ok(std::process::ExitCode::SUCCESS);
            } else {
                let error_message =
                    std::vec::Vec::from(format!("Error reading {:?}: {}\n", path, e));
                pager.write_all(&error_message)?;
            }
        }
    }
    // wait for pager
    match pager.wait() {
        Ok(0) => (),
        Ok(status) => println!("Pager exited {}", status),
        Err(e) => println!("Pager died or vanished: {}", e),
    }

    // yeah yeah whatever
    Ok(std::process::ExitCode::SUCCESS)
}

#[cfg(not(feature = "stdin"))]
fn parse_stdin(
    _: &Cli,
    _: &mut loader::Loader,
    _: &config::Config,
) -> (bool, Vec<u8>, std::io::Result<searches::ParsedFile>) {
    (
        false,
        vec![],
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "")),
    )
}

#[cfg(feature = "stdin")]
fn parse_stdin(
    cli: &Cli,
    language_loader: &mut loader::Loader,
    merged_config: &config::Config,
) -> (bool, Vec<u8>, std::io::Result<searches::ParsedFile>) {
    use std::io::Read;

    let use_stdin = cli.stdin;
    let mut stdin = vec![];
    let stdin_parsed = if use_stdin {
        std::io::stdin().read_to_end(&mut stdin).and_then(|_| {
            if stdin.is_empty() {
                Err(std::io::Error::new(std::io::ErrorKind::NotFound, ""))
            } else {
                searches::ParsedFile::from_bytes(stdin.clone(), language_loader, merged_config)
            }
        })
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, ""))
    };
    (use_stdin, stdin, stdin_parsed)
}

fn ripgrep(pattern: &regex::Regex) -> std::io::Result<Vec<std::path::PathBuf>> {
    use os_str_bytes::OsStrBytes;

    // first-pass search with ripgrep
    let mut rg = std::process::Command::new("rg");
    let rg_output = rg
        .arg("-l")
        .arg("-0")
        .arg(pattern.as_str())
        .arg("./")
        .stderr(std::process::Stdio::inherit())
        .output()?;
    if !rg_output.status.success() {
        if let Some(e) = rg_output.status.code() {
            return Err(std::io::Error::new(
                // TODO adopt this exit code as our own
                std::io::ErrorKind::Other,
                format!("ripgrep exited {:?}", e),
            ));
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("{}", rg_output.status),
        ));
    }
    // TODO is this even actually the right way to convert stdout to OsStr?
    let filenames: std::io::Result<Vec<std::path::PathBuf>> = rg_output
        .stdout
        .split(|x| *x == 0)
        .filter_map(|x| match std::ffi::OsStr::from_io_bytes(x) {
            None => Some(Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{:?}", std::vec::Vec::from(x)),
            ))),
            Some(y) => {
                if y.is_empty() {
                    None
                } else {
                    Some(Ok(y.to_owned().into()))
                }
            }
        })
        .collect();
    filenames.map(|mut f| {
        f.sort_unstable();
        f
    })
}
