![mascot](dook.svg)

dook: Definition lookup in your code
====================================

Find code definitions using [tree-sitter](https://tree-sitter.github.io/) and [ripgrep](https://github.com/BurntSushi/ripgrep), and pretty-print them using [bat](https://github.com/sharkdp/bat).

## Installation

Install [ripgrep](https://github.com/BurntSushi/ripgrep) and [bat](https://github.com/sharkdp/bat). Build and install with:

```sh
cargo install dook
```

If you want support for `--stdin`, which needs an unreleased version of `hyperpolyglot`:

```sh
cargo install --git https://github.com/pteromys/dook -F stdin
```

If you want completion assistance in bash or zsh, install either:

- assets/completions/dook.bash as `~/.local/share/bash-completion/completions/dook` or
- assets/completions/dook.zsh as a file named `_dook` somewhere on zsh's `fpath`.

## Example usage

```
pteromys@delia ~/src/dook $ dook write
───────┬────────────────────────────────────────────────────────────────
       │ File: ./src/paging.rs
───────┼────────────────────────────────────────────────────────────────
  61   │ impl std::io::Write for MaybePager {
 ...   │ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ 8< ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─
  69   │     fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
  70   │         match self.pipe() {
  71   │             Some(pipe) => pipe.write(buf),
  72   │             None => std::io::stdout().write(buf),
  73   │         }
  74   │     }
───────┴────────────────────────────────────────────────────────────────
```

Also attempts to find assignments, class definitions, etc—because why should you have to figure out the difference before you search? In old javascript codebases there isn't really a difference anyway!

## Supported languages

- c
- c++
- go
- javascript
- python
- rust
- typescript
- tsx

## Alternatives and prior art

- [symbex](https://github.com/simonw/symbex): find a definition in python, plus some other operating modes; aimed at [slicing/splicing code as input/output to an LLM](https://simonwillison.net/2023/Jun/18/symbex/).
- [cq](https://github.com/newlinedotco/cq): jq-but-for-code, using a more powerful code query syntax, also powered by tree-sitter.
- [ast-grep](https://ast-grep.github.io/) and [comby](https://comby.dev/): syntax-aware find-and-replace (so query language supports some form of captures)
- [mogglo](https://langston-barrett.github.io/mogglo/): lua-scriptable code search and transformation
- [`git grep -W 'def your_function_name\('`](https://git-scm.com/docs/git-grep) (or a language-specific analogue) is often good enough in [a bunch of languages](https://git-scm.com/docs/gitattributes#_defining_a_custom_hunk_header).
- [searching directly on GitHub](https://docs.github.com/en/repositories/working-with-files/using-files/navigating-code-on-github#precise-and-search-based-navigation), powered by [stack graphs](https://docs.rs/stack-graphs/latest/stack_graphs/graph/index.html)

## Goals vs non-goals

- Find definitions.
- Show enough context to answer the usual questions that motivate looking up a definition.
- Minimize user inputs:
  - Amount of thinking the user needs to do before typing the first invocation
  - Amount of output the user needs to read or scroll past to see what they wanted
  - Number of invocations
  - Amount of typing to make each invocation
- Balance responsiveness and thoroughness: explicitly, return results before the user thinks of a follow-up query.
- Sacrifice consistency and machine readability if necessary to improve chances of showing relevant context. (If you want machine-predictable output, use `cq` instead.)
- Sacrifice disk and RAM footprint for accuracy and my own development time—tree-sitter parsers are maybe 2 megs each on disk, and the codebase you're searching is probably larger. But do try to keep RAM usage an order of magnitude or so smaller than the IDE you're probably procrastinating opening.
- Be open to similar use cases (like find all mutations of a variable or something), but don't let that block getting good at finding definitions.

## Possible future work

- more languages (and tests in more languages 😱)
- show all calls to a function instead (`git grep -W` already gets mostly there so I care less)
- end this project and make it instead a feature of ripgrep (may not be viable because the parsers for all the languages add up to a pretty large binary size; then again maybe [rubicon](https://crates.io/crates/rubicon) offers some hope)
- make better patterns (particularly for C and C++) now that I'm on tree-sitter 0.23

## License

[MIT](./LICENSE)
