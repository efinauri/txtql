# Editor Support

`txtql lsp` is a language server. It works in any editor that speaks the
[Language Server Protocol](https://microsoft.github.io/language-server-protocol/) and talks over standard input and output.
Query files conventionally use the extension `.tql` and the language id `txtql`.

## What you get

- **Diagnostics as you type**: exactly the ones `txtql check` prints without a sample (syntax error, or every static error and lint), with codes,
  related places and help text; warnings show as warnings. Closing a file clears them.
- **Semantic highlighting** that knows what each name is: rule, alias, label, capture, loop variable, function, field or constant.
- **Go to definition, find references, highlight occurrences and rename** for rules, aliases, labels and loop variables.
  Renaming a rule also renames the implicit captures that use it. The root `TEXT` cannot be renamed, and the new name must be an identifier that is not a keyword.
- **Hover**: a rule or alias shows its definition and the `--` comment lines directly above it (no blank line between); a label or capture shows its full pattern;
  keywords, built-in patterns and functions show documentation.
- **Completion** that depends on where you are: primitives, pattern keywords and rule names in patterns; the rule's captures, functions and condition keywords in `WHERE`;
  captures, functions, `FOR` / `IN` / `LISTOF` and the constants in `AS`; `ALIAS` and `STRICT` to start the next definition.
- **An outline** of rules and aliases, with their labels.

It keeps working on unfinished queries: highlighting and navigation survive syntax errors, because definitions are found from `name =`,
`ALIAS name =` and `STRICT name =` without needing the whole query to parse. Documents are synchronised in full on every change.

The server was exercised through a stdio session for this page: an `initialize` handshake advertises definition, highlight, symbols, hover, references, rename, semantic tokens
and completion; opening a query with an undefined rule publishes `txtql::check::undefined_rule` as an error; definition, references (2), rename edits,
completion (30 items in a pattern position), the outline and semantic tokens all answer.

## Sublime Text

Copy the files from `editors/sublime/` into your `Packages/User` folder (on Linux `~/.config/sublime-text/Packages/User/`; on macOS
`~/Library/Application Support/Sublime Text/Packages/User/`):

```
mkdir -p ~/.config/sublime-text/Packages/User/txtql
cp editors/sublime/txtql.sublime-syntax editors/sublime/Comments.tmPreferences ~/.config/sublime-text/Packages/User/txtql/
```

`txtql.sublime-syntax` gives colours for `*.tql` and `Comments.tmPreferences` makes `Ctrl+/` toggle `-- ` comments. The LSP client settings and the
colour-scheme rule that semantic highlighting needs are separate files; if you already have an `LSP.sublime-settings` or a colour scheme, merge the entries from
`editors/sublime/` into it by hand instead of overwriting it, otherwise copy the files into `Packages/User` as they are.

Then, in Sublime, install Package Control (Tools, Install Package Control) and the **LSP** package (Package Control: Install Package, then LSP).
The files are in `editors/sublime/`:

| File | Purpose |
|---|---|
| `txtql.sublime-syntax` | syntax for `*.tql`; the language server adds the semantic colours |
| `Comments.tmPreferences` | comment toggling with `-- ` |
| `LSP.sublime-settings` | `"semantic_highlighting": true` and a `txtql` client running `["txtql", "lsp"]` for `source.txtql` |
| `Mariana.sublime-color-scheme`, `Breakers.sublime-color-scheme` | add a `meta.semantic-token` rule (a background marginally different from the scheme's own) to those built-in schemes, because semantic highlighting only shows when the scheme has it |

If Sublime does not see your shell's `PATH`, use the full path in `command` (for example `/usr/local/bin/txtql`). With another colour scheme, add the same
`meta.semantic-token` rule to it. To remove the support again, delete the `txtql` folder from `Packages/User` and the entries you merged.

## JetBrains IDEs (IntelliJ, RustRover, ...)

Install the **LSP4IJ** plugin (Settings, Plugins, Marketplace). Then under Settings, Languages & Frameworks, Language Servers, add a server with the command
`txtql lsp` (use the full path, such as `/usr/local/bin/txtql lsp`, if the IDE does not see your shell's `PATH`). On its Mappings tab, add the file name
pattern `*.tql` with language id `txtql`.

## Neovim (0.11 or later)

```lua
vim.filetype.add({ extension = { tql = 'txtql' } })
vim.lsp.config('txtql', { cmd = { 'txtql', 'lsp' }, filetypes = { 'txtql' } })
vim.lsp.enable('txtql')
```

## Helix

In `languages.toml`:

```toml
[[language]]
name = "txtql"
scope = "source.txtql"
file-types = ["tql"]
comment-token = "--"
language-servers = ["txtql"]

[language-server.txtql]
command = "txtql"
args = ["lsp"]
```

## Other editors

Register `txtql lsp` as the server for `*.tql` files with language id `txtql`. There is no other editor-specific package. The bundled Sublime
syntax lists every keyword and function of the language (a test keeps it in step), so it is a good starting point for a TextMate-style grammar.
Unsupported requests get a "method not found" error; a rename to an invalid name is rejected with an error naming it.
