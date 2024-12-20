changelog 🧚
============

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
