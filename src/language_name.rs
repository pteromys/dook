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
        Markdown,
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
                "markdown" => Self::Markdown,
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
            Self::Markdown => "markdown",
        };
        let event = merde::Event::Str(merde::CowStr::copy_from_str(self_str));
        serializer.write(event).await
    }
}

impl LanguageName {
    pub fn from_hyperpolyglot(hyperpolyglot_name: &str) -> Option<Self> {
        let lowercase_name = hyperpolyglot_name.to_lowercase();
        Some(match lowercase_name.as_ref() {
            "rust" => Self::Rust,
            "python" => Self::Python,
            "javascript" => Self::Js,
            "typescript" => Self::Ts,
            "tsx" => Self::Tsx,
            "c" => Self::C,
            "c++" => Self::CPlusPlus,
            "go" => Self::Go,
            "markdown" => Self::Markdown,
            _ => return None,
        })
    }
}

impl std::fmt::Display for LanguageName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Rust => "Rust",
                Self::Python => "Python",
                Self::Js => "JavaScript",
                Self::Ts => "TypeScript",
                Self::Tsx => "TSX",
                Self::C => "C",
                Self::CPlusPlus => "C++",
                Self::Go => "Go",
                Self::Markdown => "Markdown",
            }
        )
    }
}
