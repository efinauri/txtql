# Language Walkthrough

A step-by-step tour of txtql. Each step adds one idea and shows a query, an input and the exact output.
For a condensed list of everything, see [[Language Reference|Language-Reference]].

## How to follow along

Save the query as `query.tql` and the input as `input.txt`, then run:

```
txtql query.tql input.txt
```

Every example below was run with the `txtql` binary built from this repository, and the output is pasted
as printed. JSON goes to standard output; warnings and errors go to standard error and are shown separately.
When an input block says "ends with a line break", the file really ends with one (`printf 'a\n'`); the
code block just cannot show it. This matters, because **every character of the input is significant**.

The steps:
1. [Rules and TEXT](#1-rules-and-text)
2. [Literals and built-in patterns](#2-literals-and-built-in-patterns)
3. [Sequences and OR](#3-sequences-and-or)
4. [Repetition](#4-repetition)
5. [Captures, labels and default values](#5-captures-labels-and-default-values)
6. [Aliases](#6-aliases)
7. [Output templates (AS)](#7-output-templates-as)
8. [Conditions (WHERE)](#8-conditions-where)
9. [Structural matching](#9-structural-matching)
10. [Ambiguity and strict mode](#10-ambiguity-and-strict-mode)
11. [Names, spelling and reserved words](#11-names-spelling-and-reserved-words)

## 1. Rules and TEXT

A query is a list of rules. A rule has a name, a pattern, and optionally a `WHERE` condition and an `AS`
template:

```
[STRICT] name = pattern [WHERE condition] [AS template]
```

The rule called `TEXT` is the root. It must match the **whole** input, from the first character to the last.
Everything else is optional structure: the other rules exist to be used by `TEXT` or by each other.

{{ex w1_hello}}

`'hello '` is a literal (note the space inside the quotes), `WORD` is a built-in pattern, and `name:` gives the
matched word a label, which becomes a key in the result. If the text does not fit, the error says where
matching stopped and what it expected there:

{{ex w1_nomatch}}

Nothing is skipped implicitly, including the line break at the end of a file. A query that does not mention
it does not match:

{{ex w1_nl_missing}}

Add `NL` (a line break) and it does:

{{ex w1_nl}}

With no labels and no `AS`, a rule's value is simply the text it matched.

Rules can use other rules, in any order. By convention, write `TEXT` first, then the rules it uses, down to the
smallest pieces, so the query reads top-down:

{{ex w1_rules}}

Rule names are what the result is keyed by here: `greeting` and `name` are captured automatically
(see [step 5](#5-captures-labels-and-default-values)). Comments start with `--` and run to the end of the line.

## 2. Literals and built-in patterns

A **literal** is text in single or double quotes. It matches exactly that text, spaces included.
`i'text'` ignores case. Escapes `\n`, `\t`, `\r`, `\\`, `\'` and `\"` work inside quotes, and a literal may span lines.

{{ex w2_icase}}

**Built-in patterns** match a kind of text. The ones you will use most:

| Pattern | Matches |
|---|---|
| `WORD` | a whole run of letters (never part of a longer run: `abc1` is not a `WORD`) |
| `INT`, `FLOAT` | a run of digits; digits with an optional `.digits` part |
| `HEX`, `BIN` | a run of hexadecimal or binary digits |
| `IPV4`, `IPV6` | an IP address |
| `LETTER`, `DIGIT`, `PUNCT`, `ANY` | one character: a letter, a digit, punctuation, anything (line breaks included) |
| `NL`, `TAB` | a line break (`\n`, `\r\n` or `\r`), a tab |
| `LINE` | a whole line without its line break (may be empty); only at the start of a line |
| `ROW`, `COL` | no text; worth the current line and column number |
| `EOF` | no text; only matches at the end of the input |

{{ex w2_prims}}

Results are text unless a template converts them (`INT` gives `"42"`, not `42`; see [step 7](#7-output-templates-as)).
`LETTER` and `DIGIT` match one character even inside a run, which is how you split `987` into digits:

{{ex w2_single}}

`LINE` takes a whole line, and a line can be empty:

{{ex w2_line}}

`ROW` and `COL` match no text but report where they are, counting from 1 (columns in characters):

{{ex w2_rowcol}}

The [[Language Reference|Language-Reference]] has a table with an example for every built-in.

## 3. Sequences and OR

Writing patterns one after another is a **sequence**. `a OR b` is a choice between alternatives. `OR` binds
looser than a sequence, so `'a' 'b' OR 'c'` means (`'a' 'b'`) or `'c'`; use parentheses to group.

{{ex w3_or}}

Alternatives work between rules too. Here `value` is either a `number` or a `name`; each rule says
how it becomes JSON (the `AS` part is explained in [step 7](#7-output-templates-as)):

{{ex w3_or_rules}}

The same query, on a word:

{{ex w3_or_rules2}}

When several alternatives match, the first one wins (see [step 10](#10-ambiguity-and-strict-mode)).

## 4. Repetition

Repetition is written `min TO max pattern`. `max` may be `n` for "no limit". So `0 TO 1` is optional,
`1 TO n` is one or more and `0 TO n` is any number. There is no `*` or `+`.

{{ex w4_bounds}}

If you type a regex-style `*`, `+` or `?`, txtql points you to the right spelling:

{{ex w4_star}}

An optional part gives `null` when it is absent:

{{ex w4_optional}}

and its value when present:

{{ex w4_optional2}}

### SPLITBY

`SPLITBY` puts a separator between items, never before the first or after the last:

{{ex w4_splitby}}

Text after the repetition (here `' total'`) is matched as usual. Because the query then has a label, the
result becomes an object, and the repeated capture is a list:

{{ex w4_splitby_text}}

### SKIPPING

`SKIPPING p` allows text matching `p` before, between and after the items. Use it to pick values out of
prose. Text is only skipped where nothing else fits:

{{ex w4_skipping}}

`SKIPPING ANY` skips everything that is not an item. A narrower skip pattern (for example only letters and spaces) is stricter.

### UNTIL and UNTILBEFORE

`p UNTIL s` repeats `p` until `s` is found, then **consumes** `s` as well. `p UNTILBEFORE s` stops at `s` but
leaves it for what follows. The stop is never part of a value, and `UNTIL` without bounds means `0 TO n`:

{{ex w4_until}}

{{ex w4_untilbefore}}

A stop is checked wherever an iteration would begin, and it can be a literal, a built-in other than `LINE`, `ROW` and
`COL`, or a sequence / `OR` / repetition of those. `ANY UNTIL NL` is the idiom for "the rest of the line".

**Watch out: `ANY` crosses lines.** If the stop never appears on a line, `ANY UNTIL` happily continues on the next
one. Here, line 2 has no space before `=`, so the key swallows two lines and the result is wrong without any error:

{{ex u_crossing}}

Make the stop say where the value must end, so a bad line fails loudly (here, at the line break):

{{ex u_safe}}

When a failure is caused by such a run-on, the error also points at the line where the `UNTIL` started.

### LAZY and a note on greedy

By default, earlier parts of a sequence take as much text as they can while the rest still matches. `LAZY`
prefers fewer repetitions:

{{ex w4_lazy}}

Without `LAZY` the same query takes everything up to the last `z` (`-b-z-c-`), and txtql warns that another
reading exists ([step 10](#10-ambiguity-and-strict-mode)).

### A repeated pattern must consume text

A repetition whose item can match nothing (such as `LINE`, which may be empty) would loop forever, so it is a static error:

{{ex w4_empty_rep}}

Write `1 TO n LINE SPLITBY NL` instead: a separator that consumes text is enough.

## 5. Captures, labels and default values

A rule's **captures** are its labels (`name:pattern`) and the rules it refers to, which are captured under their
own name. Without `AS`, a rule's value is built from them:

{{ex c5_obj}}

A rule reference captures under the rule's name:

{{ex c5_ruleref}}

The full set of defaults, in order of precedence:

1. A single repetition: the list of the items' values (`0 TO 1`: the item's value or `null`).
2. A single rule reference: that rule's value.
3. Anything with captures: an object of the captures.
4. A single parenthesised group: that group's value.
5. Otherwise: the matched text.

{{ex c5_list_default}}

{{ex c5_text_default}}

A capture inside a repetition is a **list**, even with one element. Only `0 TO 1` and `1 TO 1` give the value itself (or `null`):

{{ex c5_rep_capture}}

{{ex c5_rep_capture1}}

A capture in an `OR` branch that was not taken is `null` in templates and conditions; in the default object,
only the branch that was taken is listed:

{{ex c5_or_null}}

{{ex c5_or_default}}

### Where a label goes matters

A label takes the nearest pattern after it; add parentheses to take more. On a repetition (in parentheses) it
captures the **text** the repetition matched; on the repeated item it captures a **list**:

{{ex c5_label_rep}}

{{ex c5_label_item}}

The same thing happens with the short form of `UNTILBEFORE`: `key:ANY UNTILBEFORE '='` labels each `ANY`, so you
get a list of characters; `key:(ANY UNTILBEFORE '=')` gives the text:

{{ex w4_until_inner}}

Parentheses group but do not scope; a label does. Captures inside a labelled pattern belong to the label's value.
The same name captured twice in one match is a static error, because the second would hide the first:

{{ex c5_dupcap}}

## 6. Aliases

An alias names a pattern fragment: `ALIAS name = pattern`. Using it is the same as writing its pattern in
parentheses, except that **an alias never captures anything**. That is why it can appear any number of times in one sequence:

{{ex a6_alias}}

To capture an alias's text, label it where it is used. Its value is the text it matched:

{{ex a6_alias_label}}

An alias cannot contain labels (write a rule if you need captures inside), cannot be recursive, and shares its name space with rules:

{{ex a6_label_in_alias}}

By convention aliases go at the end of the query.

## 7. Output templates (AS)

`AS template` decides the JSON a rule produces. Templates look like JSON, with two rules about names:
**a quoted string is always a literal, and a bare name is always a capture**, for keys and for values.

{{ex t7_obj}}

`NUM(age)` turned the text into a number. Because a bare key is a capture, the matched text can be the key:

{{ex t7_keys}}

Arrays and constants (`'text'`, numbers, `true`, `false`, `null`, in any case):

{{ex t7_list}}

### Lists with FOR

`[ element FOR list ]` builds one element per item. Inside, the item's fields are names (here `name`, a field
of each `person` object):

{{ex t7_for2}}

`FOR x IN expr` iterates any list under a name you choose; `NUM(x)` converts each element:

{{ex t7_for_in}}

Objects work the same way, with one entry per item. `p.k` reads a field:

{{ex t7_for_obj2}}

### Keyless entries merge

An entry without a key merges an object, or a list of objects, into the one being built (`null` adds nothing).
This is how a variable set of key-value pairs becomes one object:

{{ex t7_merge}}

The front-page example uses the same mechanism: `TEXT = 1 TO n rhyme AS { rhyme }` merges the list of one-entry objects `rhyme` into a single object.

### Repeated keys and LISTOF

If an object gets the same key twice, the later value wins and you get a warning that names the entry (an error with `--strict`):

{{ex t7_repeated}}

`LISTOF` collects every value of a repeated key into a list. The value is always a list, even for a key that occurs once:

{{ex t7_listof}}

### Functions

| Function | Result |
|---|---|
| `NUM(x)` | text or number to a number |
| `LOWER(x)`, `UPPER(x)`, `TRIM(x)` | text conversions |
| `COUNT(list)`, `FIRST(list)`, `LAST(list)` | list helpers (`null` for an empty list) |
| `JOIN(list, sep)` | the items joined with `sep`; **the separator is required** |
| `ZIP(keys, values)` | an object pairing each key with the value in the same position |

Function names are case-insensitive.

{{ex t7_funcs}}

`JOIN` has no default separator; leaving it out is caught before anything runs:

{{ex t7_join_nosep}}

`ZIP` is made for tables with a header row, where the keys come from the data:

{{ex t7_zip}}

A template that fails on the data, such as `NUM('abc')`, is an error that points into both the query and the input:

{{ex t7_numerr}}

## 8. Conditions (WHERE)

`WHERE` goes after the pattern (before `AS`) and decides whether a match counts. A match whose condition is false
does not count, and txtql looks for another reading. Conditions use captures and the same functions as templates:

{{ex x8_filter}}

Operators: `=`, `!=`, `<`, `<=`, `>`, `>=`, `CONTAINS`, `STARTSWITH`, `ENDSWITH`, `AND`, `OR`, `NOT` and
parentheses. `NOT` binds tighter than `AND`, which binds tighter than `OR`:

{{ex x8_ops}}

If a `WHERE` rejects every reading, the whole input fails to match:

{{ex x8_ops_fail}}

Comparing numbers deserves care. Ordering operators compare numerically when both sides are numbers or numeric
text (`'9' < '10'` is true), and as text otherwise. `=` and `!=` compare numerically only when at least one side is
a number, so two texts are compared as exact text:

{{ex x8_text_eq}}

Convert one side with `NUM` to compare numerically:

{{ex x8_num_eq}}

A bare value is true unless it is `null`, `false`, `''`, `[]` or `{}` (the number `0` is true).

`WHERE` is also the tool for settling a choice between `OR` branches: the second rule below does not match `-`,
so `place` cannot compete with `unknown`:

{{ex x8_pick}}

## 9. Structural matching

Rules can refer to themselves, directly or through other rules, so nested structure of any depth can be matched,
which regular expressions cannot do. The only rule is that every recursive step consumes text.

An s-expression reader (note `atom OR sexpr` inside the repetition):

{{ex s9_sexpr}}

Nested lists become nested JSON arrays:

{{ex s9_lists}}

Unbalanced input does not match, and the error shows where it ended:

{{ex s9_unbalanced}}

A tree whose nodes have optional children:

{{ex s9_tree}}

Matching is structural: txtql looks at the shape of the text and nothing else (no language understanding).
Indentation-based nesting is not supported, because such formats are already machine-readable.

Nesting depth is bounded by `--max-depth` (default 10,000), and deeply nested input cannot overflow the stack; see
[[Performance and Limits|Performance-and-Limits]]. Rules whose last step is a reference to themselves are quadratic;
a lint suggests `1 TO n` instead.

## 10. Ambiguity and strict mode

Sometimes the same text can be read in more than one way. txtql picks one reading deterministically (the rules are in
[[Ambiguity and Strict Mode|Ambiguity-and-Strict-Mode]]):

1. Earlier parts of a sequence take as much text as they can while the rest still matches (`LAZY`: as little).
2. Among alternatives covering the same text, the leftmost `OR` branch wins.
3. Repetitions prefer another item over stopping (`LAZY`: the reverse).

After matching, txtql checks whether another reading would have produced **different output**. If so, it
warns and shows both readings and both results:

{{ex m10_split}}

The warning goes to standard error, so the JSON is still printed. `--strict` turns the warning into an error:

{{ex m10_strict}}

`STRICT` on a rule does the same for that rule only (`STRICT TEXT = …`). `txtql check` takes a sample input and
lists ambiguities without printing JSON, with exit code 2 if it found any:

{{ex m10_check}}

Fix an ambiguity by making the pattern more specific (a literal separator), or by stating the preference with `LAZY`:

{{ex m10_fixed}}

{{ex m10_lazy}}

An `OR` whose branches both match is the other common cause. The warning explains the usual fixes: list the special case
first, or exclude it from the general branch with `WHERE`:

{{ex m10_branch}}

Ambiguity that does not change the output is not reported, and neither is ambiguity settled by `LAZY`, or
inside `SKIPPING` text and separators:

{{ex m10_harmless}}

## 11. Names, spelling and reserved words

- Keywords, built-in names and function names can be written in any case (`WORD`, `word`, `Word`). In return,
  they cannot be rule names, labels or alias names. That includes `line`, `row`, `col`, `word`, `to`, `in` and so on.
- The root rule may be spelled `TEXT` or `text`; both are the same name, so you cannot also define a rule called `text`.
- `true`, `false` and `null`, in any case, are template constants and cannot name a rule, alias, label or loop variable.
- Names start with a letter or `_` and continue with letters, digits or `_`; Unicode letters are allowed.

{{ex r11_keyword_label}}

{{ex r11_reserved}}

{{ex r11_text_dup}}

{{ex r11_case}}

## Where next

- [[Practical Examples|Practical-Examples]]: real-world recipes (CSV, logs, configs, emails, pipelines) built from these ideas.
- [[Language Reference|Language-Reference]]: everything on one page.
- [[Ambiguity and Strict Mode|Ambiguity-and-Strict-Mode]] and [[Errors and Diagnostics|Errors-and-Diagnostics]].
- The `tests/cases/` directory in the repository holds larger worked queries: invoices, CSV with a header row, HTTP headers,
  a web server log, a game playtest log and all twelve Advent of Code 2025 example inputs.
