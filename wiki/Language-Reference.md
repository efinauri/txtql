# Language Reference

A condensed but complete reference. For a guided introduction, read the
[[Language Walkthrough|Language-Walkthrough]] first. The formal definition is the Allium specification in
`txtql.allium` and `spec/` in the repository.

## Query structure

```
[STRICT] name = pattern [WHERE condition] [AS template]
ALIAS name = pattern
```

- A query is a list of rules and aliases, in any order. Rules and aliases share one name space.
- `TEXT` is the root rule. It is required, must be a rule (not an alias), and must match the whole input.
  It can be spelled `TEXT` or `text` (any case); all spellings are the same name.
- `WHERE`, if present, comes before `AS`; each appears at most once.
- `STRICT` before a rule makes output-changing ambiguity in that rule an error (see
  [[Ambiguity and Strict Mode|Ambiguity-and-Strict-Mode]]).
- Comments start with `--` and run to the end of the line.
- By convention: `TEXT` first, then the rules it uses, aliases last.

### Spelling

- **Keywords, built-in pattern names and function names are case-insensitive** (`WORD`, `word`, `Word`).
  Keywords and built-in pattern names cannot be used as rule, alias or label names. The keywords are:
  `OR AND NOT TO LAZY SPLITBY SKIPPING WHERE AS FOR IN LISTOF CONTAINS STARTSWITH ENDSWITH STRICT UNTILBEFORE UNTIL ALIAS`
  plus the built-in pattern names `WORD FLOAT INT HEX BIN IPV4 IPV6 PUNCT ANY DIGIT LETTER LINE NL TAB ROW COL EOF`.
  Function names (`NUM`, `COUNT`, ...) are not reserved: a rule may be called `count`.
- `true`, `false` and `null` (any case) are template constants and cannot name a rule, alias, label or `FOR` variable
  (`txtql::check::reserved_name`).
- Names start with a letter or `_` and continue with letters, digits or `_`. Unicode letters are allowed.
- Strings use `'...'` or `"..."`, may span lines, and support `\n \t \r \\ \' \"`. `i'...'` ignores case
  (simple per-character lower-casing, not full Unicode case folding).
- After `.` in a template path, any word is a field name, keywords included.
- Limits: finite repetition bounds up to 10,000 (use `n` beyond that); nesting of parentheses, repetitions,
  templates and conditions up to 100 levels.

## Patterns

| Pattern | Meaning |
|---|---|
| `'text'`, `"text"` | literal text; must not be empty; spaces are ordinary characters |
| `i'text'` | literal that ignores case |
| built-in | see the table below |
| `name` | a rule or alias; a rule is captured under its own name |
| `a b` | sequence |
| `a OR b` | alternatives; binds looser than a sequence |
| `( ... )` | grouping (does not create a scope) |
| `label:p` | capture `p` under `label`; takes the nearest pattern |
| `min TO max p` | repetition; `max` may be `n` (unlimited) |

### Built-in patterns

Every pattern matches exactly the characters described and nothing is skipped implicitly.

| Pattern | Matches |
|---|---|
| `WORD` | a maximal run of letters (combining marks belong to the letter before them); never part of a longer run. Digits and apostrophes end a run (`don't` holds the words `don` and `t`) |
| `INT` | a maximal run of ASCII digits, not preceded or followed by another digit (`3.14` holds two `INT`s; `12kg` starts with an `INT`) |
| `FLOAT` | a maximal run of ASCII digits with an optional `.digits` part (`42`, `3.5`); no sign, no exponent (`1.2.3` reads as `1.2`, `.`, `3`) |
| `HEX`, `BIN` | a run of hexadecimal or binary digits, not preceded by a digit of its own kind and not followed by a letter or digit (`'0x' HEX`) |
| `IPV4` | four numbers of up to three digits, each at most 255, separated by dots; not part of a longer dotted number |
| `IPV6` | the longest IPv6 text form at that point (at most one `::`, optional dotted IPv4 tail); not preceded by a letter, digit, `:` or `.`, not followed by a letter or digit. No zone suffix; a time such as `10:30:00` is not an address |
| `LETTER`, `DIGIT` | one letter, one ASCII digit; also inside a run |
| `PUNCT` | one character that is not a letter, digit or whitespace |
| `ANY` | any one character, line breaks included |
| `NL` | a line break: `\r\n`, `\n` or a lone `\r`, always as one |
| `TAB` | one tab |
| `LINE` | from the start of a line to the next line break or the end of the text, line break excluded; may be empty; only at the start of a line (the position between `\r` and `\n` is not one) |
| `ROW`, `COL` | no text; worth the current line and column, counted from 1, columns in characters; usable as keys, where they become text |
| `EOF` | no text; only at the end of the input |

Examples (every row was run; results use `--compact`):

| Pattern or feature | Query | Input | Result |
|---|---|---|---|
| `WORD` | `TEXT = 1 TO n WORD SPLITBY ' '` | `héllo wörld` | `["héllo","wörld"]` |
| `WORD` (not inside a run) | `TEXT = WORD` | `abc1` | no match |
| `INT` | `TEXT = 1 TO n INT SPLITBY '.'` | `3.14` | `["3","14"]` |
| `FLOAT` | `TEXT = 1 TO n FLOAT SPLITBY ' '` | `42 3.5` | `["42","3.5"]` |
| `HEX` | `TEXT = '0x' HEX` | `0xCAFE` | `"0xCAFE"` |
| `BIN` | `TEXT = '0b' BIN` | `0b1011` | `"0b1011"` |
| `IPV4` | `TEXT = ip:IPV4` | `199.72.81.55` | `{"ip":"199.72.81.55"}` |
| `IPV4` (part above 255) | `TEXT = ip:IPV4` | `256.1.1.1` | no match |
| `IPV6` | `TEXT = '[' a:IPV6 ']:' port:INT` | `[2001:db8::1]:8080` | `{"a":"2001:db8::1","port":"8080"}` |
| `LETTER` | `TEXT = 1 TO n LETTER` | `abc` | `["a","b","c"]` |
| `DIGIT` | `TEXT = 1 TO n DIGIT` | `987` | `["9","8","7"]` |
| `PUNCT` | `TEXT = 1 TO n PUNCT` | `,;!` | `[",",";","!"]` |
| `ANY` (crosses lines) | `TEXT = 1 TO n ANY` | `a\nb` | `["a","\n","b"]` |
| `NL` (all three line breaks) | `TEXT = 1 TO n WORD SPLITBY NL` | `a\r\nb\nc\rd` | `["a","b","c","d"]` |
| `TAB` | `TEXT = a:WORD TAB b:WORD` | `x\ty` | `{"a":"x","b":"y"}` |
| `LINE` (may be empty) | `TEXT = 1 TO n LINE SPLITBY NL` | `a\n\nb` | `["a","","b"]` |
| `LINE` (mid-line) | `TEXT = 'a' LINE` | `ab` | no match |
| `ROW`, `COL` | `TEXT = 'ab' r:ROW c:COL` | `ab` | `{"r":1,"c":3}` |
| `EOF` | `TEXT = w:WORD EOF` | `end` | `{"w":"end"}` |
| `i'…'` (ignore case) | `TEXT = i'hello'` | `HeLLo` | `"HeLLo"` |

## Repetition

```
min TO max [LAZY] item [SPLITBY sep] [SKIPPING skip] [UNTIL stop | UNTILBEFORE stop]
```

- `min` and `max` are whole numbers; `max` may be `n`. `0 TO 1` is optional, `1 TO n` one or more, `0 TO 0` never matches.
  `min` must not exceed `max` and finite bounds must not exceed 10,000.
- The clauses after the item come in any order, each at most once.
- `LAZY` prefers fewer repetitions. Without it, repetitions prefer another item over stopping.
- `SPLITBY sep`: `sep` between consecutive items, never before the first or after the last.
- `SKIPPING skip`: text matching `skip` may appear before, between and after the items (also around a separator);
  used only where nothing else fits.
- `UNTIL stop` ends at the first position where `stop` matches and consumes it (its longest match);
  `UNTILBEFORE stop` ends there and leaves it for what follows. The stop is mandatory (a repetition with a stop cannot end where its stop does
  not match), is checked wherever an iteration would begin (before the separator, if there is one), and is never part
  of a value, including a label on the repetition. `p UNTIL s` without bounds means `0 TO n p UNTIL s`; a label before
  the short form labels the item `p`, not the repetition.
- A stop is built from literals and built-ins other than `LINE`, `ROW` and `COL`, with sequences, `OR` and repetitions
  (`UNTIL (1 TO n ' ')`, `UNTILBEFORE (NL DIGIT OR NL EOF)`) and aliases of those; rules and labels are not allowed. It must not be able to match empty text (except `EOF`)
  and cannot be `ANY` alone. A stop that is not allowed gives `txtql::check::bad_stop`.
- An item that can match empty text is an error (`txtql::check::empty_loop`), unless a `SPLITBY` separator consumes text.
- Separators, `SKIPPING` patterns and stops contribute no captures.

| Pattern or feature | Query | Input | Result |
|---|---|---|---|
| `0 TO 1` | `TEXT = 'a' 0 TO 1 'b'` | `a` | `"a"` |
| `2 TO 3` | `TEXT = n:(2 TO 3 'a')` | `aaaa` | no match |
| `2 TO 3` (in range) | `TEXT = n:(2 TO 3 'a')` | `aaa` | `{"n":"aaa"}` |
| `0 TO 0` | `TEXT = 'a' 0 TO 0 'b' 'c'` | `ac` | `"ac"` |
| `SPLITBY` | `TEXT = 1 TO n WORD SPLITBY ', '` | `a, b, c` | `["a","b","c"]` |
| `SPLITBY` (no trailing separator) | `TEXT = 1 TO n WORD SPLITBY ', '` | `a, b, ` | no match |
| `SKIPPING` | `TEXT = 1 TO n n:INT SKIPPING (1 TO n LETTER OR ' ')` | `a 1 b 2 c` | `["1","2"]` |
| `UNTIL` | `TEXT = a:(ANY UNTIL ';') b:WORD` | `x y;z` | `{"a":"x y","b":"z"}` |
| `UNTILBEFORE` | `TEXT = a:(ANY UNTILBEFORE ';') ';' b:WORD` | `x y;z` | `{"a":"x y","b":"z"}` |
| `LAZY` | `TEXT = a:(1 TO n LAZY ANY) ',' rest:(0 TO n ANY)` | `a,b,c` | `{"a":"a","rest":"b,c"}` |
| greedy (default) | `TEXT = a:(1 TO n ANY) ',' rest:(0 TO n ANY)` | `a,b,c` | `{"a":"a,b","rest":"c"}` |

(The greedy row also prints an ambiguity warning, left out here; see [[Ambiguity and Strict Mode|Ambiguity-and-Strict-Mode]].)

## Aliases

`ALIAS name = pattern` is a named fragment, used as if its pattern were written in parentheses, except that it never
captures: not as itself, not through labels (a label inside an alias is an error), not through rules it refers to.
It has no `WHERE` or `AS`, cannot be recursive and cannot be the root. To capture its text, label it where it is used.
Its value is always the text it matched.

## Captures and default values

- A rule's captures are its labels and the rules it references (under the rule's own name).
- A capture inside a repetition with `max` greater than one is a list, even with one item. In a repetition with
  `max` of at most one (`0 TO 1`, `1 TO 1`) it is the value, or `null` when absent.
- A capture in an `OR` branch that was not taken is `null`.
- A label on a repetition captures the text it matched (separators and skipped text included, a consumed stop excluded);
  a label on the repeated item captures a list.
- The same capture name twice in one match is an error (`txtql::check::duplicate_capture`).

Without `AS`, a rule's value is the first that applies:

1. a single unlabelled repetition: the list of its items' values (`0 TO 1`: the item's value or `null`);
2. a single unlabelled rule reference: that rule's value;
3. a body (`OR` branch) with captures: an object of those captures, in order;
4. a single parenthesised group: that group's value;
5. otherwise: the matched text. A captured `ROW` or `COL` is a number.

## Templates (`AS`)

| Template | Meaning |
|---|---|
| `'x'`, `"x"` | text literal |
| `1`, `-2.5`, `1e-8` | number literals |
| `true`, `false`, `null` | constants (any case) |
| `name`, `name.field.field` | a capture, or fields of an object (a field of `null` or a missing field is `null`; a field of anything else is an error) |
| `[ a, b ]` | array |
| `{ k: v, ... }` | object; a quoted key is literal, a bare key is a capture; keys must evaluate to text (numbers and booleans are written as text) |
| `[ e FOR r ]` | one element per item of the list captured as `r`; inside, `r` is the item, and if it is an object its fields are names too; inner names shadow outer ones |
| `{ k: v FOR r }` | one entry per item |
| `... FOR x IN expr` | iterate any list under the name `x`; iterating `null` yields nothing, any other non-list is an error |
| `{ k: LISTOF v FOR ... }` | collect every value for a repeated key into a list (always a list) |
| `{ r, ... }` | an entry without a key merges an object, or every object of a list of objects; `null` adds nothing; other values are an error |
| `F(args)` | function call |

Object keys keep template order. Setting a key twice (by entries, iterations, merges or `ZIP`) keeps the later value and
records a `txtql::eval::repeated_key` warning (an error with `--strict`), once per entry per run. Only `LISTOF` entries do not report.

### Functions

Function names are case-insensitive.

| Function | Result |
|---|---|
| `NUM(x)` | text (trimmed) or number to a number; `null` stays `null`; anything that is not a finite number is an error |
| `LOWER(x)`, `UPPER(x)`, `TRIM(x)` | text to text; `null` stays `null`; other values are an error |
| `COUNT(x)` | number of items of a list (or keys of an object); `null` counts 0 |
| `FIRST(x)`, `LAST(x)` | first or last item of a list; `null` when empty or `null` |
| `JOIN(list, sep)` | the items joined with `sep`. **`sep` is required** (a missing one is `txtql::check::arity`) and must be text. Numbers become text, `null` items are skipped, text passes through; booleans, nested lists and objects are an error; a `null` list stays `null` |
| `ZIP(keys, values)` | an object pairing each key with the value in the same position. `null` keys or values count as empty lists; missing values are `null`; extra values are an error; keys must be text, numbers or booleans; a repeated key keeps the later value and is reported |

| Pattern or feature | Query | Input | Result |
|---|---|---|---|
| `NUM(x)` | `TEXT = a:INT AS NUM(a)` | `007` | `7` |
| `NUM(x)` (decimal) | `TEXT = a:FLOAT AS NUM(a)` | `3.50` | `3.5` |
| `LOWER(x)` | `TEXT = a:WORD AS LOWER(a)` | `ABC` | `"abc"` |
| `UPPER(x)` | `TEXT = a:WORD AS UPPER(a)` | `abc` | `"ABC"` |
| `TRIM(x)` | `TEXT = a:(ANY UNTIL ';') AS TRIM(a)` | `  a b ;` | `"a b"` |
| `COUNT(list)` | `TEXT = 1 TO n a:WORD SPLITBY ' ' AS COUNT(a)` | `a b c` | `3` |
| `FIRST(list)` | `TEXT = 1 TO n a:WORD SPLITBY ' ' AS FIRST(a)` | `a b c` | `"a"` |
| `LAST(list)` | `TEXT = 1 TO n a:WORD SPLITBY ' ' AS LAST(a)` | `a b c` | `"c"` |
| `JOIN(list, sep)` | `TEXT = 1 TO n a:WORD SPLITBY ' ' AS JOIN(a, '+')` | `a b c` | `"a+b+c"` |
| `ZIP(keys, values)` | `TEXT = 1 TO n k:WORD SPLITBY ' ' ';' 1 TO n v:INT SPLITBY ' ' AS ZIP(k, v)` | `a b;1 2` | `{"a":"1","b":"2"}` |
| `ZIP` (missing values are `null`) | `TEXT = 1 TO n k:WORD SPLITBY ' ' ';' 1 TO n v:INT SPLITBY ' ' AS ZIP(k, v)` | `a b c;1` | `{"a":"1","b":null,"c":null}` |
| `NUM` of non-numeric text | `TEXT = a:WORD AS NUM(a)` | `abc` | error: txtql::eval::error |

## Conditions (`WHERE`)

| Operator | Meaning |
|---|---|
| `=`, `!=` | equality. Numeric when at least one side is a number and both convert to numbers; otherwise exact comparison. Two texts are always compared as exact text (`'10' = '10.0'` is false) |
| `<`, `<=`, `>`, `>=` | numeric when both sides are numbers or numeric text; otherwise text by code point; other combinations are an error |
| `CONTAINS` | text contains text; a list contains an exactly equal value (no numeric conversion); an object has the key |
| `STARTSWITH`, `ENDSWITH` | text on both sides |
| `AND`, `OR`, `NOT`, `( )` | short-circuit, left to right; `NOT` binds tighter than `AND`, `AND` tighter than `OR` |

A `null` left side of a text operator is false; other mismatched combinations are errors. A bare value is true unless it
is `null`, `false`, empty text, an empty list or an empty object (the number 0 is true). A match whose `WHERE` is false
does not count, and the next reading in preference order is tried; if none satisfies the conditions, the run fails.

| Pattern or feature | Query | Input | Result |
|---|---|---|---|
| `=` on two texts | `TEXT = a:FLOAT ' ' b:FLOAT WHERE a = b` | `1.0 1` | no match |
| `=` with a number | `TEXT = a:FLOAT WHERE a = 1` | `1.0` | `{"a":"1.0"}` |
| `<` on numeric text | `TEXT = a:INT ' ' b:INT WHERE a < b` | `9 10` | `{"a":"9","b":"10"}` |
| `<` on other text | `TEXT = a:WORD ' ' b:WORD WHERE a < b` | `abc abd` | `{"a":"abc","b":"abd"}` |
| `!=` | `TEXT = a:WORD ' ' b:WORD WHERE a != b` | `x x` | no match |
| `>=` | `TEXT = a:INT WHERE NUM(a) >= 18` | `18` | `{"a":"18"}` |
| `CONTAINS` (text) | `TEXT = a:WORD WHERE a CONTAINS 'ell'` | `hello` | `{"a":"hello"}` |
| `STARTSWITH` | `TEXT = a:WORD WHERE a STARTSWITH 'he'` | `hello` | `{"a":"hello"}` |
| `ENDSWITH` | `TEXT = a:WORD WHERE a ENDSWITH 'lo'` | `hello` | `{"a":"hello"}` |
| `AND`, `OR`, `NOT` | `TEXT = a:WORD WHERE NOT a = 'x' AND (a = 'y' OR a = 'z')` | `z` | `{"a":"z"}` |
| bare value (empty text is false) | `TEXT = a:(0 TO n WORD) WHERE a` | (empty) | no match |
| bare value (number 0 is true) | `TEXT = a:INT WHERE NUM(a)` | `0` | `{"a":"0"}` |

## Disambiguation in one paragraph

Earlier parts of a sequence are greedy (`LAZY`: as little as possible); the leftmost `OR` branch wins among alternatives
that cover the same text; repetitions prefer another item over stopping; optional parts prefer to match;
`SKIPPING` text is skipped only where nothing else fits. Details and the ambiguity check are on
[[Ambiguity and Strict Mode|Ambiguity-and-Strict-Mode]].
