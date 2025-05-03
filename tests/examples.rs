#![allow(
    clippy::single_range_in_vec_init,
    reason = "They're all declared as Vec<Range>"
)]

use dook::{inputs, loader, main_search, searches};
use dook::{Config, LanguageName, QueryCompiler};

type TestCase<'a> = (&'a str, Vec<std::ops::Range<usize>>, Vec<&'a str>);

fn verify_examples(language_name: LanguageName, source: &[u8], cases: &[TestCase]) {
    let config = Config::load_default();
    let target_dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mut language_loader =
        loader::Loader::new(target_dir.clone(), Some(target_dir.clone()), false).expect(
            "should have called tree_sitter_loader::Loader::with_parser_lib_path(), not new()",
        );

    let parser_source = config.get_parser_source(language_name).unwrap();
    let language = language_loader
        .get_language(parser_source)
        .unwrap()
        .unwrap();
    let mut query_compiler = QueryCompiler::new(&config);
    let language_info = query_compiler
        .get_language_info(language_name, &language)
        .unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(
            &language_loader
                .get_language(config.get_parser_source(language_name).unwrap())
                .unwrap()
                .unwrap(),
        )
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    for (query, expect_ranges, expect_recurses) in cases {
        let pattern_str = String::from("^") + query + "$";
        let pattern = regex::Regex::new(&pattern_str).unwrap();
        let search_result =
            searches::find_definition(source, &tree, &language_info, &pattern, true);
        let result_vec: Vec<_> = search_result.ranges.iter().collect();
        assert_eq!(
            result_vec, *expect_ranges,
            "searching {:?} for {:?} got {:?}, expected {:?}",
            language_name, query, result_vec, expect_ranges
        );
        assert_eq!(
            search_result.recurse_names, *expect_recurses,
            "searching {:?} for {:?} recursed toward {:?}, expected {:?}",
            language_name, query, search_result.recurse_names, expect_recurses
        );
    }
}

type MultiPassTestCase<'a> = (&'a str, Vec<std::ops::Range<usize>>);

fn verify_multipass_examples(
    language_name: LanguageName,
    source: &[u8],
    cases: &[MultiPassTestCase],
) {
    let config = Config::load_default();
    let target_dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mut language_loader =
        loader::Loader::new(target_dir.clone(), Some(target_dir.clone()), false).expect(
            "should have called tree_sitter_loader::Loader::with_parser_lib_path(), not new()",
        );
    let mut query_compiler = QueryCompiler::new(&config);
    let input = inputs::LoadedFile {
        bytes: source.into(),
        language_name,
    };
    for (query, expect_ranges) in cases {
        let current_pattern = regex::Regex::new(query).unwrap();
        let local_pattern_str = String::from("^") + query + "$";
        let local_pattern = regex::Regex::new(&local_pattern_str).unwrap();
        let search_params = main_search::SearchParams {
            config: &config,
            local_pattern: &local_pattern,
            current_pattern: &current_pattern,
            only_names: false,
            recurse: false,
        };
        let result = main_search::search_one_file(
            &search_params,
            inputs::SearchInput::Loaded(&input),
            &mut language_loader,
            &mut query_compiler,
        )
        .unwrap();
        let result_vec: Vec<_> = result.ranges.iter().collect();
        assert_eq!(
            result_vec, *expect_ranges,
            "searching {:?} for {:?} got {:?}, expected {:?}",
            language_name, query, result_vec, expect_ranges
        );
    }
}

#[test]
fn python() {
    // these ranges are 0-indexed and bat line numbers are 1-indexed so generate them with `nl -ba -v0`
    #[rustfmt::skip]
    let cases = [
        ("one", vec![11..34], vec!["hecks"]), // hm I don't like this
        ("two", vec![13..15], vec![]),
        ("three", vec![13..14, 15..16], vec![]),
        ("four", vec![13..14, 17..24], vec![]),
        ("five", vec![13..14, 21..22], vec![]),
        ("six", vec![13..14, 25..34], vec!["hecks"]),
        ("seven", vec![40..47], vec![]),
        ("eight", vec![48..49], vec![]),
        // nine and ten are function parameters split across multiple lines;
        // I assume you want the whole signature because it'll be either short enough to not be a pain
        // or long enough to need further clarification if you only see one line from it.
        ("nine", vec![13..14, 26..33], vec![]),
        ("ten", vec![13..14, 26..33], vec![]),
        ("int", vec![], vec![]),
        ("abc", vec![43..45], vec![]),
        ("xyz", vec![43..44, 45..46], vec![]),
        ("def", vec![51..53], vec![]),
        ("factorial", vec![55..57], vec!["permutations"]),
        ("permutations", vec![59..63], vec!["permutations"]),
        ("combinations", vec![65..67], vec!["factorial", "permutations"]),
        ("combinations2", vec![69..71], vec!["factorial"]),
        ("attr", vec![73..78], vec!["__setattr__", "__setitem__", "setattr"]),
        ("eleven", vec![82..84], vec![]),
        ("twelve", vec![82..84], vec![]),
        ("thirteen", vec![82..83, 85..86], vec![]),
        ("fourteen", vec![82..83, 89..92], vec![]),  // 2nd group should be 88..92
        ("fifteen", vec![82..83, 93..94], vec![]),
        ("sixteen", vec![96..100], vec![]),
    ];
    verify_examples(
        LanguageName::PYTHON,
        include_bytes!("../test_cases/python.py"),
        &cases,
    );
}

#[test]
fn js() {
    // these ranges are 0-indexed and bat line numbers are 1-indexed so generate them with `nl -ba -v0`
    #[rustfmt::skip]
    let cases = [
        ("one", vec![0..1], vec![]),  // let
        ("two", vec![1..2], vec![]),  // const
        ("three", vec![3..6], vec![]),  // function declaration
        // old-style class, prototype shenanigans
        ("four", vec![7..10, 11..17, 20..23], vec![]),
        ("f", vec![11..15], vec![]),  // object key, bare
        ("flop", vec![11..15], vec![]),  // named function expression
        ("eff", vec![11..12, 15..16], vec![]),  // object key, in quotes
        ("g", vec![20..23], vec![]),  // assign to dot-property
        ("five", vec![24..29], vec![]),  // new-style class
        ("six", vec![24..26], vec![]),  // class member variable
        ("seven", vec![24..25, 27..28], vec![]),  // getter
        ("eight", vec![30..31], vec![]),  // function argument
        ("nine", vec![30..31], vec![]),  // function argument with default
        ("ten", vec![30..31], vec![]),  // rest parameters
        ("eleven", vec![32..33], vec![]),  // array destructuring
        ("twelve", vec![32..33], vec![]),  // array destructuring
        ("thirteen", vec![33..34], vec![]),  // object destructuring
        ("fourteen", vec![34..35], vec![]),  // shorthand object destructuring
    ];
    for language_name in [
        LanguageName::JAVASCRIPT,
        LanguageName::TYPESCRIPT,
        LanguageName::TSX,
    ] {
        verify_examples(
            language_name,
            include_bytes!("../test_cases/javascript.js"),
            &cases,
        );
    }
}

#[test]
fn tsx() {
    // these ranges are 0-indexed and bat line numbers are 1-indexed so generate them with `nl -ba -v0`
    #[rustfmt::skip]
    let cases = [
        ("eight", vec![0..1], vec![]),  // function argument
        ("nine", vec![0..1], vec![]),  // function argument with default
        ("ten", vec![0..1], vec![]),  // rest parameters
    ];
    verify_examples(
        LanguageName::TSX,
        include_bytes!("../test_cases/typescript.tsx"),
        &cases,
    );
}

#[test]
fn c() {
    // these ranges are 0-indexed and bat line numbers are 1-indexed so generate them with `nl -ba -v0`
    #[rustfmt::skip]
    let cases = [
        ("ONE", vec![2..3], vec![]),  // #define
        ("two", vec![5..6], vec![]),  // static const
        ("ThreeStruct", vec![7..11], vec![]),  // struct
        ("Three", vec![7..11], vec![]),  // typedef struct; see https://stackoverflow.com/a/1675446
        ("THREE_PTR", vec![12..13], vec![]),  // typedef of pointer to struct
        ("Pint", vec![14..15], vec![]),  // typedef pointer to other stuff
        ("Quart", vec![16..20], vec![]),  // struct not in a typedef
        ("four", vec![7..9], vec![]),  // member
        ("five", vec![7..8, 9..10], vec![]),  // array
        ("six", vec![21..22], vec![]),  // unreasonable levels of pointer nesting
        ("SEVEN", vec![23..24, 33..34], vec![]),  // macro
        ("second_order", vec![25..32], vec![]),  // function definition
        ("callback", vec![25..30], vec![]),  // function pointer
        ("right", vec![25..30], vec![]),  // other function parameter
    ];
    verify_examples(LanguageName::C, include_bytes!("../test_cases/c.c"), &cases);
}

#[test]
fn markdown_injections() {
    let cases = [
        ("Nordstrom", vec![10..11, 12..13]),
        ("spartacus", vec![22..23, 26..27]),
    ];
    verify_multipass_examples(
        LanguageName::MARKDOWN,
        include_bytes!("../test_cases/injection.md"),
        &cases,
    );
}
