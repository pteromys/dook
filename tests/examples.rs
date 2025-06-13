#![allow(
    clippy::single_range_in_vec_init,
    reason = "They're all declared as Vec<Range>"
)]

use dook::LanguageName;
use dook::{inputs, main_search, searches};

mod common;

type TestCase<'a> = (&'a str, Vec<std::ops::Range<usize>>, Vec<&'a str>);

fn verify_examples(language_name: LanguageName, source: &[u8], cases: &[TestCase]) {
    let mut query_compiler = common::get_query_compiler();
    let language_info = query_compiler.get_language_info(language_name).unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language_info.language).unwrap();
    let tree = parser.parse(source, None).unwrap();
    for (query, expect_ranges, expect_recurses) in cases {
        let pattern_str = String::from("^") + query + "$";
        let pattern = regex::Regex::new(&pattern_str).unwrap();
        let search_result =
            searches::find_definition(source, &tree, &language_info, &pattern, true);
        let result_vec: Vec<_> = search_result
            .ranges
            .iter()
            .map(|r| r.start + 1..r.end)
            .collect();
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
    let mut query_compiler = common::get_query_compiler();
    let input = inputs::LoadedFile {
        bytes: source.into(),
        language_name,
        recipe: None,
        path: None,
    };
    for (query, expect_ranges) in cases {
        let current_pattern = regex::Regex::new(query).unwrap();
        let local_pattern_str = String::from("^") + query + "$";
        let local_pattern = regex::Regex::new(&local_pattern_str).unwrap();
        let search_params = main_search::SearchParams {
            local_pattern: &local_pattern,
            current_pattern: &current_pattern,
            only_names: false,
            recurse: false,
        };
        let result = main_search::search_one_file(&search_params, &input, &mut query_compiler)
            .unwrap()
            .results;
        let result_vec: Vec<_> = result.ranges.iter().map(|r| r.start + 1..r.end).collect();
        assert_eq!(
            result_vec, *expect_ranges,
            "searching {:?} for {:?} got {:?}, expected {:?}",
            language_name, query, result_vec, expect_ranges
        );
    }
}

#[test]
fn python() {
    // these ranges are 1-indexed and include both ends
    #[rustfmt::skip]
    let cases = [
        ("one", vec![12..34], vec!["hecks"]), // hm I don't like this
        ("two", vec![14..15], vec![]),
        ("three", vec![14..14, 16..16], vec![]),
        ("four", vec![14..14, 18..24], vec![]),
        ("five", vec![14..14, 19..22], vec![]),
        ("six", vec![14..14, 26..34], vec!["hecks"]),
        ("seven", vec![41..47], vec![]),
        ("eight", vec![49..49], vec![]),
        // nine and ten are function parameters split across multiple lines;
        // I assume you want the whole signature because it'll be either short enough to not be a pain
        // or long enough to need further clarification if you only see one line from it.
        ("nine", vec![14..14, 27..33], vec![]),
        ("ten", vec![14..14, 27..33], vec![]),
        ("int", vec![], vec![]),
        ("abc", vec![44..45], vec![]),
        ("xyz", vec![44..44, 46..46], vec![]),
        ("def", vec![52..53], vec![]),
        ("factorial", vec![56..57], vec!["permutations"]),
        ("permutations", vec![60..63], vec!["permutations"]),
        ("combinations", vec![66..67], vec!["factorial", "permutations"]),
        ("combinations2", vec![70..71], vec!["factorial"]),
        ("attr", vec![74..78], vec!["__setattr__", "__setitem__", "setattr"]),
        ("eleven", vec![83..84], vec![]),
        ("twelve", vec![83..84], vec![]),
        ("thirteen", vec![83..83, 86..86], vec![]),
        ("fourteen", vec![83..83, 89..92], vec![]),
        ("fifteen", vec![83..83, 94..94], vec![]),
        ("sixteen", vec![97..100], vec![]),
    ];
    verify_examples(
        LanguageName::PYTHON,
        include_bytes!("../test_cases/python.py"),
        &cases,
    );
}

#[test]
fn cython() {
    let cases = [
        ("hello", vec![1..1]),
        ("Color", vec![3..10]),
        ("component", vec![3..4]),
        ("public", vec![]),
        ("double", vec![]),
        ("i", vec![3..3, 6..7]),
        ("gamma_encode", vec![12..13]),
        ("x", vec![12..12]),
        ("float64", vec![15..15]),
    ];
    verify_multipass_examples(
        LanguageName::CYTHON,
        include_bytes!("../test_cases/cython.pyx"),
        &cases,
    );
}

#[test]
fn js() {
    // these ranges are 1-indexed and include both ends
    #[rustfmt::skip]
    let cases = [
        ("one", vec![1..1], vec![]),  // let
        ("two", vec![2..2], vec![]),  // const
        ("three", vec![4..6], vec![]),  // function declaration
        // old-style class, prototype shenanigans
        ("four", vec![8..10, 12..17, 21..23], vec![]),
        ("f", vec![12..15], vec![]),  // object key, bare
        ("flop", vec![12..15], vec![]),  // named function expression
        ("eff", vec![12..12, 16..16], vec![]),  // object key, in quotes
        ("g", vec![21..23], vec![]),  // assign to dot-property
        ("five", vec![25..29], vec![]),  // new-style class
        ("six", vec![25..26], vec![]),  // class member variable
        ("seven", vec![25..25, 28..28], vec![]),  // getter
        ("eight", vec![31..31], vec![]),  // function argument
        ("nine", vec![31..31], vec![]),  // function argument with default
        ("ten", vec![31..31], vec![]),  // rest parameters
        ("eleven", vec![33..33], vec![]),  // array destructuring
        ("twelve", vec![33..33], vec![]),  // array destructuring
        ("thirteen", vec![34..34], vec![]),  // object destructuring
        ("fourteen", vec![35..35], vec![]),  // shorthand object destructuring
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
    // these ranges are 1-indexed and include both ends
    #[rustfmt::skip]
    let cases = [
        ("eight", vec![1..1], vec![]),  // function argument
        ("nine", vec![1..1], vec![]),  // function argument with default
        ("ten", vec![1..1], vec![]),  // rest parameters
    ];
    verify_examples(
        LanguageName::TSX,
        include_bytes!("../test_cases/typescript.tsx"),
        &cases,
    );
}

#[test]
fn c() {
    // these ranges are 1-indexed and include both ends
    #[rustfmt::skip]
    let cases = [
        ("ONE", vec![3..3], vec![]),  // #define
        ("two", vec![6..6], vec![]),  // static const
        ("ThreeStruct", vec![8..11], vec![]),  // struct
        ("Three", vec![8..11], vec![]),  // typedef struct; see https://stackoverflow.com/a/1675446
        ("THREE_PTR", vec![13..13], vec![]),  // typedef of pointer to struct
        ("Pint", vec![15..15], vec![]),  // typedef pointer to other stuff
        ("Quart", vec![17..20], vec![]),  // struct not in a typedef
        ("four", vec![8..9], vec![]),  // member
        ("five", vec![8..8, 10..10], vec![]),  // array
        ("six", vec![22..22], vec![]),  // unreasonable levels of pointer nesting
        ("SEVEN", vec![24..24, 34..34], vec![]),  // macro
        ("second_order", vec![26..32], vec![]),  // function definition
        ("callback", vec![26..30], vec![]),  // function pointer
        ("right", vec![26..30], vec![]),  // other function parameter
        ("val", vec![36..37], vec![]),  // assignment
        ("ptr", vec![36..36, 38..38], vec![]),  // assignment by one level of pointer
    ];
    verify_examples(LanguageName::C, include_bytes!("../test_cases/c.c"), &cases);
}

#[test]
fn rust() {
    let cases = [
        ("PotorooTreat", vec![1..6, 12..16]),
        ("Bug", vec![2..2, 5..5]),
        ("Treat", vec![8..10, 12..16]),
        ("eat", vec![8..9, 12..15]),
        ("thorax", vec![2..2, 5..5, 18..18, 20..20, 22..22]),
        ("abdomen", vec![2..2, 5..5, 18..20, 23..23]),
        ("hatch", vec![27..27]),
    ];
    verify_multipass_examples(
        LanguageName::RUST,
        include_bytes!("../test_cases/rust.rs"),
        &cases,
    );
}

#[test]
fn markdown_injections() {
    let mut cases = [
        ("author", vec![3..3]),
        ("Nordstrom", vec![10..10, 14..14, 17..17, 19..19, 21..21]),
        ("spartacus", vec![26..26, 30..31, 34..35]),
    ];
    if cfg!(feature = "stdin") {
        cases[2].1.extend_from_slice(&[38..38, 40..40]);
    }
    verify_multipass_examples(
        LanguageName::MARKDOWN,
        include_bytes!("../test_cases/injection.md"),
        &cases,
    );
}

#[test]
fn html() {
    let cases = [
        ("chill", vec![2..3, 5..8]), // classname is only in CSS
        ("sick", vec![2..3, 5..5, 9..11, 19..19, 23..25]), // id is in CSS and HTML
        ("Title", vec![2..2, 19..22]), // headings
        // js; also tests nuisance injection---should be substring of classname example
        ("ill", vec![2..3, 13..16]),
    ];
    verify_multipass_examples(
        LanguageName::HTML,
        include_bytes!("../test_cases/html.html"),
        &cases,
    );
}

#[test]
fn tex() {
    let cases = [
        ("thm", vec![12..12]),
        ("qq", vec![13..13]),
        ("R", vec![15..16]),
        ("Introduction", vec![22..22, 26..41]),
        ("intro", vec![22..22, 26..41]),
        ("the_question", vec![22..22, 26..26, 29..29, 34..37]),
        ("Roadmap", vec![22..22, 26..26, 39..41]),
        ("methods", vec![22..22, 43..46]),
    ];
    verify_multipass_examples(
        LanguageName::TEX,
        include_bytes!("../test_cases/tex.tex"),
        &cases,
    );
}
