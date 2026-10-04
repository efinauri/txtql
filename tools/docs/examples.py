EXAMPLES = {}
def ex(name, q, i, **kw):
    assert name not in EXAMPLES, name
    EXAMPLES[name] = dict(q=q, i=i, **kw)

# ---- walkthrough 1: rules and TEXT
ex('w1_hello', "TEXT = 'hello ' name:WORD\n", 'hello world')
ex('w1_nomatch', "TEXT = 'hello ' name:WORD\n", 'hello 42', expect=1)
ex('w1_nl_missing', "TEXT = 'hi'\n", 'hi\n', expect=1)
ex('w1_nl', "TEXT = 'hi' NL\n", 'hi\n')
ex('w1_rules', "TEXT  = greeting ' ' name:WORD\ngreeting = 'hello' OR 'hi'\n", 'hi there')

# ---- walkthrough 2: literals and built-in patterns
ex('w2_prims', "TEXT = host:IPV4 ' ' flags:('0x' HEX) ' ' ratio:FLOAT ' ' id:INT '-' tag:WORD\n", '10.0.0.7 0xCAFE 3.14 42-beta')
ex('w2_icase', "TEXT = i'select ' cols:WORD\n", 'SeLeCt name')
ex('w2_escapes', "TEXT = 'a\\tb' NL\n", 'a\tb\n')
ex('w2_single', "TEXT = 1 TO n DIGIT\n", '987')
ex('w2_punct', "TEXT = a:WORD p:PUNCT b:WORD\n", 'foo,bar')
ex('w2_line', "TEXT = 1 TO n LINE SPLITBY NL\n", 'first\n\nthird')
ex('w2_rowcol', "TEXT = 1 TO n item SPLITBY NL\nitem = 0 TO n ' ' y:ROW x:COL w:WORD\n", 'alpha\n  beta')
ex('w2_rowcol2', "TEXT = 'x ' r:ROW c:COL\n", 'x ')
ex('w2_eof', "TEXT = w:WORD EOF\n", 'done')

# ---- walkthrough 3: sequences and OR
ex('w3_or', "TEXT = animal:('cat' OR 'dog') ' says ' sound:('meow' OR 'woof')\n", 'dog says woof')
ex('w3_or_rules', "TEXT   = 'value: ' v:value\nvalue  = number OR name\nnumber = n:INT AS NUM(n)\nname   = t:WORD AS t\n", 'value: 42')
ex('w3_or_rules2', "TEXT   = 'value: ' v:value\nvalue  = number OR name\nnumber = n:INT AS NUM(n)\nname   = t:WORD AS t\n", 'value: forty')

# ---- walkthrough 4: repetition
ex('w4_bounds', "TEXT = code:(2 TO 3 LETTER) '-' num:(1 TO n DIGIT)\n", 'ab-12345')
ex('w4_optional', "TEXT = name:WORD 0 TO 1 (' ' suffix:WORD)\n", 'Ada')
ex('w4_optional2', "TEXT = name:WORD 0 TO 1 (' ' suffix:WORD)\n", 'Ada Lovelace')
ex('w4_splitby', "TEXT = 1 TO n n:INT SPLITBY ', '\n", '1, 2, 3')
ex('w4_splitby_text', "TEXT = 1 TO n n:INT SPLITBY ', ' ' total'\n", '1, 2, 3 total')
ex('w4_skipping', "TEXT = 1 TO n n:INT SKIPPING ANY\n", 'We sold 3 apples, 12 pears and 7 plums.')
ex('w4_skipping2', "TEXT = 1 TO n n:INT SKIPPING (1 TO n (' ' OR PUNCT OR WORD))\n", 'We sold 3 apples, 12 pears and 7 plums.')
ex('w4_until', "TEXT = key:(ANY UNTIL '=') value:(ANY UNTIL NL)\n", 'colour=red\n')
ex('w4_untilbefore', "TEXT = key:(ANY UNTILBEFORE '=') '=' value:(ANY UNTILBEFORE NL) NL\n", 'colour=red\n')
ex('w4_until_inner', "TEXT = key:ANY UNTILBEFORE '=' '=' value:WORD\n", 'key=v')
ex('w4_lazy', "TEXT = 'a' mid:(1 TO n LAZY ANY) 'z' rest:(0 TO n ANY)\n", 'a-b-z-c-z')
ex('w4_empty_rep', "TEXT = 1 TO n LINE\n", 'a\nb', expect=1)
ex('r11_text_dup', "TEXT = text\ntext = 'x'\n", 'x', expect=1)
ex('w4_star', "TEXT = 1 TO n WORD*\n", 'a', expect=1)

# ---- walkthrough 5: captures and default values
ex('c5_obj', "TEXT = noun:WORD ' are ' adj:WORD\n", 'roses are red')
ex('c5_ruleref', "TEXT = who ' is ' age\nwho  = WORD\nage  = INT\n", 'Ada is 36')
ex('c5_list_default', "TEXT = 1 TO n WORD SPLITBY ' '\n", 'a b c')
ex('c5_text_default', "TEXT = 'id-' INT\n", 'id-42')
ex('c5_rep_capture', "TEXT = 'nums: ' 1 TO n n:INT SPLITBY ','\n", 'nums: 1,2,3')
ex('c5_rep_capture1', "TEXT = 'nums: ' 1 TO n n:INT SPLITBY ','\n", 'nums: 7')
ex('c5_label_rep', "TEXT = ws:(1 TO n WORD SPLITBY ', ')\n", 'a, b')
ex('c5_label_item', "TEXT = 1 TO n ws:WORD SPLITBY ', '\n", 'a, b')
ex('c5_or_null', "TEXT = (num:INT OR name:WORD) AS { 'num': num, 'name': name }\n", 'abc')
ex('c5_or_default', "TEXT = 1 TO n item SPLITBY ' '\nitem = num:INT OR name:WORD\n", '12 abc')
ex('c5_dupcap', "TEXT = sp WORD sp\nsp = ' '\n", ' a ', expect=1)

# ---- walkthrough 6: aliases
ex('a6_alias', "TEXT = a:WORD sp b:WORD sp c:WORD\nALIAS sp = 1 TO n ' '\n", 'x  y z')
ex('a6_alias_label', "TEXT = key:WORD ': ' value:rest NL\nALIAS rest = ANY UNTILBEFORE NL\n", 'host: example.org\n')
ex('a6_label_in_alias', "TEXT = a:pair\nALIAS pair = k:WORD '=' v:WORD\n", 'a=b', expect=1)

# ---- walkthrough 7: templates
ex('t7_obj', "TEXT = name:WORD ' ' age:INT AS { 'name': name, 'age': NUM(age) }\n", 'Ada 36')
ex('t7_keys', "TEXT = k:WORD '=' v:WORD AS { k: v }\n", 'colour=red')
ex('t7_list', "TEXT = name:WORD ' ' age:INT AS [ name, NUM(age), true, null, 'x', -2.5 ]\n", 'Ada 36')
ex('t7_for2', "TEXT   = 1 TO n people:person AS [ name FOR people ]\nperson = name:WORD ' ' age:INT NL AS { 'name': name, 'age': NUM(age) }\n", 'Ada 36\nBob 41\n')
ex('t7_for_in', "TEXT = 1 TO n n:INT SPLITBY ',' AS [ NUM(x) FOR x IN n ]\n", '1,2,3')
ex('t7_for_obj', "TEXT = 1 TO n pair SPLITBY NL\npair = k:WORD '=' v:INT AS { k: NUM(v) }\n", 'a=1\nb=2')
ex('t7_for_obj2', "TEXT = 1 TO n pairs:pair SPLITBY NL AS { p.k: NUM(p.v) FOR p IN pairs }\npair = k:WORD '=' v:INT\n", 'a=1\nb=2')
ex('t7_listof', "TEXT = 1 TO n es:entry SPLITBY NL AS { e.team: LISTOF e.name FOR e IN es }\nentry = team:WORD ': ' name:WORD\n", 'red: ann\nblue: bob\nred: cy')
ex('t7_repeated', "TEXT = 1 TO n es:entry SPLITBY NL AS { e.team: e.name FOR e IN es }\nentry = team:WORD ': ' name:WORD\n", 'red: ann\nblue: bob\nred: cy')
ex('t7_merge', "TEXT = user:WORD ' ' 1 TO n opts:opt SPLITBY ' ' AS { 'user': user, opts }\nopt = k:WORD '=' v:WORD AS { k: v }\n", 'ada bold=yes size=big')
ex('t7_funcs', "TEXT = name:(ANY UNTIL ',') 1 TO n n:INT SPLITBY '+'\n  AS { 'name': UPPER(TRIM(name)), 'lower': LOWER(name), 'count': COUNT(n),\n       'first': NUM(FIRST(n)), 'last': NUM(LAST(n)), 'joined': JOIN(n, '-') }\n", '  Ada ,1+2+3')
ex('t7_join_nosep', "TEXT = 1 TO n n:INT SPLITBY ',' AS JOIN(n)\n", '1,2', expect=1)
ex('t7_zip', "TEXT = header:record 1 TO n rows:record AS [ ZIP(header, r) FOR r IN rows ]\nrecord = 1 TO n cells:cell SPLITBY ',' NL AS cells\nALIAS cell = ANY UNTILBEFORE (',' OR NL)\n", 'name,city\nAda,London\nBob,Paris\n')
ex('t7_numerr', "TEXT = n:WORD AS NUM(n)\n", 'abc', expect=1)

# ---- walkthrough 8: WHERE
ex('x8_filter', "TEXT = 1 TO n big SKIPPING ANY\nbig  = n:FLOAT WHERE NUM(n) > 10 AS NUM(n)\n", 'We sold 3 apples, 12 pears, 7 plums and 40 cherries in 2 days.')
ex('x8_ops', "TEXT  = 1 TO n entry SPLITBY NL\nentry = name:WORD ' ' score:INT\n    WHERE NUM(score) >= 50 AND NOT name STARTSWITH 'x' OR name = 'root'\n", 'ann 80\nroot 1')
ex('x8_ops_fail', "TEXT  = 1 TO n entry SPLITBY NL\nentry = name:WORD ' ' score:INT\n    WHERE NUM(score) >= 50 AND NOT name STARTSWITH 'x' OR name = 'root'\n", 'ann 80\nxena 90', expect=1)
ex('x8_text_eq', "TEXT = a:FLOAT ' ' b:FLOAT WHERE a = b\n", '1.0 1', expect=1)
ex('x8_num_eq', "TEXT = a:FLOAT ' ' b:FLOAT WHERE NUM(a) = NUM(b)\n", '1.0 1')
ex('x8_pick', "TEXT    = 1 TO n entry AS { entry }\nentry   = name:WORD ': ' city:(unknown OR place) NL AS { name: city }\nunknown = '-' AS null\nplace   = t:(ANY UNTILBEFORE NL) WHERE t != '-' AS t\n", 'ada: London\nbob: -\n')

# ---- walkthrough 9: structural matching
ex('s9_sexpr', "TEXT  = sexpr NL AS sexpr\nsexpr = '(' 0 TO n items:(atom OR sexpr) SPLITBY ' ' ')' AS items\natom  = WORD OR FLOAT OR '*' OR '+'\n", '(define (square x) (* x x))\n')
ex('s9_lists', "TEXT = list\nlist = '[' 0 TO n items:(num OR list) SPLITBY ', ' ']' AS items\nnum  = n:INT AS NUM(n)\n", '[1, [2, [3, []]], 4]')
ex('s9_unbalanced', "TEXT = list\nlist = '[' 0 TO n items:(num OR list) SPLITBY ', ' ']' AS items\nnum  = n:INT AS NUM(n)\n", '[1, [2, 3]', expect=1)
ex('s9_tree', "TEXT = node\nnode = name:WORD 0 TO 1 ('(' 1 TO n kids:node SPLITBY ',' ')') AS { name: kids }\n", 'root(a,b(c,d),e)')

# ---- walkthrough 10: ambiguity
ex('m10_split', "TEXT = first:(1 TO n WORD SPLITBY ' ') ' ' rest:(1 TO n WORD SPLITBY ' ') NL\n", 'one two three\n')
ex('m10_strict', "TEXT = first:(1 TO n WORD SPLITBY ' ') ' ' rest:(1 TO n WORD SPLITBY ' ') NL\n", 'one two three\n', flags=['--strict'], expect=1)
ex('m10_check', "TEXT = first:(1 TO n WORD SPLITBY ' ') ' ' rest:(1 TO n WORD SPLITBY ' ') NL\n", 'one two three\n', mode='check', expect=2)
ex('m10_fixed', "TEXT = first:(1 TO n WORD SPLITBY ' ') ' | ' rest:(1 TO n WORD SPLITBY ' ') NL\n", 'one two | three\n', mode='check')
ex('m10_lazy', "TEXT = first:(1 TO n LAZY WORD SPLITBY ' ') ' ' rest:(1 TO n WORD SPLITBY ' ') NL\n", 'one two three\n')
ex('m10_strict_rule', "STRICT TEXT = first:(1 TO n WORD SPLITBY ' ') ' ' rest:(1 TO n WORD SPLITBY ' ') NL\n", 'one two three\n', expect=1)
ex('m10_greedy', "TEXT = 'a' mid:(1 TO n ANY) 'z' rest:(0 TO n ANY)\n", 'a-b-z-c-z')
ex('m10_branch', "TEXT    = 1 TO n entry AS { entry }\nentry   = name:WORD ': ' city:(unknown OR place) NL AS { name: city }\nunknown = '-' AS null\nplace   = t:(ANY UNTILBEFORE NL) AS t\n", 'ada: London\nbob: -\n')
ex('m10_harmless', "TEXT = 1 TO n (a OR b) SPLITBY ' '\na = 'x'\nb = 'x'\n", 'x x', mode='check')

# ---- walkthrough 11: reserved
ex('r11_reserved', "TEXT = null\nnull = 'x'\n", 'x', expect=1)
ex('r11_case', "text = w:word -- the root may be spelled text, keywords in any case\n", 'x')
ex('r11_keyword_label', "TEXT = word:WORD\n", 'x', expect=1)

# ---- error catalogue: code -> (meaning, query, input, flags, mode)
# Each is run; the code must appear in the diagnostics.
CODES = {}
def code(c, meaning, q, i='x', flags=(), mode='run', shown=None, binput=None):
    CODES[c] = dict(meaning=meaning, q=q, i=i, flags=list(flags), mode=mode, binput=binput, shown=shown)
code('txtql::parse::unexpected_char', 'A character that is not part of the language (for example `|`, `+` or `*` where `OR` or `a TO b` is meant).', "TEXT = 'a' | 'b'")
code('txtql::parse::unterminated_string', 'A string literal without its closing quote.', "TEXT = 'abc")
code('txtql::parse::bad_escape', 'An unknown escape in a string; valid ones are `\\n \\t \\r \\\\ \\\' \\"`.', "TEXT = 'a\\q'")
code('txtql::parse::bad_number', 'A number literal that does not fit (for example `99999999999999999999999`).', "TEXT = 99999999999999999999999 WORD")
code('txtql::parse::unexpected_token', 'A token the grammar does not allow at that point (a keyword used as a name, a missing `)`, ...).', "TEXT = WORD )")
code('txtql::parse::unexpected_end', 'The query ends in the middle of a construct.', "TEXT =")
code('txtql::parse::too_deep', 'Parentheses, repetitions, templates or conditions nested more than 100 levels deep.', "TEXT = " + "(" * 101 + "WORD" + ")" * 101, shown='`TEXT = (((` ... `WORD` ... `)))` with 101 pairs of parentheses')
code('txtql::check::missing_root', 'There is no `TEXT` rule.', "rule1 = WORD")
code('txtql::check::duplicate_rule', 'A name is defined twice (rules and aliases share one set of names; `text` and `TEXT` are the same name).', "TEXT = WORD\nTEXT = INT")
code('txtql::check::reserved_name', '`true`, `false` or `null` (in any case) used as a rule, alias, label or loop variable name.', "TEXT = true\ntrue = 'x'")
code('txtql::check::root_is_alias', '`TEXT` was defined with `ALIAS`.', "ALIAS TEXT = WORD")
code('txtql::check::alias_cycle', 'An alias refers to itself, directly or through other aliases.', "TEXT = a\nALIAS a = 'x' a")
code('txtql::check::label_in_alias', 'A label inside an alias; aliases never capture.', "TEXT = a\nALIAS a = k:WORD")
code('txtql::check::undefined_rule', 'A pattern refers to a rule or alias that does not exist (with a "did you mean" hint).', "TEXT = wrd")
code('txtql::check::duplicate_capture', 'The same capture name twice in one match (give one a label).', "TEXT = a a\na = WORD")
code('txtql::check::unknown_capture', 'A template or `WHERE` uses a name that is not captured by that rule.', "TEXT = a:WORD AS b")
code('txtql::check::unknown_field', 'A path `x.field` asks for a field that the rule that produced `x` never sets.', "TEXT = p AS p.nope\np = a:WORD ' ' b:WORD")
code('txtql::check::not_repeated', '`FOR` over something that is not a list.', "TEXT = a:WORD AS [ a FOR a ]")
code('txtql::check::not_mergeable', 'A key-less object entry whose value is text, not an object.', "TEXT = a:WORD AS { a }")
code('txtql::check::duplicate_branch', 'An `OR` branch identical to an earlier one, so it can never be chosen.', "TEXT = 'a' OR 'a'")
code('txtql::check::empty_loop', 'A repetition whose item can match empty text.', "TEXT = 1 TO n LINE")
code('txtql::check::empty_cycle', 'A rule that can derive itself without consuming text.', "TEXT = a\na = a OR 'x'")
code('txtql::check::bad_bounds', 'A lower bound above the upper bound.', "TEXT = 3 TO 2 WORD")
code('txtql::check::bound_too_large', 'A finite bound above 10,000 (use `n`).', "TEXT = 1 TO 10001 WORD")
code('txtql::check::bad_stop', 'Something that cannot be the stop of `UNTIL`/`UNTILBEFORE` (`ANY` alone, `LINE`, `ROW`, `COL`, labels and rules; aliases of allowed patterns are fine).', "TEXT = ANY UNTIL ANY")
code('txtql::check::empty_stop', 'A stop that can match empty text (except `EOF`).', "TEXT = ANY UNTIL (0 TO 1 'x')")
code('txtql::check::empty_literal', 'A literal with no characters.', "TEXT = ''")
code('txtql::check::unknown_function', 'A function name that does not exist (with a "did you mean" hint).', "TEXT = a:WORD AS NUMM(a)")
code('txtql::check::arity', 'A function with the wrong number of arguments (for example `JOIN` without a separator).', "TEXT = 1 TO n a:WORD SPLITBY ' ' AS JOIN(a)")
code('txtql::lint::unused_alias', 'Warning: an alias that `TEXT` cannot reach.', "TEXT = WORD\nALIAS sp = ' '", mode='check')
code('txtql::lint::unused_rule', 'Warning: a rule that `TEXT` cannot reach.', "TEXT = WORD\nspare = INT", mode='check')
code('txtql::lint::nested_repetition', 'Warning: an unlimited repetition directly inside another (`1 TO n (1 TO n p)`), which can match the same text in many ways.', "TEXT = 1 TO n (1 TO n WORD)", mode='check')
code('txtql::lint::right_recursion', 'Warning: a rule that ends by referring to itself; matching is quadratic, so use `1 TO n`.', "TEXT = a\na = 'x' a OR 'x'", mode='check')
code('txtql::input::no_parse', 'The text does not match the query: shows where matching stopped and what was expected.', "TEXT = INT", i='abc')
code('txtql::input::too_expensive', 'The work needed exceeded `--max-steps`.', "TEXT = 1 TO n WORD SPLITBY ' '", i='a b c d e f g h', flags=['--max-steps', '5'])
code('txtql::input::too_deep', 'Rule matches nested more than `--max-depth` levels.', "TEXT = a\na = '(' a ')' OR 'x'", i='((((((x))))))', flags=['--max-depth', '3'])
code('txtql::input::ambiguous', 'Output-changing ambiguity (a warning; an error with `--strict` or `STRICT`).', "TEXT = 'a' mid:(1 TO n ANY) 'z' rest:(0 TO n ANY)", i='a-z-z', flags=['--strict'])
code('txtql::eval::error', 'A template or `WHERE` failed on the matched text (for example `NUM("abc")`).', "TEXT = a:WORD AS NUM(a)", i='abc')
code('txtql::eval::repeated_key', 'Warning: an object key set twice, so a value was lost (an error with `--strict`).', "TEXT = a:WORD ' ' b:WORD AS { 'k': a, 'k': b }", i='x y')
code('txtql::io', 'A file cannot be read or the output cannot be written.', "TEXT = WORD", flags=['--nofile'], shown='`txtql query.tql missing.txt`')
code('txtql::io::utf8', 'The input is not valid UTF-8.', "TEXT = WORD", binput=b'\xff\xfe', shown='input bytes `ff fe`')

# ---- mini tables: group -> list of (label, query, input, flags)
MINI = {}
def mini(group, label, q, i, *flags):
    MINI.setdefault(group, []).append(dict(label=label, q=q, i=i, flags=list(flags)))
mini('prim', '`WORD`', "TEXT = 1 TO n WORD SPLITBY ' '", 'héllo wörld')
mini('prim', '`WORD` (not inside a run)', "TEXT = WORD", 'abc1')
mini('prim', '`INT`', "TEXT = 1 TO n INT SPLITBY '.'", '3.14')
mini('prim', '`FLOAT`', "TEXT = 1 TO n FLOAT SPLITBY ' '", '42 3.5')
mini('prim', '`HEX`', "TEXT = '0x' HEX", '0xCAFE')
mini('prim', '`BIN`', "TEXT = '0b' BIN", '0b1011')
mini('prim', '`IPV4`', "TEXT = ip:IPV4", '199.72.81.55')
mini('prim', '`IPV4` (part above 255)', "TEXT = ip:IPV4", '256.1.1.1')
mini('prim', '`IPV6`', "TEXT = '[' a:IPV6 ']:' port:INT", '[2001:db8::1]:8080')
mini('prim', '`LETTER`', "TEXT = 1 TO n LETTER", 'abc')
mini('prim', '`DIGIT`', "TEXT = 1 TO n DIGIT", '987')
mini('prim', '`PUNCT`', "TEXT = 1 TO n PUNCT", ',;!')
mini('prim', '`ANY` (crosses lines)', "TEXT = 1 TO n ANY", 'a\\nb')
mini('prim', '`NL` (all three line breaks)', "TEXT = 1 TO n WORD SPLITBY NL", 'a\\r\\nb\\nc\\rd')
mini('prim', '`TAB`', "TEXT = a:WORD TAB b:WORD", 'x\\ty')
mini('prim', '`LINE` (may be empty)', "TEXT = 1 TO n LINE SPLITBY NL", 'a\\n\\nb')
mini('prim', '`LINE` (mid-line)', "TEXT = 'a' LINE", 'ab')
mini('prim', '`ROW`, `COL`', "TEXT = 'ab' r:ROW c:COL", 'ab')
mini('prim', '`EOF`', "TEXT = w:WORD EOF", 'end')
mini('prim', '`i\'…\'` (ignore case)', "TEXT = i'hello'", 'HeLLo')
mini('rep', '`0 TO 1`', "TEXT = 'a' 0 TO 1 'b'", 'a')
mini('rep', '`2 TO 3`', "TEXT = n:(2 TO 3 'a')", 'aaaa')
mini('rep', '`2 TO 3` (in range)', "TEXT = n:(2 TO 3 'a')", 'aaa')
mini('rep', '`0 TO 0`', "TEXT = 'a' 0 TO 0 'b' 'c'", 'ac')
mini('rep', '`SPLITBY`', "TEXT = 1 TO n WORD SPLITBY ', '", 'a, b, c')
mini('rep', '`SPLITBY` (no trailing separator)', "TEXT = 1 TO n WORD SPLITBY ', '", 'a, b, ')
mini('rep', '`SKIPPING`', "TEXT = 1 TO n n:INT SKIPPING (1 TO n LETTER OR ' ')", 'a 1 b 2 c')
mini('rep', '`UNTIL`', "TEXT = a:(ANY UNTIL ';') b:WORD", 'x y;z')
mini('rep', '`UNTILBEFORE`', "TEXT = a:(ANY UNTILBEFORE ';') ';' b:WORD", 'x y;z')
mini('rep', '`LAZY`', "TEXT = a:(1 TO n LAZY ANY) ',' rest:(0 TO n ANY)", 'a,b,c', '--no-ambiguity-check')
mini('rep', 'greedy (default)', "TEXT = a:(1 TO n ANY) ',' rest:(0 TO n ANY)", 'a,b,c', '--no-ambiguity-check')
mini('func', '`NUM(x)`', "TEXT = a:INT AS NUM(a)", '007')
mini('func', '`NUM(x)` (decimal)', "TEXT = a:FLOAT AS NUM(a)", '3.50')
mini('func', '`LOWER(x)`', "TEXT = a:WORD AS LOWER(a)", 'ABC')
mini('func', '`UPPER(x)`', "TEXT = a:WORD AS UPPER(a)", 'abc')
mini('func', '`TRIM(x)`', "TEXT = a:(ANY UNTIL ';') AS TRIM(a)", '  a b ;')
mini('func', '`COUNT(list)`', "TEXT = 1 TO n a:WORD SPLITBY ' ' AS COUNT(a)", 'a b c')
mini('func', '`FIRST(list)`', "TEXT = 1 TO n a:WORD SPLITBY ' ' AS FIRST(a)", 'a b c')
mini('func', '`LAST(list)`', "TEXT = 1 TO n a:WORD SPLITBY ' ' AS LAST(a)", 'a b c')
mini('func', '`JOIN(list, sep)`', "TEXT = 1 TO n a:WORD SPLITBY ' ' AS JOIN(a, '+')", 'a b c')
mini('func', '`ZIP(keys, values)`', "TEXT = 1 TO n k:WORD SPLITBY ' ' ';' 1 TO n v:INT SPLITBY ' ' AS ZIP(k, v)", 'a b;1 2')
mini('func', '`ZIP` (missing values are `null`)', "TEXT = 1 TO n k:WORD SPLITBY ' ' ';' 1 TO n v:INT SPLITBY ' ' AS ZIP(k, v)", 'a b c;1')
mini('func', '`NUM` of non-numeric text', "TEXT = a:WORD AS NUM(a)", 'abc')
mini('cond', '`=` on two texts', "TEXT = a:FLOAT ' ' b:FLOAT WHERE a = b", '1.0 1')
mini('cond', '`=` with a number', "TEXT = a:FLOAT WHERE a = 1", '1.0')
mini('cond', '`<` on numeric text', "TEXT = a:INT ' ' b:INT WHERE a < b", '9 10')
mini('cond', '`<` on other text', "TEXT = a:WORD ' ' b:WORD WHERE a < b", 'abc abd')
mini('cond', '`!=`', "TEXT = a:WORD ' ' b:WORD WHERE a != b", 'x x')
mini('cond', '`>=`', "TEXT = a:INT WHERE NUM(a) >= 18", '18')
mini('cond', '`CONTAINS` (text)', "TEXT = a:WORD WHERE a CONTAINS 'ell'", 'hello')
mini('cond', '`STARTSWITH`', "TEXT = a:WORD WHERE a STARTSWITH 'he'", 'hello')
mini('cond', '`ENDSWITH`', "TEXT = a:WORD WHERE a ENDSWITH 'lo'", 'hello')
mini('cond', '`AND`, `OR`, `NOT`', "TEXT = a:WORD WHERE NOT a = 'x' AND (a = 'y' OR a = 'z')", 'z')
mini('cond', 'bare value (empty text is false)', "TEXT = a:(0 TO n WORD) WHERE a", '')
mini('cond', 'bare value (number 0 is true)', "TEXT = a:INT WHERE NUM(a)", '0')

# ---- front page and getting started
RHYMES_Q = "TEXT   = 1 TO n rhyme                 AS { rhyme }\nrhyme  = noun:WORD ' are ' colors NL  AS { noun: colors }\ncolors = 1 TO n color SPLITBY (' and ' OR ', ')\ncolor  = WORD\n"
ex('g_rhymes', RHYMES_Q, 'roses are red\nviolets are blue\nbees are black and yellow\n')
ex('g_rhymes_err', RHYMES_Q, 'roses are red\nviolets are blue\nbees are black and\n', expect=1)
ex('g_rhymes_compact', RHYMES_Q, 'roses are red\nviolets are blue\nbees are black and yellow\n', flags=['--compact'])
ex('g_expr', "TEXT = 1 TO n LINE SPLITBY NL", 'one\ntwo\nthree', via='expr', flags=['--compact'])
ex('g_expr2', "TEXT = k:WORD '=' v:INT AS { k: NUM(v) }", 'retries=3', via='expr')
ex('g_check_ok', RHYMES_Q, 'roses are red\nbees are black and yellow\n', mode='check')
ex('g_unused', "TEXT = WORD\nALIAS sp = ' '\n", 'x', mode='check')

# ---- errors page and reference checks
ex('e_line', "TEXT = 'Title: ' title:LINE NL\n", 'Title: Hello world\n', expect=1)
ex('u_crossing', "TEXT  = 1 TO n entry\nentry = key:(ANY UNTIL ' ') '= ' value:(ANY UNTIL NL)\n", 'a = 1\nb=2\nc = 3\n')
ex('u_safe', "TEXT  = 1 TO n entry\nentry = key:(ANY UNTILBEFORE (' ' OR NL)) ' = ' value:(ANY UNTIL NL)\n", 'a = 1\nb=2\nc = 3\n', expect=1)
ex('e_rules', "TEXT   = header 0 TO n entry footer\nheader = '# start' NL\nentry  = '+ ' WORD NL\nfooter = '# end' NL\n", '# start\n+ a\n- b\n# end\n', expect=1)
ex('e_lint', "TEXT = 1 TO n groups:(1 TO n LETTER) NL\nunused = FLOAT\n", 'ab\n', mode='check', expect=2)
ex('e_dupkey_strict', "TEXT = a:WORD ' ' b:WORD AS { 'k': a, 'k': b }\n", 'x y', flags=['--strict'], expect=1)
ex('e_dupkey', "TEXT = a:WORD ' ' b:WORD AS { 'k': a, 'k': b }\n", 'x y')
ex('r_prec', "TEXT = 'a' 'b' OR 'c'\n", 'c')
ex('r_prec2', "TEXT = 'a' 'b' OR 'c'\n", 'ac', expect=1)
ex('r_stop_seq', "TEXT = a:(WORD UNTIL (1 TO n ' ')) b:WORD\n", 'x   y', )
ex('r_stop_or', "TEXT = a:(ANY UNTILBEFORE (NL DIGIT OR NL EOF)) NL rest:(0 TO n ANY)\n", 'one\ntwo\n3 three')
ex('r_for_fields', "TEXT = 1 TO n ps:person SPLITBY NL AS [ { 'who': name, 'next_year': NUM(age) } FOR ps ]\nperson = name:WORD ' ' age:INT\n", 'Ada 36\nBob 41')
ex('r_consts', "TEXT = x:WORD AS [ TRUE, Null, False, 1e-3, x ]\n", 'a')
ex('r_null_path', "TEXT = 'n=' 0 TO 1 p:pair AS { 'k': p.key }\npair = key:WORD\n", 'n=')
ex('r_alias_text', "TEXT = a:WORD sp b:WORD\nALIAS sp = ' '\n", 'x y')
ex('r_comments', "-- a query with comments\nTEXT = 'x' -- the only rule\n", 'x')
ex('r_strings', "TEXT = \"it's\" ' ' 'say \"hi\"'\n", 'it\'s say "hi"')
ex('r_multiline_lit', "TEXT = 'a\nb'\n", 'a\nb')
ex('r_max1', "TEXT = a:WORD 1 TO 1 (',' b:WORD)\n", 'x,y')

# ---- behaviour claims made in prose that are not shown as full examples: (claim, query, input, flags, expected)
# expected: compact JSON on stdout, or 'ERR:<code>' (the code must appear on stderr)
CHECKS = []
def chk(claim, q, i, expected, *flags):
    CHECKS.append(dict(claim=claim, q=q, i=i, flags=list(flags), expected=expected))
chk("text equality is exact: '10' = '10.0' is false", "TEXT = a:INT '.' b:INT WHERE '10' = '10.0'", '1.2', 'ERR:txtql::input::no_parse')
chk("FIRST of an empty list is null", "TEXT = 0 TO n a:WORD SPLITBY ' ' AS FIRST(a)", '', 'null')
chk("COUNT of null is 0", "TEXT = 0 TO 1 a:INT AS COUNT(a)", '', '0')
chk("COUNT counts the keys of an object", "TEXT = x:p AS COUNT(x)\np = a:WORD ' ' b:WORD", 'a b', '2')
chk("JOIN: numbers become text", "TEXT = 1 TO n a:INT SPLITBY ' ' AS JOIN([ NUM(a0) FOR a0 IN a ], '+')", '1 2', '"1+2"')
chk("JOIN: null items are skipped", "TEXT = 'a' 0 TO 1 x:INT AS JOIN(['p', x, 'q'], '-')", 'a', '"p-q"')
chk("JOIN: booleans are an error", "TEXT = 'a' AS JOIN([true, 'a'], ',')", 'a', 'ERR:txtql::eval::error')
chk("JOIN: a quoted 'true' is text", "TEXT = 'a' AS JOIN(['true', 'a'], ',')", 'a', '"true,a"')
chk("ZIP: extra values are an error", "TEXT = 1 TO n k:WORD SPLITBY ' ' ';' 1 TO n v:INT SPLITBY ' ' AS ZIP(k, v)", 'a;1 2', 'ERR:txtql::eval::error')
chk("ZIP: null keys count as an empty list", "TEXT = 'x' 0 TO 1 k:WORD AS ZIP(k, [])", 'x', '{}')
chk("ZIP: a repeated key keeps the later value and warns", "TEXT = 1 TO n k:WORD SPLITBY ' ' ';' 1 TO n v:INT SPLITBY ' ' AS ZIP(k, v)", 'a a;1 2', '{"a":"2"}')
chk("ZIP: a repeated key is an error with --strict", "TEXT = 1 TO n k:WORD SPLITBY ' ' ';' 1 TO n v:INT SPLITBY ' ' AS ZIP(k, v)", 'a a;1 2', 'ERR:txtql::eval::repeated_key', '--strict')
chk("CONTAINS on a list is an exact match without numeric conversion", "TEXT = 1 TO n a:INT SPLITBY ' ' WHERE a CONTAINS 42", '42', 'ERR:txtql::input::no_parse')
chk("CONTAINS on a list of text finds equal text", "TEXT = 1 TO n a:INT SPLITBY ' ' WHERE a CONTAINS '42'", '42', '["42"]')
chk("the number 0 is true", "TEXT = a:INT WHERE NUM(a)", '0', '{"a":"0"}')
chk("an empty list is false", "TEXT = 0 TO n a:WORD SPLITBY ' ' WHERE a", '', 'ERR:txtql::input::no_parse')
chk("NUM trims text", "TEXT = a:(ANY UNTIL ';') AS NUM(a)", ' 7 ;', '7')
chk("a field that the rule never sets is a static error", "TEXT = p AS p.zzz\np = a:WORD", 'x', 'ERR:txtql::check::unknown_field')
chk("OR binds looser than a sequence", "TEXT = 'a' 'b' OR 'c'", 'ab', '"ab"')
chk("0 TO 0 never matches", "TEXT = 'a' 0 TO 0 'b' 'c'", 'abc', 'ERR:txtql::input::no_parse')
chk("a bare '=' compare: texts vs a number", "TEXT = a:INT WHERE a = 7", '7', '{"a":"7"}')
chk("min above max is bad_bounds", "TEXT = 3 TO 2 WORD", 'a', 'ERR:txtql::check::bad_bounds')
chk("an unlabelled key quoted vs bare: quoted is literal", "TEXT = k:WORD AS { 'k': k }", 'x', '{"k":"x"}')
chk("numbers as keys become text", "TEXT = k:INT AS { k: 1 }", '5', '{"5":1}')
chk("null as key is an error", "TEXT = 'a' AS { null: 1 }", 'a', 'ERR:txtql::eval::error')
chk("merging text is an error at run time or check time", "TEXT = k:WORD AS { k }", 'x', 'ERR:txtql::check::not_mergeable')
chk("function names are not reserved", "TEXT = count\ncount = INT", '5', '"5"')
chk("n is a valid label", "TEXT = n:INT", '5', '{"n":"5"}')
chk("EOF inside a stop matches only at the end", "TEXT = a:(ANY UNTILBEFORE (';' OR EOF))", 'x;', 'ERR:txtql::input::no_parse')
chk("UNTIL stops at the first stop", "TEXT = a:(ANY UNTIL ';') b:(ANY UNTIL ';')", 'x;y;', '{"a":"x","b":"y"}')
chk("UNTIL consumes the longest stop", "TEXT = a:(ANY UNTIL (';' OR ';;')) b:WORD", 'x;;y', '{"a":"x","b":"y"}')
chk("clauses in any order", "TEXT = 1 TO n a:WORD UNTIL ';' SPLITBY ','", 'a,b;', '["a","b"]')
chk("--max-steps ends a run cleanly", "TEXT = 1 TO n WORD SPLITBY ' '", 'a b c d e f g h', 'ERR:txtql::input::too_expensive', '--max-steps', '5')
chk("--max-depth ends a deep run cleanly", "TEXT = a\na = '(' a ')' OR 'x'", '((((((x))))))', 'ERR:txtql::input::too_deep', '--max-depth', '3')
chk("--no-ambiguity-check silences the warning", "TEXT = first:(1 TO n WORD SPLITBY ' ') ' ' rest:(1 TO n WORD SPLITBY ' ')", 'one two three', '{"first":"one two","rest":"three"}', '--no-ambiguity-check')
chk("20,000-deep nesting is fine with default depth", "TEXT = a\na = '(' a ')' OR 'x'", '(' * 5000 + 'x' + ')' * 5000, 'OK', )

# ---- performance and limits
ex('p_steps', "TEXT = 1 TO n WORD SPLITBY ' '\n", 'a b c d e f g h', flags=['--max-steps', '5'], expect=1)
ex('p_depth', "TEXT = a\na = '(' a ')' OR 'x'\n", '((((((x))))))', flags=['--max-depth', '3'], expect=1)
ex('p_right', "TEXT = a\na = 'x' a OR 'x'\n", 'xxx', mode='check')
chk("deep nesting within --max-depth does not overflow the stack", "TEXT = a\na = '(' a ')' OR 'x'", '(' * 9000 + 'x' + ')' * 9000, 'OK')
chk("nesting beyond the default --max-depth is a clean error", "TEXT = a\na = '(' a ')' OR 'x'", '(' * 20000 + 'x' + ')' * 20000, 'ERR:txtql::input::too_deep')
chk("--max-depth can be raised", "TEXT = a\na = '(' a ')' OR 'x'", '(' * 20000 + 'x' + ')' * 20000, 'OK', '--max-depth', '100000')

# ---- Practical Examples (recipes)
PE_CSV = '''TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(ANY UNTILBEFORE ('"' (',' OR NL OR EOF))) '"' AS v
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) WHERE NOT v STARTSWITH '"' AS v
'''
PE_CSV_NAIVE = '''TEXT   = header:record NL 1 TO n rows:record SPLITBY NL 0 TO 1 NL AS [ ZIP(header, r) FOR r IN rows ]
record = several OR alone
several = 2 TO n cells:cell SPLITBY ',' AS cells
alone  = c:cell WHERE c != '' AS [c]
cell   = quoted OR plain
quoted = '"' v:(ANY UNTILBEFORE ('"' (',' OR NL OR EOF))) '"' AS v
plain  = v:(ANY UNTILBEFORE (',' OR NL OR EOF)) AS v
'''
PE_TSV = '''TEXT   = header:record 1 TO n rows:record AS [ ZIP(header, r) FOR r IN rows ]
record = 1 TO n cells:cell SPLITBY TAB NL AS cells
ALIAS cell = ANY UNTILBEFORE (TAB OR NL)
'''
PE_TSV_TYPED = '''TEXT = 'sku' TAB 'qty' TAB 'price' NL 1 TO n items:item AS items
item = sku:cell TAB 0 TO 1 qty:INT TAB price:FLOAT NL
       AS { 'sku': TRIM(sku), 'qty': NUM(qty), 'price': NUM(price) }
ALIAS cell = ANY UNTILBEFORE (TAB OR NL)
'''
PE_INI = '''TEXT    = 0 TO n top:entry SKIPPING filler 0 TO n sections:section SKIPPING filler 0 TO n filler
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
'''
PE_ACC = '''TEXT   = 1 TO n entry AS [ entry FOR entry ]
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
'''
PE_ERRS = '''-- keep only the 5xx lines; every other line is skipped whole
TEXT   = 0 TO n failed SKIPPING (LINE NL)
failed = ip:IPV4 ' - - [' ANY UNTIL '] "' method:WORD ' ' path:(ANY UNTILBEFORE ' ') ' ' ANY UNTIL '" '
         st:server ' ' ANY UNTIL NL
    AS { 'ip': ip, 'method': method, 'path': path, 'status': NUM(st) }
server = c:INT WHERE NUM(c) >= 500 AS c
'''
PE_APP = '''TEXT   = 1 TO n entry AS [ entry FOR entry ]
entry  = time:(INT '-' INT '-' INT 'T' INT ':' INT ':' FLOAT 'Z') ' ' level:level ' [' thread:(ANY UNTILBEFORE ']') '] '
         logger:(ANY UNTILBEFORE ' - ') ' - ' msg:(ANY UNTILBEFORE NL) NL 0 TO n trace:continuation
    AS { 'time': time, 'level': level, 'thread': thread,
         'logger': logger, 'message': msg, 'trace': trace }
level  = 'DEBUG' OR 'INFO' OR 'WARN' OR 'ERROR'
-- anything that does not start with a digit (the next timestamp) belongs to the entry
continuation = l:((LETTER OR ' ' OR TAB) ANY UNTILBEFORE NL) NL AS TRIM(l)
'''
PE_CHG = '''TEXT    = '# Changelog' NL gap 1 TO n releases:release
          AS [ { 'version': r.version, 'date': r.date,
                 'changes': { s.heading: s.items FOR s IN r.sections } } FOR r IN releases ]
release = '## [' version:(ANY UNTILBEFORE ']') '] - ' date:(INT '-' INT '-' INT) NL gap
          1 TO n sections:section
section = '### ' heading:(ANY UNTILBEFORE NL) NL 1 TO n items:item gap
item    = '- ' t:(ANY UNTILBEFORE NL) NL AS t
ALIAS gap = 0 TO n NL
'''
PE_MAIL = '''TEXT     = headers NL 'Hi ' customer:WORD ',' NL NL 'Thanks for your order. Summary:' NL NL
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
'''
PE_CSV_IN = '''id,name,note
1,Ada,"likes tea, and coffee"
2,"Grace ""Amazing"" Hopper",
3,Linus,"two
lines"
'''
PE_INI_IN = '''# global settings
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
'''
PE_ACC_IN = '''203.0.113.9 - - [04/Oct/2026:08:15:02 +0000] "GET /index.html HTTP/1.1" 200 5123 "-" "Mozilla/5.0 (X11; Linux x86_64) Firefox/131.0"
198.51.100.23 - alice [04/Oct/2026:08:15:07 +0000] "POST /api/login HTTP/1.1" 302 0 "https://example.org/login" "curl/8.5.0"
203.0.113.9 - - [04/Oct/2026:08:15:09 +0000] "GET /missing.png HTTP/2.0" 404 153 "https://example.org/" "Mozilla/5.0 (X11; Linux x86_64) Firefox/131.0"
192.0.2.77 - - [04/Oct/2026:08:15:12 +0000] "GET /api/report?id=7&fmt=csv HTTP/1.1" 500 - "-" "python-requests/2.32"
'''
PE_APP_IN = '''2026-10-04T08:00:01.120Z INFO [main] com.acme.Server - listening on :8080
2026-10-04T08:00:03.455Z ERROR [worker-3] com.acme.Billing - payment failed for order 1042
java.lang.IllegalStateException: card declined
	at com.acme.Billing.charge(Billing.java:88)
	at com.acme.Worker.run(Worker.java:31)
Caused by: java.net.SocketTimeoutException: read timed out
	at java.base/java.net.SocketInputStream.read(SocketInputStream.java:163)
2026-10-04T08:00:04.002Z WARN [worker-1] com.acme.Cache - eviction storm: 512 keys
'''
PE_CHG_IN = '''# Changelog

## [1.2.0] - 2026-09-30

### Added
- Support for `UNTILBEFORE` stops
- A language server

### Fixed
- Crash on empty input

## [1.1.0] - 2026-08-01

### Changed
- Error messages now point at the failing rule
'''
PE_MAIL_IN = '''From: Shop <orders@shop.example>
To: ada@example.org
Subject: Your order #1042
Date: Sat, 04 Oct 2026 09:12:00 +0000

Hi Ada,

Thanks for your order. Summary:

  2 x Widget      @  4.50
  1 x Gadget Pro  @ 12.00

Total: 21.00 EUR
Ship to: 12 Rue Example, Paris
'''
PE_TSV_IN = 'sku\tqty\tprice\n A-100\t3\t4.50\nB-7\t\t12.00\n'
PE_CSV_WIDE_IN = 'id,name\n1,Ada,extra\n'
PE_CSV_SHORT_IN = 'id,name\n1,Ada\n2\n'
PE_CSV_NONL_IN = 'id,name\n1,Ada\n2,'
PE_CSV_BLANK_IN = 'id,name\n1,Ada\n\n2,Bo\n'
ex('pe_csv', PE_CSV, PE_CSV_IN)
ex('pe_csv_short', PE_CSV, PE_CSV_SHORT_IN)
ex('pe_csv_wide', PE_CSV, PE_CSV_WIDE_IN, expect=1)
ex('pe_csv_nonl', PE_CSV, PE_CSV_NONL_IN)
ex('pe_csv_blank', PE_CSV, PE_CSV_BLANK_IN, expect=1)
ex('pe_csv_naive', PE_CSV_NAIVE, PE_CSV_IN, mode='check', expect=2)
ex('pe_csv_naive_strict', PE_CSV_NAIVE, PE_CSV_IN, flags=['--strict'], expect=1)
ex('pe_tsv', PE_TSV, PE_TSV_IN)
ex('pe_tsv_typed', PE_TSV_TYPED, PE_TSV_IN)
ex('pe_ini', PE_INI, PE_INI_IN)
ex('pe_acc', PE_ACC, PE_ACC_IN)
ex('pe_errs', PE_ERRS, PE_ACC_IN)
ex('pe_app', PE_APP, PE_APP_IN)
ex('pe_chg', PE_CHG, PE_CHG_IN)
ex('pe_mail', PE_MAIL, PE_MAIL_IN)
ex('pe_ci_ok', PE_CHG, PE_CHG_IN, mode='check')
ex('pe_pipe', "TEXT  = 1 TO n entry\nentry = kb:INT TAB path:(ANY UNTILBEFORE NL) NL AS { 'kb': NUM(kb), 'path': path }", '4096\t./src\n12\t./docs\n', via='expr', flags=['--compact'])
chk('recipe csv is free of ambiguity and repeated keys (--strict succeeds)', PE_CSV, PE_CSV_IN, 'OK', '--strict')
chk('recipe tsv is free of ambiguity and repeated keys (--strict succeeds)', PE_TSV, PE_TSV_IN, 'OK', '--strict')
chk('recipe ini is free of ambiguity and repeated keys (--strict succeeds)', PE_INI, PE_INI_IN, 'OK', '--strict')
chk('recipe acc is free of ambiguity and repeated keys (--strict succeeds)', PE_ACC, PE_ACC_IN, 'OK', '--strict')
chk('recipe errs is free of ambiguity and repeated keys (--strict succeeds)', PE_ERRS, PE_ACC_IN, 'OK', '--strict')
chk('recipe app is free of ambiguity and repeated keys (--strict succeeds)', PE_APP, PE_APP_IN, 'OK', '--strict')
chk('recipe chg is free of ambiguity and repeated keys (--strict succeeds)', PE_CHG, PE_CHG_IN, 'OK', '--strict')
chk('recipe mail is free of ambiguity and repeated keys (--strict succeeds)', PE_MAIL, PE_MAIL_IN, 'OK', '--strict')
chk('recipe tsvt is free of ambiguity and repeated keys (--strict succeeds)', PE_TSV_TYPED, PE_TSV_IN, 'OK', '--strict')
chk('recipe csv: no final line break, last field empty', PE_CSV, 'a,b\n1,\n2,', '[{"a":"1","b":""},{"a":"2","b":""}]', '--strict')
chk('recipe csv: final line break, last field empty (no phantom row)', PE_CSV, 'a,b\n1,\n2,\n', '[{"a":"1","b":""},{"a":"2","b":""}]', '--strict')
chk('recipe csv: no final line break, all filled', PE_CSV, 'a,b\n1,x\n2,y', '[{"a":"1","b":"x"},{"a":"2","b":"y"}]', '--strict')
chk('recipe csv: final line break, all filled', PE_CSV, 'a,b\n1,x\n2,y\n', '[{"a":"1","b":"x"},{"a":"2","b":"y"}]', '--strict')
chk('recipe csv accepts CRLF line breaks', PE_CSV, 'id,name\r\n1,Ada\r\n', '[{"id":"1","name":"Ada"}]', '--strict')
