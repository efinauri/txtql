# txtql

Extract structured JSON from free text with readable grammar rules instead of regular expressions.

txtql is a small query language, a command-line tool, a Rust library and a language server. You describe
the text you expect as a handful of named rules, say how each match should look as JSON, and txtql parses the
whole input and prints the result. It is meant for people who turn logs, reports, config files and other
semi-structured text into data. Compared with a regular expression it reads like a grammar (rules that
refer to rules, so nested and recursive structure works); compared with `awk` or `jq` it needs no
program logic and no pre-existing JSON. If a piece of text can be read in two ways that would give different
output, txtql tells you instead of silently picking one, and every mistake in a query or in the input is
reported as a [miette](https://docs.rs/miette) diagnostic with source snippets and hints. Editors get
diagnostics, navigation and completion through a built-in language server.

## Example

{{q g_rhymes}}

Given this input:

{{i g_rhymes}}

`txtql query.tql input.txt` prints:

{{o g_rhymes}}

`TEXT` is the root rule and must match the whole input. Every character counts, including the final line
break, so nothing is skipped behind your back. When the text does not fit, you get a pointer to the place and
what was expected there (here, the last line is cut short):

{{e g_rhymes_err}}

## Features

- Rules in any order, with `OR`, repetition as `a TO b`, `SPLITBY`, `SKIPPING`, `UNTIL` / `UNTILBEFORE`, labels and aliases.
- Recursive rules, so nested structure such as s-expressions or bracketed lists can be matched.
- Output templates (`AS`) that build any JSON shape, including matched text used as keys, loops (`FOR`), `LISTOF`,
  merging, and functions such as `NUM`, `JOIN` and `ZIP`.
- `WHERE` conditions to accept or reject a match.
- Predictable disambiguation (greedy by default, `LAZY` for less, leftmost `OR` branch first), and a warning when another reading
  would change the output; `--strict` turns it into an error.
- Stable diagnostic codes (`txtql::*`), with hints such as "did you mean" and the txtql spelling of regex habits.
- Linear time for typical queries, polynomial worst case, step and depth limits that end runaway queries cleanly.
- A language server (`txtql lsp`) and a Sublime Text package.

## Installation

Requirements: a Rust toolchain with edition 2024 support (Rust 1.85 or newer; install from <https://rustup.rs>).
There is no binary release yet; build from source.

```
git clone https://github.com/efinauri/txtql.git
cd txtql
cargo install --path .                  # installs txtql into ~/.cargo/bin
```

Or `cargo build --release` and run `target/release/txtql`.

## Quick start

```
txtql QUERY_FILE [INPUT]            # INPUT defaults to standard input
txtql -e 'TEXT = 1 TO n LINE SPLITBY NL' [INPUT]
txtql check QUERY_FILE [SAMPLE]     # check the query; with a sample, also list ambiguities (exit code 2 if any)
txtql lsp                           # language server for editors
```

Options: `--strict` (ambiguity and repeated keys become errors), `--compact` (one-line JSON),
`--max-steps N`, `--max-depth N`, `--no-ambiguity-check`. JSON goes to standard output; every diagnostic goes to
standard error.

```sh
printf 'one\ntwo\nthree' | txtql --compact -e "TEXT = 1 TO n LINE SPLITBY NL"
```

{{o g_expr}}

As a library (not on crates.io yet; depend on it by path or git):

```rust
let query = txtql::Query::compile("TEXT = 1 TO n LINE SPLITBY NL").unwrap();
let out = query.run("one\ntwo\nthree", &txtql::Options::default()).unwrap();
println!("{}", out.value);   // a serde_json::Value: ["one","two","three"]
```

A compiled `Query` is `Send + Sync` and can serve many runs; errors convert to miette reports (see
[Getting Started](https://github.com/efinauri/txtql/wiki/Getting-Started)).

## Editor support

`txtql lsp` speaks the Language Server Protocol: diagnostics as you type (the same ones `txtql check` prints),
semantic highlighting, go to definition, references, rename, hover, context-aware completion and an outline.
It keeps working on unfinished queries. Setup for Sublime Text (syntax, comment toggling, LSP client settings),
JetBrains IDEs, Neovim and Helix is on the wiki:
[Editor Support](https://github.com/efinauri/txtql/wiki/Editor-Support).

## Documentation

The documentation lives in the [wiki](https://github.com/efinauri/txtql/wiki):

- [Getting Started](https://github.com/efinauri/txtql/wiki/Getting-Started) and
  [Language Walkthrough](https://github.com/efinauri/txtql/wiki/Language-Walkthrough): install, first query, then the language step by step.
- [Practical Examples](https://github.com/efinauri/txtql/wiki/Practical-Examples): a cookbook with CSV, TSV and INI files, access logs, stack traces, changelogs, emails and CI use.
- [Language Reference](https://github.com/efinauri/txtql/wiki/Language-Reference): syntax, built-in patterns, functions, operators.
- [Ambiguity and Strict Mode](https://github.com/efinauri/txtql/wiki/Ambiguity-and-Strict-Mode) and
  [Errors and Diagnostics](https://github.com/efinauri/txtql/wiki/Errors-and-Diagnostics) (every `txtql::*` code).
- [Editor Support](https://github.com/efinauri/txtql/wiki/Editor-Support) and
  [Performance and Limits](https://github.com/efinauri/txtql/wiki/Performance-and-Limits).
- [Testing and Contributing](https://github.com/efinauri/txtql/wiki/Testing-and-Contributing) and
  [FAQ and Known Limitations](https://github.com/efinauri/txtql/wiki/FAQ-and-Known-Limitations).

The behaviour is specified formally in Allium, in `txtql.allium` and `spec/`.

## Project status

Version 0.1.0, not yet released; the language may still change. Known limitations (Unicode case folding,
error locations inside `FOR`, indentation-based formats) are in the
[FAQ](https://github.com/efinauri/txtql/wiki/FAQ-and-Known-Limitations).

## Contributing

```
cargo test                                      # unit, golden, property and CLI tests
cargo test --release --test stress -- --ignored # slow 10 MB wall-clock runs
cargo bench                                     # throughput and scaling
cargo +nightly fuzz run fuzz_parse -- -dict=fuzz/txtql.dict   # needs cargo-fuzz and nightly
```

Golden-file workflow and more:
[Testing and Contributing](https://github.com/efinauri/txtql/wiki/Testing-and-Contributing).

## License

txtql is released under the MIT license; see [LICENSE](https://github.com/efinauri/txtql/blob/main/LICENSE).
