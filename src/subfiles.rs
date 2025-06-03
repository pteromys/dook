use crate::language_name::LanguageName;
use crate::{inputs, ipynb};

pub fn extract_subfiles(
    language_name: LanguageName,
    file_bytes: &[u8],
    base_recipe: Option<String>,
) -> Option<Vec<inputs::LoadedFile>> {
    match language_name {
        LanguageName::IPYNB => ipynb::to_unaligned_markdown(file_bytes).map(|markdown_bytes| {
            vec![inputs::LoadedFile {
                recipe: Some(match base_recipe {
                    None => "STDIN <to markdown>".to_string(),
                    Some(recipe) => format!("{recipe} <to markdown>"),
                }),
                path: None,
                bytes: markdown_bytes,
                language_name: LanguageName::MARKDOWN,
            }]
        }),
        _ => None,
    }
}
