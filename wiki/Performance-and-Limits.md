# Performance and Limits

## How matching works

- **Matching uses an Earley parser over characters.** Words, numbers and literals are matched in one step each. Each rule, group and
  repetition is a small automaton, and repetitions are loops rather than recursion.
- **Typical queries run in linear time**: lists of records, lines and key/value pairs, including the ambiguity check. `cargo bench`
  measures about 2 MB/s with the ambiguity check on (a little more without it).
- **Any grammar, however ambiguous, stays polynomial**: cubic in the worst case. Right-recursive rules (`a = 'x' a OR 'x'`) are quadratic;
  a lint says so and suggests a repetition:

**Query** (`query.tql`)

```txtql
TEXT = a
a = 'x' a OR 'x'
```

**Input** (`input.txt`)

```text
xxx
```

**Command**

```
txtql check query.tql input.txt
```

**Output** (standard error, exit code 0)

```text
txtql::lint::right_recursion

  ⚠ rule `a` is right-recursive
   ╭─[query.tql:2:9]
 1 │ TEXT = a
 2 │ a = 'x' a OR 'x'
   ·         ┬
   ·         ╰── recursive reference at the end of the rule
   ╰────
  help: right recursion makes matching quadratic; a repetition such as `1 TO n
        x` is linear

query OK; no output-changing ambiguity on this input
```

- **The work is deterministic**: the same query on the same input always costs the same number of steps, which makes limits and tests reproducible.
- No query text and no input text makes txtql crash. Even queries that the static checks would reject stay bounded at run time.

An illustration, one query over generated `key = N` lines on one development machine (release build, default options; your numbers will differ,
but doubling the input should roughly double the time):

| Input size | Time |
|---|---|
| 0.6 MB | 0.2 s |
| 1.3 MB | 0.4 s |
| 2.6 MB | 0.8 s |
| 5.4 MB | 1.7 s |

## `--max-steps`

Matching, choosing a reading and evaluating share one budget of "steps" (parser items plus search steps). The default is 200,000,000.
Exceeding it ends the run with `txtql::input::too_expensive` instead of hanging:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n WORD SPLITBY ' '
```

**Input** (`input.txt`)

```text
a b c d e f g h
```

**Command**

```
txtql --max-steps 5 query.tql input.txt
```

**Error** (standard error, exit code 1)

```text
txtql::input::too_expensive

  × the query is too expensive for this input (more than 5 steps)
  help: raise the limit with --max-steps, or make the query less ambiguous
        (fewer nested repetitions, more literals)
```

The ambiguity analysis has its **own** budget (5,000,000 steps in a normal run). When it runs out, the ambiguities found so far are kept and a note
says the check stopped early; it never makes a run fail. `txtql check` runs the analysis without a practical limit.

## `--max-depth`

`--max-depth` (default 10,000) bounds how deeply rule matches can nest inside each other. Only rule matches count: the `TEXT` match is level 1,
and each rule match inside another adds a level; groups, aliases and repetitions (`0 TO 1 p`, `1 TO n p`) add none. Exceeding it ends the run with
`txtql::input::too_deep`:

**Query** (`query.tql`)

```txtql
TEXT = a
a = '(' a ')' OR 'x'
```

**Input** (`input.txt`)

```text
((((((x))))))
```

**Command**

```
txtql --max-depth 3 query.tql input.txt
```

**Error** (standard error, exit code 1)

```text
txtql::input::too_deep

  × the matched text is nested too deeply (more than 3 levels)
   ╭─[input.txt:1:3]
 1 │ ((((((x))))))
   ·   ────┬────
   ·       ╰── nesting limit reached here
   ╰────
  help: deeply recursive rules are limited to bound memory
```

Two things to know:

- **Deep nesting cannot overflow the stack.** Recursion moves to heap-allocated stack segments when needed, so `--max-depth` only bounds memory.
  The test suite includes 20,000-deep nesting on a 2 MiB thread.
- **It is a search bound, not an exact limit on the chosen reading.** Depth is checked while matches are being found, and a sub-match found once is reused at
  deeper positions without a new check. The chosen reading can therefore nest up to about twice `--max-depth`, and a candidate that
  exceeds the bound while being searched fails the run with `too_deep` even if a `WHERE` condition would later have rejected it.

Raise it when your data really is that deep (`--max-depth 100000`). Very deeply nested output values should be freed with `txtql::drop_deep`
when using the library, because `serde_json` drops values recursively.

## Known cost: `WHERE` chains

A `WHERE` clause re-evaluates its rule's captures, so deeply nested chains of rules that each have a `WHERE` cost O(depth squared). This counts against the step limit.

## Tips

- Prefer `1 TO n x` over right recursion.
- Anchor repeated patterns with a separator or a literal. The lint `txtql::lint::nested_repetition` flags `1 TO n (1 TO n p)`, which can split
  text in exponentially many ways (still bounded by `--max-steps`).
- Use `--no-ambiguity-check` on large trusted inputs once you have checked the query on samples; see [[Ambiguity and Strict Mode|Ambiguity-and-Strict-Mode]].
- Check scaling yourself with `cargo bench` (see [[Testing and Contributing|Testing-and-Contributing]]).
