use crate::LanguageName;
use crate::{inputs, loader, query_compiler, range_union, searches, subfiles};
use enum_derive_2018::EnumFromInner;

#[derive(Debug, Clone, Default)]
pub struct SubfileResults {
    pub results: SingleFileResults,
    pub subfile: Option<inputs::LoadedFile>,
}

#[derive(Debug, Clone, Default)]
pub struct SingleFileResults {
    pub ranges: range_union::RangeUnion,
    pub matched_names: Vec<String>,
    pub recurse_names: Vec<String>,
    pub import_origins: Vec<(LanguageName, String)>,
}

pub struct SearchParams<'a> {
    pub local_pattern: &'a regex::Regex,
    pub current_pattern: &'a regex::Regex,
    pub only_names: bool,
    pub recurse: bool,
}

macro_attr_2018::macro_attr! {
    #[derive(Debug, EnumFromInner!)]
    pub enum SinglePassError {
        Input(inputs::Error),
        FileParse(searches::FileParseError),
        LoaderError(loader::LoaderError),
        QueryCompilerError(query_compiler::QueryCompilerError),
    }
}

impl std::fmt::Display for SinglePassError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SinglePassError::Input(e) => write!(f, "{}", e),
            SinglePassError::FileParse(e) => write!(f, "{}", e),
            SinglePassError::LoaderError(e) => write!(f, "{}", e),
            SinglePassError::QueryCompilerError(e) => write!(f, "{}", e),
        }
    }
}

pub fn search_one_file_and_all_subfiles(
    params: &SearchParams,
    loaded_file: &inputs::LoadedFile,
    query_compiler: &mut query_compiler::QueryCompiler,
) -> Result<Vec<SubfileResults>, SinglePassError> {
    let mut results = vec![];
    let mut subfiles: Vec<Option<inputs::LoadedFile>> = vec![None];
    while let Some(subfile) = subfiles.pop() {
        let subfile_ref = match subfile.as_ref() {
            Some(f) => f,
            None => loaded_file,
        };
        let subfile_results = match search_one_file(params, subfile_ref, query_compiler) {
            Ok(x) => x,
            Err(e) => {
                log::warn!("Skipping {}: {}", subfile_ref.describe(), e);
                continue;
            }
        };
        results.push(SubfileResults {
            results: subfile_results.results,
            subfile,
        });
        subfiles.extend(subfile_results.subfiles.into_iter().map(Some));
    }
    Ok(results)
}


#[derive(Debug, Clone, Default)]
pub struct SingleFileResultsWithSubfiles {
    pub results: SingleFileResults,
    pub subfiles: Vec<inputs::LoadedFile>,
}

pub fn search_one_file(
    params: &SearchParams,
    loaded_file: &inputs::LoadedFile,
    query_compiler: &mut query_compiler::QueryCompiler,
) -> Result<SingleFileResultsWithSubfiles, SinglePassError> {
    let mut results = SingleFileResults::default();
    let mut subfiles = vec![];

    // parse the whole file, then injections
    let mut injections: Vec<Option<searches::InjectionRange>> = vec![None];
    while let Some(injection) = injections.pop() {
        let pass_results = match search_one_file_with_one_injection(
            params,
            query_compiler,
            loaded_file,
            injection.as_ref(),
        ) {
            Ok(x) => x,
            Err(e) => {
                let source_description = match injection {
                    None => loaded_file.describe(),
                    Some(i) => format!(
                        "{} {}-{}",
                        loaded_file.describe(),
                        i.range.start_point.row.saturating_add(1),
                        i.range.end_point.row.saturating_add(1),
                    ),
                };
                log::warn!("Skipping {}: {}", source_description, e);
                continue;
            }
        };
        log::debug!("results = {:#?}", &pass_results);
        match pass_results.search_result {
            SearchResult::Names(matched_names) => {
                results.matched_names.extend_from_slice(&matched_names);
            }
            SearchResult::Definitions(search_result) => {
                results.ranges.extend(&search_result.ranges);
                results
                    .recurse_names
                    .extend_from_slice(&search_result.recurse_names);
                results.import_origins.extend(
                    search_result
                        .import_origins
                        .into_iter()
                        .map(|o| (pass_results.language_name, o)),
                );
            }
            SearchResult::Subfiles(extracted_files) => {
                subfiles.extend(extracted_files);
            }
        }
        injections.extend(pass_results.injections.into_iter().map(Some));
    }
    Ok(SingleFileResultsWithSubfiles { results, subfiles })
}

#[derive(Debug, Clone)]
pub enum SearchResult {
    Definitions(searches::SearchResult),
    Names(Vec<String>),
    Subfiles(Vec<inputs::LoadedFile>),
}

#[derive(Debug, Clone)]
pub struct SinglePassResults {
    pub search_result: SearchResult,
    pub language_name: LanguageName,
    pub injections: Vec<searches::InjectionRange>,
}

pub fn search_one_file_with_one_injection(
    params: &SearchParams,
    query_compiler: &mut query_compiler::QueryCompiler,
    loaded_file: &inputs::LoadedFile,
    injection: Option<&searches::InjectionRange>,
) -> Result<SinglePassResults, SinglePassError> {
    use std::str::FromStr;

    // determine language
    let file_bytes = &loaded_file.bytes;
    let detect_start = std::time::Instant::now();
    let injection_bytes = match &injection {
        None => file_bytes,
        Some(injection) => &file_bytes[injection.range.start_byte..injection.range.end_byte],
    };
    let language_name = match &injection {
        None => loaded_file.language_name,
        Some(injection) => {
            match injection
                .language_hint
                .as_ref()
                .and_then(|hint| LanguageName::from_str(hint).ok())
            {
                Some(hinted) => hinted,
                None => inputs::detect_language_from_bytes(
                    injection_bytes,
                    injection.language_hint.as_ref().map(AsRef::as_ref),
                )?,
            }
        }
    };
    log::debug!(
        "detected {} as {:?} in {:?}",
        match injection {
            None => "file".to_string(),
            Some(i) => format!("{}-{}", i.range.start_point.row, i.range.end_point.row),
        },
        language_name,
        detect_start.elapsed()
    );

    let base_recipe = match injection {
        None => loaded_file.recipe.clone(),
        Some(i) => Some(match loaded_file.recipe.as_ref() {
            None => format!("sed -ne {},{}p", i.range.start_point.row, i.range.end_point.row),
            Some(recipe) => format!("{recipe} | sed -ne {},{}p", i.range.start_point.row, i.range.end_point.row),
        })
    };
    if let Some(extracted_files) = subfiles::extract_subfiles(language_name, injection_bytes, base_recipe) {
        return Ok(SinglePassResults {
            language_name,
            search_result: SearchResult::Subfiles(extracted_files),
            injections: vec![],
        })
    }

    // get language parser
    let parse_start = std::time::Instant::now();
    let language_info = query_compiler.get_language_info(language_name)?;
    // parse file contents
    let tree = searches::parse_ranged(
        file_bytes,
        language_name,
        &language_info.language,
        injection.map(|i| i.range),
    )?;
    log::debug!("parsed in {:?}", parse_start.elapsed());

    // search with tree_sitter
    Ok(SinglePassResults {
        language_name,
        search_result: if params.only_names {
            SearchResult::Names(searches::find_names(
                file_bytes,
                &tree,
                &language_info,
                params.local_pattern,
            ))
        } else {
            let mut result = searches::find_definition(
                file_bytes,
                &tree,
                &language_info,
                params.local_pattern,
                params.recurse,
            );
            if !result.ranges.is_empty() {
                if let Some(injection) = injection {
                    result.ranges.extend(injection.context.iter());
                }
            }
            SearchResult::Definitions(result)
        },
        injections: {
            let mut new_injections = searches::find_injections(
                file_bytes,
                &tree,
                &language_info,
                params.current_pattern,
            );
            if let Some(parent_injection) = injection {
                for i in &mut new_injections {
                    i.context.extend(parent_injection.context.iter());
                }
            }
            new_injections
        },
    })
}
