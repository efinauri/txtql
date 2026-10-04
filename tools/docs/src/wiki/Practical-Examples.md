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

{{i pe_csv}}

**Query.**

{{q pe_csv}}

**Output.**

{{o pe_csv}}

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

{{ex pe_csv_q1}}

  right before a line break inside a quoted field,

{{ex pe_csv_q2}}

  and a field made only of quotes (`""""` is one doubled quote; `""` is the empty field):

{{ex pe_csv_q3}}

- A `plain` cell stops at the next comma or line break. Without the `WHERE NOT v STARTSWITH '"'` it could also read the start
  of a quoted cell (`"likes tea`) as a plain cell, a second reading of the same text. txtql notices:
  `txtql check` on the version without the `WHERE` reports it,

{{ex pe_csv_naive}}

  and with `--strict` the same query is refused:

{{ex pe_csv_naive_strict}}

  The `WHERE` makes the two kinds of cell disjoint, and the ambiguity disappears.
- Every value is text, as in the file. A header with fewer or more columns than a record is handled by `ZIP`: missing
  values become `null` (the first example below; `NL` also accepts CRLF line breaks, so files from Windows work unchanged), and extra values are an error
  (the second).

{{ex pe_csv_short}}

{{ex pe_csv_wide}}

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

{{ex pe_csv_bs}}

A file without a final line break, ending in an empty field, is read like any other:

{{ex pe_csv_nonl}}

and a blank line is reported at the point where matching stopped:

{{ex pe_csv_blank}}

## 2. TSV and INI configuration to JSON

### Tab-separated values

**Problem.** A tab-separated export whose columns are known. Either keep everything as text (as in recipe 1), or give the
columns types.

**Sample input** (the separators are tab characters; the second line starts with a space).

{{i pe_tsv}}

**Generic query** (header names as keys, all values text):

{{q pe_tsv}}

{{o pe_tsv}}

**Typed query** for a known layout: the header is matched as literal text, a number is read with `INT` or `FLOAT`, and an
optional number (`0 TO 1`) becomes `null` when the cell is empty (`NUM(null)` is `null`):

{{q pe_tsv_typed}}

{{o pe_tsv_typed}}

The alias `cell` is "anything up to the next tab or line break". Because an alias never captures, it is labelled
(`sku:cell`) where its text is wanted. `TRIM` removes the stray space in the first cell. Note that `12.0` stays `12.0`:
numbers keep the precision they are written with.

### INI-style files

**Problem.** A configuration file with comments, blank lines, top-level `key = value` pairs and `[section]` blocks.
Turn it into nested JSON, with `true` and `false` as real booleans.

**Sample input.**

{{i pe_ini}}

**Query.**

{{q pe_ini}}

**Output.**

{{o pe_ini}}

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

{{i pe_acc}}

**Query.**

{{q pe_acc}}

**Output.**

{{o pe_acc}}

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

{{q pe_errs}}

{{o pe_errs}}

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

{{i pe_app}}

**Query.**

{{q pe_app}}

**Output.**

{{o pe_app}}

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

{{i pe_chg}}

**Query.**

{{q pe_chg}}

**Output.**

{{o pe_chg}}

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

{{i pe_mail}}

**Query.**

{{q pe_mail}}

**Output.**

{{o pe_mail}}

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

{{ex pe_pipe}}

For example, `du -k | txtql -e '...' | jq 'sort_by(-.kb) | .[0:10]'` lists the ten biggest directories, and
`txtql access.tql access.log | jq '[.[] | select(.class == "5xx")] | length'` counts the server errors. Diagnostics go to standard error,
so they do not corrupt the JSON that `jq` receives. A failed match prints nothing on standard output and exits with `1`, which
stops a pipeline run with `set -o pipefail`.

**`txtql check` in CI.** `txtql check QUERY SAMPLE` verifies the query and, given a sample, lists output-changing ambiguities.
Exit code `0` means all is well, `2` means an ambiguity or repeated key was found, `1` is any other error (a syntax error, or a
sample that does not match). Keep a small sample file next to each query, and check them together:

{{ex pe_ci_ok}}

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
