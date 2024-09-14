// Prior art:
//     https://github.com/simonw/symbex
//     https://github.com/newlinedotco/cq
//     git grep -W
//     https://dandavison.github.io/delta/grep.html
//     https://docs.github.com/en/repositories/working-with-files/using-files/navigating-code-on-github#precise-and-search-based-navigation

mod config;
mod dumptree;
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

#[derive(clap::Parser, Debug)]
/// dook: Definition lookup in your code.
struct Cli {
    /// Regex to match against symbol names.
    pattern: regex::Regex,

    /// Config file path
    #[arg(short, long, required = false)]
    config: Option<std::ffi::OsString>,

    #[arg(long, value_enum, default_value_t)]
    color: EnablementLevel,

    #[arg(long, value_enum, default_value_t)]
    paging: EnablementLevel,

    /// Apply no styling; specify twice to also disable paging.
    #[arg(short, long, action = clap::ArgAction::Count)]
    plain: u8,

    /// Recurse if the definition contains exactly one function or constructor call.
    #[arg(short, long)]
    recurse: bool,

    /// Don't recurse [default].
    #[arg(long, overrides_with = "recurse")]
    _no_recurse: bool,

    /// Dump the syntax tree of every matched file, for debugging extraction queries.
    #[arg(long)]
    dump: bool,
}

fn main() -> std::io::Result<std::process::ExitCode> {
    use clap::Parser;
    use os_str_bytes::OsStrBytes;
    use std::io::Write;

    env_logger::init();

    // grab cli args
    let cli = Cli::parse();
    let mut current_pattern = cli.pattern.clone();
    let mut local_patterns: std::vec::Vec<regex::Regex> = vec![];

    // load config
    let custom_config = config::Config::load(cli.config)?;
    let default_config = config::Config::load_default();

    // store the result here
    let mut print_ranges: Vec<(std::ffi::OsString, range_union::RangeUnion)> = Vec::new();
    loop {
        // first-pass search with ripgrep
        let mut rg = std::process::Command::new("rg");
        let rg_output = rg
            .arg("-l")
            .arg("-0")
            .arg(current_pattern.as_str())
            .arg("./")
            .stderr(std::process::Stdio::inherit())
            .output()?;
        if !rg_output.status.success() {
            if let Some(e) = rg_output.status.code() {
                return Ok(std::process::ExitCode::from(e as u8)); // truncate to 8 bits
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("{}", rg_output.status),
            ));
        }
        // TODO is this even actually the right way to convert stdout to OsStr?
        let filenames: std::io::Result<std::vec::Vec<std::ffi::OsString>> = rg_output
            .stdout
            .split(|x| *x == 0)
            .map(|x| match std::ffi::OsStr::from_io_bytes(x) {
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("{:?}", std::vec::Vec::from(x)),
                )),
                Some(y) => Ok(y.to_os_string()),
            })
            .filter(|f| match f {
                Ok(f) => !f.is_empty(),
                _ => true,
            })
            .collect();
        let mut filenames = filenames?;
        filenames.sort_unstable();

        // infer syntax, then search with tree_sitter
        let mut recurse_defs: std::vec::Vec<String> = vec![];
        local_patterns.push(
            match regex::Regex::new(&(String::from("^") + current_pattern.as_str() + "$")) {
                Ok(p) => p,
                Err(e) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, e)),
            },
        );
        let local_pattern = local_patterns.last().unwrap();
        for path in filenames {
            let file_info = match searches::ParsedFile::from_filename(&path) {
                None => continue,
                Some(f) => f,
            };
            if cli.dump {
                dumptree::dump_tree(&file_info.tree, file_info.source_code.as_slice());
                continue;
            }
            let language_info = custom_config
                .as_ref()
                .and_then(|c| c.get_language_info(file_info.language_name))
                .or_else(|| default_config.get_language_info(file_info.language_name))
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
                print_ranges.push((path, new_ranges)); // TODO extend prev if new_ranges comes after in the same file
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
    let mut pager = paging::MaybePager::new(enable_paging);
    let bat_color = if cli.color != EnablementLevel::Auto {
        cli.color
    } else if console::colors_enabled() {
        EnablementLevel::Always
    } else {
        EnablementLevel::Never
    };
    let bat_size = console::Term::stdout().size_checked();
    for (path, ranges) in print_ranges.iter() {
        let mut cmd = std::process::Command::new("bat");
        let cmd = cmd
            .arg("--paging=never")
            .arg(format!("--color={:?}", bat_color).to_lowercase());
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
            .arg(path);
        let output = match cmd.stderr(std::process::Stdio::inherit()).output() {
            Ok(output) => output.stdout,
            Err(e) => std::vec::Vec::from(format!("Error reading {:?}: {}", path, e)),
        };
        if let Err(e) = pager.write_all(&output) {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                // stdout is gone so let's just leave quietly
                return Ok(std::process::ExitCode::SUCCESS);
            }
            break;
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
