use crate::language_name::LanguageName;

pub fn dissimilarity(language_name: LanguageName, dep: &str, path: &std::path::Path) -> i32 {
    match language_name {
        LanguageName::PYTHON => {
            let dep_components = dep.split('.');
            let path_components = path.iter();
            let match_count = dep_components
                .rev()
                .zip(path_components.rev())
                .take_while(|x| x.0 == x.1)
                .count();
            -i32::try_from(match_count).unwrap_or(0)
        }
        _ => 0,
    }
}
