def
===

Find code definitions using [tree-sitter](https://tree-sitter.github.io/) and [ripgrep](https://github.com/BurntSushi/ripgrep), and pretty-print them using [bat](https://github.com/sharkdp/bat).

If you are here prior to this being published on crates.io under a more permanent name, set your quality expectations accordingly.

## Installation

Install [ripgrep](https://github.com/BurntSushi/ripgrep) and [bat](https://github.com/sharkdp/bat). Build and install with:

```sh
cargo install --git https://github.com/pteromys/def
```

## Example usage

```
pteromys@delia ~/src/def $ def write
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
- [`git grep -W 'def your_function_name\('`](https://git-scm.com/docs/git-grep) (or a language-specific analogue) is often good enough in [a bunch of languages](https://git-scm.com/docs/gitattributes#_defining_a_custom_hunk_header).
- [searching directly on GitHub](https://docs.github.com/en/repositories/working-with-files/using-files/navigating-code-on-github#precise-and-search-based-navigation), powered by [stack graphs](https://docs.rs/stack-graphs/latest/stack_graphs/graph/index.html)

## Possible future work

- tests 😱
- better language detection (e.g. llvm headers are C++ but most deviously end in lowercase .h)
- more languages
- recursive mode
- show all calls to a function instead (ripgrep already gets mostly there so I care less)
- end this project and make it instead a feature of ripgrep
- Upgrade tree-sitter version so I can make better patterns. The current C and C++ query patterns make me want to scream.

## License

[MIT](./LICENSE)
