#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, strum::EnumIter)]
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
