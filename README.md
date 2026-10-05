[![CI](https://github.com/efinauri/txtql/actions/workflows/ci.yml/badge.svg)](https://github.com/efinauri/txtql/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Contributions welcome](https://img.shields.io/badge/contributions-welcome-brightgreen.svg)](https://github.com/efinauri/txtql/wiki/Testing-and-Contributing)

<!-- TOC -->
* [txtql](#txtql)
  * [Example](#example)
* [Repository contents](#repository-contents)
  * [Language Specification](#language-specification)
  * [Command-line tool](#command-line-tool)
  * [Rust library](#rust-library)
  * [Language server](#language-server)
* [Performance](#performance)
<!-- TOC -->

# txtql

__txtql__ is a terse query language to extract structured JSON from free text.

The query is expressed through a set of named rules that uniquely partition the text, whereas the JSON structure
is defined by capture instructions that can be appended to each rule.

## Example

Input text:

```text
roses are red
violets are blue
bees are black and yellow
```

Desired output:

```json
{
  "roses": [
    "red"
  ],
  "violets": [
    "blue"
  ],
  "bees": [
    "black",
    "yellow"
  ]
}
```

Query:

```txtql
--the entry point is a rule that matches the entire text.
TEXT   = 1 TO n rhyme
--this capture at the root rule tells txtql to treat the rhyme 
--rule's capture as root elements, as opposed to an array.
    AS { rhyme }
rhyme  = noun:WORD ' are ' colors (NL OR EOF)
    AS { noun: colors }
colors = 1 TO n WORD SPLITBY ' and '
```

When the text does not fit, you get a pointer to the place and
what was expected there (here, the last line is cut short):

```text
txtql::input::no_parse

  × the text does not match the query
   ╭─[input.txt:3:15]
 2 │ violets are blue
 3 │ bees are black and
   ·               ┬
   ·               ╰── expected ' and ', a line break or the end of the text
   ╰────
  help: the text matched up to here; found " and"
```

# Repository contents

## Language Specification

Quickstart: the [Getting Started](https://github.com/efinauri/txtql/wiki/Getting-Started) page of the
[wiki](https://github.com/efinauri/txtql/wiki), then the
[Language Walkthrough](https://github.com/efinauri/txtql/wiki/Language-Walkthrough).

Full language guide: the [Language Reference](https://github.com/efinauri/txtql/wiki/Language-Reference), with the grammar,
patterns, templates, functions and conditions.

___

For contributors: the behavioral specification of this project is written in, and maintained by,
allium: [`txtql.allium`](txtql.allium). Modules in [`spec/`](spec).

## Command-line tool

Installation (requires a Rust toolchain, 1.85 or newer, from <https://rustup.rs>):

```sh
git clone https://github.com/efinauri/txtql.git
cd txtql
cargo install --path .   # installs txtql into ~/.cargo/bin
```

Example usage, with the query above saved as `rhymes.tql` and the text as `poem.txt`:

```sh
txtql rhymes.tql poem.txt                    # query file, then input file (standard input if omitted)
printf 'one\ntwo\nthree' | txtql --compact -e 'TEXT = 1 TO n LINE SPLITBY NL'
txtql check rhymes.tql poem.txt              # check the query; with input, also report ambiguities
```

```json
["one","two","three"]
```

The JSON goes to standard output and every diagnostic to standard error. Flags:

| Flag                   | Meaning                                                                              |
|------------------------|--------------------------------------------------------------------------------------|
| `-e, --expr QUERY`     | give the query inline instead of as a file; the only file argument is then the input |
| `--strict`             | treat output-changing ambiguity and repeated keys as errors                          |
| `--compact`            | print one-line JSON instead of pretty-printed JSON                                   |
| `--max-steps N`        | give up after N parser steps (default 200000000)                                     |
| `--max-depth N`        | maximum nesting depth of rule matches (default 10000)                                |
| `--no-ambiguity-check` | skip the ambiguity analysis                                                          |

`txtql check` exits with 2 when it finds an ambiguity or a repeated key.

## Rust library

txtql is not on crates.io yet, so depend on it through git. In a new project (`cargo new demo`), add to `Cargo.toml`:

```toml
[dependencies]
txtql = { git = "https://github.com/efinauri/txtql" }
```

and put this in `src/main.rs`:

```rust
use txtql::{Options, Query};

/// Splits `text` into its lines and returns them as a compact JSON array.
fn lines_as_json(text: &str) -> String {
    let query = Query::compile("TEXT = 1 TO n LINE SPLITBY NL").unwrap();
    let output = query.run(text, &Options::default()).unwrap();
    output.value.to_string()
}

fn main() {
    println!("{}", lines_as_json("one\ntwo\nthree"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lines() {
        assert_eq!(lines_as_json("one\ntwo\nthree"), r#"["one","two","three"]"#);
    }
}
```

`cargo run` prints `["one","two","three"]` and `cargo test` passes. A compiled `Query` can be reused for many runs.

## Language server

`txtql lsp` runs the language server. To use it in Sublime Text, install txtql as above so that `txtql` is on your `PATH`, then:

1. Install Package Control (Tools, Install Package Control), then the **LSP** package (Package Control: Install Package, then LSP).
2. From a checkout of this repository, copy the syntax and comment settings into your `Packages/User` folder
   (on macOS `~/Library/Application Support/Sublime Text/Packages/User`):

   ```sh
   mkdir -p ~/.config/sublime-text/Packages/User/txtql
   cp editors/sublime/txtql.sublime-syntax editors/sublime/Comments.tmPreferences ~/.config/sublime-text/Packages/User/txtql/
   ```

3. Open Preferences, Package Settings, LSP, Settings and merge in the entries from
   [`editors/sublime/LSP.sublime-settings`](editors/sublime/LSP.sublime-settings).
4. For semantic highlighting, the colour scheme needs a `meta.semantic-token` rule. If you use Mariana or Breakers, copy the matching
   `editors/sublime/*.sublime-color-scheme` file into `Packages/User`; with another scheme, add the same rule to it.

# Performance

- **Time is linear in the input size** for typical queries (lists of records, lines, key/value pairs), including the ambiguity
  check: doubling the input doubles the run time. Memory grows linearly too, at roughly 100 MB of peak memory per MB of input,
  more for queries with many alternatives.
- **The worst case is polynomial, never exponential.** A right-recursive rule (`a = 'x' a OR 'x'`) is quadratic in time and memory,
  and `txtql check` warns about it; a repetition (`1 TO n 'x'`) does the same job in linear time. A fully ambiguous grammar is cubic
  at worst. `--max-steps` and `--max-depth` end a runaway query with a clean error.
- **The ambiguity check** adds about 40% to the run time on the rhymes query below; `--no-ambiguity-check` skips it.

Measured with a release build; each figure is the median of three runs, with the input read from a file and the JSON
discarded (`txtql --compact query.tql input.txt > /dev/null`). txtql is single-threaded.

| Query (input repeated up to the size)                                                                                                                       | 1 MB   | 2 MB   | 4 MB    | 8 MB    | Peak memory at 8 MB |
|-------------------------------------------------------------------------------------------------------------------------------------------------------------|--------|--------|---------|---------|---------------------|
| Rhyme lines, `noun are color and color`, to JSON (ambiguity check on)                                                                                       | 0.71 s | 1.42 s | 3.00 s  | 6.66 s  | 797 MB              |
| The same with `--no-ambiguity-check`                                                                                                                        | 0.51 s | 1.04 s | 2.29 s  | 4.82 s  | 720 MB              |
| Log lines: filter by level, skip the noise lines                                                                                                            | 0.55 s | 1.07 s | 2.31 s  | 4.56 s  | 535 MB              |
| NASA web server log, seven request shapes ([`tests/cases/nasa_access_log`](tests/cases/nasa_access_log))                                                    | 0.93 s | 1.84 s | 3.76 s  | 7.85 s  | 721 MB              |
| Quoted CSV with embedded commas, quotes and line breaks (the wiki's [Practical Examples](https://github.com/efinauri/txtql/wiki/Practical-Examples) recipe) | 2.61 s | 5.49 s | 11.79 s | 21.61 s | 1688 MB             |

That is about 0.35 to 1.9 MB/s depending on how much work each byte needs. The quadratic case, with `a = 'x' a OR 'x'` and the ambiguity
check off, took 0.30 s for 2,000 characters, 1.4 s for 4,000 and 5.9 s for 8,000; at 16,000 it stops at the default nesting limit.

Hardware: Intel Core i7-8650U laptop (4 cores, 8 threads, 1.9 GHz base, up to 4.2 GHz), 31 GiB RAM, Linux 7.2.7 (Arch), Rust
1.101.0-nightly (2026-09-26). The CPU governor was `powersave` and other applications were running.
