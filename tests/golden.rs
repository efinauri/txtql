//! Data-driven end-to-end tests.
//!
//! Each directory in `tests/cases/` holds `query.tql`, `input.txt` and the expected outcome:
//! `expected.json` for a successful run, or `expected.err` for rendered errors. Rendered
//! warnings (lints, ambiguities and repeated keys) go to `expected.warnings`; a missing file means none.
//!
//! Run with `UPDATE_GOLDEN=1` to (re)write the expected files from the actual output.

mod common;
use std::fs;
use std::path::Path;
use txtql::{Options, Query};

fn outcome(query: &str, input: &str) -> (Result<String, String>, String) {
    let q = match Query::compile(query) {
        Ok(q) => q,
        Err(e) => return (Err(common::render(&e.reports("query.tql", query))), String::new()),
    };
    let mut warnings = common::render(&txtql::warning_reports(&q.warnings, "query.tql", query));
    match q.run(input, &Options::default()) {
        Ok(out) => {
            let mut reports = txtql::ambiguity_reports(&out.ambiguities, "input.txt", input);
            reports.extend(txtql::repeated_key_reports(&out.repeated_keys, "query.tql", query, "input.txt", input));
            let amb = common::render(&reports);
            if !amb.is_empty() {
                if !warnings.is_empty() {
                    warnings.push('\n');
                }
                warnings.push_str(&amb);
            }
            (Ok(serde_json::to_string_pretty(&out.value).unwrap() + "\n"), warnings)
        }
        Err(e) => (Err(common::render(&e.reports("query.tql", query, "input.txt", input))), warnings),
    }
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn check_file(dir: &Path, name: &str, actual: Option<&str>, update: bool, failures: &mut Vec<String>) {
    let path = dir.join(name);
    let expected = read(&path);
    let actual = actual.filter(|a| !a.is_empty());
    if update {
        match actual {
            Some(a) => fs::write(&path, a).unwrap(),
            None => {
                let _ = fs::remove_file(&path);
            }
        }
        return;
    }
    if expected.as_deref() != actual {
        failures.push(format!(
            "{}/{name}:\n--- expected\n{}\n--- actual\n{}",
            dir.display(),
            expected.as_deref().unwrap_or("<none>"),
            actual.unwrap_or("<none>")
        ));
    }
}

#[test]
fn golden_cases() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let mut dirs: Vec<_> = fs::read_dir(&root).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    assert!(!dirs.is_empty());
    let mut failures = Vec::new();
    for dir in &dirs {
        let query = read(&dir.join("query.tql")).unwrap_or_else(|| panic!("{} has no query.tql", dir.display()));
        let input = read(&dir.join("input.txt")).unwrap_or_default();
        let (result, warnings) = outcome(&query, &input);
        let (json, err) = match &result {
            Ok(j) => (Some(j.as_str()), None),
            Err(e) => (None, Some(e.as_str())),
        };
        check_file(dir, "expected.json", json, update, &mut failures);
        check_file(dir, "expected.err", err, update, &mut failures);
        check_file(dir, "expected.warnings", Some(&warnings), update, &mut failures);
    }
    assert!(failures.is_empty(), "{} golden mismatches:\n\n{}", failures.len(), failures.join("\n\n"));
}
