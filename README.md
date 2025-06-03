![mascot](dook.svg)

dook: Definition lookup in your code
====================================

Find code definitions using [tree-sitter](https://tree-sitter.github.io/) and [ripgrep](https://github.com/BurntSushi/ripgrep), and pretty-print them using [bat](https://github.com/sharkdp/bat).

## Installation

Install [ripgrep](https://github.com/BurntSushi/ripgrep), [bat](https://github.com/sharkdp/bat), and [uv](https://docs.astral.sh/uv/getting-started/installation/). Then:

```sh
uv tool install dook
```

(It's also on crates.io if you want to build it yourself. Or if you want to be on the cutting edge you can instead `cargo install --git https://github.com/pteromys/dook -F stdin,static_python`.)

If you want completion assistance in bash or zsh, install either:

- assets/completions/dook.bash as `~/.local/share/bash-completion/completions/dook` or
- assets/completions/dook.zsh as a file named `_dook` somewhere on zsh's `fpath`.

## Example usage

```
pteromys@delia ~/src/dook $ dook into_iter
───────┬────────────────────────────────────────────────────
       │ File: ./src/range_union.rs
───────┼────────────────────────────────────────────────────
  51   │ impl<'it> IntoIterator for &'it RangeUnion {
 ...   │ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ 8< ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─
  54   │     fn into_iter(self) -> Self::IntoIter {
  55   │         self.iter()
  56   │     }
───────┴────────────────────────────────────────────────────
```

Also attempts to find assignments, class definitions, etc—because why should you have to figure out the difference before you search? In old javascript codebases there isn't really a difference anyway!

## Supported languages

- C, C++, GLSL
- Go
- HTML, CSS, Markdown
- JavaScript, TypeScript, TSX
- Lua
- Python, Cython, Jupyter Notebook (as Python fragments in Markdown)
- Rust
- Shell (everything is just assumed to be bash)
- TeX (as LaTeX; requires the [tree-sitter CLI](https://tree-sitter.github.io/tree-sitter/creating-parsers/1-getting-started.html#installation))
- YAML

## How to add a language

1. Create `~/.config/dook/somelanguage.yml`, where `somelanguage` is one of the keys in `src/language_aliases.rs`. Crib from any of the files in `config/`. (`python.yml` is probably a decent example.)
2. Create a trimmed-down version of the scenario that motivated you to add this language, and put it in `test_cases/`, named anything you like.
3. Use `dook` with this language for a week or two. Every time you edit .yml file, consider adding a corresponding example to the file you created in `test_cases/`.

If you want to roll the dice on submitting a PR (see below), copy your .yml file to `config/` and submit a patch adding that and your file from `test_cases/`. If you're familiar with Rust, add a constant for your language to `LanguageName` and your list of example search terms to `tests/examples.rs`. Otherwise I'll add those for you if I ever respond.

## Files

Create `~/.config/dook/downloads_policy.txt` containing the text `yes`, `ask`, or `no` to control what dook does when it needs to download a language parser from the internet. The default is `ask` (ask for confirmation every time). If you're paranoid, you might prefer to use `ask` for a week and then switch to `no` to get pager integration.

dook creates and uses the following directories for storage:

- `~/.cache/dook/sources` for downloaded parsers
- `~/.cache/tree-sitter` for compiled parsers, using [tree-sitter-loader](https://crates.io/crates/tree-sitter-loader)

## Alternatives and prior art

- [symbex](https://github.com/simonw/symbex): find a definition in python, plus some other operating modes; aimed at [slicing/splicing code as input/output to an LLM](https://simonwillison.net/2023/Jun/18/symbex/).
- [cq](https://github.com/newlinedotco/cq): jq-but-for-code, using a more powerful code query syntax, also powered by tree-sitter.
- [ast-grep](https://ast-grep.github.io/) and [comby](https://comby.dev/): syntax-aware find-and-replace (so query language supports some form of captures)
- [mogglo](https://langston-barrett.github.io/mogglo/): lua-scriptable code search and transformation
- [`git grep -W 'def your_function_name\('`](https://git-scm.com/docs/git-grep) (or a language-specific analogue) is often good enough in [a bunch of languages](https://git-scm.com/docs/gitattributes#_defining_a_custom_hunk_header). And if you pipe it to [delta](https://dandavison.github.io/delta/grep.html) you can get syntax highlighting.
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

- keep adding more languages
- show all calls to a function instead (`git grep -W` already gets mostly there so I care less)
- add a recontextualize mode—pipe in ripgrep output to make it look like `git grep -W` output—which really ought to be an independent utility but would benefit from dook's definitions of what counts as a parent
- make better patterns (particularly for C and C++) now that I'm on tree-sitter 0.23
- descend into external libraries and standard libraries if there's a sane way to look for them

## License

[MIT](./LICENSE)

## Contributing

Contribute at your own risk! I might ignore your PRs and issues for no good reason, close them without explanation, or rewrite them from scratch and forget to tell you.

By submitting a PR, you agree to license your contributions under the MIT License, credited in [the changelog](./CHANGES.md) or somewhere reachable from this README.
