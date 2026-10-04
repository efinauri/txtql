# Practical Examples

A cookbook of real jobs for txtql. Every recipe gives the problem, a realistic sample input, the query, the exact output
and a short explanation of the interesting parts. All of them were run, and every query except the deliberately naive and
deliberately failing ones also runs clean under `--strict` (no output-changing ambiguity, no repeated keys).
If you are new to the language, read the [[Language Walkthrough|Language-Walkthrough]] first;
the [[Language Reference|Language-Reference]] has the details.

Two things apply to every recipe:

- txtql never skips anything implicitly: the query must account for every character, including the final line break.
  That is why several queries end a record with `NL` and why the sample files end with a line break. (The CSV recipe
  shows how to make that final line break optional.)
- When the text does not fit, you get an error that points at where matching stopped, not a partial result.

| Recipe | What it shows |
|---|---|
| [1. CSV to JSON](#1-csv-to-json) | header row plus records, `ZIP`, quoted fields |
| [2. TSV and INI configuration to JSON](#2-tsv-and-ini-configuration-to-json) | typed columns, sections, comments, booleans |
| [3. Web server access log](#3-web-server-access-log) | numbers, `-` as `null`, status classes with `WHERE`, filtering lines |
| [4. Application log with stack traces](#4-application-log-with-stack-traces) | multi-line entries grouped under their first line |
| [5. A Markdown changelog](#5-a-markdown-changelog) | headings and bullet lists to nested JSON, dynamic keys |
| [6. An order-confirmation email](#6-an-order-confirmation-email) | headers, prose, aligned item lines, a total |
| [7. Running it in a pipeline](#7-running-it-in-a-pipeline) | standard input, `jq`, `txtql check` and `--strict` in CI |

## 1. CSV to JSON

**Problem.** Turn a CSV file with a header row into a JSON array with one object per record. Some fields are quoted
because they contain a comma, a quote or even a line break; some are empty.

**Sample input.**

```text
id,name,note
1,Ada,"likes tea, and coffee"
2,"Grace ""Amazing"" Hopper",
3,Linus,"two
lines"
```

**Query.**

```txtql
TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(0 TO n ('""' OR run)) '"' AS v
run    = 1 TO n ANY UNTILBEFORE '"'
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) WHERE NOT v STARTSWITH '"' AS v
```

**Output.**

```json
[
  {
    "id": "1",
    "name": "Ada",
    "note": "likes tea, and coffee"
  },
  {
    "id": "2",
    "name": "Grace \"\"Amazing\"\" Hopper",
    "note": ""
  },
  {
    "id": "3",
    "name": "Linus",
    "note": "two\nlines"
  }
]
```

**How it works.**

- A `record` is a list of cells separated by commas, and its value is the list of its cells. `TEXT` reads the header record and a
  line break, then one or more records separated by line breaks, then at most one final line break. `ZIP(header, r)` pairs the
  header names with each record's cells to make an object.
- **The final line break is optional.** A file that ends with a line break and one that does not give the same output. The
  cells stop before a comma, a line break or the end of the text (`OR EOF`), and the trailing `0 TO 1 NL` takes the final line
  break if there is one. This is safe only because an empty record cannot exist: a `record` is either two or more cells
  (`several`) or a single cell that is not empty (`alone`, `WHERE c != ''`). Otherwise the text after the last line break would
  also read as one more record with a single empty cell, and an input ending in `\n` would have two readings, one of them with
  a phantom last row of empty values. (Writing `(NL OR EOF)` as the record terminator instead is rejected by `txtql check`
  as an empty repetition, because a record could then match nothing at the end of the text.) Empty fields are still fine
  anywhere in a record with two or more columns, including the last one (`2,"Grace ""Amazing"" Hopper",` below).
- A `cell` is `quoted OR plain`. A quoted cell is a quote, any number of pieces, and a closing quote. A piece is either `""`
  (a doubled quote, the CSV way to write a quote inside a field) or a `run`: one or more characters that are not a quote
  (`ANY UNTILBEFORE '"'`). So a comma or a line break inside the quotes is just text, and the closing quote is the first
  quote that is not part of a `""` pair, wherever it stands.
- **Why `run` and not `ANY`.** The shorter `('""' OR ANY)` is ambiguous: `ANY` also matches a lone quote, so a `""` could be
  read as one doubled quote or as two separate characters, and the field could end in more than one place. Because a `run`
  never contains a quote, every quote in the field is either half of a `""` or the closing one, and there is exactly one way
  to read it. The cases that a "quote followed by a comma or line break" rule gets wrong are all exact here, and `--strict`
  passes: a doubled quote right before a comma,

**Query** (`query.tql`)

```txtql
TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(0 TO n ('""' OR run)) '"' AS v
run    = 1 TO n ANY UNTILBEFORE '"'
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) WHERE NOT v STARTSWITH '"' AS v
```

**Input** (`input.txt`, ends with a line break)

```text
id,note
1,"say ""hi"", then leave"
```

**Command**

```
txtql --strict query.tql input.txt
```

**Output**

```json
[
  {
    "id": "1",
    "note": "say \"\"hi\"\", then leave"
  }
]
```

  right before a line break inside a quoted field,

**Query** (`query.tql`)

```txtql
TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(0 TO n ('""' OR run)) '"' AS v
run    = 1 TO n ANY UNTILBEFORE '"'
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) WHERE NOT v STARTSWITH '"' AS v
```

**Input** (`input.txt`, ends with a line break)

```text
id,note
1,"she said ""stop""
and left"
```

**Command**

```
txtql --strict query.tql input.txt
```

**Output**

```json
[
  {
    "id": "1",
    "note": "she said \"\"stop\"\"\nand left"
  }
]
```

  and a field made only of quotes (`""""` is one doubled quote; `""` is the empty field):

**Query** (`query.tql`)

```txtql
TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(0 TO n ('""' OR run)) '"' AS v
run    = 1 TO n ANY UNTILBEFORE '"'
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) WHERE NOT v STARTSWITH '"' AS v
```

**Input** (`input.txt`, ends with a line break)

```text
id,note
1,""""
2,""
```

**Command**

```
txtql --strict query.tql input.txt
```

**Output**

```json
[
  {
    "id": "1",
    "note": "\"\""
  },
  {
    "id": "2",
    "note": ""
  }
]
```

- A `plain` cell stops at the next comma or line break. Without the `WHERE NOT v STARTSWITH '"'` it could also read the start
  of a quoted cell (`"likes tea`) as a plain cell, a second reading of the same text. txtql notices:
  `txtql check` on the version without the `WHERE` reports it,

**Query** (`query.tql`)

```txtql
TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(0 TO n ('""' OR run)) '"' AS v
run    = 1 TO n ANY UNTILBEFORE '"'
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) AS v
```

**Input** (`input.txt`, ends with a line break)

```text
id,name,note
1,Ada,"likes tea, and coffee"
2,"Grace ""Amazing"" Hopper",
3,Linus,"two
lines"
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
   ╭─[input.txt:2:1]
 1 │      id,name,note
 2 │ ╭──▶ 1,Ada,"likes tea, and coffee"
 3 │ │    2,"Grace ""Amazing"" Hopper",
 4 │ │╭─▶ 3,Linus,"two
   · ││   ──────┬─────
   · ││         ╰── alternative: `rows` = "3,Linus,\"two"
 5 │ ├──▶ lines"
   · ╰───── matched by `TEXT`
   · ╰───── chosen: `rows` = "3,Linus,"two lines""
   ╰────
  help: txtql picked the first reading, which gives
        [{"id":"1","name":"Ada","note":"likes tea, and coffee"},
        {"id":"2","name":"Grace…; the other gives
        [{"id":"1","name":"Ada","note":"likes tea, and coffee"},
        {"id":"2","name":"Grace….
        In the chosen reading, `1 TO n ANY UNTILBEFORE '"'` runs across line
        breaks (ANY includes them).
        - To keep it on one line, add NL to its stop: `1 TO n ANY UNTILBEFORE
        ('"' OR NL)`
        - If it should cross lines, make the stop say where it ends, e.g.
        `UNTILBEFORE (NL '[' OR NL EOF)`.

txtql::input::ambiguous

  ⚠ ambiguous match in rule `several`: the text can be read in two ways with
  │ different results
   ╭─[input.txt:2:7]
 1 │ id,name,note
 2 │ 1,Ada,"likes tea, and coffee"
   ·       ───────────┬───────────┬
   ·                  │           ╰── alternative: `cells` = "\"likes tea"
   ·                  ╰── chosen: `cells` = "\"likes tea, and coffee\""
 3 │ 2,"Grace ""Amazing"" Hopper",
   ╰────
  help: txtql picked the first reading, which gives ["1","Ada","likes tea, and
        coffee"].
        However, another reading of this text exists but fails to evaluate:
        `ZIP` got 4 values for 3 keys.
        Make the pattern more specific (literals, SPLITBY, LAZY) if the other
        reading was intended.

txtql::input::ambiguous

  ⚠ ambiguous match in rule `cell`: the text can be read in two ways with
  │ different results
   ╭─[input.txt:3:3]
 2 │ 1,Ada,"likes tea, and coffee"
 3 │ 2,"Grace ""Amazing"" Hopper",
   ·   ─────────────┬────────────┬
   ·                │            ╰── alternative: `plain` = "\"Grace \"\"Amazing\"\" Hopper\""
   ·                ╰── chosen: `quoted` = "\"Grace \"\"Amazing\"\" Hopper\""
 4 │ 3,Linus,"two
   ╰────
  help: txtql picked the first reading, which gives "Grace \"\"Amazing\"\"
        Hopper"; the other gives "\"Grace \"\"Amazing\"\" Hopper\"".
        `quoted` was chosen because it comes first in the `OR`.
        - If `plain` is the special case, list it first.
        - If this order is intended, exclude the special case from `plain`
        with WHERE, e.g. `WHERE t != '…'`.

3 output-changing ambiguities found
```

  and with `--strict` the same query is refused:

**Query** (`query.tql`)

```txtql
TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(0 TO n ('""' OR run)) '"' AS v
run    = 1 TO n ANY UNTILBEFORE '"'
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) AS v
```

**Input** (`input.txt`, ends with a line break)

```text
id,name,note
1,Ada,"likes tea, and coffee"
2,"Grace ""Amazing"" Hopper",
3,Linus,"two
lines"
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
   ╭─[input.txt:2:1]
 1 │      id,name,note
 2 │ ╭──▶ 1,Ada,"likes tea, and coffee"
 3 │ │    2,"Grace ""Amazing"" Hopper",
 4 │ │╭─▶ 3,Linus,"two
   · ││   ──────┬─────
   · ││         ╰── alternative: `rows` = "3,Linus,\"two"
 5 │ ├──▶ lines"
   · ╰───── matched by `TEXT`
   · ╰───── chosen: `rows` = "3,Linus,"two lines""
   ╰────
  help: txtql picked the first reading, which gives
        [{"id":"1","name":"Ada","note":"likes tea, and coffee"},
        {"id":"2","name":"Grace…; the other gives
        [{"id":"1","name":"Ada","note":"likes tea, and coffee"},
        {"id":"2","name":"Grace….
        In the chosen reading, `1 TO n ANY UNTILBEFORE '"'` runs across line
        breaks (ANY includes them).
        - To keep it on one line, add NL to its stop: `1 TO n ANY UNTILBEFORE
        ('"' OR NL)`
        - If it should cross lines, make the stop say where it ends, e.g.
        `UNTILBEFORE (NL '[' OR NL EOF)`.

txtql::input::ambiguous

  × ambiguous match in rule `several`: the text can be read in two ways with
  │ different results
   ╭─[input.txt:2:7]
 1 │ id,name,note
 2 │ 1,Ada,"likes tea, and coffee"
   ·       ───────────┬───────────┬
   ·                  │           ╰── alternative: `cells` = "\"likes tea"
   ·                  ╰── chosen: `cells` = "\"likes tea, and coffee\""
 3 │ 2,"Grace ""Amazing"" Hopper",
   ╰────
  help: txtql picked the first reading, which gives ["1","Ada","likes tea, and
        coffee"].
        However, another reading of this text exists but fails to evaluate:
        `ZIP` got 4 values for 3 keys.
        Make the pattern more specific (literals, SPLITBY, LAZY) if the other
        reading was intended.

txtql::input::ambiguous

  × ambiguous match in rule `cell`: the text can be read in two ways with
  │ different results
   ╭─[input.txt:3:3]
 2 │ 1,Ada,"likes tea, and coffee"
 3 │ 2,"Grace ""Amazing"" Hopper",
   ·   ─────────────┬────────────┬
   ·                │            ╰── alternative: `plain` = "\"Grace \"\"Amazing\"\" Hopper\""
   ·                ╰── chosen: `quoted` = "\"Grace \"\"Amazing\"\" Hopper\""
 4 │ 3,Linus,"two
   ╰────
  help: txtql picked the first reading, which gives "Grace \"\"Amazing\"\"
        Hopper"; the other gives "\"Grace \"\"Amazing\"\" Hopper\"".
        `quoted` was chosen because it comes first in the `OR`.
        - If `plain` is the special case, list it first.
        - If this order is intended, exclude the special case from `plain`
        with WHERE, e.g. `WHERE t != '…'`.
```

  The `WHERE` makes the two kinds of cell disjoint, and the ambiguity disappears.
- Every value is text, as in the file. A header with fewer or more columns than a record is handled by `ZIP`: missing
  values become `null` (the first example below; `NL` also accepts CRLF line breaks, so files from Windows work unchanged), and extra values are an error
  (the second).

**Query** (`query.tql`)

```txtql
TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(0 TO n ('""' OR run)) '"' AS v
run    = 1 TO n ANY UNTILBEFORE '"'
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) WHERE NOT v STARTSWITH '"' AS v
```

**Input** (`input.txt`, ends with a line break)

```text
id,name
1,Ada
2
```

**Output**

```json
[
  {
    "id": "1",
    "name": "Ada"
  },
  {
    "id": "2",
    "name": null
  }
]
```

**Query** (`query.tql`)

```txtql
TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(0 TO n ('""' OR run)) '"' AS v
run    = 1 TO n ANY UNTILBEFORE '"'
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) WHERE NOT v STARTSWITH '"' AS v
```

**Input** (`input.txt`, ends with a line break)

```text
id,name
1,Ada,extra
```

**Error** (standard error, exit code 1)

```text
txtql::eval::error

  × `ZIP` got 3 values for 2 keys
   ╭─[query.tql:1:72]
 1 │ TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
   ·                                                                        ───────┬──────
   ·                                                                               ╰── while evaluating this
 2 │ record = several OR alone
   ╰────
  help: every value needs a key; missing values become null, but extra ones
        are an error

Advice:
  ☞ in this part of the input
   ╭─[input.txt:1:1]
 1 │ ╭─▶ id,name
 2 │ ├─▶ 1,Ada,extra
   · ╰──── matched text
   ╰────
```

**Limitations, stated plainly.**

- A doubled quote inside a quoted field (`""`) is kept as written (`"Grace \"\"Amazing\"\" Hopper"` above): txtql has no
  function to replace text, so the escape cannot be undone inside the query. Post-process with `jq` (`gsub("\"\""; "\"")`)
  if you need it.
- Blank lines are not allowed: a record is never empty (see above), so a blank line, or a second line break at the end of the
  file, is an error rather than a row of empty values. The same goes for a one-column file with an empty value in a row, which
  is indistinguishable from a blank line. Strip blank lines first if the file may have them (`grep -v '^$'`).
- Fixed separators other than a comma work the same way: change the `','`.

**Backslash escapes.** Some exports write `\"` inside a quoted field instead of `""`. That is one more kind of piece: a
backslash followed by any character (`esc`), with `run` also stopping at a backslash. Only the quoted rule and `run` change:

    quoted = '"' v:(0 TO n (esc OR '""' OR run)) '"' AS v
    esc    = '\\' ANY
    run    = 1 TO n ANY UNTILBEFORE ('"' OR '\\')

It is still exact under `--strict`. A backslash is always an escape here, so `C:\temp` is read as the pair `\t` (the text is kept
as written, and a lone backslash needs no special case), and a field that ends in a backslash before its closing quote (`"x\"`)
is rejected, because that really is an escaped quote followed by a missing closing quote.

**Query** (`query.tql`)

```txtql
TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(0 TO n (esc OR '""' OR run)) '"' AS v
esc    = '\\' ANY
run    = 1 TO n ANY UNTILBEFORE ('"' OR '\\')
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) WHERE NOT v STARTSWITH '"' AS v
```

**Input** (`input.txt`, ends with a line break)

```text
id,note
1,"say \"hi\", then leave"
2,"C:\temp\\x"
```

**Command**

```
txtql --strict query.tql input.txt
```

**Output**

```json
[
  {
    "id": "1",
    "note": "say \\\"hi\\\", then leave"
  },
  {
    "id": "2",
    "note": "C:\\temp\\\\x"
  }
]
```

A file without a final line break, ending in an empty field, is read like any other:

**Query** (`query.tql`)

```txtql
TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(0 TO n ('""' OR run)) '"' AS v
run    = 1 TO n ANY UNTILBEFORE '"'
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) WHERE NOT v STARTSWITH '"' AS v
```

**Input** (`input.txt`)

```text
id,name
1,Ada
2,
```

**Output**

```json
[
  {
    "id": "1",
    "name": "Ada"
  },
  {
    "id": "2",
    "name": ""
  }
]
```

and a blank line is reported at the point where matching stopped:

**Query** (`query.tql`)

```txtql
TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(0 TO n ('""' OR run)) '"' AS v
run    = 1 TO n ANY UNTILBEFORE '"'
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) WHERE NOT v STARTSWITH '"' AS v
```

**Input** (`input.txt`, ends with a line break)

```text
id,name
1,Ada

2,Bo
```

**Error** (standard error, exit code 1)

```text
txtql::input::no_parse

  × the text does not match the query
   ╭─[input.txt:1:1]
 1 │ ╭─▶ id,name
 2 │ │   1,Ada
 3 │ │
 4 │ ├─▶ 2,Bo
   · ╰──── the text matches the patterns, but no reading satisfies the WHERE conditions
   ╰────
  help: check the WHERE clauses; a rule whose WHERE is false does not match
```

## 2. TSV and INI configuration to JSON

### Tab-separated values

**Problem.** A tab-separated export whose columns are known. Either keep everything as text (as in recipe 1), or give the
columns types.

**Sample input** (the separators are tab characters; the second line starts with a space).

```text
sku	qty	price
 A-100	3	4.50
B-7		12.00
```

**Generic query** (header names as keys, all values text):

```txtql
TEXT   = header:record 1 TO n rows:record AS [ ZIP(header, r) FOR r IN rows ]
record = 1 TO n cells:cell SPLITBY TAB NL AS cells
ALIAS cell = ANY UNTILBEFORE (TAB OR NL)
```

```json
[
  {
    "sku": " A-100",
    "qty": "3",
    "price": "4.50"
  },
  {
    "sku": "B-7",
    "qty": "",
    "price": "12.00"
  }
]
```

**Typed query** for a known layout: the header is matched as literal text, a number is read with `INT` or `FLOAT`, and an
optional number (`0 TO 1`) becomes `null` when the cell is empty (`NUM(null)` is `null`):

```txtql
TEXT = 'sku' TAB 'qty' TAB 'price' NL 1 TO n items:item AS items
item = sku:cell TAB 0 TO 1 qty:INT TAB price:FLOAT NL
       AS { 'sku': TRIM(sku), 'qty': NUM(qty), 'price': NUM(price) }
ALIAS cell = ANY UNTILBEFORE (TAB OR NL)
```

```json
[
  {
    "sku": "A-100",
    "qty": 3,
    "price": 4.5
  },
  {
    "sku": "B-7",
    "qty": null,
    "price": 12.0
  }
]
```

The alias `cell` is "anything up to the next tab or line break". Because an alias never captures, it is labelled
(`sku:cell`) where its text is wanted. `TRIM` removes the stray space in the first cell. Note that `12.0` stays `12.0`:
numbers keep the precision they are written with.

### INI-style files

**Problem.** A configuration file with comments, blank lines, top-level `key = value` pairs and `[section]` blocks.
Turn it into nested JSON, with `true` and `false` as real booleans.

**Sample input.**

```text
# global settings
name = txtql demo
debug = true

[server]
host = 0.0.0.0
port = 8080
timeout = 2.5   
; the TLS block is optional
tls = false

[database]
url = postgres://user:secret@localhost/app
pool = 10
```

**Query.**

```txtql
TEXT    = 0 TO n top:entry SKIPPING filler 0 TO n sections:section SKIPPING filler 0 TO n filler
          AS { top, sections }
section = '[' name:(ANY UNTILBEFORE ']') ']' NL 0 TO n pairs:entry SKIPPING filler
          AS { name: { pairs } }
entry   = key:(1 TO n keychar) 0 TO n (' ' OR TAB) '=' v:value NL AS { key: v }
value   = yes OR no OR str
yes     = t:(ANY UNTILBEFORE NL) WHERE LOWER(TRIM(t)) = 'true' AS true
no      = t:(ANY UNTILBEFORE NL) WHERE LOWER(TRIM(t)) = 'false' AS false
str     = t:(ANY UNTILBEFORE NL) WHERE LOWER(TRIM(t)) != 'true' AND LOWER(TRIM(t)) != 'false' AS TRIM(t)
ALIAS keychar = LETTER OR DIGIT OR '_' OR '.' OR '-'
ALIAS filler  = comment OR blank
ALIAS comment = (';' OR '#') ANY UNTIL NL
ALIAS blank   = 0 TO n (' ' OR TAB) NL
```

**Output.**

```json
{
  "name": "txtql demo",
  "debug": true,
  "server": {
    "host": "0.0.0.0",
    "port": "8080",
    "timeout": "2.5",
    "tls": false
  },
  "database": {
    "url": "postgres://user:secret@localhost/app",
    "pool": "10"
  }
}
```

**How it works.**

- `SKIPPING filler` lets comment lines and blank lines appear between (and around) the entries without the entry rule
  knowing about them. Skipped text is used only where nothing else fits, so it never steals a real entry.
- `entry` is `key = value` with any spacing around the `=`. Its template `{ key: v }` makes the captured key text the JSON key.
  A bare key in a template is a capture, a quoted key is literal text.
- The top-level pairs are labelled `top:entry`, one label on each item, and merged into the result by `{ top, sections }`:
  a key-less object entry merges every object of a list. Each section becomes `{ name: { pairs } }`, merged the same way.
  (A label in front of the whole repetition would capture its text, not its items.)
- `value` is `yes OR no OR str`. Each branch captures the text and decides with `WHERE` whether it applies, so exactly one
  does; `yes` and `no` produce `true` and `false`. `LOWER(TRIM(t))` makes the check ignore case and surrounding spaces: the
  captured value includes the space after the `=` in `debug = true`, and without the `TRIM` that value would be the text `" true"`, not a boolean.

**Limitation.** There is no "is this text a number?" test, so numbers (`port = 8080`) stay text. If the keys are known,
add an entry rule for them that reads the value with `INT` or `FLOAT` and converts it with `NUM`, and exclude those keys from the
general entry rule with `WHERE key != 'port'`. Quoted values, line continuations and inline `;` comments are not handled.

## 3. Web server access log

**Problem.** Parse an nginx/Apache "combined" format log into JSON with typed fields: numbers for status and size, `null` for
the `-` placeholders, and a status class (`2xx`, `4xx`, ...).

**Sample input.**

```text
203.0.113.9 - - [04/Oct/2026:08:15:02 +0000] "GET /index.html HTTP/1.1" 200 5123 "-" "Mozilla/5.0 (X11; Linux x86_64) Firefox/131.0"
198.51.100.23 - alice [04/Oct/2026:08:15:07 +0000] "POST /api/login HTTP/1.1" 302 0 "https://example.org/login" "curl/8.5.0"
203.0.113.9 - - [04/Oct/2026:08:15:09 +0000] "GET /missing.png HTTP/2.0" 404 153 "https://example.org/" "Mozilla/5.0 (X11; Linux x86_64) Firefox/131.0"
192.0.2.77 - - [04/Oct/2026:08:15:12 +0000] "GET /api/report?id=7&fmt=csv HTTP/1.1" 500 - "-" "python-requests/2.32"
```

**Query.**

```txtql
TEXT   = 1 TO n entry AS [ entry FOR entry ]
entry  = ip:IPV4 ' - ' user ' [' time:(ANY UNTIL ']') ' "'
         method:WORD ' ' path:(ANY UNTILBEFORE ' ') ' ' protocol:(ANY UNTILBEFORE '"') '" '
         st:status ' ' bytes:(dash OR size) ' "' referer '" "' agent:(ANY UNTILBEFORE '"') '"' NL
    AS { 'ip': ip, 'user': user, 'time': time, 'method': method, 'path': path,
         'protocol': protocol, 'status': st.code, 'class': st.class,
         'bytes': bytes, 'referer': referer, 'agent': agent }

-- "-" means "not present" in this format; turn it into null
user    = dash OR name
name    = t:(ANY UNTILBEFORE ' ') WHERE t != '-' AS t
referer = dash OR url
url     = t:(ANY UNTILBEFORE '"') WHERE t != '-' AS t
dash    = '-' AS null
size    = n:INT AS NUM(n)

-- one rule per status class; the WHERE decides which rule matches
status       = ok OR redirect OR client_error OR server_error
ok           = c:INT WHERE NUM(c) >= 200 AND NUM(c) < 300 AS { 'code': NUM(c), 'class': '2xx' }
redirect     = c:INT WHERE NUM(c) >= 300 AND NUM(c) < 400 AS { 'code': NUM(c), 'class': '3xx' }
client_error = c:INT WHERE NUM(c) >= 400 AND NUM(c) < 500 AS { 'code': NUM(c), 'class': '4xx' }
server_error = c:INT WHERE NUM(c) >= 500 AND NUM(c) < 600 AS { 'code': NUM(c), 'class': '5xx' }
```

**Output.**

```json
[
  {
    "ip": "203.0.113.9",
    "user": null,
    "time": "04/Oct/2026:08:15:02 +0000",
    "method": "GET",
    "path": "/index.html",
    "protocol": "HTTP/1.1",
    "status": 200,
    "class": "2xx",
    "bytes": 5123,
    "referer": null,
    "agent": "Mozilla/5.0 (X11; Linux x86_64) Firefox/131.0"
  },
  {
    "ip": "198.51.100.23",
    "user": "alice",
    "time": "04/Oct/2026:08:15:07 +0000",
    "method": "POST",
    "path": "/api/login",
    "protocol": "HTTP/1.1",
    "status": 302,
    "class": "3xx",
    "bytes": 0,
    "referer": "https://example.org/login",
    "agent": "curl/8.5.0"
  },
  {
    "ip": "203.0.113.9",
    "user": null,
    "time": "04/Oct/2026:08:15:09 +0000",
    "method": "GET",
    "path": "/missing.png",
    "protocol": "HTTP/2.0",
    "status": 404,
    "class": "4xx",
    "bytes": 153,
    "referer": "https://example.org/",
    "agent": "Mozilla/5.0 (X11; Linux x86_64) Firefox/131.0"
  },
  {
    "ip": "192.0.2.77",
    "user": null,
    "time": "04/Oct/2026:08:15:12 +0000",
    "method": "GET",
    "path": "/api/report?id=7&fmt=csv",
    "protocol": "HTTP/1.1",
    "status": 500,
    "class": "5xx",
    "bytes": null,
    "referer": null,
    "agent": "python-requests/2.32"
  }
]
```

**How it works.**

- `entry` follows the format left to right: address, user, `[time]`, the quoted request line, status, size, referrer, agent.
  `ANY UNTILBEFORE '"'` reads "everything up to the next quote" without consuming the quote.
- `user`, `referer` and `bytes` are `dash OR something`. The `-` branch gives `null`; the other branch excludes `'-'` with `WHERE`.
  That exclusion is what keeps the two branches from both matching a lone `-` (see
  [[Ambiguity and Strict Mode|Ambiguity-and-Strict-Mode]]).
- `status` has one rule per class. Each reads the digits and uses `WHERE NUM(c) >= 400 AND NUM(c) < 500` to accept only its
  range, so a status outside every range makes the line fail loudly instead of being passed through. The rule's object
  `{ 'code': ..., 'class': ... }` is reachable in `entry` as `st.code` and `st.class`.
- `size` is read with `INT` and converted with `NUM`, so `5123` becomes a JSON number.

**Selecting only some lines.** Describe the lines you want and skip the rest. `SKIPPING (LINE NL)` skips whole lines, used only
where the wanted pattern does not fit:

```txtql
-- keep only the 5xx lines; every other line is skipped whole
TEXT   = 0 TO n failed SKIPPING (LINE NL)
failed = ip:IPV4 ' - - [' ANY UNTIL '] "' method:WORD ' ' path:(ANY UNTILBEFORE ' ') ' ' ANY UNTIL '" '
         st:server ' ' ANY UNTIL NL
    AS { 'ip': ip, 'method': method, 'path': path, 'status': NUM(st) }
server = c:INT WHERE NUM(c) >= 500 AS c
```

```json
[
  {
    "ip": "192.0.2.77",
    "method": "GET",
    "path": "/api/report?id=7&fmt=csv",
    "status": 500
  }
]
```

Two pitfalls found while writing this recipe. `LINE` only matches at the start of a line, which is why the wanted
entry ends with `ANY UNTIL NL` instead. And a filter that never matches (a typo in the pattern, say) does not fail: every
line is skipped and you get `[]`. Test a filter against a line you know should match.

**Limitations.** The address is read as an `IPV4`; a log with host names or IPv6 addresses needs an `OR` there (`IPV6`
exists as a built-in; a host name can be `ANY UNTILBEFORE ' '`). The query expects exactly one format; lines in two different
formats need two rules joined by `OR` under `entry`.

## 4. Application log with stack traces

**Problem.** A log where some entries continue over several lines (an exception and its stack). Produce one JSON object per
entry, with the continuation lines grouped in a `trace` list.

**Sample input** (tab characters in front of the `at` lines).

```text
2026-10-04T08:00:01.120Z INFO [main] com.acme.Server - listening on :8080
2026-10-04T08:00:03.455Z ERROR [worker-3] com.acme.Billing - payment failed for order 1042
java.lang.IllegalStateException: card declined
	at com.acme.Billing.charge(Billing.java:88)
	at com.acme.Worker.run(Worker.java:31)
Caused by: java.net.SocketTimeoutException: read timed out
	at java.base/java.net.SocketInputStream.read(SocketInputStream.java:163)
2026-10-04T08:00:04.002Z WARN [worker-1] com.acme.Cache - eviction storm: 512 keys
```

**Query.**

```txtql
TEXT   = 1 TO n entry AS [ entry FOR entry ]
entry  = time:(INT '-' INT '-' INT 'T' INT ':' INT ':' FLOAT 'Z') ' ' level:level ' [' thread:(ANY UNTILBEFORE ']') '] '
         logger:(ANY UNTILBEFORE ' - ') ' - ' msg:(ANY UNTILBEFORE NL) NL 0 TO n trace:continuation
    AS { 'time': time, 'level': level, 'thread': thread,
         'logger': logger, 'message': msg, 'trace': trace }
level  = 'DEBUG' OR 'INFO' OR 'WARN' OR 'ERROR'
-- anything that does not start with a digit (the next timestamp) belongs to the entry
continuation = l:((LETTER OR ' ' OR TAB) ANY UNTILBEFORE NL) NL AS TRIM(l)
```

**Output.**

```json
[
  {
    "time": "2026-10-04T08:00:01.120Z",
    "level": "INFO",
    "thread": "main",
    "logger": "com.acme.Server",
    "message": "listening on :8080",
    "trace": []
  },
  {
    "time": "2026-10-04T08:00:03.455Z",
    "level": "ERROR",
    "thread": "worker-3",
    "logger": "com.acme.Billing",
    "message": "payment failed for order 1042",
    "trace": [
      "java.lang.IllegalStateException: card declined",
      "at com.acme.Billing.charge(Billing.java:88)",
      "at com.acme.Worker.run(Worker.java:31)",
      "Caused by: java.net.SocketTimeoutException: read timed out",
      "at java.base/java.net.SocketInputStream.read(SocketInputStream.java:163)"
    ]
  },
  {
    "time": "2026-10-04T08:00:04.002Z",
    "level": "WARN",
    "thread": "worker-1",
    "logger": "com.acme.Cache",
    "message": "eviction storm: 512 keys",
    "trace": []
  }
]
```

**How it works.**

- `entry` reads the first line field by field, then `0 TO n trace:continuation`: zero or more following lines that belong to
  it. An entry without a stack gets an empty `trace` list.
- The rule for where an entry ends is the one design decision. Here: a line that starts with a letter, a space or a tab is a
  continuation; a line starting with a digit is the next entry (it begins with the year). That works for Java, Python and most
  runtimes, whose stack lines are indented or start with an exception name.
- `ANY UNTILBEFORE NL` reads to the end of a line, leaves the line break to the next part of the rule, and so
  can be used for messages of any shape.
- `AS TRIM(l)` strips the indentation of each continuation line.

**Limitations.** A continuation line that starts with a digit or a bracket would be taken for a new entry and fail to
parse. If your format differs, change `continuation` (for example to lines starting with `'\tat '` or `'Caused by:'`); a rule can also
exclude lines with `WHERE`. The same pattern works for syslog lines (`Oct  4 08:00:01 host sshd[1234]: message`): swap the
first-line part and keep the `trace` part.

## 5. A Markdown changelog

**Problem.** Turn a "keep a changelog" style Markdown file into JSON: one object per release, with the bullet lists grouped
under their headings (`Added`, `Fixed`, ...).

**Sample input.**

```text
# Changelog

## [1.2.0] - 2026-09-30

### Added
- Support for `UNTILBEFORE` stops
- A language server

### Fixed
- Crash on empty input

## [1.1.0] - 2026-08-01

### Changed
- Error messages now point at the failing rule
```

**Query.**

```txtql
TEXT    = '# Changelog' NL gap 1 TO n releases:release
          AS [ { 'version': r.version, 'date': r.date,
                 'changes': { s.heading: s.items FOR s IN r.sections } } FOR r IN releases ]
release = '## [' version:(ANY UNTILBEFORE ']') '] - ' date:(INT '-' INT '-' INT) NL gap
          1 TO n sections:section
section = '### ' heading:(ANY UNTILBEFORE NL) NL 1 TO n items:item gap
item    = '- ' t:(ANY UNTILBEFORE NL) NL AS t
ALIAS gap = 0 TO n NL
```

**Output.**

```json
[
  {
    "version": "1.2.0",
    "date": "2026-09-30",
    "changes": {
      "Added": [
        "Support for `UNTILBEFORE` stops",
        "A language server"
      ],
      "Fixed": [
        "Crash on empty input"
      ]
    }
  },
  {
    "version": "1.1.0",
    "date": "2026-08-01",
    "changes": {
      "Changed": [
        "Error messages now point at the failing rule"
      ]
    }
  }
]
```

**How it works.**

- The structure is a fixed nesting: releases contain sections, sections contain items. Each level is a rule, and each
  level's list is captured by labelling the *items* (`releases:release`, `sections:section`, `items:item`).
- `ALIAS gap = 0 TO n NL` absorbs blank lines wherever they may appear without adding anything to the output.
- The template of `TEXT` does the reshaping. `{ s.heading: s.items FOR s IN r.sections }` builds an object whose keys are the
  heading texts, so the set of headings does not have to be known in advance. Fields of a captured object are reached with
  `r.sections`, `s.heading`.
- A heading that appears twice in one release would be a repeated key (a warning, an error with `--strict`);
  `LISTOF` collects such duplicates, see the Language Reference.

**Limitation.** This reads a fixed shape. Nested bullet lists (indentation as structure) are not supported by the language:
txtql has no indentation-sensitive matching, see [[FAQ and Known Limitations|FAQ-and-Known-Limitations]].

## 6. An order-confirmation email

**Problem.** Extract structured data from a semi-structured plain-text email: the sender, the order number, the customer's
name, the line items with quantity and price, the total and the shipping address.

**Sample input.**

```text
From: Shop <orders@shop.example>
To: ada@example.org
Subject: Your order #1042
Date: Sat, 04 Oct 2026 09:12:00 +0000

Hi Ada,

Thanks for your order. Summary:

  2 x Widget      @  4.50
  1 x Gadget Pro  @ 12.00

Total: 21.00 EUR
Ship to: 12 Rue Example, Paris
```

**Query.**

```txtql
TEXT     = headers NL 'Hi ' customer:WORD ',' NL NL 'Thanks for your order. Summary:' NL NL
           1 TO n items:item NL total NL 'Ship to: ' ship_to:(ANY UNTILBEFORE NL) NL
           AS { headers, 'customer': customer, 'items': items, 'total': total, 'ship_to': ship_to }
headers  = 'From: ' ANY UNTILBEFORE ' <' ' <' sender:(ANY UNTILBEFORE '>') '>' NL
           'To: ' recipient:(ANY UNTILBEFORE NL) NL
           'Subject: Your order #' order:INT NL
           0 TO n (WORD ': ' ANY UNTILBEFORE NL NL)
           AS { 'from': sender, 'to': recipient, 'order': NUM(order) }
item     = 1 TO n ' ' qty:INT ' x ' product:(ANY UNTILBEFORE (1 TO n ' ' '@')) 1 TO n ' ' '@' 1 TO n ' ' price:FLOAT NL
           AS { 'product': product, 'qty': NUM(qty), 'price': NUM(price) }
total    = 'Total: ' amount:FLOAT ' ' currency:WORD AS { 'amount': NUM(amount), 'currency': currency }
```

**Output.**

```json
{
  "from": "orders@shop.example",
  "to": "ada@example.org",
  "order": 1042,
  "customer": "Ada",
  "items": [
    {
      "product": "Widget",
      "qty": 2,
      "price": 4.5
    },
    {
      "product": "Gadget Pro",
      "qty": 1,
      "price": 12.0
    }
  ],
  "total": {
    "amount": 21.0,
    "currency": "EUR"
  },
  "ship_to": "12 Rue Example, Paris"
}
```

**How it works.**

- The query reads the message top to bottom, the same way you would. Literal text (`'Thanks for your order. Summary:'`) pins
  down the parts that never change, so a different email fails to parse instead of producing nonsense.
- `headers` is its own rule with its own object: it reads the three headers it needs and skips the others
  (`0 TO n (WORD ': ' ANY UNTILBEFORE NL NL)`, here `Date`). `TEXT` merges that object into the result with the key-less entry `headers`.
  The other entries have explicit keys, because a key-less entry would try to merge, and only objects can be merged.
- In `item`, the padding between columns is `1 TO n ' '`, and the product name is `ANY UNTILBEFORE (1 TO n ' ' '@')`: everything
  up to the spaces before the `@`. That keeps `Gadget Pro` together and drops the alignment spaces.
- `total` is a rule with its own object, so `TEXT` can embed it as `'total': total`.

**Limitation.** An email is written by humans and templates, and txtql is exact: an extra blank line or a changed phrase makes the
text fail to match. That is useful for machine-generated mail, and a poor fit for free-form messages.

## 7. Running it in a pipeline

**Problem.** Use txtql in a shell pipeline, and make a CI job fail when a query or its sample input stops working.

**Standard input and `jq`.** With no input file txtql reads standard input, and standard output carries only the JSON, so it
composes with `jq` like any other filter. `-e` gives the query inline:

**Command**

```sh
printf '4096\t./src\n12\t./docs\n' | txtql --compact -e "TEXT  = 1 TO n entry
entry = kb:INT TAB path:(ANY UNTILBEFORE NL) NL AS { 'kb': NUM(kb), 'path': path }"
```

**Output**

```json
[{"kb":4096,"path":"./src"},{"kb":12,"path":"./docs"}]
```

For example, `du -k | txtql -e '...' | jq 'sort_by(-.kb) | .[0:10]'` lists the ten biggest directories, and
`txtql access.tql access.log | jq '[.[] | select(.class == "5xx")] | length'` counts the server errors. Diagnostics go to standard error,
so they do not corrupt the JSON that `jq` receives. A failed match prints nothing on standard output and exits with `1`, which
stops a pipeline run with `set -o pipefail`.

**`txtql check` in CI.** `txtql check QUERY SAMPLE` verifies the query and, given a sample, lists output-changing ambiguities.
Exit code `0` means all is well, `2` means an ambiguity or repeated key was found, `1` is any other error (a syntax error, or a
sample that does not match). Keep a small sample file next to each query, and check them together:

**Query** (`query.tql`)

```txtql
TEXT    = '# Changelog' NL gap 1 TO n releases:release
          AS [ { 'version': r.version, 'date': r.date,
                 'changes': { s.heading: s.items FOR s IN r.sections } } FOR r IN releases ]
release = '## [' version:(ANY UNTILBEFORE ']') '] - ' date:(INT '-' INT '-' INT) NL gap
          1 TO n sections:section
section = '### ' heading:(ANY UNTILBEFORE NL) NL 1 TO n items:item gap
item    = '- ' t:(ANY UNTILBEFORE NL) NL AS t
ALIAS gap = 0 TO n NL
```

**Input** (`input.txt`, ends with a line break)

```text
# Changelog

## [1.2.0] - 2026-09-30

### Added
- Support for `UNTILBEFORE` stops
- A language server

### Fixed
- Crash on empty input

## [1.1.0] - 2026-08-01

### Changed
- Error messages now point at the failing rule
```

**Command**

```
txtql check query.tql input.txt
```

**Output** (standard error, exit code 0)

```text
query OK; no output-changing ambiguity on this input
```

The same command on an ambiguous query exits with `2` (see the naive CSV query in recipe 1). For the production runs
use `--strict`: ambiguity and repeated keys then stop the run with exit code `1` instead of printing a warning, so
a drifting input format fails the job instead of silently changing the output.

```
# .github/workflows or any CI script
set -euo pipefail
txtql check queries/access.tql samples/access.log   # exit 2 on ambiguity, 1 on any error
txtql --strict queries/access.tql /var/log/nginx/access.log > access.json
```

**Tips.**

- `--compact` prints one-line JSON, convenient for line-oriented tools.
- `--max-steps` bounds the work on untrusted or very large input (see [[Performance and Limits|Performance-and-Limits]]).
- Keep queries in version control, and the samples with them: `txtql check` on the sample is a cheap regression test.

## Findings from writing these recipes

Things that were awkward, and where the language draws its line:

- Quote-doubling in CSV is expressed exactly (`0 TO n ('""' OR run)`, recipe 1). The one gap is the output: there is no text
  replacement, so `""` inside a quoted CSV field cannot be turned into `"` by the query (use `jq`).
- No numeric test: text cannot be classified as "a number" in a `WHERE`, so typing a value as number or string needs a
  separate rule per known field.
- `LINE` is only valid at the start of a line; inside a line use `ANY UNTILBEFORE NL`.
- A filtering query (`SKIPPING` over unwanted lines) that matches nothing returns `[]` rather than an error.
- Making the final line break optional (`(NL OR EOF)` ending a record) is rejected as an empty repetition when a record can be empty. Make
  sure a record cannot be empty (recipe 1: two or more cells, or one non-empty cell) and `NL` between records plus `0 TO 1 NL` at the end does it.
- A rule named like a keyword (`row`, `line`, ...) is rejected, so pick names such as `record` and `failed`.
- A label in front of a repetition captures the matched text, not a list: label the items to get a list of values.
