#[derive(Clone, Debug, PartialEq)]
pub struct MultiLineString(String);

impl AsRef<str> for MultiLineString {
    fn as_ref(&self) -> &str {
        let Self(inner) = self;
        inner
    }
}

impl From<MultiLineString> for String {
    fn from(mls: MultiLineString) -> Self {
        let MultiLineString(inner) = mls;
        inner
    }
}

impl<'de> merde::Deserialize<'de> for MultiLineString {
    async fn deserialize(
        de: &mut dyn merde::DynDeserializer<'de>,
    ) -> Result<Self, merde::MerdeError<'de>> {
        match de.next().await? {
            merde::Event::Str(v) => Ok(MultiLineString(String::from(v))),
            merde::Event::ArrayStart(_) => {
                let mut vs: Vec<String> = Vec::new();
                loop {
                    match de.next().await? {
                        merde::Event::ArrayEnd => break,
                        merde::Event::Str(v) => vs.push(v.trim_end_matches(['\r', '\n']).to_string()),
                        ev => Err(merde::MerdeError::UnexpectedEvent {
                            got: merde::EventType::from(&ev),
                            expected: &[merde::EventType::Str],
                            help: Some(String::from(
                                "multiline string must be a string or an array of strings",
                            )),
                        })?,
                    }
                }
                Ok(MultiLineString(vs.join("\n")))
            }
            ev => Err(merde::MerdeError::UnexpectedEvent {
                got: merde::EventType::from(&ev),
                expected: &[merde::EventType::Str, merde::EventType::ArrayStart],
                help: Some(String::from(
                    "multiline string must be a string or an array of strings",
                )),
            })?,
        }
    }
}
