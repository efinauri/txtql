//! The language server behind `txtql lsp`: diagnostics, semantic highlighting, go to
//! definition, references, highlights, rename, hover, completion and an outline.
//!
//! Documents are synchronised in full on every change (queries are small) and analysed with
//! [`crate::analysis`]. Positions are converted between byte offsets and the protocol's UTF-16
//! columns by [`LineIndex`].

use crate::analysis::{Analysis, CompletionKind, DefKind, Severity, TokenKind};
use crate::error::Span;
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types as lt;
use lsp_types::notification::Notification as _;
use std::collections::HashMap;
use std::error::Error;

pub type BoxError = Box<dyn Error + Send + Sync>;

/// Serves the protocol over standard input and output until the client says `exit`.
pub fn run_stdio() -> Result<(), BoxError> {
    let (connection, io_threads) = Connection::stdio();
    serve(&connection)?;
    drop(connection);
    io_threads.join()?;
    Ok(())
}

/// Serves the protocol on `connection`: the initialize handshake, then requests and
/// notifications until shutdown.
pub fn serve(connection: &Connection) -> Result<(), BoxError> {
    connection.initialize(serde_json::to_value(capabilities())?)?;
    let mut server = Server { docs: HashMap::new() };
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                let response = server.request(req);
                connection.sender.send(Message::Response(response))?;
            }
            Message::Notification(n) => {
                for out in server.notification(n) {
                    connection.sender.send(Message::Notification(out))?;
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

/// Semantic token types, in legend order.
const TOKEN_TYPES: &[lt::SemanticTokenType] = &[
    lt::SemanticTokenType::COMMENT,
    lt::SemanticTokenType::STRING,
    lt::SemanticTokenType::NUMBER,
    lt::SemanticTokenType::KEYWORD,
    lt::SemanticTokenType::TYPE,
    lt::SemanticTokenType::FUNCTION,
    lt::SemanticTokenType::MACRO,
    lt::SemanticTokenType::VARIABLE,
    lt::SemanticTokenType::PARAMETER,
    lt::SemanticTokenType::PROPERTY,
];
const TOKEN_MODIFIERS: &[lt::SemanticTokenModifier] =
    &[lt::SemanticTokenModifier::DECLARATION, lt::SemanticTokenModifier::DEFAULT_LIBRARY];
const DECLARATION: u32 = 1;
const DEFAULT_LIBRARY: u32 = 2;

/// Legend index and modifiers of a token kind.
fn token_type(kind: TokenKind) -> (u32, u32) {
    match kind {
        TokenKind::Comment => (0, 0),
        TokenKind::String => (1, 0),
        TokenKind::Number => (2, 0),
        TokenKind::Keyword | TokenKind::Constant => (3, 0),
        TokenKind::Primitive => (4, DEFAULT_LIBRARY),
        TokenKind::Rule => (5, 0),
        TokenKind::Function => (5, DEFAULT_LIBRARY),
        TokenKind::Alias => (6, 0),
        TokenKind::Label | TokenKind::Capture => (7, 0),
        TokenKind::Variable => (8, 0),
        TokenKind::Field => (9, 0),
    }
}

pub fn capabilities() -> lt::ServerCapabilities {
    lt::ServerCapabilities {
        text_document_sync: Some(lt::TextDocumentSyncCapability::Kind(lt::TextDocumentSyncKind::FULL)),
        semantic_tokens_provider: Some(lt::SemanticTokensServerCapabilities::SemanticTokensOptions(
            lt::SemanticTokensOptions {
                legend: lt::SemanticTokensLegend {
                    token_types: TOKEN_TYPES.to_vec(),
                    token_modifiers: TOKEN_MODIFIERS.to_vec(),
                },
                full: Some(lt::SemanticTokensFullOptions::Bool(true)),
                ..Default::default()
            },
        )),
        definition_provider: Some(lt::OneOf::Left(true)),
        references_provider: Some(lt::OneOf::Left(true)),
        document_highlight_provider: Some(lt::OneOf::Left(true)),
        rename_provider: Some(lt::OneOf::Right(lt::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        hover_provider: Some(lt::HoverProviderCapability::Simple(true)),
        completion_provider: Some(lt::CompletionOptions::default()),
        document_symbol_provider: Some(lt::OneOf::Left(true)),
        ..Default::default()
    }
}

struct Doc {
    analysis: Analysis,
    index: LineIndex,
}

struct Server {
    docs: HashMap<lt::Uri, Doc>,
}

impl Server {
    fn notification(&mut self, n: Notification) -> Vec<Notification> {
        use lt::notification::{DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument};
        match n.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let Ok(p) = n.extract::<lt::DidOpenTextDocumentParams>(DidOpenTextDocument::METHOD) else {
                    return Vec::new();
                };
                self.update(p.text_document.uri, p.text_document.text, Some(p.text_document.version))
            }
            DidChangeTextDocument::METHOD => {
                let Ok(p) = n.extract::<lt::DidChangeTextDocumentParams>(DidChangeTextDocument::METHOD) else {
                    return Vec::new();
                };
                // Full synchronisation: the last change holds the whole text.
                match p.content_changes.into_iter().last() {
                    Some(change) => self.update(p.text_document.uri, change.text, Some(p.text_document.version)),
                    None => Vec::new(),
                }
            }
            DidCloseTextDocument::METHOD => {
                let Ok(p) = n.extract::<lt::DidCloseTextDocumentParams>(DidCloseTextDocument::METHOD) else {
                    return Vec::new();
                };
                self.docs.remove(&p.text_document.uri);
                vec![publish(p.text_document.uri, Vec::new(), None)]
            }
            _ => Vec::new(),
        }
    }

    fn update(&mut self, uri: lt::Uri, text: String, version: Option<i32>) -> Vec<Notification> {
        let analysis = Analysis::new(&text);
        let index = LineIndex::new(&text);
        let diagnostics = analysis
            .diagnostics
            .iter()
            .map(|d| lt::Diagnostic {
                range: index.range(d.span),
                severity: Some(match d.severity {
                    Severity::Error => lt::DiagnosticSeverity::ERROR,
                    Severity::Warning => lt::DiagnosticSeverity::WARNING,
                }),
                code: d.code.clone().map(lt::NumberOrString::String),
                source: Some("txtql".into()),
                message: d.message.clone(),
                related_information: (!d.related.is_empty()).then(|| {
                    d.related
                        .iter()
                        .map(|(span, label)| lt::DiagnosticRelatedInformation {
                            location: lt::Location { uri: uri.clone(), range: index.range(*span) },
                            message: label.clone(),
                        })
                        .collect()
                }),
                ..Default::default()
            })
            .collect();
        self.docs.insert(uri.clone(), Doc { analysis, index });
        vec![publish(uri, diagnostics, version)]
    }

    fn request(&mut self, req: Request) -> Response {
        let id = req.id.clone();
        match self.dispatch(req) {
            Ok(value) => Response::new_ok(id, value),
            Err((code, message)) => Response::new_err(id, code as i32, message),
        }
    }

    fn dispatch(&mut self, req: Request) -> Result<serde_json::Value, (ErrorCode, String)> {
        use lt::request::*;
        let json = |v: serde_json::Result<serde_json::Value>| v.map_err(|e| (ErrorCode::InternalError, e.to_string()));
        match req.method.as_str() {
            SemanticTokensFullRequest::METHOD => {
                let (_, p) = extract::<SemanticTokensFullRequest>(req)?;
                json(serde_json::to_value(self.doc(&p.text_document.uri).map(semantic_tokens)))
            }
            GotoDefinition::METHOD => {
                let (_, p) = extract::<GotoDefinition>(req)?;
                let pos = p.text_document_position_params;
                let result = self.at(&pos, |doc, offset| {
                    let span = doc.analysis.definition_at(offset)?;
                    Some(lt::GotoDefinitionResponse::Scalar(lt::Location {
                        uri: pos.text_document.uri.clone(),
                        range: doc.index.range(span),
                    }))
                });
                json(serde_json::to_value(result))
            }
            References::METHOD => {
                let (_, p) = extract::<References>(req)?;
                let pos = p.text_document_position;
                let with_decl = p.context.include_declaration;
                let result = self.at(&pos, |doc, offset| {
                    let decl = doc.analysis.definition_at(offset);
                    let spans = doc.analysis.references_at(offset);
                    let locations = spans
                        .into_iter()
                        .filter(|s| with_decl || Some(*s) != decl)
                        .map(|s| lt::Location { uri: pos.text_document.uri.clone(), range: doc.index.range(s) })
                        .collect::<Vec<_>>();
                    Some(locations)
                });
                json(serde_json::to_value(result))
            }
            DocumentHighlightRequest::METHOD => {
                let (_, p) = extract::<DocumentHighlightRequest>(req)?;
                let pos = p.text_document_position_params;
                let result = self.at(&pos, |doc, offset| {
                    let decl = doc.analysis.definition_at(offset);
                    let highlights = doc
                        .analysis
                        .references_at(offset)
                        .into_iter()
                        .map(|s| lt::DocumentHighlight {
                            range: doc.index.range(s),
                            kind: Some(if Some(s) == decl {
                                lt::DocumentHighlightKind::WRITE
                            } else {
                                lt::DocumentHighlightKind::READ
                            }),
                        })
                        .collect::<Vec<_>>();
                    Some(highlights)
                });
                json(serde_json::to_value(result))
            }
            PrepareRenameRequest::METHOD => {
                let (_, p) = extract::<PrepareRenameRequest>(req)?;
                let result = self.at(&p, |doc, offset| {
                    let (span, name) = renamable(&doc.analysis, offset)?;
                    Some(lt::PrepareRenameResponse::RangeWithPlaceholder {
                        range: doc.index.range(span),
                        placeholder: name,
                    })
                });
                json(serde_json::to_value(result))
            }
            Rename::METHOD => {
                let (_, p) = extract::<Rename>(req)?;
                let pos = p.text_document_position;
                if !is_identifier(&p.new_name) {
                    return Err((ErrorCode::InvalidParams, format!("`{}` is not a valid name", p.new_name)));
                }
                let result = self.at(&pos, |doc, offset| {
                    renamable(&doc.analysis, offset)?;
                    let edits = doc
                        .analysis
                        .references_at(offset)
                        .into_iter()
                        .map(|s| lt::TextEdit { range: doc.index.range(s), new_text: p.new_name.clone() })
                        .collect();
                    #[allow(clippy::mutable_key_type)]
                    let changes = HashMap::from([(pos.text_document.uri.clone(), edits)]);
                    Some(lt::WorkspaceEdit { changes: Some(changes), ..Default::default() })
                });
                json(serde_json::to_value(result))
            }
            HoverRequest::METHOD => {
                let (_, p) = extract::<HoverRequest>(req)?;
                let result = self.at(&p.text_document_position_params, |doc, offset| {
                    let (span, text) = doc.analysis.hover_at(offset)?;
                    Some(lt::Hover {
                        contents: lt::HoverContents::Markup(lt::MarkupContent {
                            kind: lt::MarkupKind::Markdown,
                            value: text,
                        }),
                        range: Some(doc.index.range(span)),
                    })
                });
                json(serde_json::to_value(result))
            }
            Completion::METHOD => {
                let (_, p) = extract::<Completion>(req)?;
                let result = self.at(&p.text_document_position, |doc, offset| {
                    let items = doc
                        .analysis
                        .completions_at(offset)
                        .into_iter()
                        .map(|c| lt::CompletionItem {
                            label: c.label,
                            kind: Some(match c.kind {
                                CompletionKind::Keyword => lt::CompletionItemKind::KEYWORD,
                                CompletionKind::Primitive => lt::CompletionItemKind::CLASS,
                                CompletionKind::Rule => lt::CompletionItemKind::FUNCTION,
                                CompletionKind::Alias => lt::CompletionItemKind::CONSTANT,
                                CompletionKind::Capture => lt::CompletionItemKind::VARIABLE,
                                CompletionKind::Function => lt::CompletionItemKind::METHOD,
                                CompletionKind::Constant => lt::CompletionItemKind::VALUE,
                            }),
                            detail: (!c.detail.is_empty()).then_some(c.detail),
                            ..Default::default()
                        })
                        .collect();
                    Some(lt::CompletionResponse::Array(items))
                });
                json(serde_json::to_value(result))
            }
            DocumentSymbolRequest::METHOD => {
                let (_, p) = extract::<DocumentSymbolRequest>(req)?;
                let result =
                    self.doc(&p.text_document.uri).map(|doc| lt::DocumentSymbolResponse::Nested(document_symbols(doc)));
                json(serde_json::to_value(result))
            }
            _ => Err((ErrorCode::MethodNotFound, format!("unsupported request `{}`", req.method))),
        }
    }

    fn doc(&self, uri: &lt::Uri) -> Option<&Doc> {
        self.docs.get(uri)
    }

    /// Runs `f` on the document and byte offset of a text position.
    fn at<T>(&self, pos: &lt::TextDocumentPositionParams, f: impl FnOnce(&Doc, usize) -> Option<T>) -> Option<T> {
        let doc = self.doc(&pos.text_document.uri)?;
        f(doc, doc.index.offset(pos.position))
    }
}

fn extract<R: lt::request::Request>(req: Request) -> Result<(RequestId, R::Params), (ErrorCode, String)> {
    req.extract(R::METHOD).map_err(|e| (ErrorCode::InvalidParams, format!("{e:?}")))
}

fn publish(uri: lt::Uri, diagnostics: Vec<lt::Diagnostic>, version: Option<i32>) -> Notification {
    use lt::notification::PublishDiagnostics;
    Notification::new(PublishDiagnostics::METHOD.into(), lt::PublishDiagnosticsParams { uri, diagnostics, version })
}

/// A name that can be renamed: rules and aliases (but not `TEXT`), labels and loop variables.
fn renamable(a: &Analysis, offset: usize) -> Option<(Span, String)> {
    let d = a.def_at(offset)?;
    let def = &a.defs[d];
    if def.kind == DefKind::Rule && crate::ast::is_root_name(&def.name) {
        return None;
    }
    a.rename_target(offset)
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
        && crate::lexer::Kw::parse(s).is_none()
}

fn semantic_tokens(doc: &Doc) -> lt::SemanticTokensResult {
    let mut data = Vec::new();
    let (mut prev_line, mut prev_col) = (0u32, 0u32);
    for tok in &doc.analysis.tokens {
        let (ty, mut mods) = token_type(tok.kind);
        if tok.definition {
            mods |= DECLARATION;
        }
        // Tokens may not span lines (strings can): emit one piece per line.
        for piece in doc.index.split_lines(tok.span) {
            let start = doc.index.position(piece.start);
            let length = doc.index.utf16_len(piece);
            if length == 0 {
                continue;
            }
            let delta_line = start.line - prev_line;
            let delta_start = if delta_line == 0 { start.character - prev_col } else { start.character };
            data.push(lt::SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type: ty,
                token_modifiers_bitset: mods,
            });
            (prev_line, prev_col) = (start.line, start.character);
        }
    }
    lt::SemanticTokensResult::Tokens(lt::SemanticTokens { result_id: None, data })
}

#[allow(deprecated)]
fn document_symbols(doc: &Doc) -> Vec<lt::DocumentSymbol> {
    let a = &doc.analysis;
    a.blocks
        .iter()
        .enumerate()
        .map(|(k, b)| {
            let children: Vec<lt::DocumentSymbol> = a
                .defs
                .iter()
                .filter(|d| d.block == k && d.kind == DefKind::Label)
                .map(|d| lt::DocumentSymbol {
                    name: d.name.clone(),
                    detail: None,
                    kind: lt::SymbolKind::FIELD,
                    tags: None,
                    deprecated: None,
                    range: doc.index.range(d.span),
                    selection_range: doc.index.range(d.span),
                    children: None,
                })
                .collect();
            lt::DocumentSymbol {
                name: b.name.clone(),
                detail: Some(if b.kind == DefKind::Alias { "alias".into() } else { "rule".into() }),
                kind: if b.kind == DefKind::Alias { lt::SymbolKind::CONSTANT } else { lt::SymbolKind::FUNCTION },
                tags: None,
                deprecated: None,
                range: doc.index.range(b.span),
                selection_range: doc.index.range(a.defs[b.def].span),
                children: (!children.is_empty()).then_some(children),
            }
        })
        .collect()
}

/// Converts between byte offsets and protocol positions (lines and UTF-16 columns).
pub struct LineIndex {
    text: String,
    /// Byte offset of the start of each line.
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { text: text.to_string(), starts }
    }

    pub fn position(&self, offset: usize) -> lt::Position {
        let offset = offset.min(self.text.len());
        let line = self.starts.partition_point(|&s| s <= offset) - 1;
        let character = self.text[self.starts[line]..offset].encode_utf16().count();
        lt::Position { line: line as u32, character: character as u32 }
    }

    /// The byte offset of a position. Positions past the end of a line clamp to its end.
    pub fn offset(&self, pos: lt::Position) -> usize {
        let Some(&start) = self.starts.get(pos.line as usize) else { return self.text.len() };
        let end = self.starts.get(pos.line as usize + 1).map_or(self.text.len(), |&e| e - 1);
        let mut units = 0;
        for (i, c) in self.text[start..end].char_indices() {
            if units >= pos.character as usize {
                return start + i;
            }
            units += c.len_utf16();
        }
        end
    }

    pub fn range(&self, span: Span) -> lt::Range {
        lt::Range { start: self.position(span.start), end: self.position(span.end) }
    }

    fn utf16_len(&self, span: Span) -> u32 {
        self.text[span.start..span.end].encode_utf16().count() as u32
    }

    /// `span` cut at line breaks (the breaks themselves left out).
    fn split_lines(&self, span: Span) -> Vec<Span> {
        let mut out = Vec::new();
        let mut start = span.start;
        for (i, _) in self.text[span.start..span.end].match_indices('\n') {
            let end = span.start + i;
            let end = if self.text[..end].ends_with('\r') { end - 1 } else { end };
            out.push(Span::new(start, end.max(start)));
            start = span.start + i + 1;
        }
        out.push(Span::new(start, span.end));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_utf16_units() {
        let idx = LineIndex::new("a — b\n𝄞x\n");
        // The em dash is 3 bytes and 1 UTF-16 unit; 𝄞 is 4 bytes and 2 units.
        assert_eq!(idx.position(5), lt::Position { line: 0, character: 3 });
        assert_eq!(idx.position(12), lt::Position { line: 1, character: 2 });
        assert_eq!(idx.offset(lt::Position { line: 1, character: 2 }), 12);
        assert_eq!(idx.offset(lt::Position { line: 0, character: 99 }), 7);
        assert_eq!(idx.offset(lt::Position { line: 9, character: 0 }), 14);
    }

    #[test]
    fn multi_line_tokens_are_split() {
        let idx = LineIndex::new("x = 'a\r\nb'\n");
        assert_eq!(idx.split_lines(Span::new(4, 10)), vec![Span::new(4, 6), Span::new(8, 10)]);
    }

    #[test]
    fn identifiers_for_rename() {
        assert!(is_identifier("new_name") && is_identifier("été"));
        assert!(!is_identifier("1x") && !is_identifier("a-b") && !is_identifier("WORD") && !is_identifier(""));
    }
}
