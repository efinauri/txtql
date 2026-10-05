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

| Code | Meaning | Example query |
|---|---|---|
| `txtql::parse::unexpected_char` | A character that is not part of the language (for example `\|`, `+` or `*` where `OR` or `a TO b` is meant). | `TEXT = 'a' \| 'b'` |
| `txtql::parse::unterminated_string` | A string literal without its closing quote. | `TEXT = 'abc` |
| `txtql::parse::bad_escape` | An unknown escape in a string; valid ones are `\n \t \r \\ \' \"`. | `TEXT = 'a\q'` |
| `txtql::parse::bad_number` | A number that does not fit: an integer or repetition bound above 18446744073709551615 (for example `99999999999999999999999`), or a float that is not finite (`1e999`). | `TEXT = 99999999999999999999999 WORD` |
| `txtql::parse::unexpected_token` | A token the grammar does not allow at that point (a keyword used as a name, a missing `)`, ...). | `TEXT = WORD )` |
| `txtql::parse::unexpected_end` | The query ends in the middle of a construct. | `TEXT =` |
| `txtql::parse::too_deep` | Parentheses, repetitions, templates or conditions nested more than 100 levels deep. | `TEXT = (((` ... `WORD` ... `)))` with 101 pairs of parentheses |
| `txtql::check::missing_root` | There is no `TEXT` rule. | `rule1 = WORD` |
| `txtql::check::duplicate_rule` | A name is defined twice (rules and aliases share one set of names; `text` and `TEXT` are the same name). | `TEXT = WORD / TEXT = INT` |
| `txtql::check::reserved_name` | `true`, `false` or `null` (in any case) used as a rule, alias, label or loop variable name. | `TEXT = true / true = 'x'` |
| `txtql::check::root_is_alias` | `TEXT` was defined with `ALIAS`. | `ALIAS TEXT = WORD` |
| `txtql::check::alias_cycle` | An alias refers to itself, directly or through other aliases. | `TEXT = a / ALIAS a = 'x' a` |
| `txtql::check::label_in_alias` | A label inside an alias; aliases never capture. | `TEXT = a / ALIAS a = k:WORD` |
| `txtql::check::undefined_rule` | A pattern refers to a rule or alias that does not exist (with a "did you mean" hint). | `TEXT = wrd` |
| `txtql::check::duplicate_capture` | The same capture name twice in one match (give one a label). | `TEXT = a a / a = WORD` |
| `txtql::check::unknown_capture` | A template or `WHERE` uses a name that is not captured by that rule. | `TEXT = a:WORD AS b` |
| `txtql::check::unknown_field` | A path `x.field` asks for a field that the rule that produced `x` never sets. | `TEXT = p AS p.nope / p = a:WORD ' ' b:WORD` |
| `txtql::check::not_repeated` | `FOR` over something that is not a list. | `TEXT = a:WORD AS [ a FOR a ]` |
| `txtql::check::not_mergeable` | A key-less object entry whose value is text, not an object. | `TEXT = a:WORD AS { a }` |
| `txtql::check::duplicate_branch` | An `OR` branch identical to an earlier one, so it can never be chosen. | `TEXT = 'a' OR 'a'` |
| `txtql::check::empty_loop` | A repetition whose item can match empty text. | `TEXT = 1 TO n LINE` |
| `txtql::check::empty_cycle` | A rule that can derive itself without consuming text. | `TEXT = a / a = a OR 'x'` |
| `txtql::check::bad_bounds` | A lower bound above the upper bound. | `TEXT = 3 TO 2 WORD` |
| `txtql::check::bound_too_large` | A finite bound above 10,000 (use `n`); the message gives the bound as written. Also checked inside stops. | `TEXT = 1 TO 10001 WORD` |
| `txtql::check::bad_stop` | Something that cannot be the stop of `UNTIL`/`UNTILBEFORE` (`ANY` alone, `LINE`, `ROW`, `COL`, labels, defined rules, nested `SPLITBY`/`SKIPPING`/stops; aliases of allowed patterns are fine). Other mistakes inside a stop get their usual code instead (empty literal, undefined rule, bounds, duplicate branch). | `TEXT = ANY UNTIL ANY` |
| `txtql::check::empty_stop` | A stop that can match empty text (except `EOF`). | `TEXT = ANY UNTIL (0 TO 1 'x')` |
| `txtql::check::empty_literal` | A literal with no characters (also inside a stop). | `TEXT = ''` |
| `txtql::check::unknown_function` | A function name that does not exist (with a "did you mean" hint). | `TEXT = a:WORD AS NUMM(a)` |
| `txtql::check::arity` | A function with the wrong number of arguments (for example `JOIN` without a separator). | `TEXT = 1 TO n a:WORD SPLITBY ' ' AS JOIN(a)` |
| `txtql::lint::unused_alias` | Warning: an alias that `TEXT` cannot reach. | `TEXT = WORD / ALIAS sp = ' '` |
| `txtql::lint::unused_rule` | Warning: a rule that `TEXT` cannot reach. | `TEXT = WORD / spare = INT` |
| `txtql::lint::nested_repetition` | Warning: an unlimited repetition directly inside another (`1 TO n (1 TO n p)`), which can match the same text in many ways. | `TEXT = 1 TO n (1 TO n WORD)` |
| `txtql::lint::right_recursion` | Warning: a rule that ends by referring to itself; matching is quadratic, so use `1 TO n`. | `TEXT = a / a = 'x' a OR 'x'` |
| `txtql::input::no_parse` | The text does not match the query: shows where matching stopped and what was expected. | `TEXT = INT` |
| `txtql::input::too_expensive` | The work needed exceeded `--max-steps`. | `TEXT = 1 TO n WORD SPLITBY ' '` (--max-steps 5) |
| `txtql::input::too_deep` | Rule matches nested more than `--max-depth` levels. | `TEXT = a / a = '(' a ')' OR 'x'` (--max-depth 3) |
| `txtql::input::ambiguous` | Output-changing ambiguity (a warning; an error with `--strict` or `STRICT`). | `TEXT = 'a' mid:(1 TO n ANY) 'z' rest:(0 TO n ANY)` (--strict) |
| `txtql::eval::error` | A template or `WHERE` failed on the matched text (for example `NUM("abc")`). | `TEXT = a:WORD AS NUM(a)` |
| `txtql::eval::repeated_key` | Warning: an object key set twice, so a value was lost (an error with `--strict`). | `TEXT = a:WORD ' ' b:WORD AS { 'k': a, 'k': b }` |
| `txtql::io` | A file cannot be read or the output cannot be written. | `txtql query.tql missing.txt` |
| `txtql::io::utf8` | The input is not valid UTF-8. | input bytes `ff fe` |

`txtql::lsp` is reported only when the language server itself fails to run (`txtql lsp`); it has no example query.
Where a name is misspelled, the message adds "did you mean ...?"; where a regex habit is typed (`|`, `*`, `+`, `?`) the help
shows the txtql spelling (`OR`, `a TO b pattern`).

## Reading a "does not match" error

`txtql::input::no_parse` points at the furthest position the matcher reached (the word, number or line break there) and lists what was expected:

**Query** (`query.tql`)

```txtql
TEXT   = header 0 TO n entry footer
header = '# start' NL
entry  = '+ ' WORD NL
footer = '# end' NL
```

**Input** (`input.txt`, ends with a line break)

```text
# start
+ a
- b
# end
```

**Error** (standard error, exit code 1)

```text
txtql::input::no_parse

  × the text does not match the query
   ╭─[input.txt:3:1]
 2 │ + a
 3 │ - b
   · ┬
   · ╰── expected '+ ' (the start of `entry`) or '# end' (the start of `footer`)
 4 │ # end
   ╰────
  help: the text matched up to here; found "-"
```

- Each expectation is shown once, with the rules (other than `TEXT`) it would begin. At most eight are listed, then "or one of N other things".
- "Anything but x" expectations from stops are only listed when nothing else is expected, and "the end of the text" is listed when `TEXT` already matched up to there.
- If the text ended too early, the error points at the last character ("expected X after this"); if the input is empty, it says so.
- When `LINE` was expected in the middle of a line, the help says that `LINE` matches whole lines only and suggests `ANY UNTILBEFORE NL`:

**Query** (`query.tql`)

```txtql
TEXT = 'Title: ' title:LINE NL
```

**Input** (`input.txt`, ends with a line break)

```text
Title: Hello world
```

**Error** (standard error, exit code 1)

```text
txtql::input::no_parse

  × the text does not match the query
   ╭─[input.txt:1:8]
 1 │ Title: Hello world
   ·        ──┬──
   ·          ╰── expected the start of a line (for `LINE`)
   ╰────
  help: LINE matches whole lines only. For the rest of this line, write ANY
        UNTILBEFORE NL (e.g. ALIAS rest = ANY UNTILBEFORE NL)
```

- When an `UNTIL` / `UNTILBEFORE` repetition that started on an earlier line was still running at the failure, a second label marks where it
  started and the help says the problem is probably on that line. (See the `until_across_lines` case in `tests/cases/` for a full example; the
  [[Language Walkthrough|Language-Walkthrough]] shows the related pitfall of `ANY UNTIL` crossing lines.)
- When the patterns match but no reading satisfies the `WHERE` conditions, the error says so (see the walkthrough's `WHERE` step).

## Evaluation errors

`txtql::eval::error` points into the query (the template part being evaluated) and, as advice, into the input (the matched text the rule was building a value for):

**Query** (`query.tql`)

```txtql
TEXT = n:WORD AS NUM(n)
```

**Input** (`input.txt`)

```text
abc
```

**Error** (standard error, exit code 1)

```text
txtql::eval::error

  × cannot convert "abc" to a number
   ╭─[query.tql:1:18]
 1 │ TEXT = n:WORD AS NUM(n)
   ·                  ───┬──
   ·                     ╰── while evaluating this
   ╰────
  help: NUM accepts text such as "42" or "3.5"

Advice:
  ☞ in this part of the input
   ╭─[input.txt:1:1]
 1 │ abc
   · ─┬─
   ·  ╰── matched text
   ╰────
```

Known limit: an error inside `FOR` points at the whole rule's match, not at the specific item.

## Warnings

Repeated keys, with the entry that set the key again and the matched text:

**Query** (`query.tql`)

```txtql
TEXT = a:WORD ' ' b:WORD AS { 'k': a, 'k': b }
```

**Input** (`input.txt`)

```text
x y
```

**Output**

```json
{
  "k": "y"
}
```

**Warnings** (standard error)

```text
txtql::eval::repeated_key

  ⚠ key `k` is set more than once; the later value wins
   ╭─[query.tql:1:39]
 1 │ TEXT = a:WORD ' ' b:WORD AS { 'k': a, 'k': b }
   ·                                       ─┬─
   ·                                        ╰── this sets the key again
   ╰────
  help: to keep every value, collect them with `LISTOF`, e.g. `{ k: LISTOF v
        FOR x IN xs }`

Advice:
  ☞ in this part of the input
   ╭─[input.txt:1:1]
 1 │ x y
   · ─┬─
   ·  ╰── matched text
   ╰────
```

Lints and ambiguity together, from `txtql check`:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n groups:(1 TO n LETTER) NL
unused = FLOAT
```

**Input** (`input.txt`, ends with a line break)

```text
ab
```

**Command**

```
txtql check query.tql input.txt
```

**Output** (standard error, exit code 2)

```text
txtql::lint::nested_repetition

  ⚠ nested unlimited repetition
   ╭─[query.tql:1:8]
 1 │ TEXT = 1 TO n groups:(1 TO n LETTER) NL
   ·        ───┬──         ───┬──
   ·           │              ╰── inner repetition
   ·           ╰── outer repetition
 2 │ unused = FLOAT
   ╰────
  help: `1 TO n (1 TO n x)` can split the text in exponentially many ways; add
        a separator or a literal to anchor the inner repetition

txtql::lint::unused_rule

  ⚠ rule `unused` is never used
   ╭─[query.tql:2:1]
 1 │ TEXT = 1 TO n groups:(1 TO n LETTER) NL
 2 │ unused = FLOAT
   · ───┬──
   ·    ╰── unused
   ╰────
  help: reference it from `TEXT` (directly or indirectly), or remove it

txtql::input::ambiguous

  ⚠ ambiguous match in rule `TEXT`: the text can be read in two ways with
  │ different results
   ╭─[input.txt:1:1]
 1 │ ab
   · ─┬┬
   ·  │╰── alternative: `groups` = "a"
   ·  ╰── chosen: `groups` = "ab"
   ╰────
  help: txtql picked the first reading, which gives {"groups":["ab"]}; the
        other gives {"groups":["a","b"]}.
        Make the pattern more specific (literals, SPLITBY, LAZY) if the other
        reading was intended.

1 output-changing ambiguity found
```
