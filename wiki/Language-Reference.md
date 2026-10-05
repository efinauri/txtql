# Language Reference

The terse, authoritative guide to the language: every construct, its meaning and its exact syntax (see
[Grammar](#grammar)). For a guided introduction read the [[Language Walkthrough|Language-Walkthrough]] first;
[[Errors and Diagnostics|Errors-and-Diagnostics]] lists every diagnostic code. The formal behavioural
specification (written in Allium) is in the repository:
[`txtql.allium`](https://github.com/efinauri/txtql/blob/main/txtql.allium) and the modules in
[`spec/`](https://github.com/efinauri/txtql/tree/main/spec). It is meant for implementers and contributors and is
not needed to use the language; every example on this page is re-run against the binary.

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
- Strings use `'...'` or `"..."`, may span lines, and support `\n \t \r \\ \' \"`. In a pattern, `i'...'` ignores case
  (simple per-character lower-casing, not full Unicode case folding). The `i` prefix exists only in patterns: in a
  template or a `WHERE` condition it is a syntax error (`txtql::parse::unexpected_token`, with the help
  "case-insensitive literals are only available in patterns").
- Number literals in templates and conditions are unsigned: an integer up to 18446744073709551615 (`u64::MAX`) or a
  finite float (an exponent may carry a sign: `1e-5`). There are no negative literals: a `-` there is a
  `txtql::parse::unexpected_token` at the `-` (see "Negative numbers" in the Walkthrough for how to get negatives from
  the data). A literal beyond `u64` or a float that is not finite is `txtql::parse::bad_number`; a repetition bound is
  an integer literal and follows the same rule.
- After `.` in a template path, any word is a field name, keywords included.
- Limits: finite repetition bounds up to 10,000 (use `n` beyond that); nesting of parentheses, repetitions,
  templates and conditions up to 100 levels.

## Patterns

| Pattern | Meaning |
|---|---|
| `'text'`, `"text"` | literal text; must not be empty; spaces are ordinary characters |
| `i'text'` | literal that ignores case (patterns only) |
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
- The clauses after the item come in any order. Each kind comes at most once, and `UNTIL` and `UNTILBEFORE` share
  one slot: a repetition takes one stop. To stop at either of two patterns write `UNTIL (a OR b)`. A second stop,
  in the bounded or the short form, is `txtql::parse::unexpected_token` at the second stop keyword. A parenthesised
  group is a separate item, so `(p UNTIL a) UNTIL b` is legal: the outer stop is the short form applied to the group.
- `LAZY` prefers fewer repetitions. Without it, repetitions prefer another item over stopping.
- `SPLITBY sep`: `sep` between consecutive items, never before the first or after the last.
- `SKIPPING skip`: text matching `skip` may appear before, between and after the items (also around a separator);
  used only where nothing else fits.
- `UNTIL stop` ends at the first position where `stop` matches and consumes it (its longest match);
  `UNTILBEFORE stop` ends there and leaves it for what follows. The stop is mandatory (a repetition with a stop cannot end where its stop does
  not match), is checked wherever an iteration would begin (before the separator, if there is one), and is never part
  of a value, including a label on the repetition. `p UNTIL s` without bounds means `0 TO n p UNTIL s`; a label before
  the short form labels the item `p`, not the repetition.
- A stop is built from literals and built-ins other than `LINE`, `ROW` and `COL`, with sequences, `OR`, plain repetitions
  (no `SPLITBY`, `SKIPPING` or stop of their own: `UNTIL (1 TO n ' ')`, `UNTILBEFORE (NL DIGIT OR NL EOF)`) and aliases of those;
  rules and labels are not allowed. It must not be able to match empty text (except `EOF`)
  and cannot be `ANY` alone. A stop that is not allowed gives `txtql::check::bad_stop`. The checks that apply to every
  pattern apply inside a stop too, with their own codes: an empty literal is `txtql::check::empty_literal`, an undefined
  name is `txtql::check::undefined_rule` (with its hint), and bounds (`bad_bounds`, `bound_too_large`) and duplicate `OR`
  branches are checked as anywhere else. A stop with such a problem is not also reported as `bad_stop` or `empty_stop`
  (`UNTIL ''` is `empty_literal` only). `bad_stop` is for defined rules, labels, `ANY` alone, `LINE`, `ROW`, `COL` and
  nested `SPLITBY`, `SKIPPING` or stops.
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

## Grammar

The complete syntax, derived from the parser. Notation: `"X"` is a literal token, `[ ]` optional, `{ }` zero or more,
`|` alternative, `( )` grouping. Keywords and built-in names are written in capitals and are matched in any case.
Blanks, line breaks and comments may stand between any two tokens and mean nothing. `letter` and `digit` are Unicode
letters and digits, `ascii-digit` is `0` to `9`, `char` is any character but the closing quote and the backslash.

```ebnf
(* ---- query ---- *)
query       = { rule | alias } ;
rule        = [ "STRICT" ] name "=" pattern [ "WHERE" condition ] [ "AS" template ] ;
alias       = "ALIAS" name "=" pattern ;

(* ---- patterns ---- *)
pattern     = sequence { "OR" sequence } ;
sequence    = element { element } ;
element     = item [ stop ] ;                  (* short form: `item UNTIL s` means `0 TO n item UNTIL s` *)
item        = [ name ":" ] ( repetition | atom ) ;
repetition  = integer "TO" ( integer | "n" ) [ "LAZY" ] item { clause } ;
clause      = "SPLITBY" item | "SKIPPING" item | stop ;    (* each at most once; one stop in all *)
stop        = ( "UNTIL" | "UNTILBEFORE" ) item ;
atom        = pattern-string | builtin | name | "(" pattern ")" ;
builtin     = "WORD" | "FLOAT" | "INT" | "HEX" | "BIN" | "IPV4" | "IPV6" | "PUNCT" | "ANY"
            | "DIGIT" | "LETTER" | "LINE" | "NL" | "TAB" | "ROW" | "COL" | "EOF" ;

(* ---- templates (AS) ---- *)
template    = string | number | constant | path | call | object | array ;
constant    = "true" | "false" | "null" ;      (* any case; read before paths *)
path        = name { "." field } ;
field       = name | keyword ;                 (* any word after the dot *)
call        = name "(" [ template { "," template } ] ")" ;
object      = "{" [ entry { "," entry } [ "," ] ] "}" ;
entry       = template ":" [ "LISTOF" ] template [ iteration ]    (* key: value *)
            | template ;                                           (* merge *)
array       = "[" [ element-t { "," element-t } [ "," ] ] "]" ;
element-t   = template [ iteration ] ;
iteration   = "FOR" name [ "IN" template ] ;

(* ---- conditions (WHERE) ---- *)
condition   = conjunction { "OR" conjunction } ;
conjunction = negation { "AND" negation } ;
negation    = "NOT" negation | "(" condition ")" | template [ comparator template ] ;
comparator  = "=" | "!=" | "<" | "<=" | ">" | ">=" | "CONTAINS" | "STARTSWITH" | "ENDSWITH" ;

(* ---- tokens ---- *)
name        = ( letter | "_" ) { letter | digit | "_" } ;   (* not a keyword; not true, false or null *)
integer     = ascii-digit { ascii-digit } ;
number      = integer | float ;   (* unsigned; integers up to 18446744073709551615 *)
float       = integer "." integer [ exponent ] | integer exponent ;
exponent    = ( "e" | "E" ) [ "+" | "-" ] integer ;
string      = "'" { char | escape } "'" | '"' { char | escape } '"' ;
pattern-string = [ "i" ] string ;               (* the case-insensitive prefix is for patterns only *)
escape      = "\" ( "n" | "t" | "r" | "\" | "'" | '"' ) ;
comment     = "--" { any character except a line break } ;
```

Notes on the grammar:

- **Rule boundaries.** A pattern, condition or template ends where a name followed by `=` begins (the next rule) or
  where `ALIAS` or `STRICT` appears, so rules need no separator and may share a line.
- **`n`** is a name that is only read as "unlimited" right after `TO` (in any case); it is not reserved.
- **`i`** marks a case-insensitive literal only when it is a lowercase `i` directly before the opening quote, and only in
  a pattern (`pattern-string`). In a template or a condition it is a syntax error.
- **Number tokens.** Numbers are unsigned: a `-` before one is never part of it (`AS -5` is a syntax error); only the
  exponent of a `float` has a sign. A number needs digits after its `.`. Bounds are plain integers up to
  18446744073709551615; the static check limits finite ones to 10,000.
- **A condition operand is a template**, so any template can be compared. A bare template is a truth test.
- **Not in the grammar** (rejected after parsing, with their own codes, see
  [[Errors and Diagnostics|Errors-and-Diagnostics]]): empty literals, bounds (`min` above `max`, above 10,000, reported as written), stops
  that are not allowed (the other checks apply inside stops too), repetitions that can match nothing, labels in aliases, reserved and duplicate names, undefined
  rules, unknown captures, unknown functions and wrong argument counts. Nesting beyond 100 levels is a parse error.

### Precedence and associativity

Patterns, tightest first:

| Level | Construct | Associativity and effect |
|---|---|---|
| 1 | `( p )`, literals, built-ins, names | atoms |
| 2 | `label:` and `min TO max [LAZY]` | prefixes that take the next single item and nest to the right (a label cannot directly follow a label): `k:1 TO n x` labels the whole repetition, `1 TO n k:x` labels each item. A repetition's `SPLITBY`, `SKIPPING`, `UNTIL` and `UNTILBEFORE` clauses take one item each |
| 3 | `item UNTIL s`, `item UNTILBEFORE s` (short form) | postfix on the item just before it |
| 4 | sequence: `a b c` | juxtaposition; flat |
| 5 | `a OR b OR c` | loosest; flat, the leftmost branch that covers the same text wins |

Conditions, tightest first:

| Level | Construct | Associativity and effect |
|---|---|---|
| 1 | `( c )` | groups conditions only, not operands |
| 2 | `x = y`, `!=`, `<`, `<=`, `>`, `>=`, `CONTAINS`, `STARTSWITH`, `ENDSWITH` | at most one per comparison: they do not chain; both sides are templates |
| 3 | `NOT c` | prefix; may repeat (`NOT NOT c`); covers a whole comparison |
| 4 | `a AND b` | left to right, short-circuit |
| 5 | `a OR b` | left to right, short-circuit; loosest |

Templates have no operators, so no precedence: a template is a single term. `.` reads fields from a name only (not from
a call, a literal or a bracket), there is no `-` (number literals are unsigned), there are no parentheses, `LISTOF` only
follows an object key, a `FOR` clause ends an array element or an object entry with a key (one per element, none on a merge
entry), and function calls nest by their parentheses.

### The grammar against the parser

Every row was run (results use `--compact`). A result that starts with `txtql::` is the diagnostic the query is
rejected with; `no match` is a valid query that does not match the input.

Patterns:

| What it shows | Query | Input | Result |
|---|---|---|---|
| `OR` is looser than a sequence: the branches are `x` and `y z` | `TEXT = 'x' OR 'y' 'z'` | `x` | `"x"` |
| (second branch) | `TEXT = 'x' OR 'y' 'z'` | `yz` | `"yz"` |
| (`x z` is not a reading) | `TEXT = 'x' OR 'y' 'z'` | `xz` | no match |
| parentheses regroup | `TEXT = ('x' OR 'y') 'z'` | `xz` | `"xz"` |
| a repetition repeats one item: `(1 TO 2 a) b` | `TEXT = 1 TO 2 'a' 'b'` | `aab` | `"aab"` |
| (`(a b)` is not repeated) | `TEXT = 1 TO 2 'a' 'b'` | `abab` | no match |
| parentheses make the item longer | `TEXT = 1 TO 2 ('a' 'b')` | `abab` | `["ab","ab"]` |
| a repetition binds tighter than `OR`: `(1 TO 2 a) OR b` | `TEXT = 1 TO 2 'a' OR 'b'` | `b` | `"b"` |
| (`a b` is not a reading) | `TEXT = 1 TO 2 'a' OR 'b'` | `ab` | no match |
| a label takes one item | `TEXT = k:'a' 'b' AS k` | `ab` | `"a"` |
| a group takes more | `TEXT = k:('a' 'b') AS k` | `ab` | `"ab"` |
| a label binds tighter than `OR` | `TEXT = k:'a' OR 'b' AS k` | `b` | `null` |
| a label before a repetition labels the whole repetition | `TEXT = k:1 TO n 'a' 'b' AS k` | `aab` | `"aa"` |
| a label on the repeated item labels each item | `TEXT = 1 TO n k:'a' 'b' AS k` | `aab` | `["a","a"]` |
| a short-form stop applies to the preceding item (the label is part of it) | `TEXT = x:'a' y:ANY UNTILBEFORE 'b' 'b'` | `axyb` | `{"x":"a","y":["x","y"]}` |
| a clause takes one item, the next one continues the sequence | `TEXT = 1 TO n 'a' SPLITBY ',' ';'` | `a,a;` | `"a,a;"` |
| a second `UNTIL` is an error (write `UNTIL (a OR b)`) | `TEXT = 1 TO n ANY UNTIL ';' UNTIL '!'` | `a;` | error: txtql::parse::unexpected_token |
| (also `UNTIL` with `UNTILBEFORE`) | `TEXT = ANY UNTILBEFORE ';' UNTIL '!'` | `a;` | error: txtql::parse::unexpected_token |
| (`UNTIL (a OR b)` is the way) | `TEXT = ANY UNTIL (';' OR '!')` | `a!` | `["a"]` |
| a group with its own stop can take an outer stop | `TEXT = (ANY UNTIL ';') UNTIL '!'` | `a;b;!` | `[["a"],["b"]]` |
| each clause at most once | `TEXT = 1 TO n 'a' SPLITBY ',' SPLITBY ';'` | `a` | error: txtql::parse::unexpected_token |
| clauses in any order | `TEXT = 1 TO n 'a' UNTIL ';' SKIPPING ' ' SPLITBY ','` | `a, a;` | `["a","a"]` |
| a name followed by `=` starts the next rule, even on the same line | `TEXT = a a = 'x'` | `x` | `"x"` |
| labels do not nest | `TEXT = a:b:'x'` | `x` | error: txtql::parse::unexpected_token |
| a vertical bar is not `OR` | `TEXT = 'a' \| 'b'` | `a` | error: txtql::parse::unexpected_char |
| a pattern is required after `OR` | `TEXT = 'a' OR` | `a` | error: txtql::parse::unexpected_end |
| a repetition needs its item | `TEXT = 1 TO n` | `a` | error: txtql::parse::unexpected_end |
| `LAZY` comes right after the bounds | `TEXT = 1 TO n 'a' LAZY` | `a` | error: txtql::parse::unexpected_token |
| empty parentheses are not a pattern | `TEXT = ()` | `a` | error: txtql::parse::unexpected_token |

Conditions:

| What it shows | Query | Input | Result |
|---|---|---|---|
| `NOT` binds tighter than `AND`: `(NOT false) AND false` | `TEXT = 'x' WHERE NOT false AND false` | `x` | no match |
| (parentheses give `NOT (false AND false)`) | `TEXT = 'x' WHERE NOT (false AND false)` | `x` | `"x"` |
| `AND` binds tighter than `OR`: `true OR (true AND false)` | `TEXT = 'x' WHERE true OR true AND false` | `x` | `"x"` |
| (and `(false AND false) OR true`) | `TEXT = 'x' WHERE false AND false OR true` | `x` | `"x"` |
| `NOT` covers a whole comparison | `TEXT = 'x' WHERE NOT 1 = 2` | `x` | `"x"` |
| `NOT` repeats | `TEXT = 'x' WHERE NOT NOT true` | `x` | `"x"` |
| `OR` does not evaluate its right side when the left holds | `TEXT = 'x' WHERE true OR NUM('abc') = 1` | `x` | `"x"` |
| `OR` evaluates left to right | `TEXT = 'x' WHERE NUM('abc') = 1 OR true` | `x` | error: txtql::eval::error |
| `AND` does not evaluate its right side when the left fails | `TEXT = 'x' WHERE false AND NUM('abc') = 1 OR true` | `x` | `"x"` |
| comparisons do not chain | `TEXT = 'x' WHERE 1 < 2 < 3` | `x` | error: txtql::parse::unexpected_token |
| (not even equality) | `TEXT = 'x' WHERE 1 = 1 = 1` | `x` | error: txtql::parse::unexpected_token |
| parentheses group conditions, not operands | `TEXT = 'x' WHERE (1) = 1` | `x` | error: txtql::parse::unexpected_token |
| (a parenthesised condition is fine) | `TEXT = 'x' WHERE (1 = 1) AND (NOT 2 = 3)` | `x` | `"x"` |
| `NOT` is not an operand | `TEXT = 'x' WHERE 1 = NOT 2` | `x` | error: txtql::parse::unexpected_token |
| operands are any template | `TEXT = 'x' WHERE [1, 2] CONTAINS 1 AND { 'k': 1 } CONTAINS 'k'` | `x` | `"x"` |
| `WHERE` comes before `AS` | `TEXT = 'x' AS 'y' WHERE true` | `x` | error: txtql::parse::unexpected_token |
| `WHERE` at most once | `TEXT = 'x' WHERE true WHERE true` | `x` | error: txtql::parse::unexpected_token |

Templates:

| What it shows | Query | Input | Result |
|---|---|---|---|
| number literals are unsigned: no negative literal | `TEXT = 'x' AS -5` | `x` | error: txtql::parse::unexpected_token |
| (a blank after the `-` changes nothing) | `TEXT = 'x' AS - 5` | `x` | error: txtql::parse::unexpected_token |
| (nor inside a list) | `TEXT = 'x' AS [-1.5]` | `x` | error: txtql::parse::unexpected_token |
| (nor in a condition) | `TEXT = n:INT WHERE NUM(n) > -5` | `1` | error: txtql::parse::unexpected_token |
| an exponent may carry a sign | `TEXT = 'x' AS [1e-5, 2.5E+3]` | `x` | `[0.00001,2500.0]` |
| a negative value comes from captured text | `TEXT = n:(0 TO 1 '-' INT) AS NUM(n)` | `-42` | `-42` |
| the sign as a boolean: `true` when present | `minus = '-' AS true
TEXT = 0 TO 1 neg:minus v:INT AS neg` | `-3` | `true` |
| (`null` when absent) | `minus = '-' AS true
TEXT = 0 TO 1 neg:minus v:INT AS neg` | `3` | `null` |
| a condition on the sign instead of a negative literal | `minus = '-' AS true
TEXT = 0 TO 1 neg:minus n:INT WHERE neg != null AND NUM(n) > 10 AS NUM(n)` | `-42` | `42` |
| `-` is not an operator | `TEXT = 'x' AS 1 - 2` | `x` | error: txtql::parse::unexpected_token |
| (nor a sign for names) | `TEXT = w:WORD AS - w` | `x` | error: txtql::parse::unexpected_token |
| no parentheses in templates | `TEXT = w:WORD AS (w)` | `x` | error: txtql::parse::unexpected_token |
| a field is read from a name only, not from a call | `TEXT = w:WORD AS FIRST(w).x` | `x` | error: txtql::parse::unexpected_token |
| trailing commas in objects and arrays | `TEXT = 'x' AS { 'a': [1,], }` | `x` | `{"a":[1]}` |
| no trailing comma in calls | `TEXT = w:WORD AS NUM(w,)` | `x` | error: txtql::parse::unexpected_token |
| a merge entry has no `FOR` | `TEXT = w:WORD AS { w FOR w }` | `x` | error: txtql::parse::unexpected_token |
| at most one `FOR` per element | `TEXT = 1 TO n w:WORD SPLITBY ' ' AS [ x FOR x IN w FOR y IN w ]` | `a b` | error: txtql::parse::unexpected_token |
| `LISTOF` only follows a key | `TEXT = w:WORD AS [ LISTOF w ]` | `x` | error: txtql::parse::unexpected_token |
| any word, keywords included, is a field name (this one fails only at run time) | `TEXT = w:WORD AS w.AS` | `x` | error: txtql::eval::error |
| function names in any case | `TEXT = n:INT AS nUm(n)` | `42` | `42` |
| constants in any case | `TEXT = 'x' AS [True, NULL, fAlse]` | `x` | `[true,null,false]` |
| an `i` prefix on a template string is a syntax error | `TEXT = 'x' AS i'Q'` | `x` | error: txtql::parse::unexpected_token |
| (and in a condition) | `TEXT = w:WORD WHERE w = i'x'` | `x` | error: txtql::parse::unexpected_token |
| integers reach the `u64` maximum | `TEXT = 'x' AS 18446744073709551615` | `x` | `18446744073709551615` |
| (beyond is a bad number) | `TEXT = 'x' AS 18446744073709551616` | `x` | error: txtql::parse::bad_number |
| (and so is a float that is not finite) | `TEXT = 'x' AS 1e999` | `x` | error: txtql::parse::bad_number |
| a float too small to represent rounds to zero | `TEXT = 'x' AS 1e-999` | `x` | `0.0` |

Lexical rules:

| What it shows | Query | Input | Result |
|---|---|---|---|
| keywords and built-in names in any case, `n` too | `TEXT = 1 to N w:word splitby ',' as w` | `a,b` | `["a","b"]` |
| rule names are case-sensitive (`foo` is not `Foo`) | `TEXT = foo Foo = 'a'` | `a` | error: txtql::check::undefined_rule |
| except the root, in any case | `text = 'a'` | `a` | `"a"` |
| a keyword is not a label | `TEXT = word:'x'` | `x` | error: txtql::parse::unexpected_token |
| a keyword is not a rule name | `TEXT = a and = 'x'` | `x` | error: txtql::parse::unexpected_token |
| `true`, `false`, `null` are not rule names | `TEXT = True True = 'x'` | `x` | error: txtql::check::reserved_name |
| (nor labels) | `TEXT = NULL:'x'` | `x` | error: txtql::check::reserved_name |
| (nor alias names) | `ALIAS False = 'x' TEXT = False` | `x` | error: txtql::check::reserved_name |
| (nor `FOR` variables) | `TEXT = w:'x' AS [ w FOR true IN [1] ]` | `x` | error: txtql::check::reserved_name |
| `n` is only special right after `TO` | `TEXT = 1 TO 2 n n = 'a'` | `aa` | `["a","a"]` |
| a comment runs to the end of the line, but not inside a string | `TEXT = 'a--b' -- c` | `a--b` | `"a--b"` |
| `--` always starts a comment | `TEXT = 'x' AS --2` | `x` | error: txtql::parse::unexpected_end |
| the case-insensitive prefix is a lowercase `i` right before the quote | `TEXT = i"AbC"` | `aBc` | `"aBc"` |
| (an uppercase `I` is a name) | `TEXT = I'abc'` | `abc` | error: txtql::check::undefined_rule |
| (so is `i` followed by a blank) | `TEXT = i 'abc' i = 'x'` | `xabc` | `{"i":"x"}` |
| string escapes and both quotes | `TEXT = 'x' AS "say \"hi\"\n"` | `x` | `"say \"hi\"\n"` |
| an unknown escape | `TEXT = 'a\q'` | `a` | error: txtql::parse::bad_escape |
| an unterminated string | `TEXT = 'a` | `a` | error: txtql::parse::unterminated_string |
| a number needs digits after the dot | `TEXT = 'x' AS 7.` | `x` | error: txtql::parse::unexpected_token |
| leading zeros and a fraction | `TEXT = 'x' AS [007, 2.50]` | `x` | `[7,2.5]` |
| a character outside the language | `TEXT = 'a' @` | `a` | error: txtql::parse::unexpected_char |
| names may use Unicode letters | `é = 'x' TEXT = é` | `x` | `"x"` |
| blanks and line breaks are free between tokens | `TEXT = a : 'x'` | `x` | `{"a":"x"}` |

## Disambiguation in one paragraph

Earlier parts of a sequence are greedy (`LAZY`: as little as possible); the leftmost `OR` branch wins among alternatives
that cover the same text; repetitions prefer another item over stopping; optional parts prefer to match;
`SKIPPING` text is skipped only where nothing else fits. Details and the ambiguity check are on
[[Ambiguity and Strict Mode|Ambiguity-and-Strict-Mode]].
