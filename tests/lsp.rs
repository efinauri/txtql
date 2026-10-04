//! The language server, driven through an in-memory connection like an editor would.

use lsp_server::{Connection, Message, Notification, Request, RequestId};
use lsp_types as lt;
use serde_json::{Value, json};
use std::thread::JoinHandle;

struct Client {
    conn: Connection,
    server: Option<JoinHandle<()>>,
    next_id: i32,
    /// Notifications received while waiting for responses.
    notifications: Vec<Notification>,
}

const URI: &str = "file:///work/query.tql";

impl Client {
    fn start() -> Client {
        let (server, conn) = Connection::memory();
        let handle = std::thread::spawn(move || txtql::lsp::serve(&server).expect("server runs"));
        let mut c = Client { conn, server: Some(handle), next_id: 0, notifications: Vec::new() };
        let caps = c.request("initialize", json!({ "capabilities": {} }));
        assert!(caps["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"].is_array());
        c.notify("initialized", json!({}));
        c
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        self.conn.sender.send(Message::Request(Request::new(id.clone(), method.into(), params))).unwrap();
        loop {
            match self.conn.receiver.recv().expect("server answers") {
                Message::Response(r) if r.id == id => match r.response_result {
                    Ok(v) => return v,
                    Err(e) => panic!("{method} failed: {e:?}"),
                },
                Message::Notification(n) => self.notifications.push(n),
                other => panic!("unexpected message {other:?}"),
            }
        }
    }

    fn request_err(&mut self, method: &str, params: Value) -> String {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        self.conn.sender.send(Message::Request(Request::new(id.clone(), method.into(), params))).unwrap();
        loop {
            match self.conn.receiver.recv().unwrap() {
                Message::Response(r) if r.id == id => return r.response_result.expect_err("an error").message,
                Message::Notification(n) => self.notifications.push(n),
                other => panic!("unexpected message {other:?}"),
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.conn.sender.send(Message::Notification(Notification::new(method.into(), params))).unwrap();
    }

    /// The next diagnostics published for the document.
    fn diagnostics(&mut self) -> Vec<Value> {
        let n = match self.notifications.iter().position(|n| n.method == "textDocument/publishDiagnostics") {
            Some(k) => self.notifications.remove(k),
            None => loop {
                match self.conn.receiver.recv().unwrap() {
                    Message::Notification(n) if n.method == "textDocument/publishDiagnostics" => break n,
                    Message::Notification(n) => self.notifications.push(n),
                    other => panic!("unexpected message {other:?}"),
                }
            },
        };
        n.params["diagnostics"].as_array().unwrap().clone()
    }

    fn open(&mut self, text: &str) -> Vec<Value> {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": URI, "languageId": "txtql", "version": 1, "text": text } }),
        );
        self.diagnostics()
    }

    fn at(&mut self, method: &str, line: u32, character: u32) -> Value {
        self.request(
            method,
            json!({ "textDocument": { "uri": URI }, "position": { "line": line, "character": character } }),
        )
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let _ = self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        self.server.take().unwrap().join().expect("server stops cleanly");
    }
}

const QUERY: &str = "\
-- One line of the log.
TEXT = 1 TO n e:entry AS [ e.level FOR e ]
entry = '[' level:(ANY UNTIL ']') ' ' msg:rest NL
ALIAS rest = ANY UNTILBEFORE NL
";

fn range(line: u32, from: u32, to: u32) -> Value {
    json!({ "start": { "line": line, "character": from }, "end": { "line": line, "character": to } })
}

#[test]
fn diagnostics_follow_the_text() {
    let mut c = Client::start();
    assert_eq!(c.open(QUERY), Vec::<Value>::new());
    c.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": URI, "version": 2 }, "contentChanges": [{ "text": "TEXT = entri\nentry = WORD\n" }] }),
    );
    let d = c.diagnostics();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0]["range"], range(0, 7, 12));
    assert_eq!(d[0]["severity"], 1);
    assert_eq!(d[0]["code"], "txtql::check::undefined_rule");
    assert!(d[0]["message"].as_str().unwrap().contains("did you mean"), "{}", d[0]["message"]);
    // Warnings are warnings.
    c.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": URI, "version": 3 }, "contentChanges": [{ "text": "TEXT = WORD\nunused = INT\n" }] }),
    );
    assert_eq!(c.diagnostics()[0]["severity"], 2);
    c.notify("textDocument/didClose", json!({ "textDocument": { "uri": URI } }));
    assert_eq!(c.diagnostics(), Vec::<Value>::new());
}

#[test]
fn semantic_tokens() {
    let mut c = Client::start();
    c.open(QUERY);
    let caps = txtql::lsp::capabilities();
    let Some(lt::SemanticTokensServerCapabilities::SemanticTokensOptions(opts)) = caps.semantic_tokens_provider else {
        panic!()
    };
    let types: Vec<String> = opts.legend.token_types.iter().map(|t| t.as_str().to_string()).collect();
    let data = c.request("textDocument/semanticTokens/full", json!({ "textDocument": { "uri": URI } }));
    let data: Vec<u32> = serde_json::from_value(data["data"].clone()).unwrap();
    // Decode to (line, column, text, type, modifiers).
    let lines: Vec<&str> = QUERY.lines().collect();
    let (mut line, mut col) = (0, 0);
    let mut toks = Vec::new();
    for t in data.chunks(5) {
        if t[0] > 0 {
            col = 0;
        }
        line += t[0];
        col += t[1];
        let text: String = lines[line as usize].chars().skip(col as usize).take(t[2] as usize).collect();
        toks.push((line, text, types[t[3] as usize].clone(), t[4]));
    }
    let find = |text: &str, line: u32| toks.iter().find(|t| t.1 == text && t.0 == line).cloned().unwrap();
    assert_eq!(find("-- One line of the log.", 0).2, "comment");
    assert_eq!(find("TEXT", 1), (1, "TEXT".into(), "function".into(), 1));
    assert_eq!(find("e", 1).2, "variable");
    assert_eq!(find("entry", 1).2, "function");
    assert_eq!(find("level", 1).2, "property");
    assert_eq!(find("UNTIL", 2).2, "keyword");
    assert_eq!(find("ANY", 2), (2, "ANY".into(), "type".into(), 2));
    assert_eq!(find("rest", 2).2, "macro");
    assert_eq!(find("' '", 2).2, "string");
}

#[test]
fn navigation() {
    let mut c = Client::start();
    c.open(QUERY);
    // `entry` in TEXT goes to the rule on line 2.
    let def = c.at("textDocument/definition", 1, 17);
    assert_eq!(def["range"], range(2, 0, 5));
    // `e` in the template goes to the label.
    let def = c.at("textDocument/definition", 1, 27);
    assert_eq!(def["range"], range(1, 14, 15));
    // References of the alias, with and without the declaration.
    let refs = c.request(
        "textDocument/references",
        json!({ "textDocument": { "uri": URI }, "position": { "line": 3, "character": 7 }, "context": { "includeDeclaration": true } }),
    );
    assert_eq!(refs.as_array().unwrap().len(), 2);
    let refs = c.request(
        "textDocument/references",
        json!({ "textDocument": { "uri": URI }, "position": { "line": 3, "character": 7 }, "context": { "includeDeclaration": false } }),
    );
    assert_eq!(refs.as_array().unwrap().len(), 1);
    let highlights = c.at("textDocument/documentHighlight", 1, 14);
    assert_eq!(highlights.as_array().unwrap().len(), 3);
    // Nothing to find on a keyword.
    assert_eq!(c.at("textDocument/definition", 1, 9), Value::Null);
}

#[test]
fn rename() {
    let mut c = Client::start();
    c.open(QUERY);
    let prep = c.at("textDocument/prepareRename", 2, 1);
    assert_eq!(prep["placeholder"], "entry");
    // TEXT is the root and keeps its name.
    assert_eq!(c.at("textDocument/prepareRename", 1, 1), Value::Null);
    let edit = c.request(
        "textDocument/rename",
        json!({ "textDocument": { "uri": URI }, "position": { "line": 2, "character": 12 }, "newName": "lvl" }),
    );
    let edits = edit["changes"][URI].as_array().unwrap();
    assert_eq!(edits.len(), 1, "the label is only used as a label: {edits:?}");
    let err = c.request_err(
        "textDocument/rename",
        json!({ "textDocument": { "uri": URI }, "position": { "line": 2, "character": 1 }, "newName": "WORD" }),
    );
    assert!(err.contains("not a valid name"), "{err}");
}

#[test]
fn hover_completion_and_outline() {
    let mut c = Client::start();
    c.open(QUERY);
    let hover = c.at("textDocument/hover", 2, 25);
    assert!(hover["contents"]["value"].as_str().unwrap().contains("consume the stop"), "{hover}");
    let hover = c.at("textDocument/hover", 1, 17);
    let text = hover["contents"]["value"].as_str().unwrap();
    assert!(text.contains("```txtql\nentry = '['"), "{text}");
    let items = c.at("textDocument/completion", 2, 8);
    let labels: Vec<&str> = items.as_array().unwrap().iter().map(|i| i["label"].as_str().unwrap()).collect();
    assert!(labels.contains(&"WORD") && labels.contains(&"rest") && labels.contains(&"UNTILBEFORE"), "{labels:?}");
    let symbols = c.request("textDocument/documentSymbol", json!({ "textDocument": { "uri": URI } }));
    let names: Vec<&str> = symbols.as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["TEXT", "entry", "rest"]);
    assert_eq!(symbols[1]["children"][0]["name"], "level");
}

#[test]
fn unknown_requests_fail_politely() {
    let mut c = Client::start();
    let err = c.request_err(
        "textDocument/formatting",
        json!({ "textDocument": { "uri": URI }, "options": { "tabSize": 4, "insertSpaces": true } }),
    );
    assert!(err.contains("unsupported"), "{err}");
    // Requests on unknown documents answer null.
    assert_eq!(c.at("textDocument/hover", 0, 0), Value::Null);
}
