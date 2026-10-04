//! Lexer for the query language.

use crate::error::{ParseError, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Kw(Kw),
    /// A quoted string. `ci` is set for `i'…'` literals.
    Str {
        value: String,
        ci: bool,
    },
    Int(u64),
    Float(f64),
    Eq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Colon,
    Comma,
    Dot,
    Minus,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Eof,
}

macro_rules! keywords {
    ($($name:ident),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Kw { $($name),* }

        impl Kw {
            /// Every keyword, for editor support and its tests.
            pub const ALL: &[Kw] = &[$(Kw::$name),*];

            /// The keyword spelled `s`, in any case.
            pub fn parse(s: &str) -> Option<Kw> {
                match s.to_ascii_uppercase().as_str() {
                    $(stringify!($name) => Some(Kw::$name),)*
                    _ => None,
                }
            }

            pub fn as_str(self) -> &'static str {
                match self { $(Kw::$name => stringify!($name)),* }
            }
        }
    };
}

// Keywords are reserved in any case: `word`, `Word` and `WORD` are the same keyword.
keywords! {
    OR, AND, NOT, TO, LAZY, SPLITBY, SKIPPING, WHERE, AS, FOR, IN, LISTOF,
    CONTAINS, STARTSWITH, ENDSWITH, STRICT, UNTILBEFORE, UNTIL, ALIAS,
    WORD, FLOAT, INT, HEX, BIN, IPV4, IPV6, PUNCT, ANY, DIGIT, LETTER, LINE, NL, TAB, ROW, COL, EOF,
}

impl Tok {
    /// Human-readable description, used in "expected X, found Y" messages.
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(s) => format!("identifier `{s}`"),
            Tok::Kw(k) => format!("keyword `{}`", k.as_str()),
            Tok::Str { value, .. } => format!("string '{value}'"),
            Tok::Int(n) => format!("number `{n}`"),
            Tok::Float(n) => format!("number `{n}`"),
            Tok::Eof => "end of query".to_string(),
            other => format!("`{}`", other.symbol()),
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            Tok::Eq => "=",
            Tok::NotEq => "!=",
            Tok::Lt => "<",
            Tok::Le => "<=",
            Tok::Gt => ">",
            Tok::Ge => ">=",
            Tok::Colon => ":",
            Tok::Comma => ",",
            Tok::Dot => ".",
            Tok::Minus => "-",
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::LBrace => "{",
            Tok::RBrace => "}",
            Tok::LBracket => "[",
            Tok::RBracket => "]",
            _ => "?",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

pub fn lex(src: &str) -> Result<Vec<Token>, ParseError> {
    let lexed = lex_tolerant(src);
    match lexed.errors.into_iter().next() {
        Some(e) => Err(e),
        None => Ok(lexed.tokens),
    }
}

/// The tokens of a query that may contain errors, for editors: every error is recorded and
/// lexing goes on. An unterminated or malformed string runs to its closing quote or the end of
/// its line; an unexpected character or invalid number is skipped.
#[derive(Debug, Clone, PartialEq)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    /// `--` comments, up to (not including) the line break.
    pub comments: Vec<Span>,
    pub errors: Vec<ParseError>,
}

pub fn lex_tolerant(src: &str) -> Lexed {
    let mut lexer = Lexer { src, pos: 0, comments: Vec::new(), errors: Vec::new() };
    let tokens = lexer.run();
    Lexed { tokens, comments: lexer.comments, errors: lexer.errors }
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    comments: Vec<Span>,
    errors: Vec<ParseError>,
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek2(&self) -> Option<char> {
        let mut it = self.src[self.pos..].chars();
        it.next();
        it.next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn run(&mut self) -> Vec<Token> {
        let mut out = Vec::new();
        loop {
            self.skip_trivia();
            let start = self.pos;
            let Some(c) = self.peek() else {
                out.push(Token { tok: Tok::Eof, span: Span::new(start, start) });
                return out;
            };
            match self.token(c, start) {
                Ok(tok) => out.push(Token { tok, span: Span::new(start, self.pos) }),
                Err(e) => {
                    let string = matches!(e, ParseError::UnterminatedString { .. } | ParseError::BadEscape { .. });
                    self.errors.push(e);
                    if string {
                        self.recover_string(start);
                        let ci = self.src[start..].starts_with('i');
                        out.push(Token {
                            tok: Tok::Str { value: String::new(), ci },
                            span: Span::new(start, self.pos),
                        });
                    }
                }
            }
        }
    }

    /// After a bad string: continue after its closing quote, or at the end of its line.
    fn recover_string(&mut self, start: usize) {
        let open = start + usize::from(self.src[start..].starts_with('i'));
        let quote = self.src[open..].chars().next().expect("a string starts with a quote");
        let mut chars = self.src[open + 1..].char_indices();
        self.pos = loop {
            match chars.next() {
                None => break self.src.len(),
                Some((i, '\n')) => break open + 1 + i,
                Some((_, '\\')) => {
                    chars.next();
                }
                Some((i, c)) if c == quote => break open + 1 + i + 1,
                Some(_) => {}
            }
        };
    }

    fn token(&mut self, c: char, start: usize) -> Result<Tok, ParseError> {
        Ok(match c {
            'i' if matches!(self.peek2(), Some('\'' | '"')) => {
                self.bump();
                let value = self.string(start)?;
                Tok::Str { value, ci: true }
            }
            '\'' | '"' => Tok::Str { value: self.string(start)?, ci: false },
            c if c.is_alphabetic() || c == '_' => {
                while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    self.bump();
                }
                let word = &self.src[start..self.pos];
                match Kw::parse(word) {
                    Some(kw) => Tok::Kw(kw),
                    None => Tok::Ident(word.to_string()),
                }
            }
            c if c.is_ascii_digit() => self.number(start)?,
            _ => {
                self.bump();
                match c {
                    '=' => Tok::Eq,
                    '!' if self.peek() == Some('=') => {
                        self.bump();
                        Tok::NotEq
                    }
                    '<' if self.peek() == Some('=') => {
                        self.bump();
                        Tok::Le
                    }
                    '>' if self.peek() == Some('=') => {
                        self.bump();
                        Tok::Ge
                    }
                    '<' => Tok::Lt,
                    '>' => Tok::Gt,
                    ':' => Tok::Colon,
                    ',' => Tok::Comma,
                    '.' => Tok::Dot,
                    '-' => Tok::Minus,
                    '(' => Tok::LParen,
                    ')' => Tok::RParen,
                    '{' => Tok::LBrace,
                    '}' => Tok::RBrace,
                    '[' => Tok::LBracket,
                    ']' => Tok::RBracket,
                    '|' => {
                        return Err(ParseError::UnexpectedChar {
                            ch: c,
                            span: Span::new(start, self.pos),
                            help: Some("alternatives are written with `OR`, e.g. `'a' OR 'b'`".into()),
                        });
                    }
                    '*' | '+' | '?' => {
                        return Err(ParseError::UnexpectedChar {
                            ch: c,
                            span: Span::new(start, self.pos),
                            help: Some(
                                "repetition is written as `a TO b pattern`, e.g. `1 TO n WORD` or `0 TO 1 WORD`".into(),
                            ),
                        });
                    }
                    _ => {
                        return Err(ParseError::UnexpectedChar { ch: c, span: Span::new(start, self.pos), help: None });
                    }
                }
            }
        })
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('-') if self.peek2() == Some('-') => {
                    let start = self.pos;
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.bump();
                    }
                    let end = start + self.src[start..self.pos].trim_end_matches('\r').len();
                    self.comments.push(Span::new(start, end));
                }
                _ => return,
            }
        }
    }

    fn string(&mut self, start: usize) -> Result<String, ParseError> {
        let quote = self.bump().expect("caller checked for a quote");
        let mut value = String::new();
        loop {
            let esc_start = self.pos;
            match self.bump() {
                None => {
                    return Err(ParseError::UnterminatedString { span: Span::new(start, self.pos) });
                }
                Some(c) if c == quote => return Ok(value),
                Some('\\') => match self.bump() {
                    Some('n') => value.push('\n'),
                    Some('t') => value.push('\t'),
                    Some('r') => value.push('\r'),
                    Some(c @ ('\\' | '\'' | '"')) => value.push(c),
                    Some(c) => {
                        return Err(ParseError::BadEscape { ch: c, span: Span::new(esc_start, self.pos) });
                    }
                    None => {
                        return Err(ParseError::UnterminatedString { span: Span::new(start, self.pos) });
                    }
                },
                Some(c) => value.push(c),
            }
        }
    }

    fn number(&mut self, start: usize) -> Result<Tok, ParseError> {
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        let mut is_float = self.peek() == Some('.') && self.peek2().is_some_and(|c| c.is_ascii_digit());
        if is_float {
            self.bump();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        // JSON-style exponent (`1e-8`, `2.5E+10`), only when digits follow.
        if matches!(self.peek(), Some('e' | 'E')) {
            let rest = &self.src[self.pos + 1..];
            let digits_at = usize::from(rest.starts_with(['+', '-']));
            if rest[digits_at..].starts_with(|c: char| c.is_ascii_digit()) {
                is_float = true;
                for _ in 0..=digits_at {
                    self.bump();
                }
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.bump();
                }
            }
        }
        let text = &self.src[start..self.pos];
        let span = Span::new(start, self.pos);
        if is_float {
            text.parse().map(Tok::Float).map_err(|_| ParseError::BadNumber { span })
        } else {
            text.parse().map(Tok::Int).map_err(|_| ParseError::BadNumber { span })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn keywords_in_any_case() {
        assert_eq!(
            toks("OR or Word word words"),
            vec![
                Tok::Kw(Kw::OR),
                Tok::Kw(Kw::OR),
                Tok::Kw(Kw::WORD),
                Tok::Kw(Kw::WORD),
                Tok::Ident("words".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn strings_and_escapes() {
        assert_eq!(
            toks(r#"'a\'b' "c\"d" i'Are' 'x\ny'"#),
            vec![
                Tok::Str { value: "a'b".into(), ci: false },
                Tok::Str { value: "c\"d".into(), ci: false },
                Tok::Str { value: "Are".into(), ci: true },
                Tok::Str { value: "x\ny".into(), ci: false },
                Tok::Eof
            ]
        );
    }

    #[test]
    fn ident_starting_with_i_is_not_a_string() {
        assert_eq!(toks("id i"), vec![Tok::Ident("id".into()), Tok::Ident("i".into()), Tok::Eof]);
    }

    #[test]
    fn numbers() {
        assert_eq!(
            toks("1 TO 25 3.5 7."),
            vec![Tok::Int(1), Tok::Kw(Kw::TO), Tok::Int(25), Tok::Float(3.5), Tok::Int(7), Tok::Dot, Tok::Eof]
        );
    }

    #[test]
    fn exponents() {
        assert_eq!(toks("1e-8 2.5E+10 3e5"), vec![Tok::Float(1e-8), Tok::Float(2.5e10), Tok::Float(3e5), Tok::Eof]);
        // Without digits after it, `e` is not an exponent.
        assert_eq!(toks("2em"), vec![Tok::Int(2), Tok::Ident("em".into()), Tok::Eof]);
        assert_eq!(
            toks("2e-x"),
            vec![Tok::Int(2), Tok::Ident("e".into()), Tok::Minus, Tok::Ident("x".into()), Tok::Eof]
        );
    }

    #[test]
    fn operators_and_punctuation() {
        assert_eq!(
            toks("= != < <= > >= : , . - ( ) { } [ ]"),
            vec![
                Tok::Eq,
                Tok::NotEq,
                Tok::Lt,
                Tok::Le,
                Tok::Gt,
                Tok::Ge,
                Tok::Colon,
                Tok::Comma,
                Tok::Dot,
                Tok::Minus,
                Tok::LParen,
                Tok::RParen,
                Tok::LBrace,
                Tok::RBrace,
                Tok::LBracket,
                Tok::RBracket,
                Tok::Eof
            ]
        );
    }

    #[test]
    fn comments_are_skipped() {
        assert_eq!(
            toks("a -- comment ' unterminated\nb"),
            vec![Tok::Ident("a".into()), Tok::Ident("b".into()), Tok::Eof]
        );
    }

    #[test]
    fn spans_are_byte_offsets() {
        let t = lex("  héllo 'x'").unwrap();
        assert_eq!(t[0].span, Span::new(2, 8));
        assert_eq!(t[1].span, Span::new(9, 12));
        assert_eq!(t[2].span, Span::new(12, 12));
    }

    #[test]
    fn unicode_identifiers() {
        assert_eq!(toks("couleur_é 名前"), vec![Tok::Ident("couleur_é".into()), Tok::Ident("名前".into()), Tok::Eof]);
    }

    #[test]
    fn errors() {
        assert!(matches!(lex("'abc"), Err(ParseError::UnterminatedString { .. })));
        assert!(matches!(lex(r"'\q'"), Err(ParseError::BadEscape { ch: 'q', .. })));
        assert!(matches!(lex("a | b"), Err(ParseError::UnexpectedChar { ch: '|', help: Some(_), .. })));
        assert!(matches!(lex("a*"), Err(ParseError::UnexpectedChar { ch: '*', help: Some(_), .. })));
        assert!(matches!(lex("99999999999999999999999"), Err(ParseError::BadNumber { .. })));
        assert!(matches!(lex("@"), Err(ParseError::UnexpectedChar { ch: '@', help: None, .. })));
    }

    #[test]
    fn tolerant_lexing_goes_on_after_errors() {
        // Strings may span lines, so only a string with no closing quote at all is unterminated.
        let l = lex_tolerant("a @ b 'x\\q' c 'open\nd -- note\r\n");
        let kinds: Vec<&Tok> = l.tokens.iter().map(|t| &t.tok).collect();
        assert!(matches!(
            kinds.as_slice(),
            [Tok::Ident(a), Tok::Ident(b), Tok::Str { .. }, Tok::Ident(c), Tok::Str { .. }, Tok::Ident(d), Tok::Eof]
                if a == "a" && b == "b" && c == "c" && d == "d"
        ));
        // The bad escape's string ends at its quote; the unterminated one at its line end.
        assert_eq!(l.tokens[2].span, Span::new(6, 11));
        assert_eq!(l.tokens[4].span, Span::new(14, 19));
        assert_eq!(l.comments, vec![Span::new(22, 29)]);
        assert_eq!(l.errors.len(), 3);
        // The strict lexer reports the first error.
        assert!(matches!(lex("a @ 'x"), Err(ParseError::UnexpectedChar { ch: '@', .. })));
    }

    #[test]
    fn empty_input() {
        assert_eq!(toks(""), vec![Tok::Eof]);
        assert_eq!(toks("   -- only a comment"), vec![Tok::Eof]);
    }
}
