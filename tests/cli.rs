//! The command-line interface: exit codes, input sources, and error output.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn txtql(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_txtql"))
        .args(args)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

fn case_file(case: &str, file: &str) -> String {
    let p: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "cases", case, file].iter().collect();
    p.display().to_string()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn runs_query_file_on_input_file() {
    let o = txtql(&[&case_file("rhymes_map", "query.tql"), &case_file("rhymes_map", "input.txt")], b"");
    assert!(o.status.success(), "{}", stderr(&o));
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v, serde_json::json!({"roses": ["red"], "violets": ["blue"], "bees": ["black", "yellow"]}));
}

#[test]
fn reads_standard_input() {
    let o = txtql(&["--compact", "-e", "TEXT = 1 TO n WORD SPLITBY ' '"], b"a b c");
    assert!(o.status.success());
    assert_eq!(stdout(&o), "[\"a\",\"b\",\"c\"]\n");
    let o = txtql(&["--compact", "-e", "TEXT = 1 TO n WORD SPLITBY ' '", "-"], b"x");
    assert_eq!(stdout(&o), "[\"x\"]\n");
}

#[test]
fn explicit_run_subcommand() {
    let o = txtql(&["run", "--compact", "-e", "TEXT = WORD"], b"x");
    assert_eq!(stdout(&o), "\"x\"\n");
}

#[test]
fn compile_errors_exit_nonzero_with_diagnostics() {
    let o = txtql(&["-e", "TEXT = colr"], b"x");
    assert_eq!(o.status.code(), Some(1));
    assert!(stdout(&o).is_empty());
    assert!(stderr(&o).contains("txtql::check::undefined_rule"), "{}", stderr(&o));
}

#[test]
fn match_errors_exit_nonzero_with_diagnostics() {
    let o = txtql(&["-e", "TEXT = WORD ' are ' WORD"], b"roses is red");
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("expected ' are '"), "{}", stderr(&o));
}

#[test]
fn ambiguity_warnings_go_to_stderr() {
    let o = txtql(&["--compact", "-e", "TEXT = 1 TO n a:DIGIT 1 TO n b:DIGIT"], b"123");
    assert!(o.status.success());
    assert_eq!(stdout(&o), "{\"a\":[\"1\",\"2\"],\"b\":[\"3\"]}\n");
    assert!(stderr(&o).contains("txtql::input::ambiguous"));
    let o = txtql(&["--strict", "-e", "TEXT = 1 TO n a:DIGIT 1 TO n b:DIGIT"], b"123");
    assert_eq!(o.status.code(), Some(1));
    assert!(stdout(&o).is_empty());
    let o = txtql(&["--no-ambiguity-check", "-e", "TEXT = 1 TO n a:DIGIT 1 TO n b:DIGIT"], b"123");
    assert!(stderr(&o).is_empty(), "{}", stderr(&o));
}

#[test]
fn lint_warnings_do_not_fail() {
    let o = txtql(&["--compact", "-e", "unused = WORD\nTEXT = FLOAT"], b"1");
    assert!(o.status.success());
    assert!(stderr(&o).contains("txtql::lint::unused_rule"));
}

#[test]
fn check_subcommand() {
    let o = txtql(&["check", "-e", "TEXT = WORD"], b"");
    assert!(o.status.success());
    assert!(stderr(&o).contains("query OK"));
    // With a sample input, output-changing ambiguity gives exit code 2.
    let o =
        txtql(&["check", &case_file("ambiguous_split", "query.tql"), &case_file("ambiguous_split", "input.txt")], b"");
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert!(stderr(&o).contains("1 output-changing ambiguity found"));
    let o = txtql(&["check", &case_file("rhymes_map", "query.tql"), &case_file("rhymes_map", "input.txt")], b"");
    assert!(o.status.success(), "{}", stderr(&o));
}

#[test]
fn invalid_utf8_input() {
    let o = txtql(&["-e", "TEXT = 0 TO n ANY"], b"ok \xff\xfe bad");
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("not valid UTF-8 (invalid byte at offset 3)"), "{}", stderr(&o));
}

#[test]
fn missing_files() {
    let o = txtql(&["/nonexistent/query.tql"], b"");
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("cannot read query file"), "{}", stderr(&o));
    let o = txtql(&["-e", "TEXT = WORD", "/nonexistent/input.txt"], b"");
    assert!(stderr(&o).contains("cannot read input file"), "{}", stderr(&o));
}

#[test]
fn no_query_given() {
    let o = txtql(&[], b"");
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("no query given"));
}

#[test]
fn too_many_arguments() {
    let o = txtql(&["-e", "TEXT = WORD", "a", "b"], b"");
    assert_ne!(o.status.code(), Some(0));
}

#[test]
fn deep_nesting_through_the_cli() {
    let depth = 50_000;
    let input = format!("{}{}", "(".repeat(depth), ")".repeat(depth));
    // Deeper than the default limit, so the flag is needed.
    let o = txtql(&["--compact", "--max-depth", "100000", "-e", "p = '(' 0 TO 1 p ')'\nTEXT = p"], input.as_bytes());
    assert!(o.status.success(), "{}", stderr(&o));
}

// ---- the nesting limit matches the library's ----

fn nested(depth: usize) -> String {
    format!("{}{}", "(".repeat(depth), ")".repeat(depth))
}

#[test]
fn max_depth_default_is_the_library_default() {
    let limit = txtql::Options::default().max_depth;
    assert_eq!(limit, 10_000);
    let o = txtql(&["run", "--help"], b"");
    assert!(stdout(&o).contains(&format!("[default: {limit}]")), "{}", stdout(&o));
    let q = "p = '(' 0 TO 1 p ')'\nTEXT = p";
    // Well within the limit: fine.
    let o = txtql(&["--compact", "-e", q], nested(limit / 2).as_bytes());
    assert!(o.status.success(), "{}", stderr(&o));
    // Twice the limit: too deep, by default, without any flag.
    let o = txtql(&["--compact", "-e", q], nested(limit * 2).as_bytes());
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("txtql::input::too_deep"), "{}", stderr(&o));
    // The flag still raises it.
    let o = txtql(&["--compact", "--max-depth", &(limit * 4).to_string(), "-e", q], nested(limit * 2).as_bytes());
    assert!(o.status.success(), "{}", stderr(&o));
}

#[test]
fn check_uses_the_library_max_depth() {
    let limit = txtql::Options::default().max_depth;
    let dir = std::env::temp_dir().join(format!("txtql-cli-depth-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("deep.txt");
    std::fs::write(&input, nested(limit * 2)).unwrap();
    let o = txtql(&["check", "-e", "p = '(' 0 TO 1 p ')'\nTEXT = p", input.to_str().unwrap()], b"");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("txtql::input::too_deep"), "{}", stderr(&o));
}

// ---- `check` with a sample: exit codes and the summary ----

fn last_line(s: &str) -> String {
    s.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("").to_string()
}

#[test]
fn check_exits_2_on_repeated_keys() {
    let o = txtql(
        &["check", &case_file("repeated_key_warning", "query.tql"), &case_file("repeated_key_warning", "input.txt")],
        b"",
    );
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert!(stderr(&o).contains("txtql::eval::repeated_key"), "{}", stderr(&o));
    assert!(!stderr(&o).contains("query OK"), "{}", stderr(&o));
    let summary = last_line(&stderr(&o));
    assert!(summary.contains("1 repeated key"), "{summary}");
}

#[test]
fn check_exits_2_on_zip_repeated_keys() {
    let o = txtql(&["check", "-e", "TEXT = WORD AS ZIP(['a', 'b', 'a'], [1, 2, 3])", "-"], b"x");
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert!(stderr(&o).contains("txtql::eval::repeated_key"), "{}", stderr(&o));
    let summary = last_line(&stderr(&o));
    assert!(summary.contains("1 repeated key"), "{summary}");
}

#[test]
fn check_summary_counts_ambiguities_and_repeated_keys() {
    let dir = std::env::temp_dir().join(format!("txtql-cli-summary-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("digits.txt");
    std::fs::write(&input, "123").unwrap();
    let o = txtql(
        &["check", "-e", "TEXT = 1 TO n a:DIGIT 1 TO n b:DIGIT AS ZIP(['k', 'k'], [a, b])", input.to_str().unwrap()],
        b"",
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    let summary = last_line(&stderr(&o));
    assert!(summary.contains("1 output-changing ambiguity"), "{summary}");
    assert!(summary.contains("1 repeated key"), "{summary}");
}

#[test]
fn check_exits_1_on_strict_rule_ambiguity_and_0_when_clean() {
    let o = txtql(&["check", &case_file("strict_rule", "query.tql"), &case_file("strict_rule", "input.txt")], b"");
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("txtql::input::ambiguous"), "{}", stderr(&o));
    let o = txtql(&["check", &case_file("rhymes_map", "query.tql"), &case_file("rhymes_map", "input.txt")], b"");
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(stderr(&o).contains("query OK; no output-changing ambiguity on this input"), "{}", stderr(&o));
}
