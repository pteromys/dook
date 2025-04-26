use enum_derive_2018::IterVariants;

macro_attr_2018::macro_attr! {
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, IterVariants!(Vars))]
    pub enum LanguageName {
        Rust,
        Python,
        Js,
        Ts,
        Tsx,
        C,
        CPlusPlus,
        Go,
    }
}

impl<'de> merde::Deserialize<'de> for LanguageName {
    async fn deserialize(
        de: &mut dyn merde::DynDeserializer<'de>,
    ) -> Result<Self, merde::MerdeError<'de>> {
        Ok(match de.next().await? {
            merde::Event::Str(s) => match s.to_string().to_lowercase().as_ref() {
                "rust" => Self::Rust,
                "python" => Self::Python,
                "js" => Self::Js,
                "ts" => Self::Ts,
                "tsx" => Self::Tsx,
                "c" => Self::C,
                "cplusplus" => Self::CPlusPlus,
                "go" => Self::Go,
                _ => {
                    return Err(merde::MerdeError::StringParsingError {
                        format: "yaml",
                        message: format!("Not a supported language: {:?}", s.to_string()),
                        source: s,
                        index: 0,
                    })
                }
            },
            e => {
                return Err(merde::MerdeError::UnexpectedEvent {
                    got: merde::EventType::from(&e),
                    expected: &[merde::EventType::Str],
                    help: None,
                })
            }
        })
    }
}

impl merde::Serialize for LanguageName {
    // Required method
    async fn serialize<'fut>(
        &'fut self,
        serializer: &'fut mut dyn merde::DynSerializer,
    ) -> Result<(), merde::MerdeError<'static>> {
        let self_str = match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Js => "js",
            Self::Ts => "ts",
            Self::Tsx => "tsx",
            Self::C => "c",
            Self::CPlusPlus => "cplusplus",
            Self::Go => "go",
        };
        let event = merde::Event::Str(merde::CowStr::copy_from_str(self_str));
        serializer.write(event).await
    }
}

impl LanguageName {
    pub fn from_hyperpolyglot(hyperpolyglot_name: &str) -> Option<Self> {
        Some(match hyperpolyglot_name {
            "Rust" => Self::Rust,
            "Python" => Self::Python,
            "JavaScript" => Self::Js,
            "TypeScript" => Self::Ts,
            "TSX" => Self::Tsx,
            "C" => Self::C,
            "C++" => Self::CPlusPlus,
            "Go" => Self::Go,
            _ => return None,
        })
    }
}
