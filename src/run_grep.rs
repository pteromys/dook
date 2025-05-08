thread_local! {
    static HAS_RIPGREP: std::cell::Cell<bool> = std::cell::Cell::new(
        if std::process::Command::new("rg")
            .arg("-V")
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .output()
            .is_ok()
        {
            true
        } else {
            log::warn!("ripgrep not found on PATH; falling back to grep -r which may be slow due to not checking .gitignore");
            false
        }
    );
}

#[derive(Debug)]
pub enum RipGrepError {
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

pub fn ripgrep(
    pattern: &regex::Regex,
    ignore_case: bool,
) -> Box<dyn Iterator<Item = Result<std::path::PathBuf, RipGrepError>>> {
    use os_str_bytes::OsStrBytes;
    use std::io::BufRead;

    // first-pass search with ripgrep
    let mut rg: std::process::Command;
    if HAS_RIPGREP.get() {
        rg = std::process::Command::new("rg");
        rg.args(["-l", "--sort=path", "-0"]);
    } else {
        rg = std::process::Command::new("grep");
        rg.arg("-lIErZ");
    }
    if ignore_case {
        rg.arg("-i");
    }
    let mut child = match rg
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
