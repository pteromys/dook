use dook::config::{Config, QueryCompiler};
use dook::language_name::LanguageName;
use dook::loader;

#[test]
fn default_patterns_are_loadable() {
    let target_dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    eprintln!("CARGO_TARGET_TMPDIR is {:?}", target_dir);
    let mut language_loader =
        loader::Loader::new(target_dir.clone(), Some(target_dir.clone()), false).expect(
            "should have called tree_sitter_loader::Loader::with_parser_lib_path(), not new()",
        );
    let default_config = Config::load_default();
    let mut query_compiler = QueryCompiler::new(&default_config);
    for language_name in LanguageName::iter_variants() {
        query_compiler
            .get_language_info(language_name, &mut language_loader)
            .unwrap();
    }
}
