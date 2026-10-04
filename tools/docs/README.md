# Documentation tooling

`README.md` and `wiki/*.md` at the repo root are **generated** from the templates in `src/`.
Edit the templates, never the generated files.

    tools/docs/gen.py           regenerate README.md and wiki/
    tools/docs/gen.py --check   fail if a doc is stale or any example behaves differently
    tools/docs/run_all.sh       build, run --check, drive the language server, compile the library snippets

Every `{{ex NAME}}` placeholder is an example from `examples.py`; the generator runs it against
`target/release/txtql` and pastes the real output. Changing language behaviour therefore makes
`--check` fail until the examples and docs are updated.
