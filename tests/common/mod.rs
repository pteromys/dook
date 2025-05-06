use dook::{Config, Loader, QueryCompiler};

pub fn get_query_compiler() -> QueryCompiler {
    let target_dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let language_loader = Loader::new(
        target_dir.clone(),
        Some(target_dir.clone()),
        get_downloads_policy(),
    )
    .expect("should have called tree_sitter_loader::Loader::with_parser_lib_path(), not new()");
    QueryCompiler::new(Config::load_default(), language_loader)
}

fn get_downloads_policy() -> dook::downloads_policy::DownloadsPolicy {
    let settings_path = option_env!("CARGO_TARGET_TMPDIR")
        .map(|d| std::path::PathBuf::from(d).join("downloads_policy.txt"));
    dook::downloads_policy::get_downloads_policy_from_path(settings_path.as_ref())
}
