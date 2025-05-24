use crate::MultiLineString;

struct Notebook {
    cells: Vec<Cell>,
    metadata: NotebookMetadata,
    nbformat: Option<usize>,
    nbformat_minor: Option<usize>,
}

struct Cell {
    cell_type: CellType,
    source: MultiLineString,
    execution_count: Option<usize>,
    outputs: Option<Vec<Output>>,
    metadata: Option<Ignore>,
}

enum CellType {
    Markdown,
    Code,
}

// someday this will become an internally tagged enum when we switch to facet
// watch https://github.com/facet-rs/facet/issues/634
struct Output {
    output_type: String,
    metadata: Option<Ignore>,
    // only output_type = execute_result or display_data
    // mimetype -> content wrapping, possibly base64 encoded
    data: Option<std::collections::HashMap<String, MultiLineString>>,
    // only output_type = execute_result
    execution_count: Option<usize>,
    // only output_type = stream
    name: Option<String>,
    text: Option<MultiLineString>,
    // only OutputType::Error
    ename: Option<String>,
    evalue: Option<String>,
    traceback: Option<MultiLineString>,
}

struct NotebookMetadata {
    language_info: LanguageInfo,
    kernelspec: Option<Ignore>,
    toc: Option<Ignore>,
}

struct LanguageInfo {
    name: String,
    version: Option<String>,
    file_extension: Option<String>,
    mimetype: Option<String>,
    codemirror_mode: Option<Ignore>,
    nbconvert_exporter: Option<String>,
    pygments_lexer: Option<String>,
}

merde::derive! { impl (Deserialize) for struct Notebook {
    cells, metadata, nbformat, nbformat_minor
} }
merde::derive! { impl (Deserialize) for struct NotebookMetadata {
    language_info, kernelspec, toc
} }
merde::derive! { impl (Deserialize) for struct LanguageInfo {
    name, version,
    file_extension, mimetype,
    codemirror_mode, nbconvert_exporter, pygments_lexer
} }
merde::derive! { impl (Deserialize) for struct Cell {
    cell_type, source, execution_count, outputs, metadata
} }
merde::derive! { impl (Deserialize) for enum CellType string_like {
    "markdown" => Markdown, "code" => Code
} }
merde::derive! { impl (Deserialize) for struct Output {
    output_type, metadata,
    data, execution_count,
    name, text,
    ename, evalue, traceback
} }

// because merde's handling of unknown fields forgets to ignore the field value
struct Ignore;
impl<'de> merde::Deserialize<'de> for Ignore {
    async fn deserialize(
        de: &mut dyn merde::DynDeserializer<'de>,
    ) -> Result<Self, merde::MerdeError<'de>> {
        match de.next().await? {
            merde::Event::MapStart(_) => {
                let mut level: usize = 1;
                loop {
                    match de.next().await? {
                        merde::Event::MapStart(_) => {
                            level += 1;
                        }
                        merde::Event::MapEnd => {
                            level -= 1;
                            if level == 0 {
                                break
                            }
                        }
                        _ => (),
                    }
                }
                Ok(Self)
            }
            merde::Event::ArrayStart(_) => {
                let mut level: usize = 1;
                loop {
                    match de.next().await? {
                        merde::Event::ArrayStart(_) => {
                            level += 1;
                        }
                        merde::Event::ArrayEnd => {
                            level -= 1;
                            if level == 0 {
                                break
                            }
                        }
                        _ => (),
                    }
                }
                Ok(Self)
            }
            _ => Ok(Self),
        }
    }
}

pub fn to_unaligned_markdown(ipynb_bytes: &[u8]) -> Option<Vec<u8>> {
    use std::io::Write;
    let notebook: Notebook = merde_json::from_bytes(ipynb_bytes).inspect_err(|e| {
        log::error!("{}", e);
    }).unwrap();
    let mut result = Vec::<u8>::new();
    for cell in notebook.cells {
        match cell.cell_type {
            CellType::Markdown => {
                write!(result, "{}\n\n", cell.source.as_ref()).ok()?;
            }
            CellType::Code => {
                write!(
                    result,
                    "```{}\n{}\n```\n\n",
                    notebook.metadata.language_info.name,
                    cell.source.as_ref(),
                ).ok()?;
            }
        }
        let Some(outputs) = cell.outputs.as_ref() else { continue };
        for output in outputs {
            if let Some(text) = output.text.as_ref() {
                write!(result, "```\n{}\n```\n\n", text.as_ref()).ok()?;
            }
            if let Some(tb) = output.traceback.as_ref() {
                write!(result, "```py\n{}\n```\n\n", tb.as_ref()).ok()?;
            }
            if let Some(data) = output.data.as_ref() {
                if let Some(text) = data.get("text/plain") {
                    write!(result, "```\n{}\n```\n\n", text.as_ref()).ok()?;
                }
            }
        }
    }
    Some(result)
}
