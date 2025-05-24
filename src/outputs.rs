use dook::inputs;
use dook::RangeUnion;

pub struct OutputOptions {
    pub wrap: WrapMode,
    pub plain: u8,
    pub use_color: bool,
    pub terminal_size: Option<(u16, u16)>,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum WrapMode {
    #[default]
    Auto,
    Never,
    Character,
}

pub fn write_ranges(
    input: inputs::SearchInput,
    ranges: &RangeUnion,
    options: &OutputOptions,
) -> Result<(), PagerWriteError> {
    if HAS_BAT.get() {
        write_ranges_with_bat(input, ranges, options)
    } else {
        write_ranges_with_std_io(input, ranges, options)
    }
}

fn write_ranges_with_bat(
    input: inputs::SearchInput,
    ranges: &RangeUnion,
    options: &OutputOptions,
) -> Result<(), PagerWriteError> {
    use std::io::Write;

    let mut cmd = std::process::Command::new("bat");
    cmd.arg("--paging=never")
        .arg(format!("--wrap={:?}", options.wrap).to_lowercase())
        .arg(match options.use_color {
            true => "--color=always",
            false => "--color=never",
        });
    if let Some((_rows, cols)) = options.terminal_size.as_ref() {
        cmd.arg(format!("--terminal-width={}", cols));
    }
    if options.plain > 0 {
        cmd.arg("--plain");
    }
    cmd.args(
        ranges
            .iter_filling_gaps(1) // snip indicator - 8< - takes 1 line anyway
            .map(|x| format!("--line-range={}:{}", x.start + 1, x.end)), // bat end is inclusive
    )
    .stderr(std::process::Stdio::inherit())
    .stdout(std::process::Stdio::piped());
    let mut child = match input {
        inputs::SearchInput::Path(path) => cmd.arg(path).spawn(),
        inputs::SearchInput::Loaded(stdin) => {
            if let Some(recipe) = stdin.recipe.as_ref() {
                cmd.arg("--file-name").arg(recipe);
            }
            cmd.arg(format!("-l{}", stdin.language_name))
                .stdin(std::process::Stdio::piped());
            let mut child = cmd.spawn();
            if let Ok(child) = &mut child {
                let mut child_stdin = child
                    .stdin
                    .take()
                    .expect("BUG: should have launched bat with stdin=piped()");
                let stdin_clone = stdin.bytes.clone();
                std::thread::spawn(move || {
                    let result = child_stdin.write_all(&stdin_clone);
                    if !is_broken_pipe(&result) {
                        result.unwrap()
                    }
                });
            }
            child
        }
    }?;
    // It'd be simpler to let `bat` write directly to our stdout, but we call
    // std::io::copy ourselves (which uses more efficient syscalls than more
    // explictly reading and writing) so that if the user quits the pager, we
    // actually receive the SIGPIPE and can exit without burning more CPU or
    // file I/O.
    let mut child_stdout = child
        .stdout
        .take()
        .expect("BUG: should have launched bat with stdout=piped()");
    let copy_result = std::io::copy(&mut child_stdout, &mut std::io::stdout());
    let wait_result = child.wait();
    copy_result?;
    let exit_status = wait_result?;
    if exit_status.success() {
        Ok(())
    } else {
        Err(PagerWriteError::ReaderDied(exit_status))
    }
}

fn write_ranges_with_std_io(
    input: inputs::SearchInput,
    ranges: &RangeUnion,
    options: &OutputOptions,
) -> Result<(), PagerWriteError> {
    use std::io::BufRead;
    use std::io::Write;

    let number_lines = options.plain == 0;
    let mut stdout = std::io::stdout();
    let cols: usize = options
        .terminal_size
        .as_ref()
        .map(|(_rows, cols)| *cols)
        .unwrap_or(40)
        .saturating_sub(1)
        .into();
    let sep1 = "-".repeat(cols);
    let sep2 = "=".repeat(cols);
    let Some(max_line_number) = ranges.end() else {
        return Ok(());
    };
    let line_number_width = format!("{}", max_line_number).len();
    let reader: Box<dyn BufRead> = match input {
        inputs::SearchInput::Loaded(stdin) => {
            writeln!(stdout, "{sep2}\nstdin\n{sep2}")?;
            Box::new(std::io::Cursor::new(&stdin.bytes))
        }
        inputs::SearchInput::Path(path) => {
            let reader = std::io::BufReader::new(std::fs::File::open(path)?);
            writeln!(stdout, "{sep2}\n{}\n{sep2}", path.display())?;
            Box::new(reader)
        }
    };
    let mut ranges = ranges.iter_filling_gaps(1);
    let Some(mut current_range) = ranges.next() else {
        return Ok(());
    };
    for (i, line) in reader.lines().enumerate() {
        if i < current_range.start {
            continue;
        }
        if i >= current_range.end {
            current_range = match ranges.next() {
                None => return Ok(()),
                Some(r) => r,
            };
            writeln!(stdout, "{sep1}")?;
            continue;
        }
        if number_lines {
            write!(
                stdout,
                " {: >width$} | ",
                i.saturating_add(1),
                width = line_number_width
            )?;
        }
        writeln!(stdout, "{}", line?)?;
    }
    Ok(())
}

#[derive(Debug)]
pub enum PagerWriteError {
    IoError(std::io::Error),
    BrokenPipe,
    ReaderDied(std::process::ExitStatus),
}

impl std::fmt::Display for PagerWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PagerWriteError::IoError(e) => write!(f, "{}", e),
            PagerWriteError::BrokenPipe => write!(f, "broken pipe (someone closed our output)"),
            PagerWriteError::ReaderDied(status) => {
                write!(f, "formatting excerpt exited {}", status)
            }
        }
    }
}

impl From<std::io::Error> for PagerWriteError {
    fn from(value: std::io::Error) -> Self {
        match value.kind() {
            std::io::ErrorKind::BrokenPipe => Self::BrokenPipe,
            _ => Self::IoError(value),
        }
    }
}

thread_local! {
    static HAS_BAT: std::cell::Cell<bool> = std::cell::Cell::new(
        if std::process::Command::new("bat")
            .arg("-V")
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .output()
            .is_ok()
        {
            true
        } else {
            log::warn!("bat not found on PATH; color and wrapping will be disabled");
            false
        }
    );
}

fn is_broken_pipe<T>(result: &std::io::Result<T>) -> bool {
    match result {
        Err(e) => e.kind() == std::io::ErrorKind::BrokenPipe,
        Ok(_) => false,
    }
}
