# Publishing these pages to the GitHub wiki

The files in this directory are ready for the GitHub wiki of the repository. File names are the page titles with hyphens, which is how
GitHub names wiki pages (`Language-Walkthrough.md` becomes the page "Language Walkthrough"). Links between pages use the wiki syntax
`[[Page Title|Page-Name]]`, which GitHub renders in the wiki (it does not resolve them when you browse these files in the repository itself).
`_Sidebar.md` becomes the sidebar of every page. Do not copy this `README.md`.

1. In the GitHub repository, open the **Wiki** tab and create the first page in the web UI (any content; you will overwrite it).
   This creates the wiki's own git repository.
2. Clone it next to the main repository and copy the pages in:

   ```
   git clone https://github.com/efinauri/txtql.wiki.git
   cd txtql.wiki
   for f in ../txtql/wiki/*.md; do
     [ "$(basename "$f")" = README.md ] || cp "$f" .
   done
   git add -A
   git commit -m "Publish documentation"
   git push
   ```

   Adjust `../txtql/wiki` to wherever the main checkout is. The default branch of a wiki repository is usually `master`.
3. The pages replace the placeholder page. The first page created in the UI may be called `Home`; `Home.md` here overwrites it.

To update later, edit the templates in `tools/docs/src/`, run `python3 tools/docs/gen.py`, copy the regenerated pages over the wiki clone and push again.
`python3 tools/docs/gen.py --check` re-runs every example with the built binary and fails when a doc is stale.
