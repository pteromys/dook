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

merde::derive! {
    impl (Serialize, Deserialize) for enum LanguageName
    string_like {
        "rust" => Rust,
        "python" => Python,
        "js" => Js,
        "ts" => Ts,
        "tsx" => Tsx,
        "c" => C,
        "cplusplus" => CPlusPlus,
        "go" => Go,
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
