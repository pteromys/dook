#[derive(Debug)]
pub struct NotRecaseable {
    input: String,
    bad_position: usize,
}

impl std::fmt::Display for NotRecaseable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "input {:#?} contained non-alphanumeric character at byte {:#?}",
            self.input, self.bad_position
        )
    }
}

pub fn uncase(identifier: impl AsRef<str>) -> Result<String, NotRecaseable> {
    use heck::ToKebabCase;
    let identifier: &str = identifier.as_ref();
    match identifier.find(|c: char| !char::is_alphanumeric(c) && c != '-' && c != '_') {
        Some(idx) => Err(NotRecaseable {
            input: identifier.to_owned(),
            bad_position: idx,
        }),
        None => Ok(("-".to_string() + &identifier.to_kebab_case() + "-").replace("-", "[_-]*")),
    }
}
