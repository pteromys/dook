changelog 🧚
============

## Unreleased

- Dynamically load parsers with tree-sitter-loader.
  - Sources go in ~/.cache/dook/sources.
  - Compiled parsers go in ~/.cache/tree-sitter/lib.
  - Some parsers can be built in at compile time, using the following features:
    - `static_python`
- Support injections (embedded fragments in another language):
  - rust inside rust macros
  - fenced code blocks inside markdown
- New default languages:
  - markdown (look up sections by header)
- Config file format version 3 (never released v2):
  - Add integer "_version" field.
  - Switch to yaml since we use a lot of multi-line strings.
  - Add per-language "parser" field for specifying parser sources.
  - Change queries to single strings.
  - All fields are now optional so your config only needs to be overrides of the default.
  - Parent node types and exclusions are now a query with an optional `@exclude` capture.
  - Language names are case-sensitive and are whatever hyperpolyglot uses.
- New CLI options:
  - `--wrap` and `-S` (forwarded to `bat`)
  - `--stdin` to search on stdin, gated by `stdin` feature
  - `--only-names` to print matching names but not defs (used for completions)
  - `--offline` to not download anything
  - `--verbose` to litter console output with unstructured progress messages
- Fixes:
  - Search patterns with `|` no longer fail to restrict to whole-name match.
  - Catch names bound by destructuring in js, ts, tsx, python.
  - Fixed failing to match some recursion patterns.
  - Catch named function expressions in ts/tsx.
  - Show parent object names in js/ts/tsx.
- Cache tree-sitter query compilation for 4x speedup.
- Add bash and zsh completions.
- Order search by likely imports to try to get results sooner (python).

## 0.2.0 (2024-12-14)

- Don't recurse by default—took too long (and was wrong anyway) on python's dict.get(). Sorry!
- Smarter language detection with [hyperpolyglot](https://github.com/monkslc/hyperpolyglot).
- Fixes:
  - Don't include previous line's end-of-line comment.
  - python patterns: include the most basic setattr calls as assignment.
  - js/ts/tsx: catch rest and default params; and arrow functions as parents.
- Tests for C.

## 0.1.1 (2024-07-27)

- Ferret mascot! 🦦
- Include stuff on .prototype in js/ts/tsx patterns.

## 0.1.0 (2024-06-30)

- Rename to `dook`.
- Tests.
- Add recursive mode, on by default.
- Improve typescript patterns:
  - Include field definition and assignment.
  - Include enums.
  - Add definitions of types, interfaces, etc as parent/ancestor patterns.

## Initial push to GitHub (cc238ed, 2024-05-12)

- Translate from python and adopt tree-sitter so I can add more languages, now up to: python, go, c, c++, js, ts, tsx, rust.
- Load search patterns from a config file.
