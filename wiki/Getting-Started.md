# Getting Started

## Install

Requirements: a Rust toolchain that supports edition 2024 (Rust 1.85 or newer; get it from <https://rustup.rs>).
There are no binary releases yet; txtql is built from source.

```
git clone https://github.com/efinauri/txtql.git
cd txtql
cargo install --path .
```

This builds a release binary and installs it into `~/.cargo/bin` (make sure that directory is on your `PATH`);
`cargo uninstall txtql` removes it again. Alternatively, `cargo build --release` leaves the binary in
`target/release/txtql`. Check with `txtql --version`. Editor setup is described in [[Editor Support|Editor-Support]].

## Your first query

A query is a list of rules. `TEXT` is the root rule and has to match the whole input. Save this as `rhymes.tql`:

```txtql
TEXT   = 1 TO n rhyme                 AS { rhyme }
rhyme  = noun:WORD ' are ' colors NL  AS { noun: colors }
colors = 1 TO n color SPLITBY (' and ' OR ', ')
color  = WORD
```

and this as `rhymes.txt`:

```text
roses are red
violets are blue
bees are black and yellow
```

(the file ends with a line break; in txtql every character, including that one, is significant). Then run:

```
txtql rhymes.tql rhymes.txt
```

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

Read the query from the bottom: a `color` is a `WORD`; `colors` are one or more colors separated by `' and '` or
`', '`; a `rhyme` is a noun, `' are '`, its colors and a line break, and its `AS` template uses the matched noun as the key;
`TEXT` is one or more rhymes merged into one object. The [[Language Walkthrough|Language-Walkthrough]] builds
up each of these ideas step by step.

## The command line

```
txtql QUERY_FILE [INPUT]            # INPUT defaults to standard input (or `-`)
txtql -e 'QUERY' [INPUT]            # the query given inline
txtql check QUERY_FILE [SAMPLE]     # check a query; with a sample, also list ambiguities
txtql lsp                           # language server (see Editor Support)
```

| Option | Meaning |
|---|---|
| `-e`, `--expr QUERY` | query text given inline instead of a query file; the only file argument is then the input |
| `--strict` | output-changing ambiguity and repeated object keys become errors |
| `--compact` | print one-line JSON instead of pretty-printed JSON |
| `--max-steps N` | give up after this much work (default 200,000,000) |
| `--max-depth N` | maximum nesting of rule matches (default 10,000) |
| `--no-ambiguity-check` | skip the ambiguity analysis |
| `-V`, `--version`, `-h`, `--help` | version and help |

Standard output carries only the JSON result. Every diagnostic (errors, lints, ambiguity and repeated-key warnings) goes to
standard error, so `txtql ... > out.json` keeps your output clean. Exit codes: `0` success (warnings do not change it), `1` any
error, `2` from `txtql check` when it found output-changing ambiguity or repeated keys.

### Inline queries

For one-liners use `-e` and pipe the text in. In a shell, put the query in double quotes when it contains single-quoted
literals. Write one rule per line. A multi-line query given with `-e` needs real line breaks: a typed `\n` inside the quotes is not one (txtql reports an unexpected `\`). For anything longer than a line or two, use a query file:

**Command**

```sh
printf 'one\ntwo\nthree' | txtql --compact -e "TEXT = 1 TO n LINE SPLITBY NL"
```

**Output**

```json
["one","two","three"]
```

**Command**

```sh
printf 'retries=3' | txtql -e "TEXT = k:WORD '=' v:INT AS { k: NUM(v) }"
```

**Output**

```json
{
  "retries": 3
}
```

### Checking a query

`txtql check` verifies a query without producing output: syntax, names, bounds, templates, and lints (unused rules, nested unlimited
repetitions, right recursion). Without a sample it only checks the query. Give it a sample input and it also lists
output-changing ambiguities (up to 20, at most one per match):

**Query** (`query.tql`)

```txtql
TEXT = WORD
ALIAS sp = ' '
```

**Input** (`input.txt`)

```text
x
```

**Command**

```
txtql check query.tql input.txt
```

**Output** (standard error, exit code 0)

```text
txtql::lint::unused_alias

  ⚠ alias `sp` is never used
   ╭─[query.tql:2:7]
 1 │ TEXT = WORD
 2 │ ALIAS sp = ' '
   ·       ─┬
   ·        ╰── unused
   ╰────
  help: use it in a rule, or remove it

query OK; no output-changing ambiguity on this input
```

**Query** (`query.tql`)

```txtql
TEXT   = 1 TO n rhyme                 AS { rhyme }
rhyme  = noun:WORD ' are ' colors NL  AS { noun: colors }
colors = 1 TO n color SPLITBY (' and ' OR ', ')
color  = WORD
```

**Input** (`input.txt`, ends with a line break)

```text
roses are red
bees are black and yellow
```

**Command**

```
txtql check query.tql input.txt
```

**Output** (standard error, exit code 0)

```text
query OK; no output-changing ambiguity on this input
```

## As a library

The crate is not published on crates.io yet. Add it by path or git (`txtql = { git = "https://github.com/efinauri/txtql" }`):

```rust
let src = "TEXT = 1 TO n LINE SPLITBY NL";
match txtql::Query::compile(src) {
    Ok(query) => match query.run("one\ntwo\nthree", &txtql::Options::default()) {
        Ok(out) => println!("{}", out.value), // ["one","two","three"]
        Err(e) => {
            for r in e.reports("query.tql", src, "input", "one\ntwo\nthree") {
                eprintln!("{r:?}");
            }
        }
    },
    Err(e) => {
        for r in e.reports("query.tql", src) {
            eprintln!("{r:?}");
        }
    }
}
```

- `Query::compile(&str)` parses and checks a query. `Query` is `Send + Sync` and can be reused for many runs, also concurrently.
  Lints that did not stop compilation are in `query.warnings`.
- `query.run(text, &Options)` returns an `Output` with `value` (a `serde_json::Value`), `ambiguities`, `repeated_keys`,
  `ambiguity_check_complete` and `steps`.
- `Options` has `max_steps`, `max_depth`, `strict`, `check_ambiguity` and `ambiguity_steps`; `Options::default()` matches the CLI defaults.
- `CompileError::reports` and `RunError::reports` give [miette](https://docs.rs/miette) reports; `txtql::render_plain` renders one without colours.
  The error types do not implement `std::error::Error`, so convert them through these reports.
- Very deeply nested output values should be freed with `txtql::drop_deep`, because `serde_json` drops values recursively.

## Next

- [[Language Walkthrough|Language-Walkthrough]]
- [[Editor Support|Editor-Support]]: highlighting and diagnostics while you write queries.
