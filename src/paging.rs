extern crate console;

pub struct MaybePager {
    pager: Option<std::process::Child>,
}

impl MaybePager {
    pub fn new(enable_paging: bool) -> Self {
        let pager = if enable_paging {
            let mut pager_program = std::process::Command::new(match std::env::var_os("PAGER") {
                Some(value) => value,
                None => std::ffi::OsString::from("less"),
            });
            match (if pager_program.get_program() == "less" {
                pager_program.arg("-RF")
            } else {
                &mut pager_program
            })
            .stdin(std::process::Stdio::piped())
            .spawn()
            {
                Ok(child) => Some(child),
                Err(e) => {
                    println!("Pager didn't start: {}", e);
                    None
                }
            }
        } else {
            None
        };
        Self { pager }
    }

    pub fn wait(&mut self) -> std::io::Result<i32> {
        match &mut self.pager {
            None => Ok(0),
            Some(child) => {
                child.stdin.take();
                match child.wait() {
                    Ok(status) => match status.code() {
                        Some(c) => Ok(c),
                        None => Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Unknown exit status",
                        )),
                    },
                    Err(e) => Err(e),
                }
            }
        }
    }

    fn pipe(&mut self) -> Option<&mut std::process::ChildStdin> {
        match &mut self.pager {
            Some(child) => child.stdin.as_mut(),
            None => None,
        }
    }
}

impl std::io::Write for MaybePager {
    fn flush(&mut self) -> std::io::Result<()> {
        match self.pipe() {
            Some(pipe) => pipe.flush(),
            None => std::io::stdout().flush(),
        }
    }

    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self.pipe() {
            Some(pipe) => pipe.write(buf),
            None => std::io::stdout().write(buf),
        }
    }
}
