//! Editor files stay in step with the language.

use txtql::check::FUNCTIONS;
use txtql::lexer::Kw;

fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| !w.is_empty()).collect()
}

#[test]
fn sublime_syntax_knows_every_keyword_and_function() {
    let syntax = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/editors/sublime/txtql.sublime-syntax"))
        .expect("the syntax file exists");
    let known = words(&syntax);
    for kw in Kw::ALL {
        assert!(known.contains(&kw.as_str()), "keyword {} is missing from txtql.sublime-syntax", kw.as_str());
    }
    for (name, _, _) in FUNCTIONS {
        assert!(known.contains(name), "function {name} is missing from txtql.sublime-syntax");
    }
    assert!(syntax.contains("file_extensions: [tql]") && syntax.contains("scope: source.txtql"));
}

#[test]
fn sublime_lsp_settings_match_the_syntax_scope() {
    let settings =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/editors/sublime/LSP.sublime-settings"))
            .expect("the settings file exists");
    assert!(settings.contains(r#""selector": "source.txtql""#));
    assert!(settings.contains(r#""command": ["txtql", "lsp"]"#));
}
