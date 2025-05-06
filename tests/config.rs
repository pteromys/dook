use dook::downloads_policy::get_downloads_policy;
use dook::{Config, Loader, QueryCompiler};

#[test]
fn default_patterns_are_loadable() {
    let target_dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    eprintln!("CARGO_TARGET_TMPDIR is {:?}", target_dir);
    let language_loader = Loader::new(
        target_dir.clone(),
        Some(target_dir.clone()),
        get_downloads_policy(),
    )
    .expect("should have called tree_sitter_loader::Loader::with_parser_lib_path(), not new()");
    let mut query_compiler = QueryCompiler::new(Config::load_default(), language_loader);
    for language_name in Config::load_default().configured_languages() {
        query_compiler.get_language_info(*language_name).unwrap();
    }
}
