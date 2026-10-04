# txtql

txtql extracts structured JSON from free text with readable grammar rules instead of regular expressions.
You describe the text as a few named rules, say how each match should look as JSON, and txtql parses the whole
input, tells you when the text can be read in more than one way, and reports mistakes as miette diagnostics
with source snippets and hints. It is a command-line tool, a Rust library and a language server.

```txtql
TEXT   = 1 TO n rhyme                 AS { rhyme }
rhyme  = noun:WORD ' are ' colors NL  AS { noun: colors }
colors = 1 TO n color SPLITBY (' and ' OR ', ')
color  = WORD
```

Applied to `roses are red`, `violets are blue` and `bees are black and yellow` (one per line, ending with a line break), `txtql --compact` prints:

```json
{"roses":["red"],"violets":["blue"],"bees":["black","yellow"]}
```

## Start here

- [[Getting Started|Getting-Started]]: install, run your first query, the command line.
- [[Language Walkthrough|Language-Walkthrough]]: the language one idea at a time, with runnable examples.
- [[Practical Examples|Practical-Examples]]: a cookbook: CSV, TSV and INI files, access logs, stack traces, changelogs, emails, pipelines and CI.

## Reference

- [[Language Reference|Language-Reference]]: syntax, built-in patterns, repetition, templates, functions, operators.
- [[Ambiguity and Strict Mode|Ambiguity-and-Strict-Mode]]: how txtql chooses between readings and warns about the others.
- [[Errors and Diagnostics|Errors-and-Diagnostics]]: every `txtql::*` code.
- [[Performance and Limits|Performance-and-Limits]]: `--max-steps`, `--max-depth`, scaling.

## Tools

- [[Editor Support|Editor-Support]]: the language server, Sublime Text, JetBrains, Neovim, Helix.

## Project

- [[Testing and Contributing|Testing-and-Contributing]]
- [[FAQ and Known Limitations|FAQ-and-Known-Limitations]]
- [Source repository](https://github.com/efinauri/txtql), released under the [MIT license](https://github.com/efinauri/txtql/blob/main/LICENSE)
