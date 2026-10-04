# Errors and Diagnostics

Every problem txtql reports is a [miette](https://docs.rs/miette) diagnostic with:

- a **stable code** (`txtql::...`) that scripts, tests and editors can rely on;
- a **message**;
- **labelled snippets** of the query and/or the input (never an empty span at the very end of a text: the last character or token is marked instead);
- a **help** hint where txtql can say how to fix the problem.

Severity is an error, a warning or advice. On the command line every diagnostic goes to **standard error**; standard
output carries only the JSON result. Warnings (lints, ambiguity, repeated keys) are shown but do not stop the run;
`--strict` turns ambiguity and repeated keys into errors. Syntax errors stop at the first one; static checks report every
problem at once, ordered by position in the query.

Exit codes: `0` success, `1` any error, `2` from `txtql check` when it finds ambiguities or repeated keys.

## Codes

{{codes}}

`txtql::lsp` is reported only when the language server itself fails to run (`txtql lsp`); it has no example query.
Where a name is misspelled, the message adds "did you mean ...?"; where a regex habit is typed (`|`, `*`, `+`, `?`) the help
shows the txtql spelling (`OR`, `a TO b pattern`).

## Reading a "does not match" error

`txtql::input::no_parse` points at the furthest position the matcher reached (the word, number or line break there) and lists what was expected:

{{ex e_rules}}

- Each expectation is shown once, with the rules (other than `TEXT`) it would begin. At most eight are listed, then "or one of N other things".
- "Anything but x" expectations from stops are only listed when nothing else is expected, and "the end of the text" is listed when `TEXT` already matched up to there.
- If the text ended too early, the error points at the last character ("expected X after this"); if the input is empty, it says so.
- When `LINE` was expected in the middle of a line, the help says that `LINE` matches whole lines only and suggests `ANY UNTILBEFORE NL`:

{{ex e_line}}

- When an `UNTIL` / `UNTILBEFORE` repetition that started on an earlier line was still running at the failure, a second label marks where it
  started and the help says the problem is probably on that line. (See the `until_across_lines` case in `tests/cases/` for a full example; the
  [[Language Walkthrough|Language-Walkthrough]] shows the related pitfall of `ANY UNTIL` crossing lines.)
- When the patterns match but no reading satisfies the `WHERE` conditions, the error says so (see the walkthrough's `WHERE` step).

## Evaluation errors

`txtql::eval::error` points into the query (the template part being evaluated) and, as advice, into the input (the matched text the rule was building a value for):

{{ex t7_numerr}}

Known limit: an error inside `FOR` points at the whole rule's match, not at the specific item.

## Warnings

Repeated keys, with the entry that set the key again and the matched text:

{{ex e_dupkey}}

Lints and ambiguity together, from `txtql check`:

{{ex e_lint}}
