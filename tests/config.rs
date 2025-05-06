mod common;

#[test]
fn default_patterns_are_loadable() {
    let mut query_compiler = common::get_query_compiler();
    for language_name in dook::Config::load_default().configured_languages() {
        query_compiler.get_language_info(*language_name).unwrap();
    }
}
