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

**Query** (`query.tql`)

```txtql
TEXT = 'hello ' name:WORD
```

**Input** (`input.txt`)

```text
hello world
```

**Output**

```json
{
  "name": "world"
}
```

`'hello '` is a literal (note the space inside the quotes), `WORD` is a built-in pattern, and `name:` gives the
matched word a label, which becomes a key in the result. If the text does not fit, the error says where
matching stopped and what it expected there:

**Query** (`query.tql`)

```txtql
TEXT = 'hello ' name:WORD
```

**Input** (`input.txt`)

```text
hello 42
```

**Error** (standard error, exit code 1)

```text
txtql::input::no_parse

  × the text does not match the query
   ╭─[input.txt:1:7]
 1 │ hello 42
   ·       ─┬
   ·        ╰── expected WORD
   ╰────
  help: the text matched up to here; found "42"
```

Nothing is skipped implicitly, including the line break at the end of a file. A query that does not mention
it does not match:

**Query** (`query.tql`)

```txtql
TEXT = 'hi'
```

**Input** (`input.txt`, ends with a line break)

```text
hi
```

**Error** (standard error, exit code 1)

```text
txtql::input::no_parse

  × the text does not match the query
   ╭─[input.txt:1:3]
 1 │ hi
   ·   ┬
   ·   ╰── expected the end of the text
   ╰────
  help: the text matched up to here; found a line break
```

Add `NL` (a line break) and it does:

**Query** (`query.tql`)

```txtql
TEXT = 'hi' NL
```

**Input** (`input.txt`, ends with a line break)

```text
hi
```

**Output**

```json
"hi\n"
```

With no labels and no `AS`, a rule's value is simply the text it matched.

Rules can use other rules, in any order. By convention, write `TEXT` first, then the rules it uses, down to the
smallest pieces, so the query reads top-down:

**Query** (`query.tql`)

```txtql
TEXT  = greeting ' ' name:WORD
greeting = 'hello' OR 'hi'
```

**Input** (`input.txt`)

```text
hi there
```

**Output**

```json
{
  "greeting": "hi",
  "name": "there"
}
```

Rule names are what the result is keyed by here: `greeting` and `name` are captured automatically
(see [step 5](#5-captures-labels-and-default-values)). Comments start with `--` and run to the end of the line.

## 2. Literals and built-in patterns

A **literal** is text in single or double quotes. It matches exactly that text, spaces included.
`i'text'` ignores case. Escapes `\n`, `\t`, `\r`, `\\`, `\'` and `\"` work inside quotes, and a literal may span lines.

**Query** (`query.tql`)

```txtql
TEXT = i'select ' cols:WORD
```

**Input** (`input.txt`)

```text
SeLeCt name
```

**Output**

```json
{
  "cols": "name"
}
```

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

**Query** (`query.tql`)

```txtql
TEXT = host:IPV4 ' ' flags:('0x' HEX) ' ' ratio:FLOAT ' ' id:INT '-' tag:WORD
```

**Input** (`input.txt`)

```text
10.0.0.7 0xCAFE 3.14 42-beta
```

**Output**

```json
{
  "host": "10.0.0.7",
  "flags": "0xCAFE",
  "ratio": "3.14",
  "id": "42",
  "tag": "beta"
}
```

Results are text unless a template converts them (`INT` gives `"42"`, not `42`; see [step 7](#7-output-templates-as)).
`LETTER` and `DIGIT` match one character even inside a run, which is how you split `987` into digits:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n DIGIT
```

**Input** (`input.txt`)

```text
987
```

**Output**

```json
[
  "9",
  "8",
  "7"
]
```

`LINE` takes a whole line, and a line can be empty:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n LINE SPLITBY NL
```

**Input** (`input.txt`)

```text
first

third
```

**Output**

```json
[
  "first",
  "",
  "third"
]
```

`ROW` and `COL` match no text but report where they are, counting from 1 (columns in characters):

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n item SPLITBY NL
item = 0 TO n ' ' y:ROW x:COL w:WORD
```

**Input** (`input.txt`)

```text
alpha
  beta
```

**Output**

```json
[
  {
    "y": 1,
    "x": 1,
    "w": "alpha"
  },
  {
    "y": 2,
    "x": 3,
    "w": "beta"
  }
]
```

The [[Language Reference|Language-Reference]] has a table with an example for every built-in.

## 3. Sequences and OR

Writing patterns one after another is a **sequence**. `a OR b` is a choice between alternatives. `OR` binds
looser than a sequence, so `'a' 'b' OR 'c'` means (`'a' 'b'`) or `'c'`; use parentheses to group.

**Query** (`query.tql`)

```txtql
TEXT = animal:('cat' OR 'dog') ' says ' sound:('meow' OR 'woof')
```

**Input** (`input.txt`)

```text
dog says woof
```

**Output**

```json
{
  "animal": "dog",
  "sound": "woof"
}
```

Alternatives work between rules too. Here `value` is either a `number` or a `name`; each rule says
how it becomes JSON (the `AS` part is explained in [step 7](#7-output-templates-as)):

**Query** (`query.tql`)

```txtql
TEXT   = 'value: ' v:value
value  = number OR name
number = n:INT AS NUM(n)
name   = t:WORD AS t
```

**Input** (`input.txt`)

```text
value: 42
```

**Output**

```json
{
  "v": 42
}
```

The same query, on a word:

**Query** (`query.tql`)

```txtql
TEXT   = 'value: ' v:value
value  = number OR name
number = n:INT AS NUM(n)
name   = t:WORD AS t
```

**Input** (`input.txt`)

```text
value: forty
```

**Output**

```json
{
  "v": "forty"
}
```

When several alternatives match, the first one wins (see [step 10](#10-ambiguity-and-strict-mode)).

## 4. Repetition

Repetition is written `min TO max pattern`. `max` may be `n` for "no limit". So `0 TO 1` is optional,
`1 TO n` is one or more and `0 TO n` is any number. There is no `*` or `+`.

**Query** (`query.tql`)

```txtql
TEXT = code:(2 TO 3 LETTER) '-' num:(1 TO n DIGIT)
```

**Input** (`input.txt`)

```text
ab-12345
```

**Output**

```json
{
  "code": "ab",
  "num": "12345"
}
```

If you type a regex-style `*`, `+` or `?`, txtql points you to the right spelling:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n WORD*
```

**Input** (`input.txt`)

```text
a
```

**Error** (standard error, exit code 1)

```text
txtql::parse::unexpected_char

  × unexpected character `*`
   ╭─[query.tql:1:19]
 1 │ TEXT = 1 TO n WORD*
   ·                   ┬
   ·                   ╰── not valid here
   ╰────
  help: repetition is written as `a TO b pattern`, e.g. `1 TO n WORD` or `0 TO
        1 WORD`
```

An optional part gives `null` when it is absent:

**Query** (`query.tql`)

```txtql
TEXT = name:WORD 0 TO 1 (' ' suffix:WORD)
```

**Input** (`input.txt`)

```text
Ada
```

**Output**

```json
{
  "name": "Ada",
  "suffix": null
}
```

and its value when present:

**Query** (`query.tql`)

```txtql
TEXT = name:WORD 0 TO 1 (' ' suffix:WORD)
```

**Input** (`input.txt`)

```text
Ada Lovelace
```

**Output**

```json
{
  "name": "Ada",
  "suffix": "Lovelace"
}
```

### SPLITBY

`SPLITBY` puts a separator between items, never before the first or after the last:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n n:INT SPLITBY ', '
```

**Input** (`input.txt`)

```text
1, 2, 3
```

**Output**

```json
[
  "1",
  "2",
  "3"
]
```

Text after the repetition (here `' total'`) is matched as usual. Because the query then has a label, the
result becomes an object, and the repeated capture is a list:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n n:INT SPLITBY ', ' ' total'
```

**Input** (`input.txt`)

```text
1, 2, 3 total
```

**Output**

```json
{
  "n": [
    "1",
    "2",
    "3"
  ]
}
```

### SKIPPING

`SKIPPING p` allows text matching `p` before, between and after the items. Use it to pick values out of
prose. Text is only skipped where nothing else fits:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n n:INT SKIPPING ANY
```

**Input** (`input.txt`)

```text
We sold 3 apples, 12 pears and 7 plums.
```

**Output**

```json
[
  "3",
  "12",
  "7"
]
```

`SKIPPING ANY` skips everything that is not an item. A narrower skip pattern (for example only letters and spaces) is stricter.

### UNTIL and UNTILBEFORE

`p UNTIL s` repeats `p` until `s` is found, then **consumes** `s` as well. `p UNTILBEFORE s` stops at `s` but
leaves it for what follows. The stop is never part of a value, and `UNTIL` without bounds means `0 TO n`:

**Query** (`query.tql`)

```txtql
TEXT = key:(ANY UNTIL '=') value:(ANY UNTIL NL)
```

**Input** (`input.txt`, ends with a line break)

```text
colour=red
```

**Output**

```json
{
  "key": "colour",
  "value": "red"
}
```

**Query** (`query.tql`)

```txtql
TEXT = key:(ANY UNTILBEFORE '=') '=' value:(ANY UNTILBEFORE NL) NL
```

**Input** (`input.txt`, ends with a line break)

```text
colour=red
```

**Output**

```json
{
  "key": "colour",
  "value": "red"
}
```

A stop is checked wherever an iteration would begin, and it can be a literal, a built-in other than `LINE`, `ROW` and
`COL`, or a sequence / `OR` / repetition of those. `ANY UNTIL NL` is the idiom for "the rest of the line".

**Watch out: `ANY` crosses lines.** If the stop never appears on a line, `ANY UNTIL` happily continues on the next
one. Here, line 2 has no space before `=`, so the key swallows two lines and the result is wrong without any error:

**Query** (`query.tql`)

```txtql
TEXT  = 1 TO n entry
entry = key:(ANY UNTIL ' ') '= ' value:(ANY UNTIL NL)
```

**Input** (`input.txt`, ends with a line break)

```text
a = 1
b=2
c = 3
```

**Output**

```json
[
  {
    "key": "a",
    "value": "1"
  },
  {
    "key": "b=2\nc",
    "value": "3"
  }
]
```

Make the stop say where the value must end, so a bad line fails loudly (here, at the line break):

**Query** (`query.tql`)

```txtql
TEXT  = 1 TO n entry
entry = key:(ANY UNTILBEFORE (' ' OR NL)) ' = ' value:(ANY UNTIL NL)
```

**Input** (`input.txt`, ends with a line break)

```text
a = 1
b=2
c = 3
```

**Error** (standard error, exit code 1)

```text
txtql::input::no_parse

  × the text does not match the query
   ╭─[input.txt:2:4]
 1 │ a = 1
 2 │ b=2
   ·    ┬
   ·    ╰── expected ' = '
 3 │ c = 3
   ╰────
  help: the text matched up to here; found a line break
```

When a failure is caused by such a run-on, the error also points at the line where the `UNTIL` started.

### LAZY and a note on greedy

By default, earlier parts of a sequence take as much text as they can while the rest still matches. `LAZY`
prefers fewer repetitions:

**Query** (`query.tql`)

```txtql
TEXT = 'a' mid:(1 TO n LAZY ANY) 'z' rest:(0 TO n ANY)
```

**Input** (`input.txt`)

```text
a-b-z-c-z
```

**Output**

```json
{
  "mid": "-b-",
  "rest": "-c-z"
}
```

Without `LAZY` the same query takes everything up to the last `z` (`-b-z-c-`), and txtql warns that another
reading exists ([step 10](#10-ambiguity-and-strict-mode)).

### A repeated pattern must consume text

A repetition whose item can match nothing (such as `LINE`, which may be empty) would loop forever, so it is a static error:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n LINE
```

**Input** (`input.txt`)

```text
a
b
```

**Error** (standard error, exit code 1)

```text
txtql::check::empty_loop

  × this repetition can match empty text
   ╭─[query.tql:1:15]
 1 │ TEXT = 1 TO n LINE
   ·               ──┬─
   ·                 ╰── repeated pattern may be empty
   ╰────
  help: a repeated pattern must consume at least one character, or be split by
        a separator that does
```

Write `1 TO n LINE SPLITBY NL` instead: a separator that consumes text is enough.

## 5. Captures, labels and default values

A rule's **captures** are its labels (`name:pattern`) and the rules it refers to, which are captured under their
own name. Without `AS`, a rule's value is built from them:

**Query** (`query.tql`)

```txtql
TEXT = noun:WORD ' are ' adj:WORD
```

**Input** (`input.txt`)

```text
roses are red
```

**Output**

```json
{
  "noun": "roses",
  "adj": "red"
}
```

A rule reference captures under the rule's name:

**Query** (`query.tql`)

```txtql
TEXT = who ' is ' age
who  = WORD
age  = INT
```

**Input** (`input.txt`)

```text
Ada is 36
```

**Output**

```json
{
  "who": "Ada",
  "age": "36"
}
```

The full set of defaults, in order of precedence:

1. A single repetition: the list of the items' values (`0 TO 1`: the item's value or `null`).
2. A single rule reference: that rule's value.
3. Anything with captures: an object of the captures.
4. A single parenthesised group: that group's value.
5. Otherwise: the matched text.

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n WORD SPLITBY ' '
```

**Input** (`input.txt`)

```text
a b c
```

**Output**

```json
[
  "a",
  "b",
  "c"
]
```

**Query** (`query.tql`)

```txtql
TEXT = 'id-' INT
```

**Input** (`input.txt`)

```text
id-42
```

**Output**

```json
"id-42"
```

A capture inside a repetition is a **list**, even with one element. Only `0 TO 1` and `1 TO 1` give the value itself (or `null`):

**Query** (`query.tql`)

```txtql
TEXT = 'nums: ' 1 TO n n:INT SPLITBY ','
```

**Input** (`input.txt`)

```text
nums: 1,2,3
```

**Output**

```json
{
  "n": [
    "1",
    "2",
    "3"
  ]
}
```

**Query** (`query.tql`)

```txtql
TEXT = 'nums: ' 1 TO n n:INT SPLITBY ','
```

**Input** (`input.txt`)

```text
nums: 7
```

**Output**

```json
{
  "n": [
    "7"
  ]
}
```

A capture in an `OR` branch that was not taken is `null` in templates and conditions; in the default object,
only the branch that was taken is listed:

**Query** (`query.tql`)

```txtql
TEXT = (num:INT OR name:WORD) AS { 'num': num, 'name': name }
```

**Input** (`input.txt`)

```text
abc
```

**Output**

```json
{
  "num": null,
  "name": "abc"
}
```

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n item SPLITBY ' '
item = num:INT OR name:WORD
```

**Input** (`input.txt`)

```text
12 abc
```

**Output**

```json
[
  {
    "num": "12"
  },
  {
    "name": "abc"
  }
]
```

### Where a label goes matters

A label takes the nearest pattern after it; add parentheses to take more. On a repetition (in parentheses) it
captures the **text** the repetition matched; on the repeated item it captures a **list**:

**Query** (`query.tql`)

```txtql
TEXT = ws:(1 TO n WORD SPLITBY ', ')
```

**Input** (`input.txt`)

```text
a, b
```

**Output**

```json
{
  "ws": "a, b"
}
```

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n ws:WORD SPLITBY ', '
```

**Input** (`input.txt`)

```text
a, b
```

**Output**

```json
[
  "a",
  "b"
]
```

The same thing happens with the short form of `UNTILBEFORE`: `key:ANY UNTILBEFORE '='` labels each `ANY`, so you
get a list of characters; `key:(ANY UNTILBEFORE '=')` gives the text:

**Query** (`query.tql`)

```txtql
TEXT = key:ANY UNTILBEFORE '=' '=' value:WORD
```

**Input** (`input.txt`)

```text
key=v
```

**Output**

```json
{
  "key": [
    "k",
    "e",
    "y"
  ],
  "value": "v"
}
```

Parentheses group but do not scope; a label does. Captures inside a labelled pattern belong to the label's value.
The same name captured twice in one match is a static error, because the second would hide the first:

**Query** (`query.tql`)

```txtql
TEXT = sp WORD sp
sp = ' '
```

**Input** (`input.txt`)

```text
 a 
```

**Error** (standard error, exit code 1)

```text
txtql::check::duplicate_capture

  × capture `sp` appears twice in the same match
   ╭─[query.tql:1:8]
 1 │ TEXT = sp WORD sp
   ·        ─┬      ─┬
   ·         │       ╰── captured again here
   ·         ╰── first capture
 2 │ sp = ' '
   ╰────
  help: give one of them a different name with a label, e.g. `other:sp`
```

## 6. Aliases

An alias names a pattern fragment: `ALIAS name = pattern`. Using it is the same as writing its pattern in
parentheses, except that **an alias never captures anything**. That is why it can appear any number of times in one sequence:

**Query** (`query.tql`)

```txtql
TEXT = a:WORD sp b:WORD sp c:WORD
ALIAS sp = 1 TO n ' '
```

**Input** (`input.txt`)

```text
x  y z
```

**Output**

```json
{
  "a": "x",
  "b": "y",
  "c": "z"
}
```

To capture an alias's text, label it where it is used. Its value is the text it matched:

**Query** (`query.tql`)

```txtql
TEXT = key:WORD ': ' value:rest NL
ALIAS rest = ANY UNTILBEFORE NL
```

**Input** (`input.txt`, ends with a line break)

```text
host: example.org
```

**Output**

```json
{
  "key": "host",
  "value": "example.org"
}
```

An alias cannot contain labels (write a rule if you need captures inside), cannot be recursive, and shares its name space with rules:

**Query** (`query.tql`)

```txtql
TEXT = a:pair
ALIAS pair = k:WORD '=' v:WORD
```

**Input** (`input.txt`)

```text
a=b
```

**Error** (standard error, exit code 1)

```text
txtql::check::label_in_alias

  × aliases cannot capture: `k` is labelled inside one
   ╭─[query.tql:2:14]
 1 │ TEXT = a:pair
 2 │ ALIAS pair = k:WORD '=' v:WORD
   ·              ───┬──
   ·                 ╰── label inside an alias
   ╰────
  help: label the alias where it is used (`x:alias`), or write a rule instead
        if you need captures inside it

txtql::check::label_in_alias

  × aliases cannot capture: `v` is labelled inside one
   ╭─[query.tql:2:25]
 1 │ TEXT = a:pair
 2 │ ALIAS pair = k:WORD '=' v:WORD
   ·                         ───┬──
   ·                            ╰── label inside an alias
   ╰────
  help: label the alias where it is used (`x:alias`), or write a rule instead
        if you need captures inside it
```

By convention aliases go at the end of the query.

## 7. Output templates (AS)

`AS template` decides the JSON a rule produces. Templates look like JSON, with two rules about names:
**a quoted string is always a literal, and a bare name is always a capture**, for keys and for values.

**Query** (`query.tql`)

```txtql
TEXT = name:WORD ' ' age:INT AS { 'name': name, 'age': NUM(age) }
```

**Input** (`input.txt`)

```text
Ada 36
```

**Output**

```json
{
  "name": "Ada",
  "age": 36
}
```

`NUM(age)` turned the text into a number. Because a bare key is a capture, the matched text can be the key:

**Query** (`query.tql`)

```txtql
TEXT = k:WORD '=' v:WORD AS { k: v }
```

**Input** (`input.txt`)

```text
colour=red
```

**Output**

```json
{
  "colour": "red"
}
```

Arrays and constants (`'text'`, numbers, `true`, `false`, `null`, in any case):

**Query** (`query.tql`)

```txtql
TEXT = name:WORD ' ' age:INT AS [ name, NUM(age), true, null, 'x', -2.5 ]
```

**Input** (`input.txt`)

```text
Ada 36
```

**Output**

```json
[
  "Ada",
  36,
  true,
  null,
  "x",
  -2.5
]
```

### Lists with FOR

`[ element FOR list ]` builds one element per item. Inside, the item's fields are names (here `name`, a field
of each `person` object):

**Query** (`query.tql`)

```txtql
TEXT   = 1 TO n people:person AS [ name FOR people ]
person = name:WORD ' ' age:INT NL AS { 'name': name, 'age': NUM(age) }
```

**Input** (`input.txt`, ends with a line break)

```text
Ada 36
Bob 41
```

**Output**

```json
[
  "Ada",
  "Bob"
]
```

`FOR x IN expr` iterates any list under a name you choose; `NUM(x)` converts each element:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n n:INT SPLITBY ',' AS [ NUM(x) FOR x IN n ]
```

**Input** (`input.txt`)

```text
1,2,3
```

**Output**

```json
[
  1,
  2,
  3
]
```

Objects work the same way, with one entry per item. `p.k` reads a field:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n pairs:pair SPLITBY NL AS { p.k: NUM(p.v) FOR p IN pairs }
pair = k:WORD '=' v:INT
```

**Input** (`input.txt`)

```text
a=1
b=2
```

**Output**

```json
{
  "a": 1,
  "b": 2
}
```

### Keyless entries merge

An entry without a key merges an object, or a list of objects, into the one being built (`null` adds nothing).
This is how a variable set of key-value pairs becomes one object:

**Query** (`query.tql`)

```txtql
TEXT = user:WORD ' ' 1 TO n opts:opt SPLITBY ' ' AS { 'user': user, opts }
opt = k:WORD '=' v:WORD AS { k: v }
```

**Input** (`input.txt`)

```text
ada bold=yes size=big
```

**Output**

```json
{
  "user": "ada",
  "bold": "yes",
  "size": "big"
}
```

The front-page example uses the same mechanism: `TEXT = 1 TO n rhyme AS { rhyme }` merges the list of one-entry objects `rhyme` into a single object.

### Repeated keys and LISTOF

If an object gets the same key twice, the later value wins and you get a warning that names the entry (an error with `--strict`):

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n es:entry SPLITBY NL AS { e.team: e.name FOR e IN es }
entry = team:WORD ': ' name:WORD
```

**Input** (`input.txt`)

```text
red: ann
blue: bob
red: cy
```

**Output**

```json
{
  "red": "cy",
  "blue": "bob"
}
```

**Warnings** (standard error)

```text
txtql::eval::repeated_key

  ⚠ key `red` is set more than once; the later value wins
   ╭─[query.tql:1:40]
 1 │ TEXT = 1 TO n es:entry SPLITBY NL AS { e.team: e.name FOR e IN es }
   ·                                        ───┬──
   ·                                           ╰── this sets the key again
 2 │ entry = team:WORD ': ' name:WORD
   ╰────
  help: to keep every value, collect them with `LISTOF`, e.g. `{ k: LISTOF v
        FOR x IN xs }`

Advice:
  ☞ in this part of the input
   ╭─[input.txt:1:1]
 1 │ ╭─▶ red: ann
 2 │ │   blue: bob
 3 │ ├─▶ red: cy
   · ╰──── matched text
   ╰────
```

`LISTOF` collects every value of a repeated key into a list. The value is always a list, even for a key that occurs once:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n es:entry SPLITBY NL AS { e.team: LISTOF e.name FOR e IN es }
entry = team:WORD ': ' name:WORD
```

**Input** (`input.txt`)

```text
red: ann
blue: bob
red: cy
```

**Output**

```json
{
  "red": [
    "ann",
    "cy"
  ],
  "blue": [
    "bob"
  ]
}
```

### Functions

| Function | Result |
|---|---|
| `NUM(x)` | text or number to a number |
| `LOWER(x)`, `UPPER(x)`, `TRIM(x)` | text conversions |
| `COUNT(list)`, `FIRST(list)`, `LAST(list)` | list helpers (`null` for an empty list) |
| `JOIN(list, sep)` | the items joined with `sep`; **the separator is required** |
| `ZIP(keys, values)` | an object pairing each key with the value in the same position |

Function names are case-insensitive.

**Query** (`query.tql`)

```txtql
TEXT = name:(ANY UNTIL ',') 1 TO n n:INT SPLITBY '+'
  AS { 'name': UPPER(TRIM(name)), 'lower': LOWER(name), 'count': COUNT(n),
       'first': NUM(FIRST(n)), 'last': NUM(LAST(n)), 'joined': JOIN(n, '-') }
```

**Input** (`input.txt`)

```text
  Ada ,1+2+3
```

**Output**

```json
{
  "name": "ADA",
  "lower": "  ada ",
  "count": 3,
  "first": 1,
  "last": 3,
  "joined": "1-2-3"
}
```

`JOIN` has no default separator; leaving it out is caught before anything runs:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n n:INT SPLITBY ',' AS JOIN(n)
```

**Input** (`input.txt`)

```text
1,2
```

**Error** (standard error, exit code 1)

```text
txtql::check::arity

  × `JOIN` takes 2 argument(s), got 1
   ╭─[query.tql:1:36]
 1 │ TEXT = 1 TO n n:INT SPLITBY ',' AS JOIN(n)
   ·                                    ───┬───
   ·                                       ╰── wrong number of arguments
   ╰────
  help: JOIN needs a separator: `JOIN(n, ' ')`
```

`ZIP` is made for tables with a header row, where the keys come from the data:

**Query** (`query.tql`)

```txtql
TEXT = header:record 1 TO n rows:record AS [ ZIP(header, r) FOR r IN rows ]
record = 1 TO n cells:cell SPLITBY ',' NL AS cells
ALIAS cell = ANY UNTILBEFORE (',' OR NL)
```

**Input** (`input.txt`, ends with a line break)

```text
name,city
Ada,London
Bob,Paris
```

**Output**

```json
[
  {
    "name": "Ada",
    "city": "London"
  },
  {
    "name": "Bob",
    "city": "Paris"
  }
]
```

A template that fails on the data, such as `NUM('abc')`, is an error that points into both the query and the input:

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

## 8. Conditions (WHERE)

`WHERE` goes after the pattern (before `AS`) and decides whether a match counts. A match whose condition is false
does not count, and txtql looks for another reading. Conditions use captures and the same functions as templates:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n big SKIPPING ANY
big  = n:FLOAT WHERE NUM(n) > 10 AS NUM(n)
```

**Input** (`input.txt`)

```text
We sold 3 apples, 12 pears, 7 plums and 40 cherries in 2 days.
```

**Output**

```json
[
  12,
  40
]
```

Operators: `=`, `!=`, `<`, `<=`, `>`, `>=`, `CONTAINS`, `STARTSWITH`, `ENDSWITH`, `AND`, `OR`, `NOT` and
parentheses. `NOT` binds tighter than `AND`, which binds tighter than `OR`:

**Query** (`query.tql`)

```txtql
TEXT  = 1 TO n entry SPLITBY NL
entry = name:WORD ' ' score:INT
    WHERE NUM(score) >= 50 AND NOT name STARTSWITH 'x' OR name = 'root'
```

**Input** (`input.txt`)

```text
ann 80
root 1
```

**Output**

```json
[
  {
    "name": "ann",
    "score": "80"
  },
  {
    "name": "root",
    "score": "1"
  }
]
```

If a `WHERE` rejects every reading, the whole input fails to match:

**Query** (`query.tql`)

```txtql
TEXT  = 1 TO n entry SPLITBY NL
entry = name:WORD ' ' score:INT
    WHERE NUM(score) >= 50 AND NOT name STARTSWITH 'x' OR name = 'root'
```

**Input** (`input.txt`)

```text
ann 80
xena 90
```

**Error** (standard error, exit code 1)

```text
txtql::input::no_parse

  × the text does not match the query
   ╭─[input.txt:1:1]
 1 │ ╭─▶ ann 80
 2 │ ├─▶ xena 90
   · ╰──── the text matches the patterns, but no reading satisfies the WHERE conditions
   ╰────
  help: check the WHERE clauses; a rule whose WHERE is false does not match
```

Comparing numbers deserves care. Ordering operators compare numerically when both sides are numbers or numeric
text (`'9' < '10'` is true), and as text otherwise. `=` and `!=` compare numerically only when at least one side is
a number, so two texts are compared as exact text:

**Query** (`query.tql`)

```txtql
TEXT = a:FLOAT ' ' b:FLOAT WHERE a = b
```

**Input** (`input.txt`)

```text
1.0 1
```

**Error** (standard error, exit code 1)

```text
txtql::input::no_parse

  × the text does not match the query
   ╭─[input.txt:1:1]
 1 │ 1.0 1
   · ──┬──
   ·   ╰── the text matches the patterns, but no reading satisfies the WHERE conditions
   ╰────
  help: check the WHERE clauses; a rule whose WHERE is false does not match
```

Convert one side with `NUM` to compare numerically:

**Query** (`query.tql`)

```txtql
TEXT = a:FLOAT ' ' b:FLOAT WHERE NUM(a) = NUM(b)
```

**Input** (`input.txt`)

```text
1.0 1
```

**Output**

```json
{
  "a": "1.0",
  "b": "1"
}
```

A bare value is true unless it is `null`, `false`, `''`, `[]` or `{}` (the number `0` is true).

`WHERE` is also the tool for settling a choice between `OR` branches: the second rule below does not match `-`,
so `place` cannot compete with `unknown`:

**Query** (`query.tql`)

```txtql
TEXT    = 1 TO n entry AS { entry }
entry   = name:WORD ': ' city:(unknown OR place) NL AS { name: city }
unknown = '-' AS null
place   = t:(ANY UNTILBEFORE NL) WHERE t != '-' AS t
```

**Input** (`input.txt`, ends with a line break)

```text
ada: London
bob: -
```

**Output**

```json
{
  "ada": "London",
  "bob": null
}
```

## 9. Structural matching

Rules can refer to themselves, directly or through other rules, so nested structure of any depth can be matched,
which regular expressions cannot do. The only rule is that every recursive step consumes text.

An s-expression reader (note `atom OR sexpr` inside the repetition):

**Query** (`query.tql`)

```txtql
TEXT  = sexpr NL AS sexpr
sexpr = '(' 0 TO n items:(atom OR sexpr) SPLITBY ' ' ')' AS items
atom  = WORD OR FLOAT OR '*' OR '+'
```

**Input** (`input.txt`, ends with a line break)

```text
(define (square x) (* x x))
```

**Output**

```json
[
  "define",
  [
    "square",
    "x"
  ],
  [
    "*",
    "x",
    "x"
  ]
]
```

Nested lists become nested JSON arrays:

**Query** (`query.tql`)

```txtql
TEXT = list
list = '[' 0 TO n items:(num OR list) SPLITBY ', ' ']' AS items
num  = n:INT AS NUM(n)
```

**Input** (`input.txt`)

```text
[1, [2, [3, []]], 4]
```

**Output**

```json
[
  1,
  [
    2,
    [
      3,
      []
    ]
  ],
  4
]
```

Unbalanced input does not match, and the error shows where it ended:

**Query** (`query.tql`)

```txtql
TEXT = list
list = '[' 0 TO n items:(num OR list) SPLITBY ', ' ']' AS items
num  = n:INT AS NUM(n)
```

**Input** (`input.txt`)

```text
[1, [2, 3]
```

**Error** (standard error, exit code 1)

```text
txtql::input::no_parse

  × the text does not match the query
   ╭─[input.txt:1:10]
 1 │ [1, [2, 3]
   ·          ┬
   ·          ╰── expected ', ' or ']' after this
   ╰────
  help: the text ended before the query was complete
```

A tree whose nodes have optional children:

**Query** (`query.tql`)

```txtql
TEXT = node
node = name:WORD 0 TO 1 ('(' 1 TO n kids:node SPLITBY ',' ')') AS { name: kids }
```

**Input** (`input.txt`)

```text
root(a,b(c,d),e)
```

**Output**

```json
{
  "root": [
    {
      "a": null
    },
    {
      "b": [
        {
          "c": null
        },
        {
          "d": null
        }
      ]
    },
    {
      "e": null
    }
  ]
}
```

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

**Query** (`query.tql`)

```txtql
TEXT = first:(1 TO n WORD SPLITBY ' ') ' ' rest:(1 TO n WORD SPLITBY ' ') NL
```

**Input** (`input.txt`, ends with a line break)

```text
one two three
```

**Output**

```json
{
  "first": "one two",
  "rest": "three"
}
```

**Warnings** (standard error)

```text
txtql::input::ambiguous

  ⚠ ambiguous match in rule `TEXT`: the text can be read in two ways with
  │ different results
   ╭─[input.txt:1:1]
 1 │ one two three
   · ───┬───┬
   ·    │   ╰── alternative: `first` = "one"
   ·    ╰── chosen: `first` = "one two"
   ╰────
  help: txtql picked the first reading, which gives {"first":"one
        two","rest":"three"}; the other gives {"first":"one","rest":"two
        three"}.
        Make the pattern more specific (literals, SPLITBY, LAZY) if the other
        reading was intended.
```

The warning goes to standard error, so the JSON is still printed. `--strict` turns the warning into an error:

**Query** (`query.tql`)

```txtql
TEXT = first:(1 TO n WORD SPLITBY ' ') ' ' rest:(1 TO n WORD SPLITBY ' ') NL
```

**Input** (`input.txt`, ends with a line break)

```text
one two three
```

**Command**

```
txtql --strict query.tql input.txt
```

**Error** (standard error, exit code 1)

```text
txtql::input::ambiguous

  × ambiguous match in rule `TEXT`: the text can be read in two ways with
  │ different results
   ╭─[input.txt:1:1]
 1 │ one two three
   · ───┬───┬
   ·    │   ╰── alternative: `first` = "one"
   ·    ╰── chosen: `first` = "one two"
   ╰────
  help: txtql picked the first reading, which gives {"first":"one
        two","rest":"three"}; the other gives {"first":"one","rest":"two
        three"}.
        Make the pattern more specific (literals, SPLITBY, LAZY) if the other
        reading was intended.
```

`STRICT` on a rule does the same for that rule only (`STRICT TEXT = …`). `txtql check` takes a sample input and
lists ambiguities without printing JSON, with exit code 2 if it found any:

**Query** (`query.tql`)

```txtql
TEXT = first:(1 TO n WORD SPLITBY ' ') ' ' rest:(1 TO n WORD SPLITBY ' ') NL
```

**Input** (`input.txt`, ends with a line break)

```text
one two three
```

**Command**

```
txtql check query.tql input.txt
```

**Output** (standard error, exit code 2)

```text
txtql::input::ambiguous

  ⚠ ambiguous match in rule `TEXT`: the text can be read in two ways with
  │ different results
   ╭─[input.txt:1:1]
 1 │ one two three
   · ───┬───┬
   ·    │   ╰── alternative: `first` = "one"
   ·    ╰── chosen: `first` = "one two"
   ╰────
  help: txtql picked the first reading, which gives {"first":"one
        two","rest":"three"}; the other gives {"first":"one","rest":"two
        three"}.
        Make the pattern more specific (literals, SPLITBY, LAZY) if the other
        reading was intended.

1 output-changing ambiguity found
```

Fix an ambiguity by making the pattern more specific (a literal separator), or by stating the preference with `LAZY`:

**Query** (`query.tql`)

```txtql
TEXT = first:(1 TO n WORD SPLITBY ' ') ' | ' rest:(1 TO n WORD SPLITBY ' ') NL
```

**Input** (`input.txt`, ends with a line break)

```text
one two | three
```

**Command**

```
txtql check query.tql input.txt
```

**Output** (standard error, exit code 0)

```text
query OK; no output-changing ambiguity on this input
```

**Query** (`query.tql`)

```txtql
TEXT = first:(1 TO n LAZY WORD SPLITBY ' ') ' ' rest:(1 TO n WORD SPLITBY ' ') NL
```

**Input** (`input.txt`, ends with a line break)

```text
one two three
```

**Output**

```json
{
  "first": "one",
  "rest": "two three"
}
```

An `OR` whose branches both match is the other common cause. The warning explains the usual fixes: list the special case
first, or exclude it from the general branch with `WHERE`:

**Query** (`query.tql`)

```txtql
TEXT    = 1 TO n entry AS { entry }
entry   = name:WORD ': ' city:(unknown OR place) NL AS { name: city }
unknown = '-' AS null
place   = t:(ANY UNTILBEFORE NL) AS t
```

**Input** (`input.txt`, ends with a line break)

```text
ada: London
bob: -
```

**Output**

```json
{
  "ada": "London",
  "bob": null
}
```

**Warnings** (standard error)

```text
txtql::input::ambiguous

  ⚠ ambiguous match in rule `entry`: the text can be read in two ways with
  │ different results
   ╭─[input.txt:2:6]
 1 │ ada: London
 2 │ bob: -
   ·      ┬┬
   ·      │╰── alternative: `place` = "-"
   ·      ╰── chosen: `unknown` = "-"
   ╰────
  help: txtql picked the first reading, which gives {"bob":null}; the other
        gives {"bob":"-"}.
        `unknown` was chosen because it comes first in the `OR`.
        - If `place` is the special case, list it first.
        - If this order is intended, exclude the special case from `place`
        with WHERE, e.g. `WHERE t != '-'`.
```

Ambiguity that does not change the output is not reported, and neither is ambiguity settled by `LAZY`, or
inside `SKIPPING` text and separators:

**Query** (`query.tql`)

```txtql
TEXT = 1 TO n (a OR b) SPLITBY ' '
a = 'x'
b = 'x'
```

**Input** (`input.txt`)

```text
x x
```

**Command**

```
txtql check query.tql input.txt
```

**Output** (standard error, exit code 0)

```text
query OK; no output-changing ambiguity on this input
```

## 11. Names, spelling and reserved words

- Keywords, built-in names and function names can be written in any case (`WORD`, `word`, `Word`). In return,
  they cannot be rule names, labels or alias names. That includes `line`, `row`, `col`, `word`, `to`, `in` and so on.
- The root rule may be spelled `TEXT` or `text`; both are the same name, so you cannot also define a rule called `text`.
- `true`, `false` and `null`, in any case, are template constants and cannot name a rule, alias, label or loop variable.
- Names start with a letter or `_` and continue with letters, digits or `_`; Unicode letters are allowed.

**Query** (`query.tql`)

```txtql
TEXT = word:WORD
```

**Input** (`input.txt`)

```text
x
```

**Error** (standard error, exit code 1)

```text
txtql::parse::unexpected_token

  × expected a label, found keyword `word`
   ╭─[query.tql:1:8]
 1 │ TEXT = word:WORD
   ·        ──┬─
   ·          ╰── unexpected keyword `word`
   ╰────
  help: `word` is a keyword (in any spelling), so it cannot be a label; choose
        another one
```

**Query** (`query.tql`)

```txtql
TEXT = null
null = 'x'
```

**Input** (`input.txt`)

```text
x
```

**Error** (standard error, exit code 1)

```text
txtql::check::reserved_name

  × `null` is a template constant, not a name
   ╭─[query.tql:2:1]
 1 │ TEXT = null
 2 │ null = 'x'
   · ──┬─
   ·   ╰── rule named after a constant
   ╰────
  help: `true`, `false` and `null` (in any case) are constants in templates
        and conditions, so a rule, alias, label or loop variable called `null`
        could never be referenced; choose another name, e.g. `null_value`
```

**Query** (`query.tql`)

```txtql
TEXT = text
text = 'x'
```

**Input** (`input.txt`)

```text
x
```

**Error** (standard error, exit code 1)

```text
txtql::check::duplicate_rule

  × `text` is defined more than once
   ╭─[query.tql:1:1]
 1 │ TEXT = text
   · ──┬─
   ·   ╰── first definition
 2 │ text = 'x'
   · ──┬─
   ·   ╰── redefined here
   ╰────
  help: rules and aliases share one set of names; rename one of them

txtql::check::undefined_rule

  × rule `text` is not defined
   ╭─[query.tql:1:8]
 1 │ TEXT = text
   ·        ──┬─
   ·          ╰── unknown rule
 2 │ text = 'x'
   ╰────
  help: did you mean `TEXT`?
```

**Query** (`query.tql`)

```txtql
text = w:word -- the root may be spelled text, keywords in any case
```

**Input** (`input.txt`)

```text
x
```

**Output**

```json
{
  "w": "x"
}
```

## Where next

- [[Practical Examples|Practical-Examples]]: real-world recipes (CSV, logs, configs, emails, pipelines) built from these ideas.
- [[Language Reference|Language-Reference]]: everything on one page.
- [[Ambiguity and Strict Mode|Ambiguity-and-Strict-Mode]] and [[Errors and Diagnostics|Errors-and-Diagnostics]].
- The `tests/cases/` directory in the repository holds larger worked queries: invoices, CSV with a header row, HTTP headers,
  a web server log, a game playtest log and all twelve Advent of Code 2025 example inputs.
