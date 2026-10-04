# FAQ and Known Limitations

## FAQ

**Why does my query fail on a file that looks right?**
Every character is significant, including the line break at the end of the file and spaces and tabs. A query that does not mention the final line break
does not match a file that ends with one. Add `NL` (or `EOF` where nothing is expected), and read the error: it points at the furthest position reached and lists what was expected.

**Why are my numbers strings?**
Built-ins produce text. Convert in a template: `AS { 'age': NUM(age) }`.

**Why is a capture a list when there is only one item?**
A capture inside a repetition with an upper bound above one is always a list, so the shape does not depend on the data. A repetition with at most one item (`0 TO 1`, `1 TO 1`)
gives the value itself, or `null` when absent.

**How do I match the rest of a line? How do I read until a character?**
`ANY UNTIL NL` consumes the line break too; `ANY UNTILBEFORE NL` leaves it. `LINE` matches a whole line, only at the start of a line; use `ALIAS rest = ANY UNTILBEFORE NL` for the rest of a line.
`ANY` crosses lines, so give the stop the line break too (`ANY UNTILBEFORE (':' OR NL)`) when a value must stay on one line.

**I get a warning about ambiguity. Is something wrong?**
Another reading of the text exists that would give different output, and txtql picked the first one by its rules. If that is what you want, state it (`LAZY`, a more specific pattern, an `OR` order);
if not, fix the query. `--strict` makes it an error. See [[Ambiguity and Strict Mode|Ambiguity-and-Strict-Mode]].

**Why can't I name a rule `word`, `line`, `to` or `text`?**
Keywords and built-in names are case-insensitive, so all spellings are reserved. `text` is the same name as the root `TEXT`. `true`, `false` and `null` are reserved too.

**Why must `TEXT` be a rule and not an alias?**
Only rules can have captures, a `WHERE` and an `AS` template, and the output of the whole query comes from `TEXT` (`txtql::check::root_is_alias`).

**`\n` in a `-e` query gives "unexpected character"?**
A backslash-n typed in a shell string is not a line break. Use real line breaks, or a query file. (Inside a txtql string literal, `'\n'` is fine.)

**Can I use regular expressions?**
No. Use patterns: literals, built-ins, repetition, `UNTIL`. Typing a regex habit (`|`, `*`, `+`, `?`) gives the txtql spelling.

**Can it parse JSON, XML, YAML?**
Those formats already have parsers. txtql is for text without one. Structure that is nested by brackets can be matched with recursive rules; indentation-based nesting (YAML and similar) is out of scope.

**How do I group rows by a key?**
In a template: `{ r.team: LISTOF r.name FOR r IN rows }` collects every value of a repeated key into a list. See the [[Language Walkthrough|Language-Walkthrough]].

**Where do the diagnostics go?**
Standard error. JSON goes to standard output. Exit code 1 for any error, 2 for `txtql check` findings.

## Known limitations

- **Case-insensitive literals** (`i'text'`) use simple per-character lower-casing, not full Unicode case folding (`ß` does not match `SS`).
- **`WHERE` chains**: a `WHERE` clause re-evaluates its rule's captures, so deeply nested chains of rules that each have a `WHERE` cost O(depth squared). This counts against the step limit.
- **Template errors inside `FOR`** point at the whole rule's match, not at the specific item.
- **`--max-depth` is a search bound**, so the chosen reading can nest up to about twice that depth; see [[Performance and Limits|Performance-and-Limits]].
- **The ambiguity check has a budget** and stops quietly (with a note) on very ambiguous input; `txtql check` has no practical limit.
- **Reports are capped** at one per match and 20 per run.
- **Not covered by design**: indentation-based nesting, and anything that needs language understanding (txtql is purely structural).
- **Not yet released**: version 0.1.0; the language may still change.
- **Number formats**: `FLOAT` has no sign or exponent (write `0 TO 1 '-' FLOAT`); `INT` and `FLOAT` read ASCII digits only.
- **`IPV6`** matches RFC 4291 text forms without a zone suffix.
