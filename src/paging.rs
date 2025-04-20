pub struct MaybePager {
    pager: Option<std::process::Child>,
    abandoned: bool,
}

impl MaybePager {
    pub fn new(enable_paging: bool, chop: bool) -> Self {
        let pager = if enable_paging {
            let mut pager_program = std::process::Command::new(match std::env::var_os("PAGER") {
                Some(value) => value,
                None => std::ffi::OsString::from("less"),
            });
            let pager_command = if pager_program.get_program() == "less" {
                pager_program.arg("-RF")
            } else {
                &mut pager_program
            };
            let pager_command = if chop && pager_command.get_program() == "less" {
                pager_command.arg("-S")
            } else {
                &mut pager_program
            };
            match pager_command.stdin(std::process::Stdio::piped()).spawn() {
                Ok(child) => Some(child),
                Err(e) => {
                    eprintln!("Pager didn't start: {:?}", e);
                    None
                }
            }
        } else {
            None
        };
        Self {
            pager,
            abandoned: false,
        }
    }

    pub fn wait(&mut self) -> std::io::Result<i32> {
        match &mut self.pager {
            None => Ok(0),
            Some(child) => {
                child.stdin.take();
                match child.wait() {
                    Ok(status) => match status.code() {
                        Some(c) => Ok(c),
                        None => Err(std::io::Error::other("Unknown exit status")),
                    },
                    Err(e) => Err(e),
                }
            }
        }
    }

    /// whether it's ever seen a std::io::ErrorKind::BrokenPipe
    pub fn is_abandoned(&self) -> bool {
        self.abandoned
    }

    /// get the child process's stdin
    fn pipe(&mut self) -> Option<&mut std::process::ChildStdin> {
        self.pager.as_mut().and_then(|child| child.stdin.as_mut())
    }
}

pub fn is_broken_pipe<T>(result: &std::io::Result<T>) -> bool {
    match result {
        Err(e) => e.kind() == std::io::ErrorKind::BrokenPipe,
        Ok(_) => false,
    }
}

impl std::io::Write for MaybePager {
    fn flush(&mut self) -> std::io::Result<()> {
        let result = match self.pipe() {
            Some(pipe) => pipe.flush(),
            None => std::io::stdout().flush(),
        };
        self.abandoned |= is_broken_pipe(&result);
        result
    }

    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let result = match self.pipe() {
            Some(pipe) => pipe.write(buf),
            None => std::io::stdout().write(buf),
        };
        self.abandoned |= is_broken_pipe(&result);
        result
    }
}
