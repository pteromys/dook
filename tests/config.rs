mod common;

#[test]
fn default_patterns_are_loadable() {
    use std::str::FromStr;
    let mut query_compiler = common::get_query_compiler();
    for language_name_str in dook::DEFAULT_CONFIG.keys() {
        let language_name = dook::LanguageName::from_str(language_name_str).unwrap();
        query_compiler.get_language_info(language_name).unwrap();
    }
}
