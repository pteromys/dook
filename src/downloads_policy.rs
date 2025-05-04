use crate::config;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum DownloadsPolicy {
    Yes,
    Ask,
    No,
}

impl std::fmt::Display for DownloadsPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Yes => write!(f, "YES"),
            Self::Ask => write!(f, "ASK"),
            Self::No => write!(f, "NO"),
        }
    }
}

/// Get saved downloads policy setting from downloads_policy.txt.
pub fn get_downloads_policy() -> DownloadsPolicy {
    let Some(path) = settings_path() else {
        return DownloadsPolicy::Ask;
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return DownloadsPolicy::Ask;
    };
    let Ok(setting) = std::str::from_utf8(bytes.as_ref()) else {
        return DownloadsPolicy::Ask;
    };
    match setting
        .to_ascii_lowercase()
        .trim_matches(char::is_whitespace)
    {
        "yes" => DownloadsPolicy::Yes,
        "ask" => DownloadsPolicy::Ask,
        "no" => DownloadsPolicy::No,
        _ => DownloadsPolicy::Ask,
    }
}

pub fn settings_path() -> Option<std::path::PathBuf> {
    use etcetera::AppStrategy;
    if cfg!(test) {
        option_env!("CARGO_TARGET_TMPDIR")
            .map(|d| std::path::PathBuf::from(d).join("downloads_policy.txt"))
    } else {
        config::dirs()
            .ok()
            .map(|d| d.config_dir().join("downloads_policy.txt"))
    }
}

pub fn can_download(url: &str, policy: DownloadsPolicy) -> bool {
    use std::io::Write;
    match policy {
        DownloadsPolicy::Yes => true,
        DownloadsPolicy::No => false,
        DownloadsPolicy::Ask => {
            let mut stdout = console::Term::stdout();
            let response = write!(&mut stdout, "download from {url:?}? (y/n): ")
                .and_then(|_| stdout.flush())
                .and_then(|_| stdout.read_line());
            match response {
                Ok(response) => response.eq_ignore_ascii_case("y"),
                Err(_) => false,
            }
        }
    }
}
