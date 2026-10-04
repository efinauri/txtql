# Ambiguity and Strict Mode

Free text can often be read in more than one way. txtql always picks exactly one reading, deterministically, and then tells you
when another reading would have produced different output.

## Which reading is chosen

1. **Earlier parts of a sequence take as much text as they can** while the rest still matches. A `LAZY` repetition takes as little as it can.
2. **Among alternatives that cover the same text, the leftmost `OR` branch wins.** An `OR` that is one step of a sequence still prefers its longest match first.
3. **Repetitions prefer another item over stopping** (`LAZY` reverses this). Optional parts (`0 TO 1`) prefer to match.
   `SKIPPING` text is only skipped where nothing else fits.
4. **A match whose `WHERE` is false does not count**: the next reading in preference order is tried. If none satisfies
   the conditions, the run fails.

The preferred reading of a rule, group or repetition over a given stretch of text is chosen once, independently of what
uses it. An enclosing `WHERE` can make a different stretch be chosen, but not a different reading of the same stretch. This keeps matching polynomial.

## The ambiguity check

After a successful match, txtql compares each match in the chosen reading with readings that differ from it in exactly one
decision (another `OR` branch, another span, one more or one fewer item) and that still satisfy every `WHERE` on the way up.

- **Only output changes count.** Readings that give the same captures and value, or that an enclosing template ignores, are not reported.
- **Explicit choices are not reported:** differences settled by `LAZY`, and differences inside `SKIPPING` text, separators or consumed stops.
- **A failing alternative counts as different output.** If another reading fails to evaluate (a template error, or nesting deeper than `--max-depth`),
  it is still reported as an ambiguity, with the reason, instead of failing the run.
- **At most one report per match and at most 20 per run.** `txtql check` lists up to the same 20.
- **Reports name the nearest rule whose value changes**, label the chosen and the alternative part of the input, and show both values.
- **The analysis has its own budget.** When it runs out, the ambiguities found so far are kept and the run prints
  `note: the ambiguity check stopped early on this input; some ambiguities may be unreported`. (`txtql check` has no practical budget limit.)

An ambiguity is a warning on standard error; the JSON is still printed:

{{ex m10_split}}

### Advice

The help text depends on the cause:

- **Two `OR` branches both match.** The first branch won because it comes first; either list the special case first, or exclude it from the general branch with `WHERE`:

{{ex m10_branch}}

- **A stop repetition ran across line breaks** while another reading would not: the advice shows the repetition with `NL` added to its stop, or suggests a stop that says where a multi-line value ends.
- **Anything else:** make the pattern more specific (literals, `SPLITBY`, `LAZY`).

## Making an ambiguity go away

State what you mean: add a literal separator, or choose a side with `LAZY`.

{{ex m10_fixed}}

{{ex m10_lazy}}

Greedy is the default, so this query takes everything up to the last `z` and reports the lazy alternative:

{{ex m10_greedy}}

Ambiguity that does not change the output is not reported:

{{ex m10_harmless}}

## Strict mode

`--strict` turns every output-changing ambiguity and every repeated object key into an error, and no JSON is printed:

{{ex m10_strict}}

`STRICT` before a rule makes ambiguity in that rule (the rule whose value changes) an error without the flag:

{{ex m10_strict_rule}}

A repeated object key (`{ 'k': a, 'k': b }`, an iteration or merge that sets a key twice, or `ZIP` with repeated keys) keeps the later value
and warns; with `--strict` it is an error. `LISTOF` entries are the way to keep every value; see the
[[Language Walkthrough|Language-Walkthrough]].

## `txtql check` and exit codes

`txtql check QUERY SAMPLE` runs the query on a sample and reports ambiguities and repeated keys, without printing JSON.
It exits with `0` when nothing was found, `1` for an error and `2` when it found output-changing ambiguities or repeated keys:

{{ex m10_check}}

## `--no-ambiguity-check`

The analysis costs time (about the same order as matching, see [[Performance and Limits|Performance-and-Limits]]).
`--no-ambiguity-check` skips it and so never reports ambiguity. Use it for queries you have already checked against
representative samples.
