#!/bin/bash
# Re-checks every documented example against the built binary.
#  gen.py --check : runs all examples, error-code triggers, table rows and prose claims; fails if a doc is stale
#  lsp_check.py   : drives `txtql lsp` over stdio
#  libtest        : compiles and runs the library snippets used in README / Getting-Started
set -e
cd "$(dirname "$0")"
(cd ../.. && cargo build --release -q)
python3 gen.py --check
timeout 60 python3 lsp_check.py | head -3
(cd libtest && for b in short full; do CARGO_TARGET_DIR=../../../target/libtest cargo run -q --bin $b; done)
