// Prior art:
//     https://github.com/simonw/symbex
//     https://github.com/newlinedotco/cq
//     git grep -W
//     https://dandavison.github.io/delta/grep.html
//     https://docs.github.com/en/repositories/working-with-files/using-files/navigating-code-on-github#precise-and-search-based-navigation

mod config;
mod dumptree;
mod paging;
mod searches;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum EnablementLevel {
    #[default]
    Auto,
    Never,
    Always,
}

#[derive(clap::Parser, Debug)]
/// Find a definition.
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

    /// Dump the syntax tree of every matched file, for debugging extraction queries.
    #[arg(long)]
    dump: bool,
}

fn main() -> std::io::Result<std::process::ExitCode> {
    use clap::Parser;
    use os_str_bytes::{OsStrBytes, OsStrBytesExt};
    use std::io::Write;

    env_logger::init();

    // grab cli args
    let cli = Cli::parse();
    let local_pattern = match regex::Regex::new(&(String::from("^") + cli.pattern.as_str() + "$")) {
        Ok(p) => p,
        Err(e) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, e)),
    };

    // load config
    let custom_config = config::Config::load(cli.config)?;
    let default_config = config::Config::load_default();

    // first-pass search with ripgrep
    let mut rg = std::process::Command::new("rg");
    let rg_output = rg
        .arg("-l")
        .arg("-0")
        .arg(cli.pattern.as_str())
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
    let filenames = filenames?;

    // infer syntax, then search with tree_sitter
    // TODO 0: add more languages
    // TODO 1: sniff syntax by content
    //     maybe use shebangs
    //     maybe https://github.com/sharkdp/bat/blob/master/src/syntax_mapping.rs
    let mut print_ranges: std::collections::HashMap<std::ffi::OsString, searches::RangeUnion> =
        std::collections::HashMap::new();
    for path in filenames {
        // TODO group by language and do a second pass with language-specific regexes?
        let language_name = if path.ends_with(".rs") {
            config::LanguageName::Rust
        } else if path.ends_with(".py") || path.ends_with(".pyx") {
            config::LanguageName::Python
        } else if path.ends_with(".js") {
            config::LanguageName::Js
        } else if path.ends_with(".ts") {
            config::LanguageName::Ts
        } else if path.ends_with(".tsx") {
            config::LanguageName::Tsx
        } else if path.ends_with(".c") || path.ends_with(".h") {
            config::LanguageName::C
        } else if path.ends_with(".cpp")
            || path.ends_with(".hpp")
            || path.ends_with(".cxx")
            || path.ends_with(".hxx")
            || path.ends_with(".C")
            || path.ends_with(".H")
        {
            config::LanguageName::CPlusPlus
        } else if path.ends_with(".go") {
            config::LanguageName::Go
        } else {
            continue;
        };
        if cli.dump {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(language_name.get_language()).unwrap();
            let source_code = std::fs::read(&path)?;
            let tree = parser.parse(&source_code, None).unwrap();
            dumptree::dump_tree(&tree, source_code.as_slice());
            continue;
        }
        let language_info = custom_config
            .as_ref()
            .and_then(|c| c.get_language_info(language_name))
            .or_else(|| default_config.get_language_info(language_name))
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "No config contains definitions for language: {:?}",
                        language_name
                    ),
                )
            })?
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{}", e)))?;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(language_info.language).unwrap();
        let source_code = std::fs::read(&path)?;
        let tree = parser.parse(&source_code, None).unwrap();
        let target_ranges = print_ranges.entry(path.clone()).or_default();
        target_ranges.extend(searches::find_definition(
            source_code.as_slice(),
            &tree,
            &language_info,
            &local_pattern,
        ));
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
                    .as_ranges()
                    .iter()
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
