//! Recursive-descent parser for queries.
//!
//! ```text
//! query   := (alias | rule)* EOF
//! alias   := 'ALIAS' IDENT '=' alt
//! rule    := ['STRICT'] IDENT '=' alt ['WHERE' cond] ['AS' tmpl]
//! alt     := seq ('OR' seq)*
//! seq     := item+
//! seq     := (item [until])+          -- `x UNTILBEFORE s` alone means `0 TO n x UNTILBEFORE s`
//! item    := [IDENT ':'] (repeat | atom)
//! repeat  := INT 'TO' (INT | 'n') ['LAZY'] item clause*   -- each clause at most once
//! clause  := 'SPLITBY' item | 'SKIPPING' item | until
//! until   := ('UNTILBEFORE' | 'UNTIL') item
//! atom    := STRING | PRIM | IDENT | '(' alt ')'
//! ```

use crate::ast::*;
use crate::error::{ParseError, Span};
use crate::lexer::{Kw, Tok, Token, lex};

/// Maximum nesting of parentheses, repetitions, templates and conditions.
pub const MAX_DEPTH: usize = 100;

pub fn parse(src: &str) -> Result<Query, ParseError> {
    let tokens = lex(src)?;
    let mut p = Parser { src, tokens, pos: 0, depth: 0 };
    p.query()
}

struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
}

type PResult<T> = Result<T, ParseError>;

impl Parser<'_> {
    fn src_text(&self, span: Span) -> &str {
        &self.src[span.start..span.end]
    }

    fn peek(&self) -> &Tok {
        &self.tokens[self.pos].tok
    }

    fn peek_at(&self, n: usize) -> &Tok {
        let i = (self.pos + n).min(self.tokens.len() - 1);
        &self.tokens[i].tok
    }

    fn span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn prev_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1)].span
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == tok {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_kw(&mut self, kw: Kw) -> bool {
        self.eat(&Tok::Kw(kw))
    }

    fn unexpected(&self, expected: &str) -> ParseError {
        self.unexpected_help(expected, None)
    }

    fn unexpected_help(&self, expected: &str, help: Option<String>) -> ParseError {
        if self.peek() == &Tok::Eof && self.pos > 0 {
            // A zero-width span at the very end renders without a pointer; point at the last token.
            return ParseError::UnexpectedEnd { expected: expected.to_string(), span: self.prev_span(), help };
        }
        // A keyword is shown as it was written, since any spelling is that keyword.
        let found = match self.peek() {
            Tok::Kw(_) => format!("keyword `{}`", self.src_text(self.span())),
            other => other.describe(),
        };
        ParseError::Unexpected { expected: expected.to_string(), found, span: self.span(), help }
    }

    fn expect(&mut self, tok: Tok, expected: &str) -> PResult<Span> {
        if self.peek() == &tok { Ok(self.bump().span) } else { Err(self.unexpected(expected)) }
    }

    fn ident(&mut self, what: &str) -> PResult<Ident> {
        match self.peek().clone() {
            Tok::Ident(name) => Ok(Ident { name, span: self.bump().span }),
            Tok::Kw(_) => Err(self.unexpected_help(
                what,
                Some(format!(
                    "`{}` is a keyword (in any spelling), so it cannot be a name; choose another one",
                    self.src_text(self.span())
                )),
            )),
            _ => Err(self.unexpected(what)),
        }
    }

    /// A field after `.`: any word, keywords included, since fields come from the data.
    fn field_name(&mut self) -> PResult<Ident> {
        match self.peek() {
            Tok::Kw(_) => {
                let span = self.bump().span;
                Ok(Ident { name: self.src_text(span).to_string(), span })
            }
            _ => self.ident("a field name"),
        }
    }

    fn enter(&mut self) -> PResult<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { Err(ParseError::TooDeep { span: self.span() }) } else { Ok(()) }
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    // ---- rules ----

    fn query(&mut self) -> PResult<Query> {
        let mut rules = Vec::new();
        let mut aliases = Vec::new();
        while self.peek() != &Tok::Eof {
            if self.eat_kw(Kw::ALIAS) {
                aliases.push(self.alias()?);
            } else {
                rules.push(self.rule()?);
            }
        }
        let end = self.span().end;
        Ok(Query { aliases, rules, span: Span::new(0, end) })
    }

    fn at_rule_start(&self) -> bool {
        // `word = …` starts a definition too, so that it gets the "keyword as a name" error.
        matches!(self.peek(), Tok::Kw(Kw::STRICT | Kw::ALIAS))
            || (matches!(self.peek(), Tok::Ident(_) | Tok::Kw(_)) && self.peek_at(1) == &Tok::Eq)
    }

    fn alias(&mut self) -> PResult<Alias> {
        let name = self.ident("an alias name")?;
        if self.peek() != &Tok::Eq {
            return Err(
                self.unexpected_help("`=`", Some(format!("aliases are written `ALIAS {} = pattern`", name.name)))
            );
        }
        self.bump();
        let pattern = self.alt()?;
        if !self.at_rule_start() && self.peek() != &Tok::Eof {
            let help = matches!(self.peek(), Tok::Kw(Kw::WHERE | Kw::AS))
                .then(|| "an alias is only a pattern; use a rule for `WHERE` and `AS`".to_string());
            return Err(self.unexpected_help("the next rule or end of query", help));
        }
        Ok(Alias { name, pattern })
    }

    fn rule(&mut self) -> PResult<Rule> {
        let strict = self.eat_kw(Kw::STRICT);
        let name = self.ident("a rule name")?;
        if self.peek() != &Tok::Eq {
            return Err(self.unexpected_help("`=`", Some(format!("rules are written `{} = pattern`", name.name))));
        }
        self.bump();
        let pattern = self.alt()?;
        let filter = if self.eat_kw(Kw::WHERE) { Some(self.cond()?) } else { None };
        let template = if self.eat_kw(Kw::AS) { Some(self.tmpl()?) } else { None };
        if !self.at_rule_start() && self.peek() != &Tok::Eof {
            let help = match self.peek() {
                Tok::Kw(Kw::WHERE) if template.is_some() => Some("`WHERE` must come before `AS`".to_string()),
                Tok::Kw(Kw::WHERE | Kw::AS) => Some("each rule can have at most one `WHERE` and one `AS`".to_string()),
                _ => None,
            };
            let expected = match (&filter, &template) {
                (_, Some(_)) => "the next rule or end of query",
                (Some(_), None) => "`AS`, the next rule, or end of query",
                (None, None) => "a pattern, `WHERE`, `AS`, the next rule, or end of query",
            };
            return Err(self.unexpected_help(expected, help));
        }
        Ok(Rule { name, strict, pattern, filter, template })
    }

    // ---- patterns ----

    fn alt(&mut self) -> PResult<Pattern> {
        self.enter()?;
        let first = self.seq()?;
        let mut alts = vec![first];
        while self.eat_kw(Kw::OR) {
            alts.push(self.seq()?);
        }
        self.leave();
        Ok(if alts.len() == 1 {
            alts.pop().unwrap()
        } else {
            let span = alts[0].span.to(alts[alts.len() - 1].span);
            Pattern { kind: PatKind::Or(alts), span }
        })
    }

    fn at_item_start(&self) -> bool {
        if self.peek_at(1) == &Tok::Eq {
            return false;
        }
        match self.peek() {
            Tok::Str { .. } | Tok::Int(_) | Tok::LParen => true,
            Tok::Kw(kw) => matches!(
                kw,
                Kw::WORD
                    | Kw::FLOAT
                    | Kw::INT
                    | Kw::HEX
                    | Kw::BIN
                    | Kw::IPV4
                    | Kw::IPV6
                    | Kw::PUNCT
                    | Kw::ANY
                    | Kw::DIGIT
                    | Kw::LETTER
                    | Kw::LINE
                    | Kw::NL
                    | Kw::TAB
                    | Kw::ROW
                    | Kw::COL
                    | Kw::EOF
            ),
            Tok::Ident(_) => self.peek_at(1) != &Tok::Eq,
            _ => false,
        }
    }

    fn seq(&mut self) -> PResult<Pattern> {
        let mut items = Vec::new();
        while self.at_item_start() {
            let item = self.item()?;
            // The bound comes first in `0 TO n x UNTILBEFORE s`, so only the short form, with
            // no bounds, is seen here. A label binds to the item alone: `k:ANY UNTILBEFORE s`
            // repeats `k:ANY`.
            let item = match self.until()? {
                Some(until) => {
                    let span = item.span.to(self.prev_span());
                    let rep =
                        Repeat { min: 0, max: None, lazy: false, item, sep: None, skip: None, until: Some(until) };
                    Pattern { kind: PatKind::Repeat(Box::new(rep)), span }
                }
                None => item,
            };
            items.push(item);
        }
        match items.len() {
            0 => Err(self.unexpected("a pattern")),
            1 => Ok(items.pop().unwrap()),
            _ => {
                let span = items[0].span.to(items[items.len() - 1].span);
                Ok(Pattern { kind: PatKind::Seq(items), span })
            }
        }
    }

    fn item(&mut self) -> PResult<Pattern> {
        self.enter()?;
        if matches!(self.peek(), Tok::Kw(_)) && self.peek_at(1) == &Tok::Colon {
            let word = self.src_text(self.span()).to_string();
            return Err(self.unexpected_help(
                "a label",
                Some(format!("`{word}` is a keyword (in any spelling), so it cannot be a label; choose another one")),
            ));
        }
        let result = if matches!(self.peek(), Tok::Ident(_)) && self.peek_at(1) == &Tok::Colon {
            let label = self.ident("a label")?;
            self.bump();
            if !self.at_item_start() {
                return Err(self.unexpected("a pattern after the label"));
            }
            let inner = self.item_body()?;
            let span = label.span.to(inner.span);
            Pattern { kind: PatKind::Label(label, Box::new(inner)), span }
        } else {
            self.item_body()?
        };
        self.leave();
        Ok(result)
    }

    fn item_body(&mut self) -> PResult<Pattern> {
        if matches!(self.peek(), Tok::Int(_)) { self.repeat() } else { self.atom() }
    }

    fn bound(&mut self) -> PResult<(u32, Span)> {
        let span = self.span();
        match self.peek() {
            &Tok::Int(n) => {
                self.bump();
                // Oversized bounds are reported by `check` with a better message.
                Ok((u32::try_from(n).unwrap_or(u32::MAX), span))
            }
            _ => Err(self.unexpected("a number")),
        }
    }

    fn repeat(&mut self) -> PResult<Pattern> {
        let (min, start) = self.bound()?;
        if !self.eat_kw(Kw::TO) {
            return Err(self.unexpected_help(
                "`TO`",
                Some("repetitions are written `min TO max pattern`, e.g. `1 TO n WORD`".into()),
            ));
        }
        let max = match self.peek() {
            Tok::Ident(n) if n.eq_ignore_ascii_case("n") => {
                self.bump();
                None
            }
            Tok::Int(_) => Some(self.bound()?.0),
            _ => {
                return Err(self.unexpected_help(
                    "a number or `n`",
                    Some("use `n` for an unlimited number of repetitions".into()),
                ));
            }
        };
        let lazy = self.eat_kw(Kw::LAZY);
        if !self.at_item_start() {
            return Err(self.unexpected("the pattern to repeat"));
        }
        let item = self.item()?;
        let mut sep = None;
        let mut skip = None;
        let mut until = None;
        loop {
            if sep.is_none() && self.eat_kw(Kw::SPLITBY) {
                sep = Some(self.item()?);
            } else if skip.is_none() && self.eat_kw(Kw::SKIPPING) {
                skip = Some(self.item()?);
            } else if until.is_none()
                && let Some(u) = self.until()?
            {
                until = Some(u);
            } else {
                break;
            }
        }
        let span = start.to(self.prev_span());
        Ok(Pattern { kind: PatKind::Repeat(Box::new(Repeat { min, max, lazy, item, sep, skip, until })), span })
    }

    /// An `UNTILBEFORE s` / `UNTIL s` clause, if one comes next.
    fn until(&mut self) -> PResult<Option<Until>> {
        let kind = match self.peek() {
            Tok::Kw(Kw::UNTILBEFORE) => Stop::Before,
            Tok::Kw(Kw::UNTIL) => Stop::After,
            _ => return Ok(None),
        };
        self.bump();
        if !self.at_item_start() {
            return Err(self.unexpected(&format!("a pattern after `{}`", kind.keyword())));
        }
        Ok(Some(Until { stop: self.item()?, kind }))
    }

    fn atom(&mut self) -> PResult<Pattern> {
        let span = self.span();
        let kind = match self.peek().clone() {
            Tok::Str { value, ci } => {
                self.bump();
                PatKind::Lit { text: value, ci }
            }
            Tok::Kw(kw) => {
                let prim = match kw {
                    Kw::WORD => Prim::Word,
                    Kw::FLOAT => Prim::Float,
                    Kw::INT => Prim::Int,
                    Kw::HEX => Prim::Hex,
                    Kw::BIN => Prim::Bin,
                    Kw::IPV4 => Prim::Ipv4,
                    Kw::IPV6 => Prim::Ipv6,
                    Kw::PUNCT => Prim::Punct,
                    Kw::ANY => Prim::Any,
                    Kw::DIGIT => Prim::Digit,
                    Kw::LETTER => Prim::Letter,
                    Kw::LINE => Prim::Line,
                    Kw::NL => Prim::Newline,
                    Kw::TAB => Prim::Tab,
                    Kw::ROW => Prim::Row,
                    Kw::COL => Prim::Col,
                    Kw::EOF => Prim::Eof,
                    _ => return Err(self.unexpected("a pattern")),
                };
                self.bump();
                PatKind::Prim(prim)
            }
            Tok::Ident(_) => PatKind::Ref(self.ident("a rule name")?),
            Tok::LParen => {
                self.bump();
                let inner = self.alt()?;
                self.expect(Tok::RParen, "`)`")?;
                // Parentheses only group; they do not appear in the tree.
                return Ok(Pattern { kind: inner.kind, span: span.to(self.prev_span()) });
            }
            _ => return Err(self.unexpected("a pattern")),
        };
        Ok(Pattern { kind, span })
    }

    // ---- conditions ----

    fn cond(&mut self) -> PResult<Cond> {
        self.enter()?;
        let mut left = self.cond_and()?;
        while self.eat_kw(Kw::OR) {
            let right = self.cond_and()?;
            let span = left.span.to(right.span);
            left = Cond { kind: CondKind::Or(Box::new(left), Box::new(right)), span };
        }
        self.leave();
        Ok(left)
    }

    fn cond_and(&mut self) -> PResult<Cond> {
        let mut left = self.cond_not()?;
        while self.eat_kw(Kw::AND) {
            let right = self.cond_not()?;
            let span = left.span.to(right.span);
            left = Cond { kind: CondKind::And(Box::new(left), Box::new(right)), span };
        }
        Ok(left)
    }

    fn cond_not(&mut self) -> PResult<Cond> {
        let start = self.span();
        if self.eat_kw(Kw::NOT) {
            self.enter()?;
            let inner = self.cond_not()?;
            self.leave();
            let span = start.to(inner.span);
            return Ok(Cond { kind: CondKind::Not(Box::new(inner)), span });
        }
        if self.eat(&Tok::LParen) {
            let inner = self.cond()?;
            self.expect(Tok::RParen, "`)`")?;
            return Ok(Cond { kind: inner.kind, span: start.to(self.prev_span()) });
        }
        let left = self.tmpl()?;
        let op = match self.peek() {
            Tok::Eq => Some(CmpOp::Eq),
            Tok::NotEq => Some(CmpOp::NotEq),
            Tok::Lt => Some(CmpOp::Lt),
            Tok::Le => Some(CmpOp::Le),
            Tok::Gt => Some(CmpOp::Gt),
            Tok::Ge => Some(CmpOp::Ge),
            Tok::Kw(Kw::CONTAINS) => Some(CmpOp::Contains),
            Tok::Kw(Kw::STARTSWITH) => Some(CmpOp::StartsWith),
            Tok::Kw(Kw::ENDSWITH) => Some(CmpOp::EndsWith),
            _ => None,
        };
        let Some(op) = op else {
            let span = left.span;
            return Ok(Cond { kind: CondKind::Truthy(left), span });
        };
        self.bump();
        let right = self.tmpl()?;
        let span = left.span.to(right.span);
        Ok(Cond { kind: CondKind::Cmp(left, op, right), span })
    }

    // ---- templates ----

    fn tmpl(&mut self) -> PResult<Template> {
        self.enter()?;
        let t = self.tmpl_inner()?;
        self.leave();
        Ok(t)
    }

    fn tmpl_inner(&mut self) -> PResult<Template> {
        let start = self.span();
        let kind = match self.peek().clone() {
            Tok::Str { value, .. } => {
                self.bump();
                TmplKind::Str(value)
            }
            Tok::Int(n) => {
                self.bump();
                TmplKind::Num(n.into())
            }
            Tok::Float(f) => {
                self.bump();
                TmplKind::Num(serde_json::Number::from_f64(f).ok_or(ParseError::BadNumber { span: start })?)
            }
            Tok::Minus => {
                self.bump();
                let span = self.span();
                let num = match *self.peek() {
                    Tok::Int(n) => i64::try_from(n)
                        .ok()
                        .and_then(|n| n.checked_neg())
                        .map(serde_json::Number::from)
                        .ok_or(ParseError::BadNumber { span })?,
                    Tok::Float(f) => serde_json::Number::from_f64(-f).ok_or(ParseError::BadNumber { span })?,
                    _ => return Err(self.unexpected("a number after `-`")),
                };
                self.bump();
                TmplKind::Num(num)
            }
            Tok::Ident(name) => {
                let ident = Ident { name, span: self.bump().span };
                match ident.name.to_ascii_lowercase().as_str() {
                    // Function names are case-insensitive, like keywords.
                    _ if self.peek() == &Tok::LParen => {
                        self.call(Ident { name: ident.name.to_ascii_uppercase(), span: ident.span })?
                    }
                    "true" => TmplKind::Bool(true),
                    "false" => TmplKind::Bool(false),
                    "null" => TmplKind::Null,
                    _ => {
                        let mut path = vec![ident];
                        while self.eat(&Tok::Dot) {
                            path.push(self.field_name()?);
                        }
                        TmplKind::Path(path)
                    }
                }
            }
            Tok::LBrace => {
                self.bump();
                self.object()?
            }
            Tok::LBracket => {
                self.bump();
                self.array()?
            }
            _ => return Err(self.unexpected("a value (string, number, name, `{`, or `[`)")),
        };
        Ok(Template { kind, span: start.to(self.prev_span()) })
    }

    fn call(&mut self, name: Ident) -> PResult<TmplKind> {
        self.expect(Tok::LParen, "`(`")?;
        let mut args = Vec::new();
        if !self.eat(&Tok::RParen) {
            loop {
                args.push(self.tmpl()?);
                if self.eat(&Tok::RParen) {
                    break;
                }
                self.expect(Tok::Comma, "`,` or `)`")?;
            }
        }
        Ok(TmplKind::Call(name, args))
    }

    fn for_each(&mut self) -> PResult<Option<Box<ForEach>>> {
        let start = self.span();
        if !self.eat_kw(Kw::FOR) {
            return Ok(None);
        }
        let var = self.ident("a name after `FOR`")?;
        let source = if self.eat_kw(Kw::IN) { Some(self.tmpl()?) } else { None };
        Ok(Some(Box::new(ForEach { var, source, span: start.to(self.prev_span()) })))
    }

    fn object(&mut self) -> PResult<TmplKind> {
        let mut entries = Vec::new();
        while !self.eat(&Tok::RBrace) {
            // `key: value`, or a value alone, which is merged.
            let key = self.tmpl()?;
            if self.eat(&Tok::Colon) {
                let listof = self.eat_kw(Kw::LISTOF);
                let value = self.tmpl()?;
                let each = self.for_each()?;
                entries.push(ObjEntry::Pair { key, value, each, listof });
            } else {
                entries.push(ObjEntry::Merge(key));
            }
            if !self.eat(&Tok::Comma) {
                if self.peek() != &Tok::RBrace {
                    return Err(self.unexpected_help(
                        "`:`, `,` or `}`",
                        Some("object entries are written `key: value`, or `name` alone to merge an object".into()),
                    ));
                }
                self.bump();
                break;
            }
        }
        Ok(TmplKind::Object(entries))
    }

    fn array(&mut self) -> PResult<TmplKind> {
        let mut elems = Vec::new();
        while !self.eat(&Tok::RBracket) {
            let value = self.tmpl()?;
            let each = self.for_each()?;
            elems.push(ArrElem { value, each });
            if !self.eat(&Tok::Comma) {
                self.expect(Tok::RBracket, "`,` or `]`")?;
                break;
            }
        }
        Ok(TmplKind::Array(elems))
    }
}
